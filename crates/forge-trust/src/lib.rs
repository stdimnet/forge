//! Project trust, like PHPStorm's "Trust and Open Project?" and Safe Mode.
//!
//! A project is trusted when the user trusted it, when it lies in a trusted location, or when
//! Forge created it. An untrusted project opens in **Safe Mode**: Forge shows and edits its
//! files but runs none of its code (scripts, run configurations, tests, the debugger, build
//! tools, project-local language servers, external tools on save). The app keeps a
//! [`SafeMode`] flag for the open project and every command that would start a project
//! program asks it first ([`SafeMode::check`]).
//!
//! The decisions live in the user config dir (`<config dir>/forge/trusted-projects.toml`),
//! never in the project and never in the settings (a project's `.forge/settings.toml` could
//! otherwise trust itself).
//!
//! Projects are identified by a key: a local absolute path, or `ssh://user@host:port/path`
//! for a project on a remote host ([`remote_key`]). Locations use the same form and cover
//! every project below them on the same machine.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use forge_core::{ForgeError, Result};
use serde::{Deserialize, Serialize};

/// File name of the trust store in `<config dir>/forge`.
pub const STORE_FILE: &str = "trusted-projects.toml";

/// Prefix of remote project keys.
const REMOTE: &str = "ssh://";

/// Catalog language servers that run project code even when the server itself is installed
/// outside the project: ESLint and Tailwind CSS load JavaScript config files, rust-analyzer
/// runs build scripts and proc macros, and the Ruby, Java, Kotlin and C# servers run the
/// project's Bundler, Gradle, Maven or MSBuild files.
pub const SERVERS_RUNNING_PROJECT_CODE: &[&str] = &[
    "vscode-eslint-language-server",
    "tailwindcss-language-server",
    "rust-analyzer",
    "ruby-lsp",
    "jdtls",
    "kotlin-language-server",
    "kotlin-lsp",
    "metals",
    "csharp-ls",
    "roslyn",
    "fsautocomplete",
];

/// Why a project is trusted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TrustReason {
    /// The project itself was trusted (or created by Forge).
    Project,
    /// The project lies in this trusted location (a key, see [`display`]).
    Location { location: String },
}

/// A project's trust state, for the "Trust and Open Project?" dialog and the Safe Mode banner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrustCheck {
    /// The project's key.
    pub project: String,
    /// The project as the UI shows it (`user@host:/path` for remote projects).
    pub display: String,
    pub trusted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<TrustReason>,
    /// The folder that contains the project, for "Trust projects in …" (`None` when the
    /// parent is a file system root, which cannot be trusted as a whole).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// [`TrustCheck::parent`] as the UI shows it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_display: Option<String>,
}

/// Trusted projects and trusted locations, stored in the user config dir.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustStore {
    /// Trusted project keys.
    #[serde(default)]
    pub projects: Vec<String>,
    /// Trusted location keys: every project below one is trusted.
    #[serde(default)]
    pub locations: Vec<String>,
}

