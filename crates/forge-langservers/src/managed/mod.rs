//! Zero-prerequisite installs of npm language servers (Intelephense, typescript-language-server,
//! vscode-langservers-extracted, yaml-language-server, …) on the user's machine:
//!
//! 1. a Node.js >= 18: the machine's, Forge's earlier download, or the official LTS build
//!    for the platform downloaded from nodejs.org (or a mirror) and verified against
//!    `SHASUMS256.txt` ([`node`]),
//! 2. the packages: straight from the npm registry with `dist.integrity` (sha512) checked
//!    when they have no runtime dependencies ([`npm`]), otherwise with npm (the one bundled
//!    with Forge's Node.js when the machine has none) — always into
//!    `<data dir>/forge/langservers`, never globally,
//! 3. a daily check for newer versions of what Forge installed ([`updates`]).
//!
//! [`stubs`] downloads JetBrains' phpstorm-stubs (pinned, SHA-256 checked) for Go to
//! Declaration on PHP's own symbols.
//!
//! Nothing is bundled with Forge: servers whose licence forbids redistribution
//! (Intelephense) are downloaded by each user, with their consent, from the official source.

pub mod archive;
pub mod http;
pub mod node;
pub mod npm;
pub mod pypi;
pub mod stubs;
pub mod updates;

#[cfg(test)]
pub(crate) mod testserver;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::catalog::InstallOption;
use crate::detect::Detector;
use crate::install::{self, InstallOutcome, Stream};
pub use http::Http;
pub use node::DistTarget;
pub use updates::PackageUpdate;

/// What an install reports while it runs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Event {
    /// A step for the log ("Verified SHA-256 of …").
    Step { message: String },
    /// Download progress in bytes (`total` when the server sent a length).
    Progress {
        label: String,
        done: u64,
        total: Option<u64>,
    },
    /// Output of npm, for packages that need it.
    Output { stream: Stream, line: String },
}

impl Event {
    pub fn step(message: impl Into<String>) -> Self {
        Event::Step {
            message: message.into(),
        }
    }

    pub fn progress(label: &str, done: u64, total: Option<u64>) -> Self {
        Event::Progress {
            label: label.to_string(),
            done,
            total,
        }
    }
}

/// Where downloads come from (settings `langservers.download.npmRegistry` and
/// `langservers.download.nodeMirror`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sources {
    pub registry: String,
    pub node_mirror: String,
}

impl Default for Sources {
    fn default() -> Self {
        Sources {
            registry: npm::DEFAULT_REGISTRY.into(),
            node_mirror: node::DEFAULT_MIRROR.into(),
        }
    }
}

impl Sources {
    /// From settings values; empty ones keep the defaults.
    pub fn new(registry: Option<&str>, node_mirror: Option<&str>) -> Self {
        let pick = |v: Option<&str>, default: &str| {
            v.map(str::trim)
                .filter(|v| !v.is_empty())
                .unwrap_or(default)
                .trim_end_matches('/')
                .to_string()
        };
        Sources {
            registry: pick(registry, npm::DEFAULT_REGISTRY),
            node_mirror: pick(node_mirror, node::DEFAULT_MIRROR),
        }
    }
}

/// `<data dir>/forge/runtimes`: Node.js builds Forge downloaded.
pub fn default_runtimes_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("forge").join("runtimes"))
}

/// The packages of a global npm install option (`npm install -g a b@1` → `[a, b@1]`).
pub fn npm_packages(option: &InstallOption) -> Option<Vec<String>> {
    if option.tool != "npm" {
        return None;
    }
    let (program, rest) = option.command.split_first()?;
    let (verb, rest) = rest.split_first()?;
    let global = |a: &String| a == "-g" || a == "--global";
    if program != "npm"
        || !matches!(verb.as_str(), "install" | "i" | "add")
        || !rest.iter().any(global)
    {
        return None;
    }
    let packages: Vec<String> = rest
        .iter()
        .filter(|a| !a.starts_with('-'))
        .cloned()
        .collect();
    (!packages.is_empty()).then_some(packages)
}

