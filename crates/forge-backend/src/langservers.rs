//! Which catalog language servers (`forge-langservers`) start for a project, planned on the
//! machine that has the project's files and the installed servers: the desktop app for a
//! local project, the Forge agent for a remote one (its servers then start on the host
//! through the LSP byte channel).

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::Mutex;

use forge_core::Result;
use forge_langservers::plan::current_platform;
use forge_langservers::{
    plan, rows, scan_project, Catalog, ConfigContext, Detector, LiveState, Prefs, ProjectScan,
    RowStatus, ServerRow, Skip,
};
use forge_lsp::LspServerConfig;
use serde::{Deserialize, Serialize};
use serde_json::Value;

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
        // Servers the PHP pack starts itself: listed for status and install hints only.
        let packs = rows(Catalog::pack_servers(), &ctx, &[], &live, platform);
        out.extend(packs.into_iter().map(|mut r| {
            r.source = "php".into();
            r.enabled = true;
            r.status = match r.status {
                RowStatus::Disabled => RowStatus::Installed,
                s => s,
            };
            r
        }));
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
        assert!(off.servers.iter().all(|s| s.skip == Some(Skip::AutoStartOff)));
    }
}