impl TrustStore {
    /// `<config dir>/forge/trusted-projects.toml`.
    pub fn default_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("forge").join(STORE_FILE))
    }

    /// Read the store; a missing file is an empty store.
    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text)
                .map_err(|e| ForgeError::Other(format!("{}: {e}", path.display()))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(ForgeError::io(path, e)),
        }
    }

    /// Write the store, creating its folder.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| ForgeError::io(dir, e))?;
        }
        let text = toml::to_string_pretty(self).map_err(|e| ForgeError::Other(e.to_string()))?;
        std::fs::write(path, text).map_err(|e| ForgeError::io(path, e))
    }

    /// Why `project` is trusted, or `None` when it is not.
    pub fn reason(&self, project: &str) -> Option<TrustReason> {
        let key = normalize(project);
        if self.projects.iter().any(|p| same(p, &key)) {
            return Some(TrustReason::Project);
        }
        self.locations
            .iter()
            .find(|l| contains(l, &key))
            .map(|l| TrustReason::Location {
                location: l.clone(),
            })
    }

    pub fn is_trusted(&self, project: &str) -> bool {
        self.reason(project).is_some()
    }

    /// The dialog's and banner's view of `project`.
    pub fn check(&self, project: &str) -> TrustCheck {
        let key = normalize(project);
        let reason = self.reason(&key);
        let parent = parent_location(&key);
        TrustCheck {
            display: display(&key),
            trusted: reason.is_some(),
            reason,
            parent_display: parent.as_deref().map(display),
            parent,
            project: key,
        }
    }

    /// Trust `project`. `false` when it already was trusted by itself.
    pub fn trust(&mut self, project: &str) -> Result<bool> {
        let key = absolute_key(project)?;
        if self.projects.iter().any(|p| same(p, &key)) {
            return Ok(false);
        }
        self.projects.push(key);
        self.projects.sort();
        Ok(true)
    }

    /// Forget that `project` was trusted by itself (a trusted location still covers it).
    pub fn distrust(&mut self, project: &str) -> bool {
        let key = normalize(project);
        let before = self.projects.len();
        self.projects.retain(|p| !same(p, &key));
        before != self.projects.len()
    }

    /// Trust every project in `location` (`~` is the home folder). Returns the stored key.
    pub fn add_location(&mut self, location: &str) -> Result<String> {
        let key = absolute_key(&expand_home(location.trim()))?;
        if is_root(&key) {
            return Err(ForgeError::Other(format!(
                "{} is a file system root; choose a folder",
                display(&key)
            )));
        }
        if !self.locations.iter().any(|l| same(l, &key)) {
            self.locations.push(key.clone());
            self.locations.sort();
        }
        Ok(key)
    }

    /// Stop trusting `location`. `false` when it was not in the list.
    pub fn remove_location(&mut self, location: &str) -> bool {
        let key = normalize(&expand_home(location.trim()));
        let before = self.locations.len();
        self.locations.retain(|l| !same(l, &key));
        before != self.locations.len()
    }
}

/// The open project's Safe Mode flag, shared by every command that may start project
/// programs. Off by default (no project, or a trusted one).
#[derive(Debug, Clone, Default)]
pub struct SafeMode(Arc<AtomicBool>);

impl SafeMode {
    pub fn is_on(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }

    pub fn set(&self, on: bool) {
        self.0.store(on, Ordering::SeqCst);
    }

    /// `Err` with [`refusal`] when Safe Mode is on. `what` names the refused action
    /// ("Running run configurations").
    pub fn check(&self, what: &str) -> Result<()> {
        if self.is_on() {
            Err(ForgeError::Other(refusal(what)))
        } else {
            Ok(())
        }
    }
}

/// The error text of a command refused in Safe Mode, in the IDE language (`what` is the
/// English action, translated through `errors.safeMode.action.<slug>` when it can be).
pub fn refusal(what: &str) -> String {
    let what = forge_i18n::t_or(
        &format!("errors.safeMode.action.{}", forge_i18n::slug(what)),
        what,
    );
    forge_i18n::t_args(
        "errors.safeMode.refused",
        &forge_i18n::args! { "what" => what },
    )
}

/// Where the target of an SSH terminal session (Tools | Start SSH Session…) comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SshTarget {
    /// One of the user's own SSH configurations (Settings | Tools | SSH Configurations).
    UserConfiguration,
    /// A deployment server, defined in the project's `.forge/deployment.toml`.
    ProjectServer,
}

/// Whether starting an SSH session needs the user's confirmation first. Browsing a server
/// and transferring files only copy files, but a session logs the user in on a host with
/// their own credentials: in Safe Mode, a host chosen by an untrusted project's files is
/// connected to only after the user confirmed it. The user's own configurations never ask.
pub fn ssh_session_needs_confirmation(safe_mode: bool, target: SshTarget) -> bool {
    safe_mode && target == SshTarget::ProjectServer
}

/// The error of an unconfirmed SSH session to a project's server in Safe Mode.
pub fn ssh_confirmation_required(server: &str) -> String {
    format!(
        "The deployment server {server} is defined in the files of an untrusted project. \
         Confirm the SSH session to it first, or trust the project."
    )
}

/// The key of a project folder on a remote host.
pub fn remote_key(user: &str, host: &str, port: u16, path: &str) -> String {
    let path = if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    };
    normalize(&format!("{REMOTE}{user}@{host}:{port}{path}"))
}

/// The key of a local project folder: its canonical absolute path (symlinks resolved, without
/// Windows' `\\?\` prefix), or the path itself when it cannot be resolved.
pub fn local_key(path: &Path) -> String {
    let resolved = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let text = resolved.to_string_lossy();
    let text = match text.strip_prefix(r"\\?\") {
        Some(rest) if rest.starts_with("UNC\\") => format!(r"\\{}", &rest[4..]),
        Some(rest) => rest.to_string(),
        None => text.into_owned(),
    };
    normalize(&text)
}

