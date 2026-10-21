//! Which catalog language servers (`forge-langservers`) start for a project, planned on the
//! machine that has the project's files and the installed servers: the desktop app for a
//! local project, the Forge agent for a remote one (its servers then start on the host
//! through the LSP byte channel). Installs from the settings page run on that machine too
//! ([`install`]): for a remote project the agent downloads Node.js and the packages on the
//! host.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::{Arc, Mutex};

use forge_core::Result;
use forge_langservers::install::InstallOutcome;
use forge_langservers::managed::{self, Event, Installer, PackageUpdate, Sources};
use forge_langservers::plan::current_platform;
use forge_langservers::{
    plan, rows, scan_project, Catalog, ConfigContext, Detector, LiveState, Prefs, ProjectScan,
    RowStatus, ServerRow, Skip,
};
use forge_lsp::LspServerConfig;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::Notify;

/// Prefix of the settings the planner reads.
pub const SETTINGS_PREFIX: &str = "langservers.";

/// What the app knows that the planner needs.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LangServersRequest {
    /// The effective `langservers.*` settings.
    pub settings: BTreeMap<String, Value>,
    /// Ids of disabled plugins (servers of a disabled plugin do not start).
    pub disabled_plugins: Vec<String>,
    /// State of the servers the app's LSP manager runs, for the settings page.
    pub live: Vec<LiveServer>,
    /// Scan the project's languages again instead of using the last scan.
    pub rescan: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveServer {
    pub id: String,
    pub running: bool,
    pub detail: Option<String>,
}

/// The decision for one catalog server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedServer {
    pub id: String,
    /// Config to register (executables are paths on the project's machine).
    pub config: Option<LspServerConfig>,
    /// Why it does not start.
    pub skip: Option<Skip>,
}

/// Reply to a [`LangServersRequest`].
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LangServersAnswer {
    /// Catalog servers in priority order.
    pub servers: Vec<PlannedServer>,
    /// Rows of Settings | Languages & Frameworks | Language Servers (catalog servers, then
    /// the PHP pack's servers).
    pub rows: Vec<ServerRow>,
    /// `linux`, `macos` or `windows`: install commands are for this platform.
    pub platform: String,
}

/// The planner of a folder on this machine; remembers the last language scan.
#[derive(Default)]
pub struct LocalLangServers {
    scan: Mutex<Option<ProjectScan>>,
}

fn cache_dir() -> std::path::PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("forge")
}

impl LocalLangServers {
    fn scan(&self, root: &Path, limit: usize, rescan: bool) -> ProjectScan {
        let mut guard = self.scan.lock().expect("scan lock");
        if let (false, Some(scan)) = (rescan, guard.as_ref()) {
            return scan.clone();
        }
        let scan = scan_project(root, Catalog::builtin(), limit);
        *guard = Some(scan.clone());
        scan
    }

    /// Plan the servers of the project at `root` and build the settings page rows.
    pub fn plan(&self, root: &Path, req: &LangServersRequest) -> Result<LangServersAnswer> {
        let ids = Catalog::builtin().servers.iter().map(|s| s.id.as_str());
        let prefs = Prefs::load(|k| req.settings.get(k).cloned(), ids);
        let scan = self.scan(root, prefs.scan_limit, req.rescan);
        let detector = Detector::for_project(root);
        let ctx = ConfigContext {
            root,
            cache_dir: cache_dir(),
            detector: &detector,
            prefs: &prefs,
            scan: &scan,
        };
        let enabled = |plugin: &str| !req.disabled_plugins.iter().any(|p| p == plugin);
        let decisions = plan(Catalog::builtin(), &ctx, enabled);
        let live: HashMap<String, LiveState> = req
            .live
            .iter()
            .map(|l| {
                (
                    l.id.clone(),
                    LiveState {
                        running: l.running,
                        detail: l.detail.clone(),
                    },
                )
            })
            .collect();
        let platform = current_platform();
        let mut out = rows(Catalog::builtin(), &ctx, &decisions, &live, platform);
        let packs = rows(Catalog::pack_servers(), &ctx, &[], &live, platform);
        out.extend(packs.into_iter().map(pack_row));
        let servers = decisions
            .into_iter()
            .map(|d| match d.outcome {
                Ok(config) => PlannedServer {
                    id: d.id,
                    config: Some(config),
                    skip: None,
                },
                Err(skip) => PlannedServer {
                    id: d.id,
                    config: None,
                    skip: Some(skip),
                },
            })
            .collect();
        Ok(LangServersAnswer {
            servers,
            rows: out,
            platform: platform.into(),
        })
    }
}

/// Settings page rows when no project is open (servers detected on this machine).
pub fn system_rows(req: &LangServersRequest) -> Vec<ServerRow> {
    let ids = Catalog::builtin().servers.iter().map(|s| s.id.as_str());
    let prefs = Prefs::load(|k| req.settings.get(k).cloned(), ids);
    let detector = Detector::system();
    let scan = ProjectScan::default();
    let root = std::path::PathBuf::new();
    let ctx = ConfigContext {
        root: &root,
        cache_dir: cache_dir(),
        detector: &detector,
        prefs: &prefs,
        scan: &scan,
    };
    let live = HashMap::new();
    let platform = current_platform();
    let mut out = rows(Catalog::builtin(), &ctx, &[], &live, platform);
    out.extend(
        rows(Catalog::pack_servers(), &ctx, &[], &live, platform)
            .into_iter()
            .map(pack_row),
    );
    out
}

