//! Which servers to start for a project, and the rows of the Language Servers settings
//! page.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use forge_lsp::LspServerConfig;
use serde::Serialize;
use serde_json::{json, Value};

use crate::catalog::{Catalog, ServerSpec};
use crate::config::{build_config, ConfigContext};

/// Why a catalog server is not started for the project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "reason", rename_all = "camelCase")]
pub enum Skip {
    AutoStartOff,
    Disabled,
    PluginDisabled {
        plugin: String,
    },
    /// None of its languages occur in the project.
    NotInProject,
    /// Its `when` markers (config files, packages) are missing.
    NotUsed,
    NotInstalled,
    /// Another server of the same alternative group is used.
    Alternative {
        used: String,
    },
}

#[derive(Debug, Clone)]
pub struct Decision {
    pub id: String,
    pub executable: Option<PathBuf>,
    pub outcome: Result<LspServerConfig, Skip>,
}

/// Decide for every catalog server whether it starts in the project described by `ctx`.
/// Servers are returned in catalog (= priority) order.
pub fn plan(
    catalog: &Catalog,
    ctx: &ConfigContext,
    plugin_enabled: impl Fn(&str) -> bool,
) -> Vec<Decision> {
    let mut taken: HashMap<String, String> = HashMap::new();
    catalog
        .servers
        .iter()
        .map(|spec| {
            let prefs = ctx.prefs.server(&spec.id);
            let executable = ctx.detector.resolve(&spec.command, prefs.path.as_deref());
            let outcome = decide(
                catalog,
                spec,
                ctx,
                &plugin_enabled,
                executable.as_ref(),
                &taken,
            );
            if let (Ok(_), Some(group)) = (&outcome, &spec.alternative) {
                taken.insert(group.clone(), spec.id.clone());
            }
            Decision {
                id: spec.id.clone(),
                executable,
                outcome,
            }
        })
        .collect()
}

fn decide(
    catalog: &Catalog,
    spec: &ServerSpec,
    ctx: &ConfigContext,
    plugin_enabled: &impl Fn(&str) -> bool,
    executable: Option<&PathBuf>,
    taken: &HashMap<String, String>,
) -> Result<LspServerConfig, Skip> {
    if !ctx.prefs.auto_start {
        return Err(Skip::AutoStartOff);
    }
    if !ctx.prefs.is_enabled(&spec.id, spec.enabled) {
        return Err(Skip::Disabled);
    }
    if let Some(plugin) = spec.plugin.as_deref().filter(|p| !plugin_enabled(p)) {
        return Err(Skip::PluginDisabled {
            plugin: plugin.to_string(),
        });
    }
    if !spec
        .languages
        .iter()
        .any(|l| ctx.scan.languages.contains(l))
    {
        return Err(Skip::NotInProject);
    }
    if !spec.when.is_empty()
        && !ctx.scan.has_root_file(&spec.when.files)
        && !ctx.scan.has_package(&spec.when.packages)
    {
        return Err(Skip::NotUsed);
    }
    if let Some(used) = spec.alternative.as_ref().and_then(|g| taken.get(g)) {
        return Err(Skip::Alternative { used: used.clone() });
    }
    let Some(executable) = executable else {
        return Err(Skip::NotInstalled);
    };
    Ok(build_config(catalog, spec, executable, ctx))
}

/// Install option as shown in the UI.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallRow {
    /// Index into the server's `install` list (what `langservers_install` takes).
    pub index: usize,
    pub tool: String,
    /// The command line, or the download page.
    pub command: String,
    /// Forge can run it (no root, no browser needed).
    pub runnable: bool,
    /// The tool (`npm`, `pipx`, …) was found on this machine.
    pub available: bool,
    pub url: Option<String>,
}

/// Status chip on the settings page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RowStatus {
    Running,
    Installed,
    NotInstalled,
    Disabled,
}

/// One row of Settings > Languages & Frameworks > Language Servers.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerRow {
    pub id: String,
    pub name: String,
    pub group: String,
    pub languages: Vec<String>,
    pub command: String,
    pub args: Vec<String>,
    pub homepage: String,
    pub license: String,
    pub note: Option<String>,
    pub enabled: bool,
    pub default_enabled: bool,
    pub status: RowStatus,
    /// Server state detail, e.g. "Indexing 45%" or a start error.
    pub detail: Option<String>,
    /// Detected or custom executable.
    pub path: Option<String>,
    pub custom_path: Option<String>,
    /// One of its languages occurs in the open project.
    pub in_project: bool,
    /// Why it is not running in this project (when it is not).
    pub skip: Option<Skip>,
    pub install: Vec<InstallRow>,
    /// Built-in initialization options and settings, shown next to the extra JSON editor.
    pub defaults: Value,
    pub extra_settings: Option<String>,
    pub extra_settings_error: Option<String>,
    /// Who owns the row: `"catalog"`, or a pack (`"php"`) whose own settings page
    /// configures the server.
    pub source: String,
}