/// A key as the UI shows it: local paths unchanged, remote ones as `user@host:/path`
/// (`user@host:2222:/path` for another port than 22).
pub fn display(key: &str) -> String {
    match split(key) {
        (Some(authority), path) => {
            let authority = authority.strip_suffix(":22").unwrap_or(authority);
            format!("{authority}:{path}")
        }
        (None, path) => path.to_string(),
    }
}

/// A clean key: `/` separators, no `.`, `..` or repeated or trailing separators. Paths are
/// not resolved on disk (remote ones cannot be); the app canonicalizes local paths first.
pub fn normalize(key: &str) -> String {
    let key = key.trim();
    let (authority, path) = split(key);
    let path = path.replace('\\', "/");
    let (prefix, rest) = if let Some(rest) = path.strip_prefix("//") {
        ("//".to_string(), rest.to_string())
    } else if let Some((drive, rest)) = drive(&path) {
        (
            format!("{}:/", drive.to_ascii_uppercase()),
            rest.to_string(),
        )
    } else if let Some(rest) = path.strip_prefix('/') {
        ("/".to_string(), rest.to_string())
    } else {
        (String::new(), path.clone())
    };
    let mut parts: Vec<&str> = Vec::new();
    for part in rest.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if parts.last().is_some_and(|p| *p != "..") {
                    parts.pop();
                } else if prefix.is_empty() {
                    parts.push("..");
                }
            }
            p => parts.push(p),
        }
    }
    let path = format!("{prefix}{}", parts.join("/"));
    let path = if path.is_empty() { ".".into() } else { path };
    match authority {
        Some(a) => format!("{REMOTE}{a}{path}"),
        None => path,
    }
}

/// Whether `project` is `location` or lies below it, on the same machine.
pub fn contains(location: &str, project: &str) -> bool {
    let (la, lp) = split_owned(&normalize(location));
    let (pa, pp) = split_owned(&normalize(project));
    if la != pa || !is_absolute(&lp) {
        return false;
    }
    let fold = la.is_none() && cfg!(windows);
    let (lp, pp) = if fold {
        (lp.to_lowercase(), pp.to_lowercase())
    } else {
        (lp, pp)
    };
    if lp == pp {
        return true;
    }
    let dir = if lp.ends_with('/') {
        lp
    } else {
        format!("{lp}/")
    };
    pp.starts_with(&dir)
}

/// The folder that contains `project`, for "Trust projects in …"; `None` when that is a file
/// system root (or `project` has no parent).
pub fn parent_location(project: &str) -> Option<String> {
    let key = normalize(project);
    let (authority, path) = split_owned(&key);
    if !is_absolute(&path) || is_root(&key) {
        return None;
    }
    let cut = path.rfind('/')?;
    let parent = if cut == 0 || path[..cut].ends_with(':') {
        path[..=cut].to_string()
    } else {
        path[..cut].to_string()
    };
    let parent = match authority {
        Some(a) => format!("{REMOTE}{a}{parent}"),
        None => parent,
    };
    (!is_root(&parent)).then_some(parent)
}

/// Whether a server or tool started as `program args…` for the project in `root` runs a
/// program from the project: the program itself lies in the project (`node_modules/.bin`,
/// `vendor/bin`, `./tool`), or an argument names a script or file inside it
/// (`node /project/node_modules/x/server.js`, also as an option value:
/// `--require=/project/x.js`). Paths are compared with [`forge_core::paths::is_within`]:
/// as written (verbatim prefixes, separators, `..`, letter case where the file system
/// ignores it) and as this machine resolves them (symlinks, `/proc/self/root/…`). Remote
/// roots are compared as written.
pub fn runs_project_program(root: &Path, program: &str, args: &[String]) -> bool {
    let inside = |p: &str| {
        let p = p.trim();
        let p = if p.starts_with('-') {
            match p.split_once('=') {
                Some((_, value)) => value.trim(),
                None => return false,
            }
        } else {
            p
        };
        if p.is_empty() || !(p.contains('/') || p.contains('\\')) {
            return false;
        }
        let path = if is_absolute(&p.replace('\\', "/")) {
            PathBuf::from(p)
        } else {
            root.join(p)
        };
        forge_core::paths::is_within(root, &path)
    };
    inside(program) || args.iter().any(|a| inside(a))
}

