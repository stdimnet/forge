//! The guest project backend end to end: a host with a `LocalBackend` on a temporary project
//! (a real git repository, secret files, symlinks that leave the project) and guests that use
//! it through `CollabBackend` over the in-memory network. Includes the hostile guest: every
//! call with traversal paths, symlinks, `.git/` and secret files must be refused.

#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use forge_backend::protocol::PhpCall;
use forge_backend::{framework_call, Backend, FrameworkCall, LocalBackend};
use forge_collab::excludes::Excludes;
use forge_collab::guest::{GuestEvent, GuestSession};
use forge_collab::guest_backend::calls::{GitAction, GitQuery, HostCall, LspQuery};
use forge_collab::guest_backend::CollabBackend;
use forge_collab::host::{
    DevAutoAccept, HostConfig, HostServices, HostSession, LspRequest, DEV_PREVIEW_NOTICE,
};
use forge_collab::permissions::{FilesAccess, Permissions, RunAccess, VcsAccess};
use forge_collab::protocol::{ViewOnly, MAX_VIEW_SIZE};
use forge_collab::reconnect::ReconnectPolicy;
use forge_collab::transport::memory::MemoryNetwork;
use forge_core::search::FindRequest;
use forge_core::FsChanges;
use forge_vcs::LogQuery;
use futures_util::future::BoxFuture;

const TIMEOUT: Duration = Duration::from_secs(20);

/// A file's diagnostics as a guest received them.
type Diagnostics = (String, Vec<u8>);

/// Answers language server requests and records what the host was asked.
#[derive(Default)]
struct FakeLsp {
    seen: Mutex<Vec<LspRequest>>,
}

impl HostServices for FakeLsp {
    fn lsp(&self, req: LspRequest) -> BoxFuture<'static, Result<Vec<u8>, String>> {
        let reply = match &req.query {
            LspQuery::Hover { .. } => serde_json::json!({
                "contents": "```php\nclass User\n```",
                "range": null
            }),
            LspQuery::Definition { .. } => serde_json::json!([
                { "path": "app/Models/User.php", "line": 0, "character": 6 },
                { "path": ".env", "line": 0, "character": 0 },
                { "path": "/home/host/.ssh/id_rsa", "line": 0, "character": 0 },
                { "path": "config/master.key", "line": 0, "character": 0 }
            ]),
            _ => serde_json::json!(null),
        };
        self.seen.lock().unwrap().push(req);
        Box::pin(async move { Ok(serde_json::to_vec(&reply).unwrap()) })
    }
}

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .current_dir(dir)
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .env("GIT_AUTHOR_NAME", "Robert")
        .env("GIT_AUTHOR_EMAIL", "r@example.com")
        .env("GIT_COMMITTER_NAME", "Robert")
        .env("GIT_COMMITTER_EMAIL", "r@example.com")
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?}");
}

struct World {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    outside: PathBuf,
    net: MemoryNetwork,
    host: Arc<HostSession>,
    lsp: Arc<FakeLsp>,
    next_key: u8,
}

struct Guest {
    session: Arc<GuestSession>,
    backend: Arc<CollabBackend>,
}

impl World {
    fn new(permissions: Permissions) -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("shop");
        let outside = tmp.path().join("outside");
        std::fs::create_dir_all(root.join("app/Models")).unwrap();
        std::fs::create_dir_all(root.join("app/Http")).unwrap();
        std::fs::create_dir_all(root.join("config")).unwrap();
        std::fs::create_dir_all(root.join("secrets_dir")).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("secret.txt"), "TOP SECRET\n").unwrap();
        std::fs::write(
            root.join(".env"),
            "APP_KEY=base64:topsecret\nDB_PASSWORD=hunter2\n",
        )
        .unwrap();
        std::fs::write(root.join(".env.example"), "APP_KEY=\nDB_PASSWORD=\n").unwrap();
        std::fs::write(root.join(".gitignore"), "vendor/\n").unwrap();
        std::fs::write(
            root.join("composer.json"),
            "{\n  \"name\": \"acme/shop\"\n}\n",
        )
        .unwrap();
        std::fs::write(root.join("app/Models/User.php"), "<?php\nclass User {}\n").unwrap();
        std::fs::write(root.join("app/Http/Order.php"), "<?php\nclass Order {}\n").unwrap();
        std::fs::write(root.join("config/master.key"), "MASTERKEY\n").unwrap();
        std::fs::write(root.join("secrets_dir/readme.txt"), "x\n").unwrap();
        git(&root, &["init", "-q", "-b", "main"]);
        git(&root, &["add", "-A", "-f"]);
        git(&root, &["commit", "-q", "-m", "Initial shop"]);
        // Changes after the commit: a visible one and a hidden one.
        std::fs::write(root.join("app/Models/User.php"), "<?php\nclass User { }\n").unwrap();
        std::fs::write(root.join(".env"), "APP_KEY=base64:rotated\n").unwrap();
        // Links that leave the project or point at things guests must not reach.
        std::os::unix::fs::symlink("../outside", root.join("link_out")).unwrap();
        std::os::unix::fs::symlink("../outside/secret.txt", root.join("leak.txt")).unwrap();
        std::os::unix::fs::symlink(".env", root.join("envlink")).unwrap();
        std::os::unix::fs::symlink(".git", root.join("gitlink")).unwrap();
        std::os::unix::fs::symlink("app/Models/User.php", root.join("user_link.php")).unwrap();

        let backend: Arc<dyn Backend> = Arc::new(LocalBackend::open(&root).unwrap());
        let net = MemoryNetwork::new();
        let host = HostSession::start(
            Arc::new(net.endpoint([1; 32])),
            backend,
            HostConfig {
                host_name: "Robert".into(),
                machine: Some("laptop".into()),
                permissions,
                excludes: Excludes::defaults(),
                protected: Excludes::protected_defaults(),
                max_file_size: 64 * 1024,
                notice: Some(DEV_PREVIEW_NOTICE.into()),
            },
            Arc::new(DevAutoAccept),
            Arc::new(|_| {}),
        );
        let lsp = Arc::new(FakeLsp::default());
        host.set_services(lsp.clone());
        Self {
            _tmp: tmp,
            root,
            outside,
            net,
            host,
            lsp,
            next_key: 2,
        }
    }

    async fn join(&mut self, name: &str) -> Guest {
        self.join_policy(name, ReconnectPolicy::default()).await
    }

    /// The endpoint key of the guest that joined last.
    fn last_key(&self) -> [u8; 32] {
        [self.next_key - 1; 32]
    }

    async fn join_policy(&mut self, name: &str, policy: ReconnectPolicy) -> Guest {
        let key = [self.next_key; 32];
        self.next_key += 1;
        let link = self.host.link();
        let session = GuestSession::join_with(
            Arc::new(self.net.endpoint(key)),
            &link,
            name,
            Arc::new(|_: GuestEvent| {}),
            policy,
        )
        .await
        .unwrap();
        let s = session.clone();
        let backend = tokio::task::spawn_blocking(move || CollabBackend::open(s))
            .await
            .unwrap()
            .unwrap();
        Guest {
            session,
            backend: Arc::new(backend),
        }
    }
}

