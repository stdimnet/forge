//! The transfer engine against every server type: a local folder, an in-process FTP server
//! and an in-process SSH server with SFTP. No network beyond 127.0.0.1.

mod common;

use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use forge_deploy::compare::Entry;
use forge_deploy::config::{DeployServer, ExcludedPath, ExcludedSide, Mapping, ServerKind};
use forge_deploy::fs::ServerFs;
use forge_deploy::ftp::{FtpFs, FtpParams};
use forge_deploy::local::LocalFolder;
use forge_deploy::options::{DeployOptions, WarnNewer};
use forge_deploy::remote_host;
use forge_deploy::sftp::SftpFs;
use forge_deploy::transfer::{Deployer, Direction, FileStatus};
use forge_diff::DiffStatus;
use forge_remote::session::ConnectParams;
use forge_remote::AuthMethod;

fn server() -> DeployServer {
    DeployServer {
        id: "shop".into(),
        name: "Shop".into(),
        kind: ServerKind::Local,
        mappings: vec![
            Mapping {
                local: "".into(),
                deployment: "/".into(),
                web: "".into(),
            },
            Mapping {
                local: "public".into(),
                deployment: "/htdocs".into(),
                web: "/".into(),
            },
        ],
        excluded: vec![ExcludedPath {
            side: ExcludedSide::Local,
            path: "storage".into(),
        }],
        ..DeployServer::default()
    }
}

fn paths(p: &[&str]) -> Vec<String> {
    p.iter().map(|s| s.to_string()).collect()
}

