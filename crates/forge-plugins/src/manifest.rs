//! The manifest of a third-party plugin (`forge-plugin.json` at the root of a
//! `.forgeplugin` package) and its validation.
//!
//! ```json
//! {
//!   "manifestVersion": 1,
//!   "id": "example.word-count",
//!   "name": "Word Count",
//!   "version": "1.0.0",
//!   "publisher": "Example",
//!   "engines": { "forge": ">=0.1.0" },
//!   "main": "dist/plugin.js",
//!   "activationEvents": ["onStartup"],
//!   "permissions": { "editor": ["read"] },
//!   "contributes": { "commands": [{ "id": "example.word-count.show", "title": "Show Word Count" }] }
//! }
//! ```
//!
//! Every id a plugin contributes (commands, status items, tool windows, settings pages,
//! setting keys) starts with the plugin id and a dot, so plugins cannot take over built-in
//! actions or each other's settings.

use serde::{Deserialize, Serialize};

use forge_plugin_api::{
    CommandContribution, FileTypeContribution, KeybindingContribution, SettingSpec,
    ToolWindowContribution,
};

/// File name of the manifest inside a package.
pub const MANIFEST_FILE: &str = "forge-plugin.json";
/// The manifest format this Forge reads.
pub const MANIFEST_VERSION: u32 = 1;
/// Plugin ids under this prefix belong to Forge's built-in plugins.
const RESERVED_PREFIX: &str = "forge.";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginManifest {
    pub manifest_version: u32,
    /// Reverse-DNS id, e.g. `example.word-count`.
    pub id: String,
    pub name: String,
    /// SemVer version of the plugin.
    pub version: String,
    #[serde(default)]
    pub publisher: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    pub engines: Engines,
    /// The UI bundle (an ES2020 script) relative to the package root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main: Option<String>,
    /// Backend logic. Declared in the format, not run by this Forge version (see the SDK
    /// docs: WASM components and JSON-RPC processes are planned, native code never).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend: Option<BackendSpec>,
    /// When the UI bundle is loaded: `onStartup`, `onCommand:<id>`, `onLanguage:<file type>`,
    /// `onView:<tool window id>`, `workspaceContains:<file name>`.
    #[serde(default)]
    pub activation_events: Vec<String>,
    #[serde(default)]
    pub permissions: Permissions,
    #[serde(default)]
    pub contributes: Contributes,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Engines {
    /// SemVer range of Forge versions the plugin works with, e.g. `>=0.1.0`.
    pub forge: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum BackendSpec {
    /// A WebAssembly component run by a capability-based WASI host.
    Wasm { module: String },
    /// An external program speaking JSON-RPC over stdio.
    Process {
        command: String,
        #[serde(default)]
        args: Vec<String>,
    },
}

/// What a plugin may do outside its sandbox. The user grants them on install; backend
/// calls are checked against the granted set ([`crate::permissions`]).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Permissions {
    #[serde(default, skip_serializing_if = "FsPermissions::is_empty")]
    pub fs: FsPermissions,
    /// Host names the plugin may send HTTP(S) requests to (`api.example.com`,
    /// `*.example.com`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub network: Vec<String>,
    /// Program names the plugin may start (no paths), e.g. `git`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub process: Vec<String>,
    /// Setting keys outside its own namespace the plugin may read and write (`editor.font.size`,
    /// `editor.font.*`). Its own settings need no permission.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub settings: Vec<String>,
    /// Access to the open editor: `read` (text, selection, file name) and `write` (edits).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub editor: Vec<EditorAccess>,
}

impl Permissions {
    pub fn is_empty(&self) -> bool {
        self == &Permissions::default()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FsPermissions {
    /// Readable roots: `${project}`, `${pluginData}` or a folder below them
    /// (`${project}/docs`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub read: Vec<String>,
    /// Writable roots, same forms.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub write: Vec<String>,
}

impl FsPermissions {
    pub fn is_empty(&self) -> bool {
        self.read.is_empty() && self.write.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EditorAccess {
    Read,
    Write,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Contributes {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commands: Vec<CommandContribution>,
    /// Default shortcuts of the plugin's commands (`Ctrl+Alt+W`); `keymap = "macos"` for the
    /// macOS keymap only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keybindings: Vec<KeybindingContribution>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub status_items: Vec<StatusItemContribution>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_windows: Vec<ToolWindowContribution>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub settings_pages: Vec<SettingsPageContribution>,
    /// Typed settings shown on the plugin's settings pages (`group` = a page id).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub settings: Vec<SettingSpec>,
    /// File types (languages) the plugin adds: names, extensions, indentation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub languages: Vec<FileTypeContribution>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusItemContribution {
    pub id: String,
    #[serde(default)]
    pub alignment: StatusAlignment,
    /// Lower comes first.
    #[serde(default)]
    pub order: i32,
    /// Command run when the item is clicked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StatusAlignment {
    Left,
    #[default]
    Right,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPageContribution {
    pub id: String,
    pub title: String,
}

/// One problem found by [`PluginManifest::validate`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    /// JSON path of the offending field, e.g. `contributes.commands[0].id`.
    pub field: String,
    pub message: String,
}

fn problem(field: impl Into<String>, message: impl Into<String>) -> Problem {
    Problem {
        field: field.into(),
        message: message.into(),
    }
}

/// Render problems as one error message.
pub fn describe(problems: &[Problem]) -> String {
    problems
        .iter()
        .map(|p| format!("{}: {}", p.field, p.message))
        .collect::<Vec<_>>()
        .join("; ")
}

impl PluginManifest {
    pub fn parse(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|e| format!("invalid {MANIFEST_FILE}: {e}"))
    }

    /// Check the manifest against the format rules. `forge_version` is the running Forge
    /// version; a plugin whose engine range excludes it is reported too.
    pub fn validate(&self, forge_version: &str) -> Vec<Problem> {
        let mut out = Vec::new();
        if self.manifest_version != MANIFEST_VERSION {
            out.push(problem(
                "manifestVersion",
                format!(
                    "unsupported manifest version {} (this Forge reads {MANIFEST_VERSION})",
                    self.manifest_version
                ),
            ));
        }
        if !is_plugin_id(&self.id) {
            out.push(problem(
                "id",
                "must be a lowercase reverse-DNS id like `publisher.plugin-name`",
            ));
        } else if self.id.starts_with(RESERVED_PREFIX) {
            out.push(problem(
                "id",
                "ids starting with `forge.` are reserved for built-in plugins",
            ));
        }
        if self.name.trim().is_empty() || self.name.len() > 64 {
            out.push(problem("name", "must be 1 to 64 characters"));
        }
        if semver::Version::parse(&self.version).is_err() {
            out.push(problem("version", "must be a SemVer version like `1.0.0`"));
        }
        match semver::VersionReq::parse(&self.engines.forge) {
            Err(e) => out.push(problem("engines.forge", format!("invalid range: {e}"))),
            Ok(req) => {
                if let Ok(v) = semver::Version::parse(forge_version) {
                    if !req.matches(&v) {
                        out.push(problem(
                            "engines.forge",
                            format!(
                                "needs Forge {}, this is Forge {forge_version}",
                                self.engines.forge
                            ),
                        ));
                    }
                }
            }
        }
        if let Some(main) = &self.main {
            if !is_package_path(main) || !main.ends_with(".js") {
                out.push(problem(
                    "main",
                    "must be a relative `.js` path inside the package",
                ));
            }
        }
        if let Some(backend) = &self.backend {
            match backend {
                BackendSpec::Wasm { module } if !is_package_path(module) => out.push(problem(
                    "backend.module",
                    "must be a relative path inside the package",
                )),
                BackendSpec::Process { command, .. } if !is_program_name(command) => out.push(
                    problem("backend.command", "must be a program name without a path"),
                ),
                _ => {}
            }
        }
        self.validate_permissions(&mut out);
        self.validate_contributions(&mut out);
        self.validate_activation(&mut out);
        out
    }

    fn owns(&self, id: &str) -> bool {
        id.len() > self.id.len() + 1
            && id.starts_with(&self.id)
            && id.as_bytes()[self.id.len()] == b'.'
    }

    fn validate_permissions(&self, out: &mut Vec<Problem>) {
        let p = &self.permissions;
        for (kind, list) in [("read", &p.fs.read), ("write", &p.fs.write)] {
            for (i, scope) in list.iter().enumerate() {
                if crate::permissions::FsScope::parse(scope).is_none() {
                    out.push(problem(
                        format!("permissions.fs.{kind}[{i}]"),
                        "must be `${project}` or `${pluginData}`, optionally followed by a sub folder",
                    ));
                }
            }
        }
        for (i, host) in p.network.iter().enumerate() {
            if !is_host_pattern(host) {
                out.push(problem(
                    format!("permissions.network[{i}]"),
                    "must be a host name like `api.example.com` or `*.example.com`",
                ));
            }
        }
        for (i, program) in p.process.iter().enumerate() {
            if !is_program_name(program) {
                out.push(problem(
                    format!("permissions.process[{i}]"),
                    "must be a program name without a path",
                ));
            }
        }
        for (i, key) in p.settings.iter().enumerate() {
            let base = key.strip_suffix(".*").unwrap_or(key);
            if base.is_empty() || base.split('.').any(|s| s.is_empty() || s == "*") {
                out.push(problem(
                    format!("permissions.settings[{i}]"),
                    "must be a setting key or a `prefix.*` pattern",
                ));
            }
        }
    }

    fn validate_contributions(&self, out: &mut Vec<Problem>) {
        let c = &self.contributes;
        let own = |field: String, id: &str, out: &mut Vec<Problem>| {
            if !self.owns(id) {
                out.push(problem(
                    field,
                    format!("must start with the plugin id: `{}.…`", self.id),
                ));
            }
        };
        let mut seen = std::collections::HashSet::new();
        for (i, cmd) in c.commands.iter().enumerate() {
            own(format!("contributes.commands[{i}].id"), &cmd.id, out);
            if cmd.title.trim().is_empty() {
                out.push(problem(
                    format!("contributes.commands[{i}].title"),
                    "must not be empty",
                ));
            }
            if !seen.insert(cmd.id.as_str()) {
                out.push(problem(
                    format!("contributes.commands[{i}].id"),
                    "is declared twice",
                ));
            }
        }
        let command_known = |id: &str| c.commands.iter().any(|cmd| cmd.id == id);
        for (i, kb) in c.keybindings.iter().enumerate() {
            if !command_known(&kb.command) {
                out.push(problem(
                    format!("contributes.keybindings[{i}].command"),
                    "is not one of the plugin's commands",
                ));
            }
            let platform = if kb.keymap.as_deref() == Some("macos") {
                forge_keymap::Platform::Mac
            } else {
                forge_keymap::Platform::Other
            };
            if let Err(e) = forge_keymap::shortcut::parse_shortcut(&kb.key, platform) {
                out.push(problem(format!("contributes.keybindings[{i}].key"), e));
            }
        }
        for (i, s) in c.status_items.iter().enumerate() {
            own(format!("contributes.statusItems[{i}].id"), &s.id, out);
            if let Some(cmd) = &s.command {
                if !command_known(cmd) {
                    out.push(problem(
                        format!("contributes.statusItems[{i}].command"),
                        "is not one of the plugin's commands",
                    ));
                }
            }
        }
        for (i, tw) in c.tool_windows.iter().enumerate() {
            own(format!("contributes.toolWindows[{i}].id"), &tw.id, out);
        }
        for (i, page) in c.settings_pages.iter().enumerate() {
            own(format!("contributes.settingsPages[{i}].id"), &page.id, out);
        }
        for (i, s) in c.settings.iter().enumerate() {
            own(format!("contributes.settings[{i}].key"), &s.key, out);
            if !c.settings_pages.iter().any(|p| p.id == s.group) {
                out.push(problem(
                    format!("contributes.settings[{i}].group"),
                    "must be one of the plugin's settings pages",
                ));
            }
            if s.project {
                out.push(problem(
                    format!("contributes.settings[{i}].project"),
                    "plugin settings are user settings; project settings are not supported",
                ));
            }
            if let Err(e) = s.kind.validate(&s.default) {
                out.push(problem(format!("contributes.settings[{i}].default"), e));
            }
        }
        for (i, lang) in c.languages.iter().enumerate() {
            if lang.id.is_empty() || lang.extensions.is_empty() && lang.file_names.is_empty() {
                out.push(problem(
                    format!("contributes.languages[{i}]"),
                    "needs an id and at least one extension or file name",
                ));
            }
        }
    }

    fn validate_activation(&self, out: &mut Vec<Problem>) {
        if self.main.is_none() && !self.activation_events.is_empty() {
            out.push(problem(
                "activationEvents",
                "a plugin without `main` has nothing to activate",
            ));
        }
        for (i, event) in self.activation_events.iter().enumerate() {
            let field = format!("activationEvents[{i}]");
            match ActivationEvent::parse(event) {
                None => out.push(problem(
                    field,
                    format!("unknown activation event `{event}`"),
                )),
                Some(ActivationEvent::OnCommand(id))
                    if !self.contributes.commands.iter().any(|c| c.id == id) =>
                {
                    out.push(problem(field, "is not one of the plugin's commands"))
                }
                Some(ActivationEvent::OnView(id))
                    if !self.contributes.tool_windows.iter().any(|t| t.id == id) =>
                {
                    out.push(problem(field, "is not one of the plugin's tool windows"))
                }
                _ => {}
            }
        }
    }
}

/// A parsed activation event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivationEvent {
    OnStartup,
    OnCommand(String),
    OnLanguage(String),
    OnView(String),
    WorkspaceContains(String),
}

impl ActivationEvent {
    pub fn parse(s: &str) -> Option<Self> {
        if s == "onStartup" {
            return Some(Self::OnStartup);
        }
        let (kind, arg) = s.split_once(':')?;
        if arg.is_empty() {
            return None;
        }
        Some(match kind {
            "onCommand" => Self::OnCommand(arg.into()),
            "onLanguage" => Self::OnLanguage(arg.into()),
            "onView" => Self::OnView(arg.into()),
            "workspaceContains" if is_package_path(arg) => Self::WorkspaceContains(arg.into()),
            _ => return None,
        })
    }
}

/// `publisher.name` with lowercase ASCII letters, digits and `-`, at most 64 characters.
pub fn is_plugin_id(id: &str) -> bool {
    let parts: Vec<&str> = id.split('.').collect();
    id.len() <= 64
        && parts.len() >= 2
        && parts.iter().all(|p| {
            !p.is_empty()
                && !p.starts_with('-')
                && !p.ends_with('-')
                && p.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
}

/// A relative path with `/` separators that stays inside its root.
pub fn is_package_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.contains(':')
        && path
            .split('/')
            .all(|p| !p.is_empty() && p != "." && p != "..")
}

fn is_program_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        && !name.starts_with('.')
}

fn is_host_pattern(host: &str) -> bool {
    let rest = host.strip_prefix("*.").unwrap_or(host);
    let labels: Vec<&str> = rest.split('.').collect();
    labels.len() >= 2
        && labels.iter().all(|l| {
            !l.is_empty()
                && l.len() <= 63
                && l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                && !l.starts_with('-')
        })
        || host == "localhost"
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub const WORD_COUNT: &str = r#"{
      "manifestVersion": 1,
      "id": "example.word-count",
      "name": "Word Count",
      "version": "1.0.0",
      "publisher": "Example",
      "description": "Counts words.",
      "engines": { "forge": ">=0.1.0" },
      "main": "dist/plugin.js",
      "activationEvents": ["onStartup", "onCommand:example.word-count.show"],
      "permissions": { "editor": ["read"] },
      "contributes": {
        "commands": [{ "id": "example.word-count.show", "title": "Show Word Count", "category": "Word Count" }],
        "keybindings": [{ "command": "example.word-count.show", "key": "Ctrl+Alt+Shift+W" }],
        "statusItems": [{ "id": "example.word-count.status", "command": "example.word-count.show" }],
        "settingsPages": [{ "id": "example.word-count.page", "title": "Word Count" }],
        "settings": [{
          "key": "example.word-count.includeCode", "title": "Count words in code files",
          "type": "bool", "default": false, "group": "example.word-count.page"
        }]
      }
    }"#;

    #[test]
    fn the_example_manifest_is_valid() {
        let m = PluginManifest::parse(WORD_COUNT).unwrap();
        assert_eq!(m.validate("0.1.0"), vec![]);
        assert_eq!(
            m.contributes.status_items[0].alignment,
            StatusAlignment::Right
        );
        assert_eq!(m.permissions.editor, vec![EditorAccess::Read]);
    }

    #[test]
    fn reports_every_problem_with_its_field() {
        let mut m = PluginManifest::parse(WORD_COUNT).unwrap();
        m.manifest_version = 2;
        m.id = "forge.evil".into();
        m.version = "one".into();
        m.engines.forge = ">=9.0.0".into();
        m.main = Some("../x.js".into());
        m.contributes.commands.push(CommandContribution {
            id: "GotoClass".into(),
            title: "Hijack".into(),
            category: None,
        });
        m.permissions.network = vec!["https://x.com".into()];
        m.permissions.process = vec!["/bin/sh".into()];
        m.permissions.fs.read = vec!["/etc".into()];
        m.activation_events.push("onSomething".into());
        let fields: Vec<String> = m.validate("0.1.0").into_iter().map(|p| p.field).collect();
        for f in [
            "manifestVersion",
            "id",
            "version",
            "engines.forge",
            "main",
            "permissions.network[0]",
            "permissions.process[0]",
            "permissions.fs.read[0]",
            "activationEvents[2]",
        ] {
            assert!(fields.iter().any(|x| x == f), "{f} missing in {fields:?}");
        }
        // The id change makes every contribution foreign, and the built-in action id too.
        assert!(fields.contains(&"contributes.commands[1].id".to_string()));
    }

    #[test]
    fn checks_references_and_shortcuts() {
        let mut m = PluginManifest::parse(WORD_COUNT).unwrap();
        m.contributes.keybindings[0].key = "Ctrl+Nope+".into();
        m.contributes.keybindings.push(KeybindingContribution {
            command: "example.word-count.missing".into(),
            key: "Ctrl+1".into(),
            when: None,
            keymap: None,
        });
        m.contributes.settings[0].group = "editor.general".into();
        m.contributes.settings[0].default = serde_json::json!("yes");
        m.activation_events = vec!["onView:example.word-count.none".into()];
        let fields: Vec<String> = m.validate("0.1.0").into_iter().map(|p| p.field).collect();
        assert_eq!(
            fields,
            [
                "contributes.keybindings[0].key",
                "contributes.keybindings[1].command",
                "contributes.settings[0].group",
                "contributes.settings[0].default",
                "activationEvents[0]",
            ]
        );
    }

    #[test]
    fn ids_paths_and_hosts() {
        assert!(is_plugin_id("acme.tools"));
        assert!(is_plugin_id("acme.my-tool2"));
        assert!(!is_plugin_id("acme"));
        assert!(!is_plugin_id("Acme.Tools"));
        assert!(!is_plugin_id("acme..tools"));
        assert!(is_package_path("dist/plugin.js"));
        assert!(!is_package_path("dist/../../x.js"));
        assert!(!is_package_path("C:/x.js"));
        assert!(is_host_pattern("api.example.com"));
        assert!(is_host_pattern("*.example.com"));
        assert!(is_host_pattern("localhost"));
        assert!(!is_host_pattern("*"));
        assert!(!is_host_pattern("*.com"));
        assert!(!is_host_pattern("example.com/path"));
        assert_eq!(
            ActivationEvent::parse("onLanguage:markdown"),
            Some(ActivationEvent::OnLanguage("markdown".into()))
        );
        assert_eq!(ActivationEvent::parse("workspaceContains:../x"), None);
        assert_eq!(ActivationEvent::parse("onCommand:"), None);
    }

    #[test]
    fn backend_declarations_parse() {
        let mut v: serde_json::Value = serde_json::from_str(WORD_COUNT).unwrap();
        v["backend"] = serde_json::json!({ "kind": "process", "command": "word-count-server" });
        let m: PluginManifest = serde_json::from_value(v.clone()).unwrap();
        assert!(m.validate("0.1.0").is_empty());
        v["backend"] = serde_json::json!({ "kind": "native", "library": "x.so" });
        assert!(serde_json::from_value::<PluginManifest>(v).is_err());
    }
}