/// Live state of a registered server as the LSP manager reports it.
#[derive(Debug, Clone, PartialEq)]
pub struct LiveState {
    pub running: bool,
    pub detail: Option<String>,
}

pub fn current_platform() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

/// Rows for every catalog server. `decisions` (from [`plan`]) explain why a server is not
/// used in the open project; `live` holds the manager's state per server id.
pub fn rows(
    catalog: &Catalog,
    ctx: &ConfigContext,
    decisions: &[Decision],
    live: &HashMap<String, LiveState>,
    platform: &str,
) -> Vec<ServerRow> {
    let mut tools: HashMap<String, bool> = HashMap::new();
    catalog
        .servers
        .iter()
        .map(|spec| {
            let prefs = ctx.prefs.server(&spec.id);
            let decision = decisions.iter().find(|d| d.id == spec.id);
            let executable = decision
                .and_then(|d| d.executable.clone())
                .or_else(|| ctx.detector.resolve(&spec.command, prefs.path.as_deref()));
            let enabled = ctx.prefs.is_enabled(&spec.id, spec.enabled);
            let state = live.get(&spec.id);
            let status = if !enabled {
                RowStatus::Disabled
            } else if state.is_some_and(|s| s.running) {
                RowStatus::Running
            } else if executable.is_some() {
                RowStatus::Installed
            } else {
                RowStatus::NotInstalled
            };
            let install = spec
                .install
                .iter()
                .enumerate()
                .filter(|(_, o)| o.applies_to(platform))
                .map(|(index, o)| {
                    let program = o.command.first().cloned().unwrap_or_default();
                    let available = !program.is_empty()
                        && *tools
                            .entry(program.clone())
                            .or_insert_with(|| ctx.detector.find(&program).is_some());
                    InstallRow {
                        index,
                        tool: o.tool.clone(),
                        command: o.display(),
                        runnable: o.runnable(),
                        available,
                        url: o.url.clone(),
                    }
                })
                .collect();
            ServerRow {
                id: spec.id.clone(),
                name: spec.name.clone(),
                group: spec.group.clone(),
                languages: catalog.language_names(spec),
                command: spec.command.clone(),
                args: spec.args.clone(),
                homepage: spec.homepage.clone(),
                license: spec.license.clone(),
                note: spec.note.clone(),
                enabled,
                default_enabled: spec.enabled,
                status,
                detail: state.and_then(|s| s.detail.clone()),
                path: executable.map(|p| p.display().to_string()),
                custom_path: prefs.path.as_ref().map(|p| p.display().to_string()),
                in_project: spec
                    .languages
                    .iter()
                    .any(|l| ctx.scan.languages.contains(l)),
                skip: decision.and_then(|d| d.outcome.clone().err()),
                install,
                defaults: json!({
                    "initializationOptions": spec.initialization_options,
                    "settings": spec.settings,
                }),
                extra_settings: prefs.settings_json.clone(),
                extra_settings_error: prefs.extra_settings().err(),
                source: "catalog".into(),
            }
        })
        .collect()
}