fn mtime(path: &Path) -> i64 {
    std::fs::metadata(path)
        .unwrap()
        .modified()
        .unwrap()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

/// The whole feature set against `remote` (whose files are visible at `remote_dir`).
fn exercise(remote: &mut dyn ServerFs, remote_dir: &Path) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    common::project(root);
    common::touch(&root.join("app/Models/User.php"), 1_700_000_000);
    let mut project = LocalFolder::new(root);
    let server = server();
    let options = DeployOptions::default();

    // Upload the project: mappings, excluded paths and names, timestamps.
    let mut seen = Vec::new();
    let summary = Deployer::new(&mut project, remote, &server, &options)
        .on_progress(|r| seen.push(r.local.clone()))
        .transfer(Direction::Upload, &paths(&[""]), false);
    assert_eq!(summary.failed, 0, "{:?}", summary.results);
    assert_eq!(summary.transferred, 6, "{:?}", summary.results);
    assert_eq!(seen.len(), summary.results.len());
    assert!(remote_dir.join("htdocs/index.php").is_file());
    assert!(remote_dir.join("htdocs/css/app.css").is_file());
    assert!(remote_dir.join("app/Models/User.php").is_file());
    assert!(!remote_dir.join("public").exists());
    assert!(!remote_dir.join("storage").exists());
    assert!(!remote_dir.join(".git").exists());
    assert!(!remote_dir.join("app/.DS_Store").exists());
    assert_eq!(
        mtime(&remote_dir.join("app/Models/User.php")),
        1_700_000_000
    );

    // Again: everything is up to date.
    let summary = Deployer::new(&mut project, remote, &server, &options).transfer(
        Direction::Upload,
        &paths(&[""]),
        false,
    );
    assert_eq!(summary.transferred, 0, "{:?}", summary.results);
    assert!(summary
        .results
        .iter()
        .all(|r| r.status == FileStatus::UpToDate));

    // A changed file is uploaded alone; explicit excluded paths are reported.
    std::fs::write(
        root.join("app/Models/User.php"),
        "<?php class User { use HasFactory; }",
    )
    .unwrap();
    common::touch(&root.join("app/Models/User.php"), 1_700_000_100);
    let summary = Deployer::new(&mut project, remote, &server, &options).transfer(
        Direction::Upload,
        &paths(&["app", "storage/logs/laravel.log"]),
        false,
    );
    assert_eq!(summary.transferred, 1, "{:?}", summary.results);
    let user = summary
        .results
        .iter()
        .find(|r| r.local == "app/Models/User.php")
        .unwrap();
    assert_eq!(user.status, FileStatus::Transferred);
    assert!(
        user.remote.ends_with("app/Models/User.php"),
        "{}",
        user.remote
    );
    assert_eq!(summary.results.last().unwrap().status, FileStatus::Excluded);

    // The server's copy is newer and different: not overwritten without asking.
    remote
        .write(
            "app/Models/Order.php",
            b"<?php class Order { /* hotfix */ }",
        )
        .unwrap();
    remote
        .set_modified("app/Models/Order.php", 1_900_000_000)
        .unwrap();
    let summary = Deployer::new(&mut project, remote, &server, &options).transfer(
        Direction::Upload,
        &paths(&["app/Models/Order.php"]),
        false,
    );
    assert_eq!(summary.newer_on_target, vec!["app/Models/Order.php"]);
    let content = DeployOptions {
        warn_newer: WarnNewer::Content,
        ..DeployOptions::default()
    };
    let summary = Deployer::new(&mut project, remote, &server, &content).transfer(
        Direction::Upload,
        &paths(&["app/Models/Order.php"]),
        false,
    );
    assert_eq!(summary.newer_on_target.len(), 1);

    // Sync with Deployed: the hotfix differs, a file exists only on the server.
    remote.write("extra.txt", b"only remote").unwrap();
    let sync = Deployer::new(&mut project, remote, &server, &options)
        .sync_compare("")
        .unwrap();
    let status = |p: &str| {
        sync.compare
            .entries
            .iter()
            .find(|e| e.path == p)
            .map(|e| e.status)
    };
    assert_eq!(status("app/Models/Order.php"), Some(DiffStatus::Different));
    assert_eq!(status("app/Models/User.php"), Some(DiffStatus::Equal));
    assert_eq!(status("extra.txt"), Some(DiffStatus::RightOnly));
    // The deployed public folder is htdocs; it is not compared as part of the root.
    assert_eq!(status("htdocs"), Some(DiffStatus::RightOnly));
    assert_eq!(status("storage"), None);
    assert_eq!(status(".git"), None);
    let public = Deployer::new(&mut project, remote, &server, &options)
        .sync_compare("public")
        .unwrap();
    assert!(public.remote.ends_with("htdocs"), "{}", public.remote);
    assert!(public
        .compare
        .entries
        .iter()
        .all(|e| e.status == DiffStatus::Equal));

    // Download takes the hotfix (overwriting the older local file).
    let summary = Deployer::new(&mut project, remote, &server, &options).transfer(
        Direction::Download,
        &paths(&["app"]),
        false,
    );
    assert_eq!(summary.failed, 0, "{:?}", summary.results);
    assert_eq!(
        std::fs::read_to_string(root.join("app/Models/Order.php")).unwrap(),
        "<?php class Order { /* hotfix */ }"
    );
    assert_eq!(mtime(&root.join("app/Models/Order.php")), 1_900_000_000);

    // Compare with Deployed Version reads one file.
    let (bytes, display) = Deployer::new(&mut project, remote, &server, &options)
        .read_remote("public/index.php")
        .unwrap();
    assert_eq!(bytes.unwrap(), b"<?php require '../vendor/autoload.php';");
    assert!(display.ends_with("htdocs/index.php"), "{display}");
    let (missing, _) = Deployer::new(&mut project, remote, &server, &options)
        .read_remote("public/new.php")
        .unwrap();
    assert!(missing.is_none());
    assert!(Deployer::new(&mut project, remote, &server, &options)
        .read_remote("storage/logs/laravel.log")
        .is_err());

    // "Delete target items when source ones do not exist".
    remote.write("app/Stale.php", b"<?php").unwrap();
    let delete = DeployOptions {
        delete_target_items: true,
        ..DeployOptions::default()
    };
    let summary = Deployer::new(&mut project, remote, &server, &delete).transfer(
        Direction::Upload,
        &paths(&["app"]),
        false,
    );
    assert_eq!(summary.deleted, 1, "{:?}", summary.results);
    assert!(!remote_dir.join("app/Stale.php").exists());
    assert!(remote_dir.join("app/Models/User.php").exists());

    // Delete on the server; the root path itself is refused.
    let summary = Deployer::new(&mut project, remote, &server, &options)
        .delete_remote(&paths(&["public/css", ""]));
    assert_eq!(summary.deleted, 1);
    assert_eq!(summary.failed, 1);
    assert!(!remote_dir.join("htdocs/css").exists());
    assert!(remote_dir.join("htdocs/index.php").exists());

    // Cancelling stops before the next file.
    let cancel = AtomicBool::new(true);
    let summary = Deployer::new(&mut project, remote, &server, &options)
        .cancel(&cancel)
        .transfer(Direction::Upload, &paths(&["composer.json"]), true);
    assert!(summary.cancelled);
    assert!(summary.results.is_empty());

    remote_host_operations(remote, remote_dir, &server);
}