/// A row of a server the PHP pack starts itself: listed for status and install hints only.
fn pack_row(mut r: ServerRow) -> ServerRow {
    r.source = "php".into();
    r.enabled = true;
    r.status = match r.status {
        RowStatus::Disabled => RowStatus::Installed,
        s => s,
    };
    r
}

/// What to install on the project's machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum InstallTarget {
    /// Install option `index` of catalog (or PHP pack) server `id`.
    Server { id: String, index: usize },
    /// An npm package Forge installed before, to its latest version (`name` or
    /// `name@version`).
    Package { spec: String },
}

/// An install of a language server on the machine that runs the project's servers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallRequest {
    pub target: InstallTarget,
    /// `langservers.download.npmRegistry`
    #[serde(default)]
    pub registry: Option<String>,
    /// `langservers.download.nodeMirror`
    #[serde(default)]
    pub node_mirror: Option<String>,
}

/// Receives the steps, download progress and output of an install.
pub type InstallSink = Arc<dyn Fn(Event) + Send + Sync>;

/// The name an install reports when it is done ("Intelephense installed.").
pub fn install_name(target: &InstallTarget) -> String {
    match target {
        InstallTarget::Server { id, .. } => Catalog::builtin()
            .server(id)
            .or_else(|| Catalog::pack_servers().server(id))
            .map(|s| s.name.clone())
            .unwrap_or_else(|| id.clone()),
        InstallTarget::Package { spec } => spec.clone(),
    }
}

/// Install on this machine: npm packages with Forge's managed installer (Node.js is
/// downloaded when the machine has none), other catalog commands (`pipx`, `go`, …) as they
/// are. `cancel` stops it. Errors are worded for the install log.
pub async fn install(
    req: &InstallRequest,
    cancel: Arc<Notify>,
    on: &mut (dyn FnMut(Event) + Send),
) -> std::result::Result<InstallOutcome, String> {
    let sources = Sources::new(req.registry.as_deref(), req.node_mirror.as_deref());
    let detector = Detector::system();
    let (packages, option) = match &req.target {
        InstallTarget::Package { spec } => (Some(vec![spec.clone()]), None),
        InstallTarget::Server { id, index } => {
            let spec = Catalog::builtin()
                .server(id)
                .or_else(|| Catalog::pack_servers().server(id))
                .ok_or_else(|| format!("unknown language server {id}"))?;
            let option = spec
                .install
                .get(*index)
                .ok_or("unknown install option")?
                .clone();
            let packages = detector
                .managed_dir()
                .and_then(|_| managed::npm_packages(&option));
            (packages, Some(option))
        }
    };
    if let Some(packages) = packages {
        let installer = Installer::system(sources)?;
        return tokio::select! {
            r = installer.install(&packages, Arc::new(Notify::new()), on) => r,
            _ = cancel.notified() => Ok(InstallOutcome::Cancelled),
        };
    }
    let option = option.ok_or("nothing to install")?;
    let (program, args) = forge_langservers::install::resolve_command(&option, &detector)?;
    on(Event::step(format!(
        "$ {}",
        forge_langservers::install::display_command(&option, &detector)
    )));
    Ok(forge_langservers::install::run(
        program,
        args,
        detector.dirs().to_vec(),
        cancel,
        |stream, line| on(Event::Output { stream, line }),
    )
    .await)
}

/// Newer versions of the npm servers Forge installed on this machine (see
/// [`Installer::check_updates`]).
pub async fn check_updates(
    registry: Option<&str>,
    node_mirror: Option<&str>,
    force: bool,
) -> std::result::Result<Option<Vec<PackageUpdate>>, String> {
    let installer = Installer::system(Sources::new(registry, node_mirror))?;
    Ok(installer
        .check_updates(force, managed::updates::now_secs())
        .await)
}

/// Run `fut` from a blocking thread: on the current tokio runtime when there is one (the
/// agent's blocking pool), else on a small runtime of its own.
pub fn block_on<F: std::future::Future>(fut: F) -> Result<F::Output> {
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => Ok(handle.block_on(fut)),
        Err(_) => tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map(|rt| rt.block_on(fut))
            .map_err(|e| forge_core::ForgeError::Other(format!("cannot start a runtime: {e}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans_servers_for_the_languages_of_the_project() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("main.lua"), "print(1)\n").unwrap();
        let planner = LocalLangServers::default();
        let mut req = LangServersRequest::default();
        req.settings
            .insert("langservers.autoStart".into(), Value::Bool(true));
        let answer = planner.plan(dir.path(), &req).unwrap();
        let lua = answer
            .servers
            .iter()
            .find(|s| s.id == "lua-language-server")
            .unwrap();
        // Not installed in the test environment, but its language is in the project.
        assert!(lua.config.is_some() || lua.skip == Some(Skip::NotInstalled));
        let ts = answer
            .servers
            .iter()
            .find(|s| s.id == "typescript-language-server")
            .unwrap();
        assert_eq!(ts.skip, Some(Skip::NotInProject));
        assert!(answer.rows.iter().any(|r| r.source == "php"));

        // Turned off globally: nothing starts.
        req.settings
            .insert("langservers.autoStart".into(), Value::Bool(false));
        req.rescan = true;
        let off = planner.plan(dir.path(), &req).unwrap();
        assert!(off
            .servers
            .iter()
            .all(|s| s.skip == Some(Skip::AutoStartOff)));
    }
}