/// The project of a PyPI install option (`pypi install ruff` → `ruff`): its executable is
/// downloaded from PyPI's wheels by [`pypi::install`].
pub fn pypi_package(option: &InstallOption) -> Option<String> {
    match (option.tool.as_str(), option.command.as_slice()) {
        ("pypi", [program, verb, name]) if program == "pypi" && verb == "install" => {
            Some(name.clone())
        }
        _ => None,
    }
}

/// Installs npm packages into Forge's directory with a Node.js it finds or downloads.
pub struct Installer {
    pub http: Http,
    pub sources: Sources,
    /// `<data dir>/forge/langservers`
    pub managed_dir: PathBuf,
    /// `<data dir>/forge/runtimes`
    pub runtimes_dir: PathBuf,
    /// The Node.js build for this machine (`None`: no official build, use the machine's).
    pub target: Option<DistTarget>,
    pub detector: Detector,
}

impl Installer {
    /// An installer for this machine with Forge's directories, or `None` when there is no
    /// user data directory.
    pub fn system(sources: Sources) -> Result<Self, String> {
        let detector = Detector::system();
        let managed_dir = detector
            .managed_dir()
            .map(Path::to_path_buf)
            .ok_or("no user data directory for language servers")?;
        let runtimes_dir = detector
            .runtimes_dir()
            .map(Path::to_path_buf)
            .ok_or("no user data directory for Node.js")?;
        Ok(Installer {
            http: Http::new()?,
            sources,
            managed_dir,
            runtimes_dir,
            target: DistTarget::current(),
            detector,
        })
    }

    /// A Node.js >= 18 to run the servers with: Forge's download, else the machine's, else
    /// a fresh download of the current LTS.
    pub async fn ensure_node(&self, on: &mut (dyn FnMut(Event) + Send)) -> Result<PathBuf, String> {
        if let Some(node) = self.detector.node() {
            on(Event::step(format!("Using Node.js at {}.", node.display())));
            return Ok(node);
        }
        let target = self.target.as_ref().ok_or_else(|| {
            "No Node.js 18 or newer was found and there is no official build for this platform. Install Node.js and try again.".to_string()
        })?;
        on(Event::step(
            "No Node.js 18 or newer found; Forge downloads its own copy.",
        ));
        std::fs::create_dir_all(&self.runtimes_dir)
            .map_err(|e| format!("cannot create {}: {e}", self.runtimes_dir.display()))?;
        node::download(
            &self.http,
            &self.sources.node_mirror,
            target,
            &self.runtimes_dir,
            on,
        )
        .await
    }

    /// Install `specs` (`name` / `name@version`) into the managed directory. Packages with
    /// runtime dependencies are installed with npm, the others straight from the registry.
    pub async fn install(
        &self,
        specs: &[String],
        cancel: Arc<tokio::sync::Notify>,
        on: &mut (dyn FnMut(Event) + Send),
    ) -> Result<InstallOutcome, String> {
        std::fs::create_dir_all(&self.managed_dir)
            .map_err(|e| format!("cannot create {}: {e}", self.managed_dir.display()))?;
        let node = self.ensure_node(on).await?;
        let mut metas = Vec::new();
        for spec in specs {
            on(Event::step(format!(
                "Fetching {spec} from {}…",
                self.sources.registry
            )));
            metas.push(npm::metadata(&self.http, &self.sources.registry, spec).await?);
        }
        if metas.iter().all(npm::PackageMeta::standalone) {
            for meta in &metas {
                npm::install_standalone(&self.http, meta, &self.managed_dir, on).await?;
            }
            return Ok(InstallOutcome::Succeeded);
        }
        // Dependencies need npm's resolver: the one next to the Node.js in use.
        let (program, mut args) = self.npm_command(&node)?;
        args.extend(
            [
                "install",
                "--prefix",
                &self.managed_dir.display().to_string(),
                "--no-fund",
                "--no-audit",
                "--registry",
                &self.sources.registry,
            ]
            .map(String::from),
        );
        // Exact versions, as checked above.
        args.extend(metas.iter().map(|m| format!("{}@{}", m.name, m.version)));
        on(Event::step(format!(
            "$ {}",
            crate::catalog::display_args(
                &std::iter::once(program.display().to_string())
                    .chain(args.iter().cloned())
                    .collect::<Vec<_>>()
            )
        )));
        let node_dir: Vec<PathBuf> = node.parent().map(Path::to_path_buf).into_iter().collect();
        let outcome = install::run(program, args, node_dir, cancel, |stream, line| {
            on(Event::Output { stream, line })
        })
        .await;
        Ok(outcome)
    }