/// The Remote Host tool window's tree and context menu on `remote`.
fn remote_host_operations(remote: &mut dyn ServerFs, remote_dir: &Path, server: &DeployServer) {
    // The tree: folders first, mappings and the project paths they lead to.
    let top = remote_host::list(remote, server, "").unwrap();
    let names: Vec<&str> = top.iter().map(|n| n.name.as_str()).collect();
    assert_eq!(&names[..2], ["app", "htdocs"], "{names:?}");
    let htdocs = top.iter().find(|n| n.name == "htdocs").unwrap();
    assert!(htdocs.dir && htdocs.mapping_root);
    assert_eq!(htdocs.local.as_deref(), Some("public"));
    let inner = remote_host::list(remote, server, "htdocs").unwrap();
    let index = inner.iter().find(|n| n.name == "index.php").unwrap();
    assert_eq!(index.local.as_deref(), Some("public/index.php"));
    assert!(index.size > 0);

    // New Directory, New File, Rename, Delete.
    remote_host::create(remote, "htdocs", "assets", true).unwrap();
    remote_host::create(remote, "htdocs/assets", "site.css", false).unwrap();
    assert!(remote_dir.join("htdocs/assets/site.css").is_file());
    assert!(remote_host::create(remote, "htdocs", "assets", true).is_err());
    let moved = remote_host::rename(remote, "htdocs/assets/site.css", "main.css").unwrap();
    assert_eq!(moved, "htdocs/assets/main.css");
    assert!(remote_dir.join("htdocs/assets/main.css").is_file());
    assert!(!remote_dir.join("htdocs/assets/site.css").exists());
    // Refresh sees changes made by others.
    std::fs::write(remote_dir.join("htdocs/robots.txt"), "User-agent: *").unwrap();
    let inner = remote_host::list(remote, server, "htdocs").unwrap();
    assert!(inner.iter().any(|n| n.name == "robots.txt"), "{inner:?}");

    // Edit Remote File: the temporary copy uploads back; a changed server copy conflicts.
    let cache = tempfile::tempdir().unwrap();
    let mut edited =
        remote_host::start_edit(remote, &server.id, "htdocs/robots.txt", cache.path()).unwrap();
    assert!(edited.temp.starts_with(cache.path()));
    assert_eq!(
        std::fs::read_to_string(&edited.temp).unwrap(),
        "User-agent: *"
    );
    let saved = remote_host::save_edit(remote, &mut edited, b"Disallow: /admin", false).unwrap();
    assert_eq!(saved, remote_host::SaveOutcome::Uploaded);
    assert_eq!(
        std::fs::read_to_string(remote_dir.join("htdocs/robots.txt")).unwrap(),
        "Disallow: /admin"
    );
    std::fs::write(
        remote_dir.join("htdocs/robots.txt"),
        "changed by a colleague",
    )
    .unwrap();
    remote.invalidate();
    let saved = remote_host::save_edit(remote, &mut edited, b"mine", false).unwrap();
    assert_eq!(saved, remote_host::SaveOutcome::Conflict { deleted: false });
    remote_host::save_edit(remote, &mut edited, b"mine", true).unwrap();
    assert_eq!(
        remote_host::read_text(remote, "htdocs/robots.txt")
            .unwrap()
            .text
            .as_deref(),
        Some("mine")
    );

    let deleted =
        remote_host::delete(remote, &paths(&["htdocs/assets", "htdocs/robots.txt"])).unwrap();
    assert_eq!(deleted, 2);
    assert!(!remote_dir.join("htdocs/assets").exists());
    assert!(remote_host::delete(remote, &paths(&[""])).is_err());
}