impl Guest {
    /// Run a blocking backend call off the async runtime, as the app does.
    async fn on<T: Send + 'static>(
        &self,
        f: impl FnOnce(Arc<CollabBackend>) -> T + Send + 'static,
    ) -> T {
        let b = self.backend.clone();
        tokio::task::spawn_blocking(move || f(b)).await.unwrap()
    }

    async fn call(&self, call: HostCall) -> Result<Vec<u8>, String> {
        let s = self.session.clone();
        tokio::task::spawn_blocking(move || s.call_blocking(&call, TIMEOUT))
            .await
            .unwrap()
    }
}

fn names(entries: &[forge_core::FileEntry]) -> Vec<String> {
    entries.iter().map(|e| e.name.clone()).collect()
}

const EDIT: Permissions = Permissions::EDIT_FILES;
const READ: Permissions = Permissions::READ_ONLY;
const FULL: Permissions = Permissions::FULL_ACCESS;

/// The text of a document as `update` (one Yjs update) holds it.
fn text_of(update: &[u8]) -> String {
    use yrs::updates::decoder::Decode;
    use yrs::{GetString, Transact};
    let replica = yrs::Doc::with_options(yrs::Options {
        offset_kind: yrs::OffsetKind::Utf16,
        ..yrs::Options::default()
    });
    let text = replica.get_or_insert_text("content");
    replica
        .transact_mut()
        .apply_update(yrs::Update::decode_v1(update).unwrap())
        .unwrap();
    let s = text.get_string(&replica.transact());
    s
}