/// Ids of servers in `decisions` that should run.
pub fn wanted(decisions: &[Decision]) -> HashSet<String> {
    decisions
        .iter()
        .filter(|d| d.outcome.is_ok())
        .map(|d| d.id.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detect::tests::fake_bin;
    use crate::detect::{DetectEnv, Detector};
    use crate::prefs::{Prefs, ServerPrefs};
    use crate::scan::ProjectScan;

    struct Fixture {
        _tmp: tempfile::TempDir,
        root: PathBuf,
        detector: Detector,
    }

    fn fixture(installed: &[&str]) -> Fixture {
        let tmp = tempfile::tempdir().unwrap();
        let bin = tmp.path().join("bin");
        for name in installed {
            fake_bin(&bin.join(name));
        }
        let detector = Detector::new(DetectEnv {
            path: Some(bin.clone().into_os_string()),
            ..Default::default()
        });
        Fixture {
            root: tmp.path().join("project"),
            _tmp: tmp,
            detector,
        }
    }

    fn scan(langs: &[&str]) -> ProjectScan {
        ProjectScan {
            languages: langs.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }

    fn outcome<'a>(d: &'a [Decision], id: &str) -> &'a Result<LspServerConfig, Skip> {
        &d.iter().find(|x| x.id == id).unwrap().outcome
    }

    #[test]
    fn starts_installed_servers_for_languages_in_the_project() {
        let f = fixture(&[
            "typescript-language-server",
            "vtsls",
            "gopls",
            "basedpyright-langserver",
            "vscode-eslint-language-server",
        ]);
        let prefs = Prefs::default();
        let s = scan(&["typescript", "python", "markdown"]);
        let ctx = ConfigContext {
            root: &f.root,
            cache_dir: f.root.join("cache"),
            detector: &f.detector,
            prefs: &prefs,
            scan: &s,
        };
        let d = plan(Catalog::builtin(), &ctx, |_| true);
        assert!(outcome(&d, "typescript-language-server").is_ok());
        assert_eq!(
            outcome(&d, "vtsls"),
            &Err(Skip::Alternative {
                used: "typescript-language-server".into()
            })
        );
        assert!(outcome(&d, "basedpyright").is_ok());
        assert_eq!(
            outcome(&d, "pyright"),
            &Err(Skip::Alternative {
                used: "basedpyright".into()
            })
        );
        assert_eq!(outcome(&d, "gopls"), &Err(Skip::NotInProject));
        assert_eq!(outcome(&d, "marksman"), &Err(Skip::NotInstalled));
        assert_eq!(
            outcome(&d, "vscode-eslint-language-server"),
            &Err(Skip::NotUsed)
        );
        let wanted = wanted(&d);
        assert_eq!(wanted.len(), 2);

        // Plugins switched off in Settings > Plugins keep their servers off.
        let d = plan(Catalog::builtin(), &ctx, |p| p != "forge.web");
        assert_eq!(
            outcome(&d, "typescript-language-server"),
            &Err(Skip::PluginDisabled {
                plugin: "forge.web".into()
            })
        );
    }

    #[test]
    fn user_choices_change_the_plan() {
        let f = fixture(&[
            "typescript-language-server",
            "vtsls",
            "vscode-eslint-language-server",
        ]);
        let mut prefs = Prefs::default();
        prefs.servers.insert(
            "typescript-language-server".into(),
            ServerPrefs {
                enabled: Some(false),
                ..Default::default()
            },
        );
        let mut s = scan(&["typescript"]);
        s.packages.insert("eslint".into());
        let ctx = ConfigContext {
            root: &f.root,
            cache_dir: f.root.join("cache"),
            detector: &f.detector,
            prefs: &prefs,
            scan: &s,
        };
        let d = plan(Catalog::builtin(), &ctx, |_| true);
        assert_eq!(
            outcome(&d, "typescript-language-server"),
            &Err(Skip::Disabled)
        );
        assert!(outcome(&d, "vtsls").is_ok(), "the alternative takes over");
        assert!(
            outcome(&d, "vscode-eslint-language-server").is_ok(),
            "eslint dependency"
        );

        let off = Prefs {
            auto_start: false,
            ..Prefs::default()
        };
        let ctx = ConfigContext { prefs: &off, ..ctx };
        assert!(plan(Catalog::builtin(), &ctx, |_| true)
            .iter()
            .all(|d| d.outcome == Err(Skip::AutoStartOff)));
    }

    #[test]
    fn rows_report_status_paths_and_install_options() {
        let f = fixture(&["gopls", "npm", "clangd"]);
        let mut prefs = Prefs::default();
        prefs.servers.insert(
            "clangd".into(),
            ServerPrefs {
                enabled: Some(false),
                ..Default::default()
            },
        );
        prefs.servers.insert(
            "zls".into(),
            ServerPrefs {
                path: Some(PathBuf::from("/nope/zls")),
                settings_json: Some("{".into()),
                ..Default::default()
            },
        );
        let s = scan(&["go"]);
        let ctx = ConfigContext {
            root: &f.root,
            cache_dir: f.root.join("cache"),
            detector: &f.detector,
            prefs: &prefs,
            scan: &s,
        };
        let c = Catalog::builtin();
        let d = plan(c, &ctx, |_| true);
        let live: HashMap<String, LiveState> = [(
            "gopls".to_string(),
            LiveState {
                running: true,
                detail: Some("Indexing 45%".into()),
            },
        )]
        .into_iter()
        .collect();
        let rows = rows(c, &ctx, &d, &live, "linux");
        let row = |id: &str| rows.iter().find(|r| r.id == id).unwrap();

        assert_eq!(row("gopls").status, RowStatus::Running);
        assert_eq!(row("gopls").detail.as_deref(), Some("Indexing 45%"));
        assert!(row("gopls").in_project);
        assert!(row("gopls").path.as_deref().unwrap().ends_with("bin/gopls"));
        assert_eq!(row("clangd").status, RowStatus::Disabled);
        assert_eq!(row("taplo").status, RowStatus::NotInstalled);
        assert_eq!(row("zls").custom_path.as_deref(), Some("/nope/zls"));
        assert_eq!(
            row("zls").path,
            None,
            "a missing custom path is not installed"
        );
        assert!(row("zls").extra_settings_error.is_some());

        let ts = row("typescript-language-server");
        assert_eq!(ts.languages, vec!["TypeScript", "JavaScript"]);
        assert_eq!(ts.skip, Some(Skip::NotInProject));
        let npm = ts.install.iter().find(|i| i.tool == "npm").unwrap();
        assert!(npm.runnable && npm.available);
        let brew = ts.install.iter().find(|i| i.tool == "brew").unwrap();
        assert!(!brew.available);
        assert_eq!(ts.defaults["initializationOptions"]["hostInfo"], "forge");

        let windows = super::rows(c, &ctx, &d, &live, "windows");
        let ts = windows
            .iter()
            .find(|r| r.id == "typescript-language-server")
            .unwrap();
        assert!(ts.install.iter().all(|i| i.tool == "npm"));
    }
}