#[test]
fn local_folder_server() {
    let target = tempfile::tempdir().unwrap();
    let mut fs = LocalFolder::new(target.path());
    exercise(&mut fs, target.path());
}

#[test]
fn local_servers_are_opened_through_connect() {
    let project = tempfile::tempdir().unwrap();
    common::project(project.path());
    std::fs::create_dir_all(project.path().join("dist")).unwrap();
    let server = DeployServer {
        name: "dist".into(),
        kind: ServerKind::Local,
        root_path: "dist".into(),
        ..DeployServer::default()
    };
    let mut opened = forge_deploy::connect::open(&server, None, Some(project.path())).unwrap();
    assert_eq!(opened.inside_project.as_deref(), Some("dist"));
    let mut local = LocalFolder::new(project.path());
    let summary = Deployer::new(
        &mut local,
        opened.fs.as_mut(),
        &server,
        &DeployOptions::default(),
    )
    .skip_local(opened.inside_project.clone())
    .transfer(Direction::Upload, &paths(&[""]), false);
    assert_eq!(summary.failed, 0, "{:?}", summary.results);
    assert!(project.path().join("dist/composer.json").exists());
    assert!(!project.path().join("dist/dist").exists());

    let test = forge_deploy::connect::test(&server, None, Some(project.path()));
    assert!(test.ok, "{}", test.message);
    let itself = DeployServer {
        root_path: ".".into(),
        ..server.clone()
    };
    assert!(forge_deploy::connect::open(&itself, None, Some(project.path())).is_err());
    let missing = DeployServer {
        root_path: "nope".into(),
        ..server
    };
    let test = forge_deploy::connect::test(&missing, None, Some(project.path()));
    assert!(!test.ok);
    assert!(test.message.contains("does not exist"), "{}", test.message);
}

fn ftp_params(port: u16, password: &str, root: &str) -> FtpParams {
    FtpParams {
        host: "127.0.0.1".into(),
        port,
        user: Some("deploy".into()),
        password: password.into(),
        secure: false,
        passive: true,
        root: root.into(),
        timeout: Duration::from_secs(10),
    }
}

#[test]
fn ftp_server() {
    let target = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(target.path().join("www")).unwrap();
    let mock = common::ftp::start(target.path(), common::ftp::FtpOptions::default());
    let mut fs = FtpFs::connect(&ftp_params(mock.port, "secret", "/www")).unwrap();
    assert_eq!(fs.welcome(), Some("220 Forge test FTP"));
    assert_eq!(fs.root(), "/www");
    assert!(
        fs.describe("a.txt").ends_with("/www/a.txt"),
        "{}",
        fs.describe("a.txt")
    );
    exercise(&mut fs, &mock.root.join("www"));
}