    /// How to run npm with `node`: Forge's Node.js runs its bundled `npm-cli.js`; a machine
    /// Node.js uses the npm next to it or on `PATH`.
    fn npm_command(&self, node: &Path) -> Result<(PathBuf, Vec<String>), String> {
        if let Some(runtime) = node::runtime_of(node, &self.runtimes_dir) {
            let windows = self.target.as_ref().is_some_and(|t| t.windows);
            let cli = node::npm_cli(&runtime, windows);
            if cli.is_file() {
                return Ok((node.to_path_buf(), vec![cli.display().to_string()]));
            }
        }
        let beside = node.parent().and_then(|dir| {
            ["npm", "npm.cmd"]
                .iter()
                .map(|n| dir.join(n))
                .find(|p| p.is_file())
        });
        beside
            .or_else(|| self.detector.find("npm"))
            .map(|npm| (npm, vec![]))
            .ok_or_else(|| "npm was not found next to Node.js".to_string())
    }

    /// Newer versions of the packages Forge installed. `None` when the last check was less
    /// than a day ago (and `force` is not set).
    pub async fn check_updates(&self, force: bool, now: u64) -> Option<Vec<PackageUpdate>> {
        let state = updates::state_file(&self.managed_dir);
        if !force && !updates::due(&state, now) {
            return None;
        }
        let mut found = Vec::new();
        for name in npm::recorded_packages(&self.managed_dir) {
            let Some(installed) = npm::installed_version(&self.managed_dir, &name) else {
                continue;
            };
            match npm::metadata(&self.http, &self.sources.registry, &name).await {
                Ok(meta) if updates::is_newer(&meta.version, &installed) => {
                    found.push(PackageUpdate {
                        name,
                        installed,
                        latest: meta.version,
                    })
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::info!("update check for {name} failed: {e}");
                    // Offline: try again next time instead of waiting a day.
                    return Some(found);
                }
            }
        }
        updates::mark_checked(&state, now);
        Some(found)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detect::DetectEnv;
    use crate::managed::archive::tests::tar_gz;
    use crate::managed::npm::tests::{fake_intelephense, sri};

    fn installer(server: &testserver::TestServer, root: &Path, path: &[PathBuf]) -> Installer {
        let managed_dir = root.join("data/forge/langservers");
        let runtimes_dir = root.join("data/forge/runtimes");
        Installer {
            http: Http::direct().unwrap(),
            sources: Sources::new(Some(&server.url), Some(&server.url)),
            detector: Detector::new(DetectEnv {
                path: Some(std::env::join_paths(path).unwrap()),
                managed_dir: Some(managed_dir.clone()),
                runtimes_dir: Some(runtimes_dir.clone()),
                skip_system_dirs: true,
                ..Default::default()
            }),
            managed_dir,
            runtimes_dir,
            target: DistTarget::for_platform("linux", "x86_64"),
        }
    }

    fn serve_node(server: &testserver::TestServer) {
        let archive = tar_gz(&[(
            "node-v22.11.0-linux-x64/bin/node",
            b"#!/bin/sh\necho v22.11.0\n",
            0o755,
        )]);
        server.routes.set(
            "/index.json",
            r#"[{"version":"v22.11.0","lts":"Jod","files":["linux-x64"]}]"#,
        );
        server.routes.set(
            "/v22.11.0/SHASUMS256.txt",
            format!(
                "{}  node-v22.11.0-linux-x64.tar.gz\n",
                node::sha256_hex(&archive)
            ),
        );
        server
            .routes
            .set("/v22.11.0/node-v22.11.0-linux-x64.tar.gz", archive);
    }

    #[test]
    fn npm_packages_of_install_options() {
        let opt = |cmd: &[&str]| InstallOption {
            tool: "npm".into(),
            command: cmd.iter().map(|s| s.to_string()).collect(),
            url: None,
            platforms: vec![],
        };
        assert_eq!(
            npm_packages(&opt(&[
                "npm",
                "install",
                "-g",
                "typescript-language-server",
                "typescript"
            ])),
            Some(vec![
                "typescript-language-server".into(),
                "typescript".into()
            ])
        );
        assert_eq!(
            npm_packages(&opt(&["npm", "install", "intelephense"])),
            None
        );
        assert_eq!(npm_packages(&opt(&["npm", "install", "-g"])), None);
        assert_eq!(
            Sources::new(Some(" https://mirror.example/npm/ "), Some("")),
            Sources {
                registry: "https://mirror.example/npm".into(),
                node_mirror: node::DEFAULT_MIRROR.into(),
            }
        );
    }

    /// No Node.js and no npm on the machine: Forge downloads Node.js, then Intelephense from
    /// the registry, and the server is detected and launched with that Node.js.
    #[cfg(unix)]
    #[tokio::test]
    async fn zero_prerequisite_install_of_intelephense() {
        let server = testserver::start().await;
        serve_node(&server);
        fake_intelephense(&server, "1.12.6", false);
        let tmp = tempfile::tempdir().unwrap();
        let empty_path = tmp.path().join("empty-bin");
        std::fs::create_dir_all(&empty_path).unwrap();
        let inst = installer(&server, tmp.path(), &[empty_path]);
        let mut steps = Vec::new();
        let outcome = inst
            .install(
                &["intelephense".into()],
                Arc::new(tokio::sync::Notify::new()),
                &mut |e| {
                    if let Event::Step { message } = e {
                        steps.push(message)
                    }
                },
            )
            .await
            .unwrap();
        assert_eq!(outcome, InstallOutcome::Succeeded);
        assert!(
            steps.iter().any(|s| s.contains("downloads its own copy")),
            "{steps:?}"
        );

        let node = inst.runtimes_dir.join("node-v22.11.0/bin/node");
        let script = inst
            .managed_dir
            .join("node_modules/intelephense/lib/intelephense.js");
        // A fresh detector (as after the install) finds and launches it with Forge's Node.js.
        let d = Detector::new(inst.detector.env().clone());
        let exe = d.find("intelephense").unwrap();
        assert_eq!(exe, inst.managed_dir.join("node_modules/.bin/intelephense"));
        assert_eq!(d.node(), Some(node.clone()));
        assert_eq!(
            d.managed_launch("intelephense"),
            Some((node.clone(), script.clone()))
        );
        let (cmd, args) = d.launch(&exe, &["--stdio".to_string()]);
        assert_eq!(cmd, node.display().to_string());
        assert_eq!(args, vec![script.display().to_string(), "--stdio".into()]);

        // A second install reuses Node.js (no new download).
        inst.install(
            &["intelephense".into()],
            Arc::new(tokio::sync::Notify::new()),
            &mut |_| {},
        )
        .await
        .unwrap();
        assert_eq!(
            server
                .routes
                .hits("/v22.11.0/node-v22.11.0-linux-x64.tar.gz"),
            1
        );
    }

    /// Packages with dependencies go through npm (here a fake one next to Node.js).
    #[cfg(unix)]
    #[tokio::test]
    async fn packages_with_dependencies_use_npm() {
        let server = testserver::start().await;
        let tmp = tempfile::tempdir().unwrap();
        let bin = tmp.path().join("usr/bin");
        // A machine Node.js 20 with npm next to it; npm records its arguments.
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("node"), "#!/bin/sh\necho v20.1.0\n").unwrap();
        let log = tmp.path().join("npm-args");
        std::fs::write(
            bin.join("npm"),
            format!(
                "#!/bin/sh\necho \"$@\" > '{}'\necho added 3 packages\n",
                log.display()
            ),
        )
        .unwrap();
        for f in ["node", "npm"] {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(bin.join(f), std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let tgz = tar_gz(&[("package/package.json", b"{}", 0o644)]);
        server.routes.set(
            "/yaml-language-server/latest",
            serde_json::json!({
                "name": "yaml-language-server", "version": "1.15.0",
                "dependencies": {"vscode-languageserver": "^9"},
                "dist": {"tarball": format!("{}/y.tgz", server.url), "integrity": sri(&tgz)}
            })
            .to_string(),
        );
        let inst = installer(&server, tmp.path(), std::slice::from_ref(&bin));
        let mut output = Vec::new();
        let outcome = inst
            .install(
                &["yaml-language-server".into()],
                Arc::new(tokio::sync::Notify::new()),
                &mut |e| {
                    if let Event::Output { line, .. } = e {
                        output.push(line)
                    }
                },
            )
            .await
            .unwrap();
        assert_eq!(outcome, InstallOutcome::Succeeded);
        assert_eq!(output, vec!["added 3 packages"]);
        let args = std::fs::read_to_string(&log).unwrap();
        assert!(args.starts_with("install --prefix "), "{args}");
        assert!(
            args.contains(&format!("--registry {}", server.url)),
            "{args}"
        );
        assert!(
            args.trim_end().ends_with("yaml-language-server@1.15.0"),
            "{args}"
        );
        assert_eq!(
            server.routes.hits("/y.tgz"),
            0,
            "npm downloads it, not Forge"
        );
        assert!(
            !inst.runtimes_dir.exists(),
            "the machine's Node.js 20 is used"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn update_check_runs_at_most_once_a_day() {
        let server = testserver::start().await;
        serve_node(&server);
        fake_intelephense(&server, "1.12.6", false);
        let tmp = tempfile::tempdir().unwrap();
        let inst = installer(&server, tmp.path(), &[]);
        inst.install(
            &["intelephense".into()],
            Arc::new(tokio::sync::Notify::new()),
            &mut |_| {},
        )
        .await
        .unwrap();
        let day = 24 * 60 * 60;
        let t0 = 1_800_000_000;
        assert_eq!(inst.check_updates(false, t0).await, Some(vec![]));
        // A new release appears; within the day nothing is asked.
        fake_intelephense(&server, "1.14.4", false);
        assert_eq!(inst.check_updates(false, t0 + 3600).await, None);
        assert_eq!(
            server.routes.hits("/intelephense/latest"),
            2,
            "install + first check"
        );
        let updates = inst.check_updates(false, t0 + day + 1).await.unwrap();
        assert_eq!(
            updates,
            vec![PackageUpdate {
                name: "intelephense".into(),
                installed: "1.12.6".into(),
                latest: "1.14.4".into()
            }]
        );
        // "Check now" ignores the interval.
        assert!(inst.check_updates(true, t0 + day + 2).await.is_some());
        // Updating installs the new version in place.
        inst.install(
            &["intelephense".into()],
            Arc::new(tokio::sync::Notify::new()),
            &mut |_| {},
        )
        .await
        .unwrap();
        assert_eq!(
            npm::installed_version(&inst.managed_dir, "intelephense").as_deref(),
            Some("1.14.4")
        );
        assert_eq!(inst.check_updates(true, t0 + day + 3).await, Some(vec![]));
    }
}