/// Whether the catalog server `id` runs project code (see [`SERVERS_RUNNING_PROJECT_CODE`]).
pub fn server_runs_project_code(id: &str) -> bool {
    SERVERS_RUNNING_PROJECT_CODE.contains(&id)
}

fn absolute_key(path: &str) -> Result<String> {
    let key = normalize(path);
    if is_absolute(split(&key).1) {
        Ok(key)
    } else {
        Err(ForgeError::Other(format!("not an absolute path: {path}")))
    }
}

fn expand_home(path: &str) -> String {
    let rest = path
        .strip_prefix("~/")
        .or_else(|| path.strip_prefix("~\\"))
        .or((path == "~").then_some(""));
    match (rest, dirs::home_dir()) {
        (Some(rest), Some(home)) => home.join(rest).to_string_lossy().into_owned(),
        _ => path.to_string(),
    }
}

/// `(Some("user@host:port"), "/path")` for remote keys, `(None, key)` for local ones.
fn split(key: &str) -> (Option<&str>, &str) {
    match key.strip_prefix(REMOTE) {
        Some(rest) => match rest.find('/') {
            Some(i) => (Some(&rest[..i]), &rest[i..]),
            None => (Some(rest), "/"),
        },
        None => (None, key),
    }
}

fn split_owned(key: &str) -> (Option<String>, String) {
    let (a, p) = split(key);
    (a.map(str::to_string), p.to_string())
}

fn drive(path: &str) -> Option<(char, &str)> {
    let mut chars = path.chars();
    let letter = chars.next().filter(char::is_ascii_alphabetic)?;
    let rest = path.get(1..)?;
    let rest = rest.strip_prefix(":/").or((rest == ":").then_some(""))?;
    Some((letter, rest))
}

fn is_absolute(path: &str) -> bool {
    path.starts_with('/') || drive(path).is_some()
}

fn is_root(key: &str) -> bool {
    let path = split(key).1;
    path == "/" || path == "//" || drive(path).is_some_and(|(_, rest)| rest.is_empty())
}