#[test]
fn ftp_without_mlsd_and_mfmt_uses_list() {
    let target = tempfile::tempdir().unwrap();
    let options = common::ftp::FtpOptions {
        mlsd: false,
        mfmt: false,
        ..Default::default()
    };
    let mock = common::ftp::start(target.path(), options);
    let mut fs = FtpFs::connect(&ftp_params(mock.port, "secret", "")).unwrap();
    assert_eq!(fs.root(), "/");
    fs.mkdir_all("a/b").unwrap();
    fs.write("a/b/c.txt", b"hello").unwrap();
    assert!(!fs.set_modified("a/b/c.txt", 1).unwrap());
    let listed = fs.list("a/b").unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!((listed[0].path.as_str(), listed[0].size), ("a/b/c.txt", 5));
    assert_eq!(fs.stat("a").unwrap().map(|e| e.dir), Some(true));
    assert_eq!(fs.stat("missing/x").unwrap(), None);
    assert_eq!(fs.read("a/b/c.txt").unwrap(), b"hello");
    fs.remove_all("a").unwrap();
    assert!(!target.path().join("a").exists());
    assert_eq!(mock.stores.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[test]
fn ftp_login_errors_are_reported() {
    let target = tempfile::tempdir().unwrap();
    let mock = common::ftp::start(target.path(), Default::default());
    let err = FtpFs::connect(&ftp_params(mock.port, "wrong", ""))
        .err()
        .unwrap();
    assert!(err.to_string().contains("Login to 127.0.0.1"), "{err}");
    assert!(err.to_string().contains("Login incorrect"), "{err}");
    // FTPS needs AUTH TLS, which this server refuses.
    let mut secure = ftp_params(mock.port, "secret", "");
    secure.secure = true;
    let err = FtpFs::connect(&secure).err().unwrap();
    assert!(err.to_string().contains("TLS"), "{err}");
    // Nothing listens on the port anymore.
    let port = forge_ports::free_port().unwrap();
    assert!(FtpFs::connect(&ftp_params(port, "secret", "")).is_err());
}

fn ssh_params(
    port: u16,
    known_hosts: &Path,
    accept: Option<String>,
    password: &str,
) -> ConnectParams {
    ConnectParams {
        host: "127.0.0.1".into(),
        port,
        user: "deploy".into(),
        auth: AuthMethod::Password,
        secret: Some(password.into()),
        known_hosts: known_hosts.to_path_buf(),
        accept_fingerprint: accept,
        timeout: Duration::from_secs(10),
        ..ConnectParams::default()
    }
}

#[test]
fn sftp_server() {
    let target = tempfile::tempdir().unwrap();
    let keys = tempfile::tempdir().unwrap();
    let known_hosts = keys.path().join("known_hosts");
    std::fs::create_dir_all(target.path().join("var/www")).unwrap();
    let mock = common::sftp::start(target.path());

    // The unknown host key must be trusted first.
    let err = SftpFs::connect(
        &ssh_params(mock.port, &known_hosts, None, "secret"),
        "/var/www",
    )
    .err()
    .unwrap();
    let message = err.to_string();
    assert!(message.contains("not trusted yet"), "{message}");
    let fingerprint = message
        .split_whitespace()
        .find(|w| w.starts_with("SHA256:"))
        .unwrap()
        .trim_end_matches(").")
        .trim_end_matches(')')
        .to_string();
    let err = SftpFs::connect(
        &ssh_params(mock.port, &known_hosts, Some(fingerprint.clone()), "wrong"),
        "/var/www",
    )
    .err()
    .unwrap();
    assert!(!err.to_string().contains("not trusted"), "{err}");

    let mut fs = SftpFs::connect(
        &ssh_params(mock.port, &known_hosts, None, "secret"),
        "/var/www",
    )
    .unwrap();
    assert_eq!(fs.root(), "/var/www");
    assert_eq!(fs.stat("").unwrap().map(|e| e.dir), Some(true));
    exercise(&mut fs, &target.path().join("var/www"));
    drop(fs);

    // An empty root path is the login folder.
    let mut home =
        SftpFs::connect(&ssh_params(mock.port, &known_hosts, None, "secret"), "").unwrap();
    assert_eq!(home.root(), "/");
    let names: Vec<Entry> = home.list("").unwrap();
    assert_eq!(names.len(), 1);
    assert_eq!(names[0].path, "var");
}