// ---- the project as the guest sees it --------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_guest_browses_and_reads_the_hosts_project() {
    let mut w = World::new(READ);
    let g = w.join("Alex").await;
    let info = g.backend.info().clone();
    assert_eq!(info.name, "shop");
    assert_eq!(
        info.root,
        PathBuf::from("/shop"),
        "the host's path is not sent"
    );
    let collab = info.collab.clone().unwrap();
    assert_eq!(collab.name, "Robert");
    assert!(collab.label.contains("laptop"));
    assert!(!info.is_local());

    let root = g.on(|b| b.list_dir("")).await.unwrap();
    let listed = names(&root);
    assert!(listed.contains(&"app".to_string()));
    assert!(listed.contains(&"composer.json".to_string()));
    assert!(listed.contains(&".env.example".to_string()));
    for hidden in [".env", ".git", "config"] {
        if hidden == "config" {
            continue;
        }
        assert!(!listed.contains(&hidden.to_string()), "{hidden} is listed");
    }
    let config = g.on(|b| b.list_dir("config")).await.unwrap();
    assert!(config.is_empty(), "master.key is hidden: {config:?}");

    let text = g.on(|b| b.read_file("app/Models/User.php")).await.unwrap();
    assert_eq!(text, "<?php\nclass User { }\n", "the file as it is on disk");
    assert_eq!(g.on(|b| b.read_optional("nope.php")).await.unwrap(), None);
    // The project is detected on the host.
    let detected = g.on(|b| b.detect_project()).await.unwrap();
    assert!(detected.uses("git"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn secret_files_and_git_are_invisible_everywhere() {
    let mut w = World::new(FULL);
    let g = w.join("Alex").await;
    for path in [
        ".env",
        ".env.local",
        "config/master.key",
        ".git/config",
        ".git/HEAD",
    ] {
        let p = path.to_string();
        let err = g
            .on(move |b| b.read_file(&p))
            .await
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("not shared") || err.contains("internal files"),
            "{path}: {err}"
        );
    }
    // Missing and hidden look the same to code that probes for settings files.
    assert_eq!(g.on(|b| b.read_optional(".env")).await.unwrap(), None);
    assert_eq!(
        g.on(|b| b.read_optional(".git/config")).await.unwrap(),
        None
    );
    assert!(g.on(|b| b.list_dir(".git")).await.is_err());

    let hits = g
        .on(|b| {
            b.find_in_files(&FindRequest {
                query: "hunter2|topsecret|MASTERKEY|TOP SECRET|rotated".into(),
                regex: true,
                ..FindRequest::default()
            })
        })
        .await
        .unwrap();
    assert!(hits.files.is_empty(), "{hits:?}");
    assert_eq!(hits.match_count, 0);
    assert!(!hits.truncated);
    let mut sees = Vec::new();
    for q in [".env", "master", "config", "HEAD"] {
        let q = q.to_string();
        sees.extend(g.on(move |b| b.find_files(&q, 50, false)).await.unwrap());
    }
    for m in &sees {
        assert!(
            !m.path.starts_with(".git/") && m.path != ".env" && !m.path.ends_with("master.key"),
            "{}",
            m.path
        );
    }
    // A search over the visible files works.
    let hits = g
        .on(|b| {
            b.find_in_files(&FindRequest {
                query: "class Order".into(),
                ..FindRequest::default()
            })
        })
        .await
        .unwrap();
    assert_eq!(hits.files.len(), 1);
    assert_eq!(hits.files[0].path, "app/Http/Order.php");
    let files = g.on(|b| b.find_files("order", 10, false)).await.unwrap();
    assert_eq!(files[0].path, "app/Http/Order.php");
}

// ---- path traversal and symlinks ---------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn paths_outside_the_project_are_refused() {
    let mut w = World::new(FULL);
    let g = w.join("Alex").await;
    for path in [
        "../outside/secret.txt",
        "/etc/passwd",
        "app/../../outside/secret.txt",
        "app/../.env",
        "./.env",
        "app//Models/User.php",
        "..",
        "C:\\Windows\\win.ini",
        "app\\..\\.env",
        "app/Models/User.php\0.txt",
    ] {
        let p = path.to_string();
        assert!(
            g.on(move |b| b.read_file(&p)).await.is_err(),
            "read {path:?}"
        );
        let p = path.to_string();
        assert!(
            g.on(move |b| b.list_dir(&p)).await.is_err(),
            "list {path:?}"
        );
        let p = path.to_string();
        assert!(
            g.on(move |b| b.write_file(&p, "x")).await.is_err(),
            "write {path:?}"
        );
        let p = path.to_string();
        assert!(
            g.on(move |b| b.delete(&p)).await.is_err(),
            "delete {path:?}"
        );
    }
    assert!(w.outside.join("secret.txt").exists());
    assert_eq!(
        std::fs::read_to_string(w.outside.join("secret.txt")).unwrap(),
        "TOP SECRET\n"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn symlinks_cannot_be_used_to_leave_the_project_or_reach_hidden_files() {
    let mut w = World::new(FULL);
    let g = w.join("Alex").await;
    // Reading: a link to a file outside, a folder outside, a hidden file and .git.
    for path in [
        "leak.txt",
        "link_out/secret.txt",
        "envlink",
        "gitlink/config",
        "gitlink/HEAD",
    ] {
        let p = path.to_string();
        let r = g.on(move |b| b.read_file(&p)).await;
        assert!(r.is_err(), "{path} was readable: {r:?}");
        let p = path.to_string();
        let r = g.on(move |b| b.read_optional(&p)).await;
        assert!(!matches!(r, Ok(Some(_))), "{path} was readable: {r:?}");
    }
    // Listing a link to a folder outside the project.
    assert!(g.on(|b| b.list_dir("link_out")).await.is_err());
    assert!(g.on(|b| b.list_dir("gitlink")).await.is_err());
    // Writing, creating, renaming and deleting through links.
    assert!(g
        .on(|b| b.write_file("leak.txt", "overwritten"))
        .await
        .is_err());
    assert!(g
        .on(|b| b.write_file("link_out/new.txt", "planted"))
        .await
        .is_err());
    assert!(g.on(|b| b.create_file("link_out/new2.txt")).await.is_err());
    assert!(g.on(|b| b.create_dir("link_out/dir")).await.is_err());
    assert!(g.on(|b| b.write_file("envlink", "APP_KEY=")).await.is_err());
    assert!(g
        .on(|b| b.rename("app/Http/Order.php", "link_out/Order.php"))
        .await
        .is_err());
    assert!(g.on(|b| b.rename("leak.txt", "leaked.txt")).await.is_err());
    assert!(g.on(|b| b.delete("link_out/secret.txt")).await.is_err());
    // The files outside are untouched and nothing was planted.
    assert_eq!(
        std::fs::read_to_string(w.outside.join("secret.txt")).unwrap(),
        "TOP SECRET\n"
    );
    assert_eq!(std::fs::read_dir(&w.outside).unwrap().count(), 1);
    assert_eq!(
        std::fs::read_to_string(w.root.join(".env")).unwrap(),
        "APP_KEY=base64:rotated\n"
    );
    // A link inside the project to a visible file in the project is fine.
    assert_eq!(
        g.on(|b| b.read_file("user_link.php")).await.unwrap(),
        "<?php\nclass User { }\n"
    );
    // The listing shows the links like files; reading them is what is refused.
    let root = g.on(|b| b.list_dir("")).await.unwrap();
    assert!(names(&root).contains(&"link_out".to_string()));
}

// ---- read-only versus edit ----------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_read_only_guest_cannot_change_anything() {
    let mut w = World::new(READ);
    let g = w.join("Alex").await;
    let before = std::fs::read_to_string(w.root.join("app/Models/User.php")).unwrap();
    assert!(g
        .on(|b| b.write_file("app/Models/User.php", "<?php // pwned"))
        .await
        .is_err());
    assert!(g.on(|b| b.create_file("app/New.php")).await.is_err());
    assert!(g.on(|b| b.create_dir("app/newdir")).await.is_err());
    assert!(g
        .on(|b| b.rename("app/Models/User.php", "app/Models/Person.php"))
        .await
        .is_err());
    assert!(g.on(|b| b.delete("app/Models/User.php")).await.is_err());
    assert_eq!(
        std::fs::read_to_string(w.root.join("app/Models/User.php")).unwrap(),
        before
    );
    assert!(!w.root.join("app/New.php").exists());
    // Git changes need Full access to version control.
    let repo = g.on(|b| b.repo()).await.unwrap();
    let r = g
        .on(move |_| repo.stage(&["app/Models/User.php".to_string()]))
        .await;
    assert!(r.is_err(), "{r:?}");
    // The formatting request would change a file.
    let r = g
        .call(HostCall::Lsp(LspQuery::Format {
            path: "app/Models/User.php".into(),
            tab_size: 4,
            insert_spaces: true,
        }))
        .await;
    assert!(r.is_err());
    assert!(w.lsp.seen.lock().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_edit_guest_creates_renames_and_deletes_files() {
    let mut w = World::new(EDIT);
    let g = w.join("Alex").await;
    g.on(|b| b.create_dir("app/Services")).await.unwrap();
    g.on(|b| b.create_file("app/Services/Mailer.php"))
        .await
        .unwrap();
    g.on(|b| b.write_file("app/Services/Mailer.php", "<?php\nclass Mailer {}\n"))
        .await
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(w.root.join("app/Services/Mailer.php")).unwrap(),
        "<?php\nclass Mailer {}\n"
    );
    // Creating never replaces an existing file.
    assert!(g
        .on(|b| b.create_file("app/Services/Mailer.php"))
        .await
        .is_err());
    g.on(|b| b.rename("app/Services/Mailer.php", "app/Services/Postman.php"))
        .await
        .unwrap();
    assert!(w.root.join("app/Services/Postman.php").exists());
    assert!(!w.root.join("app/Services/Mailer.php").exists());
    g.on(|b| b.delete("app/Services")).await.unwrap();
    assert!(!w.root.join("app/Services").exists());
    // The project root is never changed, and hidden names cannot be created or taken over.
    assert!(g.on(|b| b.delete("")).await.is_err());
    assert!(g.on(|b| b.create_file(".env.production")).await.is_err());
    assert!(g
        .on(|b| b.rename("app/Http/Order.php", ".env.backup"))
        .await
        .is_err());
    assert!(g
        .on(|b| b.create_file(".git/hooks/pre-commit"))
        .await
        .is_err());
    assert!(w.root.join("app/Http/Order.php").exists());
    // Git needs Full access to version control.
    let r = g
        .on(|b| {
            b.repo()
                .unwrap()
                .stage(&["app/Models/User.php".to_string()])
        })
        .await;
    assert!(r.is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn files_that_configure_what_runs_need_full_access() {
    let mut w = World::new(EDIT);
    let g = w.join("Alex").await;
    for path in [
        "composer.json",
        "package.json",
        "Makefile",
        ".forge/settings.toml",
        ".forge/runConfigurations.toml",
        ".vscode/tasks.json",
        ".idea/workspace.xml",
        ".husky/pre-commit",
        "artisan",
    ] {
        let p = path.to_string();
        let r = g.on(move |b| b.write_file(&p, "{}")).await;
        assert!(
            r.as_ref()
                .is_err_and(|e| e.to_string().contains("Full access")),
            "{path}: {r:?}"
        );
        let p = path.to_string();
        assert!(
            g.on(move |b| b.create_file(&p)).await.is_err(),
            "create {path}"
        );
    }
    assert!(g.on(|b| b.delete("composer.json")).await.is_err());
    assert!(g
        .on(|b| b.rename("composer.json", "composer.old"))
        .await
        .is_err());
    assert!(g
        .on(|b| b.rename("app/Http/Order.php", "Makefile"))
        .await
        .is_err());
    assert_eq!(
        std::fs::read_to_string(w.root.join("composer.json")).unwrap(),
        "{\n  \"name\": \"acme/shop\"\n}\n"
    );

    // With Run Full access (Full access) they can.
    let mut w = World::new(Permissions {
        files: FilesAccess::Edit,
        terminal: forge_collab::permissions::TerminalAccess::None,
        run: RunAccess::Full,
        vcs: VcsAccess::ReadOnly,
    });
    let g = w.join("Sam").await;
    g.on(|b| b.write_file("composer.json", "{\"name\":\"x/y\"}"))
        .await
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(w.root.join("composer.json")).unwrap(),
        "{\"name\":\"x/y\"}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_folder_with_hidden_files_cannot_be_deleted_by_a_guest() {
    let mut w = World::new(EDIT);
    std::fs::write(w.root.join("secrets_dir/.env"), "TOKEN=1\n").unwrap();
    let g = w.join("Alex").await;
    let err = g.on(|b| b.delete("secrets_dir")).await.unwrap_err();
    assert!(err.to_string().contains("not shared"), "{err}");
    assert!(w.root.join("secrets_dir/.env").exists());
    // A folder without hidden content can be deleted.
    g.on(|b| b.create_dir("tmp/a")).await.unwrap();
    g.on(|b| b.delete("tmp")).await.unwrap();
}

// ---- git ------------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn git_status_log_blame_and_diff_come_from_the_host_without_hidden_files() {
    let mut w = World::new(READ);
    std::fs::write(w.root.join("app/Untracked.php"), "<?php\n").unwrap();
    std::fs::write(w.root.join("secrets_dir/.env"), "TOKEN=1\n").unwrap();
    let g = w.join("Alex").await;
    let repo = g.on(|b| b.repo()).await.unwrap();
    assert!(repo.root().starts_with("/shop"), "{:?}", repo.root());

    let (r, status) = {
        let r = repo.clone();
        let s = g.on(move |_| r.status()).await.unwrap();
        (repo.clone(), s)
    };
    let paths: Vec<String> = status.entries.iter().map(|e| e.path.clone()).collect();
    assert!(
        paths.contains(&"app/Models/User.php".to_string()),
        "{paths:?}"
    );
    assert!(
        paths.contains(&"app/Untracked.php".to_string()),
        "{paths:?}"
    );
    assert!(
        !paths.iter().any(|p| p.ends_with(".env")),
        "hidden files show up in the status: {paths:?}"
    );
    assert_eq!(status.branch.as_deref(), Some("main"));

    let r2 = r.clone();
    let log = g.on(move |_| r2.log(&LogQuery::default())).await.unwrap();
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].subject, "Initial shop");

    let r2 = r.clone();
    let blame = g
        .on(move |_| r2.blame("app/Models/User.php", None))
        .await
        .unwrap();
    assert!(!blame.lines.is_empty());

    let r2 = r.clone();
    let diff = g
        .on(move |_| r2.diff_text(&["app/Models/User.php".to_string()], false))
        .await
        .unwrap();
    assert!(diff.contains("+class User { }"), "{diff}");
    // The diff of everything leaves out the hidden file.
    let r2 = r.clone();
    let all = g.on(move |_| r2.diff_head_text(&[])).await.unwrap();
    assert!(all.contains("User.php"));
    assert!(
        !all.contains("rotated") && !all.contains("topsecret"),
        "{all}"
    );

    // File content at a revision: visible yes, hidden no, even though it is in history.
    let r2 = r.clone();
    let head = g
        .on(move |_| r2.show("HEAD", "app/Models/User.php"))
        .await
        .unwrap();
    assert_eq!(head.as_deref(), Some("<?php\nclass User {}\n"));
    let r2 = r.clone();
    let hidden = g.on(move |_| r2.show("HEAD", ".env")).await;
    assert!(hidden.is_err(), "{hidden:?}");
    let r2 = r.clone();
    let details = g.on(move |_| r2.commit_details("HEAD")).await.unwrap();
    let files: Vec<&str> = details.files.iter().map(|f| f.path.as_str()).collect();
    assert!(files.contains(&"app/Models/User.php"));
    assert!(
        !files
            .iter()
            .any(|p| p.ends_with(".env") || p.ends_with("master.key")),
        "{files:?}"
    );
    let r2 = r.clone();
    let branches = g.on(move |_| r2.branches()).await.unwrap();
    assert_eq!(branches.current.as_deref(), Some("main"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn git_actions_need_full_access_and_use_the_hosts_repository() {
    let mut w = World::new(FULL);
    let g = w.join("Alex").await;
    let repo = g.on(|b| b.repo()).await.unwrap();
    let r = repo.clone();
    g.on(move |_| r.stage(&["app/Models/User.php".to_string()]))
        .await
        .unwrap();
    let r = repo.clone();
    let sha = g
        .on(move |_| r.commit("Reformat User", false, None))
        .await
        .unwrap();
    assert_eq!(sha.len(), 40);
    let out = Command::new("git")
        .current_dir(&w.root)
        .args(["log", "--format=%s", "-n", "1"])
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "Reformat User");
    // The hidden file's change was not part of the commit.
    let out = Command::new("git")
        .current_dir(&w.root)
        .args(["status", "--porcelain"])
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&out.stdout).contains(".env"));
    let r = repo.clone();
    g.on(move |_| r.create_branch("feature/x", None, true))
        .await
        .unwrap();
    // A pathspec outside the project or on a hidden file is refused.
    let r = repo.clone();
    assert!(g.on(move |_| r.stage(&[".env".to_string()])).await.is_err());
    assert!(g
        .on(|b| {
            b.git().exec(&forge_vcs::GitRequest {
                cwd: "/shop".into(),
                args: vec![
                    "restore".into(),
                    "--staged".into(),
                    "--".into(),
                    "../x".into(),
                ],
                stdin: None,
                non_interactive: false,
            })
        })
        .await
        .is_err());
}

// ---- the hostile guest ----------------------------------------------------------------

fn hostile_calls() -> Vec<HostCall> {
    let bad = [
        "../outside/secret.txt",
        "/etc/passwd",
        "app/../../etc/passwd",
        ".git/config",
        ".git",
        "app/.git/config",
        ".env",
        "config/master.key",
        "leak.txt",
        "link_out/secret.txt",
        "envlink",
        "gitlink/HEAD",
        "",
        "C:/x",
        "a\0b",
    ];
    let mut calls = Vec::new();
    for p in bad {
        let p = || p.to_string();
        calls.extend([
            HostCall::ReadFile { path: p() },
            HostCall::WriteFile {
                path: p(),
                contents: "x".into(),
            },
            HostCall::CreateFile { path: p() },
            HostCall::CreateDir { path: p() },
            HostCall::Rename {
                from: p(),
                to: "app/x".into(),
            },
            HostCall::Rename {
                from: "app/Http/Order.php".into(),
                to: p(),
            },
            HostCall::Delete { path: p() },
            HostCall::Git(GitQuery::Show {
                rev: "HEAD".into(),
                path: p(),
            }),
            HostCall::Git(GitQuery::Blame {
                path: p(),
                contents: None,
            }),
            HostCall::Git(GitQuery::Diff {
                staged: false,
                against_head: false,
                paths: vec![p()],
            }),
            HostCall::Git(GitQuery::LsFiles { paths: vec![p()] }),
            HostCall::GitAction(GitAction::Stage { paths: vec![p()] }),
            HostCall::GitAction(GitAction::RestoreFromHead { paths: vec![p()] }),
            HostCall::Php(PhpCall::Structure {
                path: p(),
                content: None,
                inherited: false,
            }),
            HostCall::Php(PhpCall::Hover {
                path: p(),
                offset: 0,
                content: None,
            }),
            HostCall::Php(PhpCall::Generate {
                path: p(),
                offset: 0,
                kind: phpforge_lang_php::generate::GenerateKind::Getters,
                options: Default::default(),
                content: None,
            }),
            HostCall::Framework(FrameworkCall::LaravelGoto {
                path: p(),
                offset: 0,
                content: None,
            }),
            HostCall::Lsp(LspQuery::Hover {
                path: p(),
                line: 0,
                character: 0,
            }),
            HostCall::Lsp(LspQuery::Sync { path: p() }),
            HostCall::Lsp(LspQuery::Format {
                path: p(),
                tab_size: 4,
                insert_spaces: true,
            }),
        ]);
        if !p().is_empty() {
            // An empty directory is the whole project.
            calls.push(HostCall::FindInFiles(FindRequest {
                query: "x".into(),
                directory: p(),
                ..FindRequest::default()
            }));
        }
    }
    calls.extend([
        // Raw options in places a typed value must not carry them.
        HostCall::Git(GitQuery::CommitMeta {
            rev: "--output=/tmp/pwned".into(),
        }),
        HostCall::Git(GitQuery::CommitMeta {
            rev: "HEAD:.env".into(),
        }),
        HostCall::Git(GitQuery::Show {
            rev: "HEAD:../x".into(),
            path: "a".into(),
        }),
        HostCall::Git(GitQuery::Log {
            limit: 1,
            skip: 0,
            text: None,
            author: None,
            rev: Some("--all".into()),
            all: false,
            path: None,
        }),
        HostCall::GitAction(GitAction::FetchRefspec {
            remote: "--upload-pack=touch /tmp/pwned".into(),
            refspec: "x".into(),
        }),
        HostCall::GitAction(GitAction::Checkout {
            rev: "--orphan".into(),
        }),
        HostCall::GitAction(GitAction::Push {
            force: true,
            set_upstream: Some("--receive-pack=sh".into()),
        }),
        // Language servers are the host's business.
        HostCall::Php(PhpCall::LspConfig {
            settings: Default::default(),
        }),
        HostCall::Php(PhpCall::DetectServers {
            settings: Default::default(),
        }),
    ]);
    calls
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_hostile_guest_is_refused_every_call_at_every_permission() {
    for permissions in [READ, EDIT, FULL] {
        let mut w = World::new(permissions);
        let g = w.join("Mallory").await;
        // Git actions last: acting on a link (they may delete it) changes the project.
        let mut calls = hostile_calls();
        calls.sort_by_key(|c| matches!(c, HostCall::GitAction(_)));
        for call in calls {
            let name = format!("{call:?}");
            let r = g.call(call.clone()).await;
            // Git reads its own object database and never follows a link out of the
            // project: a path it does not know is an empty answer or git's failure, never
            // content. No other call may succeed.
            if let (HostCall::Git(_), Ok(bytes)) = (&call, &r) {
                let out: forge_vcs::GitOutput =
                    forge_backend::protocol::decode_value(bytes).unwrap();
                assert!(out.stdout.is_empty(), "{permissions:?}: {name} -> {out:?}");
                continue;
            }
            // Git acting on a link acts on the link itself (its text), which is a file of
            // the project; nothing behind the link is read or written. Git itself refuses
            // paths beyond a link (`ok: false`).
            if let (HostCall::GitAction(_), Ok(bytes)) = (&call, &r) {
                let out: forge_vcs::GitOutput =
                    forge_backend::protocol::decode_value(bytes).unwrap();
                if !out.ok || name.contains("\"leak.txt\"") || name.contains("\"envlink\"") {
                    continue;
                }
            }
            assert!(r.is_err(), "{permissions:?}: {name} -> {r:?}");
        }
        for (p, hidden) in [(".env", true), ("config/master.key", true), ("../x", false)] {
            let r = g.call(HostCall::ReadOptional { path: p.into() }).await;
            if hidden {
                let bytes = r.unwrap();
                assert_eq!(
                    forge_backend::protocol::decode_value::<Option<String>>(&bytes).unwrap(),
                    None
                );
            } else {
                assert!(r.is_err());
            }
        }
        // Nothing happened on disk.
        assert_eq!(
            std::fs::read_to_string(w.outside.join("secret.txt")).unwrap(),
            "TOP SECRET\n"
        );
        assert_eq!(std::fs::read_dir(&w.outside).unwrap().count(), 1);
        assert_eq!(
            std::fs::read_to_string(w.root.join(".env")).unwrap(),
            "APP_KEY=base64:rotated\n"
        );
        assert!(
            w.lsp.seen.lock().unwrap().is_empty(),
            "no hostile query reached the servers"
        );
        assert!(!Path::new("/tmp/pwned").exists());
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_host_survives_garbage_and_floods() {
    let mut w = World::new(FULL);
    let g = w.join("Mallory").await;
    let huge = HostCall::WriteFile {
        path: "app/big.txt".into(),
        contents: "x".repeat(3 * 1024 * 1024),
    };
    // Within the frame limit: fine.
    g.call(huge).await.unwrap();
    // Reading it back works; many calls at once are answered or refused, never lost.
    let mut set = tokio::task::JoinSet::new();
    for _ in 0..200 {
        let s = g.session.clone();
        set.spawn(async move {
            s.call(&HostCall::ListDir { path: "app".into() }, TIMEOUT)
                .await
        });
    }
    let mut ok = 0;
    let mut refused = 0;
    while let Some(r) = set.join_next().await {
        match r.unwrap() {
            Ok(_) => ok += 1,
            Err(e) => {
                assert!(e.contains("Too many requests"), "{e}");
                refused += 1;
            }
        }
    }
    assert_eq!(ok + refused, 200);
    assert!(ok > 0);
    // And the guest still works afterwards.
    assert!(!g.on(|b| b.list_dir("app")).await.unwrap().is_empty());
}

// ---- file changes ---------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_guest_is_told_about_changes_it_may_see() {
    let mut w = World::new(READ);
    let g = w.join("Alex").await;
    let seen: Arc<Mutex<Vec<FsChanges>>> = Arc::default();
    let sink = seen.clone();
    let guard = g
        .on(move |b| b.watch(Arc::new(move |c| sink.lock().unwrap().push(c))))
        .await
        .unwrap();
    w.host
        .on_fs_changed(&FsChanges {
            changed: vec![
                "app/Models/User.php".into(),
                ".env".into(),
                ".git/index".into(),
                "config/master.key".into(),
                "link/../x".into(),
            ],
            removed: vec!["app/Old.php".into(), ".env.local".into()],
            git: true,
        })
        .await;
    for _ in 0..200 {
        if !seen.lock().unwrap().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let got = seen.lock().unwrap().clone();
    assert_eq!(got.len(), 1, "{got:?}");
    assert_eq!(got[0].changed, vec!["app/Models/User.php".to_string()]);
    assert_eq!(got[0].removed, vec!["app/Old.php".to_string()]);
    assert!(got[0].git);
    // Only hidden files changed: nothing is sent.
    w.host
        .on_fs_changed(&FsChanges {
            changed: vec![".env".into()],
            removed: vec![],
            git: false,
        })
        .await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(seen.lock().unwrap().len(), 1);
    drop(guard);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_resumed_guest_gets_the_changes_it_missed() {
    let mut w = World::new(READ);
    let fast = ReconnectPolicy {
        backoff: vec![Duration::from_millis(20)],
        give_up_after: Duration::from_secs(10),
    };
    let g = w.join_policy("Alex", fast).await;
    let key = w.last_key();
    let seen: Arc<Mutex<Vec<FsChanges>>> = Arc::default();
    let sink = seen.clone();
    let guard = g
        .on(move |b| b.watch(Arc::new(move |c| sink.lock().unwrap().push(c))))
        .await
        .unwrap();

    w.net.set_offline(key, true);
    let host = w.host.clone();
    for _ in 0..500 {
        if host.participants().iter().any(|p| p.reconnecting) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(host.participants().iter().any(|p| p.reconnecting));
    // While Alex is away: a file is created and deleted again, one is created, one deleted,
    // a hidden one changes, and a commit moves HEAD.
    for (changed, removed, git) in [
        (
            vec!["app/Tmp.php", "app/New.php"],
            vec!["app/Old.php"],
            false,
        ),
        (vec![".env"], vec!["app/Tmp.php"], true),
    ] {
        w.host
            .on_fs_changed(&FsChanges {
                changed: changed.into_iter().map(String::from).collect(),
                removed: removed.into_iter().map(String::from).collect(),
                git,
            })
            .await;
    }
    assert!(seen.lock().unwrap().is_empty());

    w.net.set_offline(key, false);
    for _ in 0..500 {
        if !seen.lock().unwrap().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let got = seen.lock().unwrap().clone();
    assert_eq!(got.len(), 1, "{got:?}");
    assert_eq!(got[0].changed, vec!["app/New.php".to_string()]);
    assert_eq!(
        got[0].removed,
        vec!["app/Old.php".to_string(), "app/Tmp.php".to_string()]
    );
    assert!(got[0].git);
    // Once back, changes flow live again and nothing is sent twice.
    w.host
        .on_fs_changed(&FsChanges {
            changed: vec!["app/Later.php".into()],
            removed: vec![],
            git: false,
        })
        .await;
    for _ in 0..500 {
        if seen.lock().unwrap().len() > 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let got = seen.lock().unwrap().clone();
    assert_eq!(got.len(), 2, "{got:?}");
    assert_eq!(got[1].changed, vec!["app/Later.php".to_string()]);
    drop(guard);
}

// ---- language servers and PHP -----------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn language_server_requests_run_on_the_host_with_its_current_text() {
    let mut w = World::new(READ);
    // The host has the file open with unsaved changes: that text is what the servers see.
    let o = w
        .host
        .open_doc(
            "app/Models/User.php",
            Some("<?php\nclass User { /* unsaved */ }\n".into()),
        )
        .await
        .unwrap();
    assert_eq!(o.path, "app/Models/User.php");
    let g = w.join("Alex").await;
    let reply = g
        .session
        .lsp(LspQuery::Hover {
            path: "app/Models/User.php".into(),
            line: 1,
            character: 7,
        })
        .await
        .unwrap();
    let hover: serde_json::Value = serde_json::from_slice(&reply).unwrap();
    assert!(hover["contents"].as_str().unwrap().contains("class User"));
    {
        let seen = w.lsp.seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert!(
            seen[0].text.as_deref().unwrap().contains("unsaved"),
            "{:?}",
            seen[0].text
        );
    }
    // A file only the guest looks at is read from disk.
    g.session
        .lsp(LspQuery::Sync {
            path: "app/Http/Order.php".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        w.lsp.seen.lock().unwrap()[1].text.as_deref(),
        Some("<?php\nclass Order {}\n")
    );
    // Locations in hidden files and outside the project are dropped from the answer.
    let reply = g
        .session
        .lsp(LspQuery::Definition {
            path: "app/Models/User.php".into(),
            line: 1,
            character: 7,
        })
        .await
        .unwrap();
    let locations: serde_json::Value = serde_json::from_slice(&reply).unwrap();
    let paths: Vec<&str> = locations
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["path"].as_str().unwrap())
        .collect();
    assert_eq!(paths, vec!["app/Models/User.php"]);
    // Hidden files never reach the servers.
    assert!(g
        .session
        .lsp(LspQuery::Hover {
            path: ".env".into(),
            line: 0,
            character: 0
        })
        .await
        .is_err());
    assert_eq!(w.lsp.seen.lock().unwrap().len(), 3);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn diagnostics_reach_only_guests_with_the_file_open() {
    let mut w = World::new(READ);
    let g = w.join("Alex").await;
    let other = w.join("Bea").await;
    let got: Arc<Mutex<Vec<Diagnostics>>> = Arc::default();
    let sink = got.clone();
    g.session.set_diagnostics_sink(Some(Arc::new(move |p, d| {
        sink.lock().unwrap().push((p, d))
    })));
    let other_got: Arc<Mutex<Vec<String>>> = Arc::default();
    let sink = other_got.clone();
    other
        .session
        .set_diagnostics_sink(Some(Arc::new(move |p, _| sink.lock().unwrap().push(p))));
    g.session
        .lsp(LspQuery::Sync {
            path: "app/Models/User.php".into(),
        })
        .await
        .unwrap();
    w.host
        .publish_diagnostics("app/Models/User.php", b"[]".to_vec());
    w.host.publish_diagnostics(".env", b"[]".to_vec());
    for _ in 0..200 {
        if !got.lock().unwrap().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    tokio::time::sleep(Duration::from_millis(50)).await;
    let got = got.lock().unwrap();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].0, "app/Models/User.php");
    assert!(other_got.lock().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn frameworks_never_run_project_code_for_guests_and_do_not_leak_env_values() {
    let mut w = World::new(FULL);
    // A Laravel-looking project whose `artisan` is a script that would leave a trace.
    let marker = w._tmp.path().join("artisan-ran");
    std::fs::write(
        w.root.join("artisan"),
        format!("#!/bin/sh\ntouch {}\n", marker.display()),
    )
    .unwrap();
    std::fs::create_dir_all(w.root.join("vendor")).unwrap();
    std::fs::write(w.root.join("vendor/autoload.php"), "<?php\n").unwrap();
    std::fs::write(
        w.root.join("composer.json"),
        r#"{"require":{"laravel/framework":"^11.0"}}"#,
    )
    .unwrap();
    std::fs::create_dir_all(w.root.join("routes")).unwrap();
    std::fs::write(
        w.root.join("routes/web.php"),
        "<?php\nRoute::get('/', fn () => 1)->name('home');\n",
    )
    .unwrap();
    let evil = w._tmp.path().join("evil.sh");
    std::fs::write(&evil, format!("#!/bin/sh\ntouch {}\n", marker.display())).unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&evil, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let g = w.join("Mallory").await;
    let evil_path = evil.to_string_lossy().into_owned();
    let routes: forge_backend::frameworks::LaravelRoutes = g
        .on(move |b| {
            framework_call(
                &*b,
                FrameworkCall::LaravelRoutes {
                    source: "artisan".into(),
                    php: evil_path,
                },
            )
        })
        .await
        .unwrap();
    assert_eq!(routes.source, "static");
    let evil_path = evil.to_string_lossy().into_owned();
    let cmds: forge_backend::frameworks::ArtisanCommands = g
        .on(move |b| {
            framework_call(
                &*b,
                FrameworkCall::LaravelArtisanCommands { php: evil_path },
            )
        })
        .await
        .unwrap();
    assert!(!cmds.from_project, "the built-in list, nothing was run");
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(!marker.exists(), "the host ran a program a guest named");

    // env() completion keeps the keys and drops the values from `.env`.
    let src = "<?php\n$x = env('DB_';\n";
    let offset = src.find("DB_").unwrap() + 3;
    let reply = g
        .on(move |b| {
            b.framework(FrameworkCall::LaravelComplete {
                path: "app/Http/Order.php".into(),
                offset,
                content: Some(src.into()),
            })
        })
        .await
        .unwrap();
    let text = String::from_utf8_lossy(&reply).into_owned();
    assert!(!text.contains("hunter2"), "{text}");
    assert!(!text.contains("topsecret"), "{text}");
    // Hostile framework settings are refused.
    let r = g
        .call(HostCall::Framework(FrameworkCall::Symfony {
            config: phpforge_fw_symfony_config("../../etc", "bin/console"),
            query: forge_backend::frameworks::SymfonyQuery::Routes,
        }))
        .await;
    assert!(r.is_err());
}

fn phpforge_fw_symfony_config(app: &str, console: &str) -> phpforge_fw_symfony::Config {
    phpforge_fw_symfony::Config {
        app_directory: app.into(),
        console_path: console.into(),
        ..phpforge_fw_symfony::Config::default()
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn terminals_processes_and_servers_are_not_available_to_guests() {
    let mut w = World::new(FULL);
    let g = w.join("Alex").await;
    let r = g
        .on(|b| {
            b.open_terminal(
                forge_backend::TerminalSpec {
                    cwd: "/shop".into(),
                    cols: 80,
                    rows: 24,
                    ..Default::default()
                },
                Arc::new(|_| {}),
            )
            .map(|_| ())
        })
        .await;
    assert!(r.is_err());
    let r = g
        .on(|b| {
            b.start_process(&forge_process::CommandLine::new("sh"), Arc::new(|_| {}))
                .map(|_| ())
        })
        .await;
    assert!(r.is_err());
    assert!(g
        .on(|b| b.find_executable(&["php".into()], None))
        .await
        .unwrap()
        .is_none());
    assert!(g.on(|b| b.tunnels()).await.is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_guest_opens_any_visible_file_as_a_shared_document() {
    let mut w = World::new(EDIT);
    let g = w.join("Alex").await;
    let o = g.session.open_doc("app/Http/Order.php").await.unwrap();
    assert!(!o.read_only);
    for hidden in [
        ".env",
        ".git/config",
        "leak.txt",
        "link_out/secret.txt",
        "envlink",
        "gitlink/HEAD",
        "../x",
    ] {
        let r = g.session.open_doc(hidden).await;
        assert!(r.is_err(), "{hidden}: {r:?}");
    }
    // Over the co-editing limit (the test host's is 64 KB): read-only for everyone.
    std::fs::write(w.root.join("big.txt"), "x".repeat(100 * 1024)).unwrap();
    let big = g.session.open_doc("big.txt").await.unwrap();
    assert!(big.read_only);
    assert_eq!(big.view, Some(ViewOnly::Large { size: 100 * 1024 }));
    let ro = {
        let mut w2 = World::new(READ);
        let g2 = w2.join("Bea").await;
        g2.session
            .open_doc("app/Http/Order.php")
            .await
            .unwrap()
            .read_only
    };
    assert!(ro);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn project_settings_are_read_from_the_host() {
    let mut w = World::new(READ);
    std::fs::create_dir_all(w.root.join(".forge")).unwrap();
    std::fs::write(
        w.root.join(".forge/settings.toml"),
        "[editor]\ntabSize = 2\n",
    )
    .unwrap();
    std::fs::write(w.root.join(".forge/settings.local.toml"), "secret = 1\n").unwrap();
    let g = w.join("Alex").await;
    let files = forge_backend::BackendFiles(g.backend.clone());
    let s = {
        use forge_core::ProjectFiles;
        let f = files;
        tokio::task::spawn_blocking(move || {
            (
                f.read(".forge/settings.toml").unwrap(),
                f.read(".forge/settings.local.toml").unwrap(),
            )
        })
        .await
        .unwrap()
    };
    assert!(s.0.unwrap().contains("tabSize"));
    assert_eq!(s.1, None);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn shared_documents_of_protected_files_are_read_only_without_full_access() {
    let mut w = World::new(EDIT);
    let g = w.join("Alex").await;
    let order = g.session.open_doc("app/Http/Order.php").await.unwrap();
    assert!(!order.read_only);
    assert!(g.session.save(order.doc).await.is_ok());
    let composer = g.session.open_doc("composer.json").await.unwrap();
    assert!(composer.read_only, "composer.json configures what runs");
    let err = g.session.save(composer.doc).await.unwrap_err();
    assert!(err.contains("Full access"), "{err}");
    // An update to it is dropped before it reaches the document.
    use yrs::updates::decoder::Decode;
    use yrs::{GetString, ReadTxn, Text, Transact};
    let replica = yrs::Doc::with_options(yrs::Options {
        offset_kind: yrs::OffsetKind::Utf16,
        ..yrs::Options::default()
    });
    let text = replica.get_or_insert_text("content");
    replica
        .transact_mut()
        .apply_update(yrs::Update::decode_v1(&composer.update).unwrap())
        .unwrap();
    let before = replica.transact().state_vector();
    text.insert(&mut replica.transact_mut(), 0, "PWNED");
    let update = replica.transact().encode_state_as_update_v1(&before);
    g.session.update(composer.doc, update);
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(text.get_string(&replica.transact()).starts_with("PWNED"));
    let other = w.join("Bea").await;
    let seen = other.session.open_doc("composer.json").await.unwrap();
    let replica2 = yrs::Doc::with_options(yrs::Options {
        offset_kind: yrs::OffsetKind::Utf16,
        ..yrs::Options::default()
    });
    replica2
        .transact_mut()
        .apply_update(yrs::Update::decode_v1(&seen.update).unwrap())
        .unwrap();
    assert!(!replica2
        .get_or_insert_text("content")
        .get_string(&replica2.transact())
        .contains("PWNED"));
    assert_eq!(
        std::fs::read_to_string(w.root.join("composer.json")).unwrap(),
        "{\n  \"name\": \"acme/shop\"\n}\n"
    );

    // With Full access the same file is editable.
    let mut w = World::new(FULL);
    let g = w.join("Sam").await;
    let composer = g.session.open_doc("composer.json").await.unwrap();
    assert!(!composer.read_only);
    assert!(g.session.save(composer.doc).await.is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn large_and_binary_files_open_read_only_for_guests() {
    let mut w = World::new(FULL);
    let g = w.join("Alex").await;

    // Text over the co-editing limit: the guest reads it, nobody edits or saves it.
    let big = format!("{}\n", "a".repeat(80 * 1024));
    std::fs::write(w.root.join("big.log"), &big).unwrap();
    let o = g.session.open_doc("big.log").await.unwrap();
    assert!(o.read_only, "read-only even with Full access");
    assert_eq!(
        o.view,
        Some(ViewOnly::Large {
            size: big.len() as u64
        })
    );
    assert_eq!(text_of(&o.update), big);
    let err = g.session.save(o.doc).await.unwrap_err();
    assert!(err.contains("read-only"), "{err}");
    // The host's own tab of the file stays a normal tab.
    let host = w.host.open_doc("big.log", None).await.unwrap_err();
    assert!(host.contains("size limit"), "{host}");
    // It follows the file on disk.
    let changed = format!("{}\n", "b".repeat(80 * 1024));
    std::fs::write(w.root.join("big.log"), &changed).unwrap();
    w.host
        .on_fs_changed(&FsChanges {
            changed: vec!["big.log".into()],
            ..FsChanges::default()
        })
        .await;
    let other = w.join("Bea").await;
    let seen = other.session.open_doc("big.log").await.unwrap();
    assert_eq!(text_of(&seen.update), changed);

    // Binary: the guest learns its size, not its bytes.
    let png = [0x89u8, b'P', b'N', b'G', 0, 0, 0, 13, 0xff, 0xfe];
    std::fs::write(w.root.join("logo.png"), png).unwrap();
    let o = g.session.open_doc("logo.png").await.unwrap();
    assert!(o.read_only);
    assert_eq!(o.view, Some(ViewOnly::Binary { size: 10 }));
    assert_eq!(text_of(&o.update), "");
    let err = g.session.save(o.doc).await.unwrap_err();
    assert!(err.contains("binary"), "{err}");
    let host = w.host.open_doc("logo.png", None).await.unwrap_err();
    assert!(host.contains("binary"), "{host}");
    // Not UTF-8 without a NUL byte is binary too.
    std::fs::write(w.root.join("latin1.txt"), [b'c', b'a', b'f', 0xe9]).unwrap();
    let o = g.session.open_doc("latin1.txt").await.unwrap();
    assert_eq!(o.view, Some(ViewOnly::Binary { size: 4 }));

    // Over the viewing limit: refused.
    let huge = std::fs::File::create(w.root.join("huge.sql")).unwrap();
    huge.set_len(MAX_VIEW_SIZE as u64 + 1).unwrap();
    assert!(g.session.open_doc("huge.sql").await.is_err());

    // Small text files are still shared for editing.
    let o = g.session.open_doc("app/Http/Order.php").await.unwrap();
    assert!(!o.read_only);
    assert_eq!(o.view, None);
}

/// Some app commands call `Backend` methods directly on a runtime worker; the guest's backend
/// must answer them instead of panicking ("Cannot block the current thread").
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn backend_calls_on_a_runtime_worker_do_not_panic() {
    let mut w = World::new(READ);
    let g = w.join("Alex").await;
    // Directly on the worker, as `inspections_profiles` does.
    let text = g.backend.read_optional("app/Http/Order.php").unwrap();
    assert!(text.unwrap().contains("class Order"));
    // And from a task the runtime spawned.
    let b = g.backend.clone();
    let listed = tokio::spawn(async move { b.list_dir("app").map(|e| e.len()) })
        .await
        .unwrap()
        .unwrap();
    assert!(listed > 0);
}

/// A call over the host's frame limit (e.g. analysis that sends a multi-megabyte editor text)
/// fails at once on the guest; the connection and later calls are unaffected.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_oversized_call_fails_at_once_and_the_session_goes_on() {
    let mut w = World::new(FULL);
    let g = w.join("Alex").await;
    let started = std::time::Instant::now();
    let err = g
        .call(HostCall::WriteFile {
            path: "big.txt".into(),
            contents: "x".repeat(5 << 20),
        })
        .await
        .unwrap_err();
    assert!(err.contains("too large"), "{err}");
    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(!w.root.join("big.txt").exists());
    let text = g.on(|b| b.read_file("app/Http/Order.php")).await.unwrap();
    assert!(text.contains("class Order"));
}