fn same(a: &str, b: &str) -> bool {
    let (a, b) = (normalize(a), normalize(b));
    if cfg!(windows) && split(&a).0.is_none() && split(&b).0.is_none() {
        a.eq_ignore_ascii_case(&b)
    } else {
        a == b
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssh_sessions_to_project_servers_ask_in_safe_mode() {
        assert!(ssh_session_needs_confirmation(
            true,
            SshTarget::ProjectServer
        ));
        assert!(!ssh_session_needs_confirmation(
            false,
            SshTarget::ProjectServer
        ));
        assert!(!ssh_session_needs_confirmation(
            true,
            SshTarget::UserConfiguration
        ));
        assert!(!ssh_session_needs_confirmation(
            false,
            SshTarget::UserConfiguration
        ));
        assert!(ssh_confirmation_required("shop-production").contains("shop-production"));
    }

    #[test]
    fn normalizes_separators_dots_and_drives() {
        assert_eq!(normalize("/home/me/./shop/"), "/home/me/shop");
        assert_eq!(normalize("/home/me/x/../shop"), "/home/me/shop");
        assert_eq!(normalize("c:\\Users\\me\\shop\\"), "C:/Users/me/shop");
        assert_eq!(normalize("/"), "/");
        assert_eq!(normalize("/.."), "/");
        assert_eq!(
            normalize("ssh://me@box:22/srv/www/../shop/"),
            "ssh://me@box:22/srv/shop"
        );
    }

    #[test]
    fn local_keys_resolve_symlinks_and_dots() {
        let dir = tempfile::tempdir().unwrap();
        let shop = dir.path().join("shop");
        std::fs::create_dir(&shop).unwrap();
        let canonical = local_key(&shop);
        assert_eq!(local_key(&dir.path().join("shop/./../shop")), canonical);
        assert_eq!(
            local_key(Path::new("/no/such/forge/dir/")),
            "/no/such/forge/dir"
        );
    }

    #[test]
    fn remote_keys_and_display() {
        let key = remote_key("me", "box", 22, "/srv/shop");
        assert_eq!(key, "ssh://me@box:22/srv/shop");
        assert_eq!(display(&key), "me@box:/srv/shop");
        assert_eq!(
            display(&remote_key("me", "box", 2222, "srv")),
            "me@box:2222:/srv"
        );
        assert_eq!(display("/home/me/shop"), "/home/me/shop");
    }

    #[test]
    fn locations_contain_projects_below_them_only() {
        assert!(contains("/home/me/work", "/home/me/work/shop"));
        assert!(contains("/home/me/work", "/home/me/work"));
        assert!(contains("/home/me/work/", "/home/me/work/a/b"));
        assert!(!contains("/home/me/work", "/home/me/workshop"));
        assert!(!contains("/home/me/work", "/home/me"));
        assert!(!contains("/home/me/work", "/home/me/work/../other"));
        assert!(!contains("relative", "relative/x"));
    }

    #[test]
    fn remote_locations_only_cover_their_host() {
        let loc = remote_key("me", "box", 22, "/srv");
        assert!(contains(&loc, &remote_key("me", "box", 22, "/srv/shop")));
        assert!(!contains(&loc, &remote_key("me", "other", 22, "/srv/shop")));
        assert!(!contains(&loc, &remote_key("me", "box", 2222, "/srv/shop")));
        assert!(!contains(&loc, "/srv/shop"));
        assert!(!contains("/srv", &remote_key("me", "box", 22, "/srv/shop")));
    }

    #[test]
    fn parent_locations() {
        assert_eq!(
            parent_location("/home/me/shop").as_deref(),
            Some("/home/me")
        );
        assert_eq!(parent_location("/shop"), None);
        assert_eq!(parent_location("/"), None);
        assert_eq!(parent_location("C:/shop"), None);
        assert_eq!(parent_location("C:/work/shop").as_deref(), Some("C:/work"));
        assert_eq!(
            parent_location("ssh://me@box:22/srv/shop").as_deref(),
            Some("ssh://me@box:22/srv")
        );
        assert_eq!(parent_location("ssh://me@box:22/shop"), None);
    }

    #[test]
    fn trust_projects_and_locations() {
        let mut store = TrustStore::default();
        assert!(!store.is_trusted("/home/me/shop"));
        assert!(store.trust("/home/me/shop/").unwrap());
        assert!(!store.trust("/home/me/shop").unwrap());
        assert_eq!(store.reason("/home/me/shop"), Some(TrustReason::Project));
        assert!(!store.is_trusted("/home/me/shop2"));

        let loc = store.add_location("/home/me/work").unwrap();
        assert_eq!(loc, "/home/me/work");
        store.add_location("/home/me/work/").unwrap();
        assert_eq!(store.locations.len(), 1);
        assert_eq!(
            store.reason("/home/me/work/api"),
            Some(TrustReason::Location {
                location: "/home/me/work".into()
            })
        );
        assert!(store.remove_location("/home/me/work/"));
        assert!(!store.remove_location("/home/me/work"));
        assert!(!store.is_trusted("/home/me/work/api"));

        assert!(store.distrust("/home/me/shop"));
        assert!(!store.is_trusted("/home/me/shop"));
    }

    #[test]
    fn rejects_relative_paths_and_roots() {
        let mut store = TrustStore::default();
        assert!(store.trust("shop").is_err());
        assert!(store.add_location("work").is_err());
        assert!(store.add_location("/").is_err());
        assert!(store.add_location("D:\\").is_err());
        assert!(store
            .add_location(&remote_key("me", "box", 22, "/"))
            .is_err());
    }

    #[test]
    fn home_is_expanded_in_locations() {
        let Some(home) = dirs::home_dir() else { return };
        let mut store = TrustStore::default();
        let key = store.add_location("~/Projects").unwrap();
        assert_eq!(key, normalize(&home.join("Projects").to_string_lossy()));
        assert!(store.remove_location("~/Projects"));
    }

    #[test]
    fn check_describes_the_project() {
        let mut store = TrustStore::default();
        let check = store.check("/home/me/shop/");
        assert_eq!(check.project, "/home/me/shop");
        assert!(!check.trusted);
        assert_eq!(check.parent.as_deref(), Some("/home/me"));
        store.add_location("/home/me").unwrap();
        let check = store.check("/home/me/shop");
        assert!(check.trusted);
        assert!(matches!(check.reason, Some(TrustReason::Location { .. })));

        let remote = store.check(&remote_key("me", "box", 22, "/srv/shop"));
        assert!(!remote.trusted);
        assert_eq!(remote.display, "me@box:/srv/shop");
        assert_eq!(remote.parent_display.as_deref(), Some("me@box:/srv"));
    }

    #[test]
    fn store_round_trips_through_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("forge").join(STORE_FILE);
        assert_eq!(TrustStore::load(&path).unwrap(), TrustStore::default());
        let mut store = TrustStore::default();
        store.trust("/home/me/shop").unwrap();
        store.add_location("/home/me/work").unwrap();
        store.save(&path).unwrap();
        assert_eq!(TrustStore::load(&path).unwrap(), store);
        std::fs::write(&path, "projects = 3").unwrap();
        assert!(TrustStore::load(&path).is_err());
    }

    #[test]
    fn safe_mode_refuses_with_a_clear_error() {
        let safe = SafeMode::default();
        assert!(safe.check("Running run configurations").is_ok());
        safe.clone().set(true);
        let err = safe.check("Running run configurations").unwrap_err();
        assert!(err
            .to_string()
            .starts_with("Running run configurations is not available in Safe Mode"));
        safe.set(false);
        assert!(!safe.is_on());
    }

    #[test]
    fn project_programs_are_detected() {
        let root = Path::new("/home/me/shop");
        let none: Vec<String> = Vec::new();
        assert!(runs_project_program(
            root,
            "/home/me/shop/node_modules/.bin/eslint",
            &none
        ));
        assert!(runs_project_program(root, "vendor/bin/phpstan", &none));
        assert!(runs_project_program(root, "./tool", &none));
        assert!(!runs_project_program(
            root,
            "intelephense",
            &["--stdio".into()]
        ));
        assert!(!runs_project_program(root, "/usr/bin/php", &none));
        assert!(!runs_project_program(root, "../elsewhere/tool", &none));
        assert!(runs_project_program(
            root,
            "node",
            &[
                "/home/me/shop/node_modules/x/server.js".into(),
                "--stdio".into()
            ]
        ));
        assert!(!runs_project_program(
            root,
            "node",
            &["/opt/x/server.js".into()]
        ));
        assert!(server_runs_project_code("rust-analyzer"));
    }

    #[test]
    fn project_programs_are_found_in_every_spelling() {
        let none: Vec<String> = Vec::new();
        // Windows: a verbatim path, another letter case, `..` inside a verbatim path.
        let win = Path::new(r"C:\Users\me\proj");
        assert!(runs_project_program(
            win,
            r"\\?\C:\Users\me\proj\evil.exe",
            &none
        ));
        assert!(runs_project_program(
            win,
            r"c:\users\ME\Proj\evil.exe",
            &none
        ));
        assert!(runs_project_program(
            win,
            r"\\?\C:\Users\me\x\..\proj\evil.exe",
            &none
        ));
        assert!(!runs_project_program(
            win,
            r"C:\Program Files\Lua\stylua.exe",
            &none
        ));
        // Paths in option values.
        let root = Path::new("/home/me/shop");
        assert!(runs_project_program(
            root,
            "node",
            &["--require=/home/me/shop/evil.js".into()]
        ));
        assert!(runs_project_program(
            root,
            "php",
            &["-dauto_prepend_file=./evil.php".into()]
        ));
        assert!(!runs_project_program(
            root,
            "node",
            &["--max-old-space-size=4096".into()]
        ));
        assert!(!runs_project_program(
            root,
            "node",
            &["--require=/opt/x.js".into()]
        ));
    }

    #[test]
    fn project_programs_are_found_through_the_file_system() {
        let dir = tempfile::tempdir().unwrap();
        let base = forge_core::paths::canonicalize(dir.path()).unwrap();
        let root = base.join("proj");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("evil"), "").unwrap();
        let none: Vec<String> = Vec::new();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&root, base.join("alias")).unwrap();
            let via_link = base.join("alias/evil");
            assert!(runs_project_program(
                &root,
                &via_link.to_string_lossy(),
                &none
            ));
        }
        #[cfg(target_os = "linux")]
        {
            let via_proc = format!("/proc/self/root{}", root.join("evil").display());
            assert!(runs_project_program(&root, &via_proc, &none));
        }
        #[cfg(any(windows, target_os = "macos"))]
        {
            let upper = root.join("evil").to_string_lossy().to_uppercase();
            assert!(runs_project_program(&root, &upper, &none));
        }
        assert!(!runs_project_program(
            &root,
            &base.join("other/tool").to_string_lossy(),
            &none
        ));
        assert!(!server_runs_project_code("intelephense"));
    }
}
