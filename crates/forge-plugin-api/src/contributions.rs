//! Contribution types: everything a plugin can add to the IDE.
//!
//! All types are plain serde data so manifests can be written as TOML/JSON and the UI can
//! render them directly (settings pages, keymap, tool window stripes, ...). Field names are
//! serialized in camelCase to match the TypeScript side.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Everything a plugin adds to the IDE.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Contributions {
    /// File extensions this plugin provides language support for (without dot).
    #[serde(default)]
    pub languages: Vec<String>,
    /// File types the plugin recognises, with the extensions and names that select them.
    #[serde(default)]
    pub file_types: Vec<FileTypeContribution>,
    #[serde(default)]
    pub commands: Vec<CommandContribution>,
    #[serde(default)]
    pub keybindings: Vec<KeybindingContribution>,
    #[serde(default)]
    pub tool_windows: Vec<ToolWindowContribution>,
    /// Typed settings, rendered by the Settings dialog and stored in the layered settings.
    #[serde(default)]
    pub settings: Vec<SettingSpec>,
    #[serde(default)]
    pub lsp_servers: Vec<LspServerSpec>,
    #[serde(default)]
    pub run_configuration_types: Vec<RunConfigurationType>,
    /// Debug adapters (Debug Adapter Protocol) the shared debugger can start or attach with.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub debug_adapters: Vec<DebugAdapterContribution>,
    /// Test frameworks for the shared test runner (detection, command lines, gutter
    /// discovery, result format, Rerun Failed Tests, coverage); each is also a run
    /// configuration type.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub test_frameworks: Vec<TestFrameworkContribution>,
    #[serde(default)]
    pub ai_providers: Vec<AiProviderContribution>,
    #[serde(default)]
    pub chat_agents: Vec<ChatAgentContribution>,
    #[serde(default)]
    pub code_generators: Vec<CodeGeneratorContribution>,
    /// Entries for the main menu, e.g. `Tools > Laravel`. The UI sends the contributions of
    /// active plugins when the menu opens.
    #[serde(default)]
    pub main_menu: Vec<MainMenuContribution>,
    /// Inspections (Settings | Editor | Inspections, Inspect Code); see `forge-inspections`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inspections: Vec<InspectionContribution>,
    /// Map problems of external tools (a diagnostics source and code) onto inspections.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inspection_mappings: Vec<InspectionMappingContribution>,
}

/// An inspection declared as data (`[[contributions.inspections]]`). Defined by the pure
/// `forge-inspections` crate, which also holds profiles and runs them.
pub use forge_inspections::InspectionSpec as InspectionContribution;

/// `[[contributions.inspectionMappings]]`: problems published with `source` (and `code`)
/// belong to `inspection`.
pub use forge_inspections::ToolMapping as InspectionMappingContribution;

/// Entries a plugin adds to a main menu path:
///
/// ```toml
/// [[contributions.mainMenu]]
/// path = "Tools"
/// items = [{ type = "group", label = "_Laravel", items = [{ type = "action", id = "Laravel.ShowRoutes" }] }]
/// ```
///
/// Defined by the pure `forge-keymap` crate, which also resolves the menu.
pub use forge_keymap::MenuContribution as MainMenuContribution;

/// A file type (PHPStorm's Settings | Editor | File Types): how files are recognised, what
/// the status bar calls them and their indentation. Syntax highlighting is registered by the
/// UI under the same `id`. Defined by the pure `forge-filetypes` crate, which also matches
/// files against these contributions.
pub use forge_filetypes::FileTypeSpec as FileTypeContribution;

/// A test framework declared as data (`[[contributions.testFrameworks]]`). Defined by the
/// pure `forge-testing` crate, which also builds its commands and discovers its tests.
pub use forge_testing::TestFrameworkSpec as TestFrameworkContribution;

/// An action with a PHPStorm-style id (e.g. `settings.open`), invokable from Find Action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandContribution {
    pub id: String,
    pub title: String,
    /// Menu / Find Action category, e.g. "File" or "VCS".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
}

/// Default key binding for a command. Keys use `Ctrl+Alt+S` notation; `Meta` is Cmd on macOS.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeybindingContribution {
    pub command: String,
    pub key: String,
    /// Optional context the binding is active in, e.g. `editorFocus`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
    /// Keymap this binding belongs to (`default` or `macos`); `None` means all keymaps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keymap: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolWindowAnchor {
    Left,
    Right,
    Bottom,
}

/// A panel on one of the tool window stripes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolWindowContribution {
    pub id: String,
    pub title: String,
    pub anchor: ToolWindowAnchor,
    /// lucide icon name, e.g. `git-commit-horizontal`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default)]
    pub order: i32,
}

/// One option of an [`SettingType::Enum`] setting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnumOption {
    pub value: String,
    pub label: String,
}

/// Value type of a setting, including the constraints the UI uses to build its editor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum SettingType {
    Bool,
    Int {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<i64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<i64>,
    },
    Number {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        step: Option<f64>,
    },
    String,
    Enum {
        options: Vec<EnumOption>,
    },
    /// A file or directory path (rendered with a browse affordance).
    Path,
    /// An ordered list of strings, e.g. PHP include paths.
    List,
    /// An ordered list of tables with string values, e.g. template delimiters per
    /// (template language, data language) pair. Settings pages render them with a custom
    /// editor. No two entries may share the values of all `unique` fields.
    Objects {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        unique: Vec<String>,
    },
}

impl SettingType {
    /// Check that `value` fits this type and its constraints.
    pub fn validate(&self, value: &Value) -> Result<(), String> {
        match (self, value) {
            (Self::Bool, Value::Bool(_)) => Ok(()),
            (Self::Int { min, max }, Value::Number(n)) => {
                let n = n.as_i64().ok_or("expected an integer")?;
                check_range(n, *min, *max)
            }
            (Self::Number { min, max, .. }, Value::Number(n)) => {
                let n = n.as_f64().ok_or("expected a number")?;
                check_range(n, *min, *max)
            }
            (Self::String | Self::Path, Value::String(_)) => Ok(()),
            (Self::Enum { options }, Value::String(s)) => {
                if options.iter().any(|o| &o.value == s) {
                    Ok(())
                } else {
                    Err(format!("`{s}` is not one of the allowed values"))
                }
            }
            (Self::List, Value::Array(items)) if items.iter().all(Value::is_string) => Ok(()),
            (Self::List, _) => Err("expected a list of strings".into()),
            (Self::Objects { unique }, Value::Array(items)) => validate_objects(items, unique),
            (Self::Objects { .. }, _) => Err("expected a list of tables".into()),
            (ty, _) => Err(format!("expected a value of type {}", ty.name())),
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Bool => "bool",
            Self::Int { .. } => "int",
            Self::Number { .. } => "number",
            Self::String => "string",
            Self::Enum { .. } => "enum",
            Self::Path => "path",
            Self::List => "list",
            Self::Objects { .. } => "objects",
        }
    }
}

/// A list of tables with string values. Every entry needs a non-empty value for each
/// `unique` field, and no two entries may share all of them.
fn validate_objects(items: &[Value], unique: &[String]) -> Result<(), String> {
    let mut seen: Vec<Vec<&str>> = Vec::new();
    for item in items {
        let table = item.as_object().ok_or("expected a list of tables")?;
        if !table.values().all(Value::is_string) {
            return Err("table values must be strings".into());
        }
        let mut key = Vec::with_capacity(unique.len());
        for field in unique {
            match table.get(field).and_then(Value::as_str).map(str::trim) {
                Some(v) if !v.is_empty() => key.push(v),
                _ => return Err(format!("every entry needs a `{field}`")),
            }
        }
        if !unique.is_empty() && seen.contains(&key) {
            return Err(format!("duplicate entry for {}", key.join(" + ")));
        }
        seen.push(key);
    }
    Ok(())
}

fn check_range<T: PartialOrd + std::fmt::Display>(
    n: T,
    min: Option<T>,
    max: Option<T>,
) -> Result<(), String> {
    if let Some(min) = min.filter(|m| n < *m) {
        return Err(format!("must be at least {min}"));
    }
    if let Some(max) = max.filter(|m| n > *m) {
        return Err(format!("must be at most {max}"));
    }
    Ok(())
}

/// A typed setting. `key` is dotted (`editor.font.size`) and doubles as the TOML path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingSpec {
    pub key: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(flatten)]
    pub kind: SettingType,
    pub default: Value,
    /// Settings page id this setting is shown on, e.g. `editor.font`.
    pub group: String,
    /// Optional titled section inside the page (PHPStorm's titled separators).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    /// Stored per project (`.forge/settings.toml`) rather than per user when a project is open.
    /// Settings without it are IDE settings: a project's `.forge/settings.toml` never decides
    /// them (see [`apply_setting_specs`]).
    #[serde(default)]
    pub project: bool,
    /// Not shown as a row of its page: a dialog of the page edits it (e.g. the signing key
    /// chosen in "Configure GPG Key…").
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hidden: bool,
    #[serde(default)]
    pub order: i32,
}

/// Register `specs` with `settings`: their defaults, and every setting that is not
/// `project = true` as user-only, so the project layer never decides it (a cloned
/// repository must not choose the Git executable, the npm registry or the update feed).
pub fn apply_setting_specs<'a>(
    settings: &mut forge_core::Settings,
    specs: impl IntoIterator<Item = &'a SettingSpec>,
) {
    let specs: Vec<&SettingSpec> = specs.into_iter().collect();
    settings.set_defaults(specs.iter().map(|s| (s.key.clone(), s.default.clone())));
    settings.set_user_only(specs.iter().filter(|s| !s.project).map(|s| s.key.clone()));
}

/// How to start a language server for some file types.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LspServerSpec {
    pub id: String,
    /// LSP language id, e.g. `php`.
    pub language_id: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    /// File extensions (without dot) handled by this server.
    pub file_extensions: Vec<String>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub initialization_options: Value,
}

/// How the debugger talks to a debug adapter.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DebugAdapterTransport {
    /// Start `command` and speak DAP over its stdin/stdout (`lldb-dap`, `debugpy-adapter`).
    #[default]
    Stdio,
    /// Start `command` with `${port}` in its arguments, then connect to that port
    /// (`dlv dap --listen 127.0.0.1:${port}`).
    Connect,
    /// Listen on a free local port and start `command` with `${port}`; the adapter
    /// connects back (`js-debug` style).
    Listen,
}

/// A debug adapter (Debug Adapter Protocol), declared in a manifest:
///
/// ```toml
/// [[contributions.debugAdapters]]
/// type = "lldb"
/// label = "LLDB"
/// command = "lldb-dap"
/// commandSetting = "debugger.lldb.path"
/// languages = ["c", "cpp", "rs"]
/// launch = { program = "${program}", args = "${args}", cwd = "${cwd}", env = "${env}", stopOnEntry = "${stopOnEntry}" }
/// attach = { pid = "${pid}" }
/// coreDump = { program = "${program}", coreFile = "${core}" }
/// ```
///
/// The `launch`, `attach` and `coreDump` tables are templates of the request arguments.
/// A string that is exactly `"${args}"`, `"${env}"`, `"${pid}"` or `"${stopOnEntry}"`
/// becomes an array, object, number or bool; other `${name}` placeholders are replaced in
/// strings; keys whose placeholder has no value are dropped. Without `attach` the adapter
/// cannot attach to processes, without `coreDump` it cannot open core dumps.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugAdapterContribution {
    /// Adapter type, also the `type` of launch configurations (`lldb`, `debugpy`, `go`).
    pub r#type: String,
    pub label: String,
    /// Executable of the adapter; empty for adapters that are only attached to over TCP.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub command: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    /// Setting that overrides `command` with a user-chosen path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command_setting: Option<String>,
    #[serde(default)]
    pub transport: DebugAdapterTransport,
    /// File extensions (without dot) whose line breakpoints the adapter handles.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub languages: Vec<String>,
    /// `launch` request template; without it the adapter cannot launch programs.
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub launch: Value,
    /// `attach` request template for attaching to a local process (`${pid}`).
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub attach: Value,
    /// Template for debugging a core dump (`${program}`, `${core}`).
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub core_dump: Value,
    /// Request the core dump template is sent with (`attach` or `launch`, default `launch`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub core_dump_request: Option<String>,
    /// JSON schema of extra `launch` options, for forms and validation in the UI.
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub launch_schema: Value,
    /// JSON schema of extra `attach` options.
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub attach_schema: Value,
}

/// A kind of Run/Debug configuration, e.g. "PHP Script" or "PHPUnit".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunConfigurationType {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Whether configurations of this type can be started with the debugger.
    #[serde(default)]
    pub debuggable: bool,
    /// Form fields of the type, shown in the Run/Debug Configurations dialog and stored in
    /// the configuration's `options`. Types with a UI registration may leave this empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<RunConfigurationField>,
    /// Command line for manifest-only types. Each entry is a literal or a placeholder:
    /// `${interpreter}`, `${interpreterOptions}`, `${arguments}` or `${<field key>}`.
    /// Placeholders of the list-valued fields (`interpreterOptions`, `arguments`) expand to
    /// several arguments; empty values are dropped.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub command: Vec<String>,
    /// Interpreter used when a configuration does not set one (e.g. `lua`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_interpreter: Option<String>,
    /// Setting that holds the project's default interpreter (e.g. `lua.interpreter`); it
    /// wins over `default_interpreter` when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interpreter_setting: Option<String>,
}

/// Value type of a [`RunConfigurationField`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunConfigurationFieldKind {
    #[default]
    String,
    /// A file or directory (rendered with a browse button).
    Path,
    /// Multi-line text, e.g. an inline script.
    Text,
    Enum,
    Bool,
}

/// One form field of a run configuration type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunConfigurationField {
    /// Key in the configuration's `options`.
    pub key: String,
    pub label: String,
    #[serde(default)]
    pub kind: RunConfigurationFieldKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<EnumOption>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub default: String,
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AiProviderKind {
    Anthropic,
    OpenaiCompatible,
    Gemini,
    /// A local CLI agent (Claude Code, Gemini CLI, ...).
    Cli,
}

/// A model provider usable by AI features.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiProviderContribution {
    pub id: String,
    pub name: String,
    pub kind: AiProviderKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,
    #[serde(default)]
    pub requires_api_key: bool,
}

/// An agent selectable in the AI Chat tool window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatAgentContribution {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Provider id the agent talks to (see [`AiProviderContribution`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
}

/// An entry of the Generate (Alt+Insert) menu.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeGeneratorContribution {
    pub id: String,
    pub title: String,
    /// Language ids the generator applies to.
    #[serde(default)]
    pub languages: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn setting_spec_round_trips_with_flattened_type() {
        let spec: SettingSpec = serde_json::from_value(json!({
            "key": "editor.font.size", "title": "Size", "type": "int", "min": 6, "max": 72,
            "default": 14, "group": "editor.font"
        }))
        .unwrap();
        assert_eq!(
            spec.kind,
            SettingType::Int {
                min: Some(6),
                max: Some(72)
            }
        );
        let back = serde_json::to_value(&spec).unwrap();
        assert_eq!(back["type"], "int");
        assert_eq!(back["max"], 72);
    }

    #[test]
    fn validates_values_against_type() {
        let int = SettingType::Int {
            min: Some(1),
            max: Some(10),
        };
        assert!(int.validate(&json!(5)).is_ok());
        assert!(int.validate(&json!(11)).is_err());
        assert!(int.validate(&json!("5")).is_err());
        let level = SettingType::Enum {
            options: vec![EnumOption {
                value: "8.3".into(),
                label: "8.3".into(),
            }],
        };
        assert!(level.validate(&json!("8.3")).is_ok());
        assert!(level.validate(&json!("5.6")).is_err());
        assert!(SettingType::List.validate(&json!(["a", "b"])).is_ok());
        assert!(SettingType::List.validate(&json!([1])).is_err());
        let pairs = SettingType::Objects {
            unique: vec!["template".into(), "language".into()],
        };
        let smarty = json!({ "template": "smarty", "language": "latex", "left": "<<" });
        let twig = json!({ "template": "twig", "language": "latex", "left": "<<" });
        assert!(pairs.validate(&json!([smarty, twig])).is_ok());
        assert!(pairs
            .validate(&json!([smarty, smarty]))
            .unwrap_err()
            .contains("duplicate"));
        assert!(pairs.validate(&json!([{ "template": 1 }])).is_err());
        assert_eq!(
            pairs.validate(&json!([smarty, { "template": "twig", "language": " " }])),
            Err("every entry needs a `language`".into())
        );
        assert_eq!(
            pairs.validate(&json!([smarty, smarty])),
            Err("duplicate entry for smarty + latex".into())
        );
        let free = SettingType::Objects { unique: vec![] };
        assert!(free.validate(&json!([{}, {}])).is_ok());
        assert!(pairs.validate(&json!(["smarty"])).is_err());
        let num = SettingType::Number {
            min: Some(1.0),
            max: Some(3.0),
            step: None,
        };
        assert!(num.validate(&json!(1.4)).is_ok());
        assert!(num.validate(&json!(2)).is_ok());
    }

    #[test]
    fn object_settings_parse_from_toml() {
        let spec: SettingSpec = toml::from_str(
            r#"
            key = "templates.delimiters"
            title = "Delimiters"
            type = "objects"
            unique = ["template", "language"]
            default = [{ template = "smarty", language = "latex", left = "<<", right = ">>" }]
            group = "lang.templates"
            "#,
        )
        .unwrap();
        assert_eq!(spec.kind.name(), "objects");
        assert_eq!(spec.default[0]["left"], "<<");
        assert!(spec.kind.validate(&spec.default).is_ok());
    }

    #[test]
    fn contributions_parse_from_toml() {
        let c: Contributions = toml::from_str(
            r#"
            languages = ["php"]
            [[commands]]
            id = "php.run"
            title = "Run PHP Script"
            [[settings]]
            key = "php.languageLevel"
            title = "PHP language level"
            type = "enum"
            options = [{ value = "8.3", label = "8.3" }]
            default = "8.3"
            group = "lang.php"
            project = true
            [[lspServers]]
            id = "php-lsp"
            languageId = "php"
            command = "forge"
            args = ["--php-lsp", "--stdio"]
            fileExtensions = ["php"]
            "#,
        )
        .unwrap();
        assert_eq!(c.commands[0].id, "php.run");
        assert!(c.run_configuration_types.is_empty());
        assert!(c.settings[0].project);
        assert_eq!(c.lsp_servers[0].args, vec!["--php-lsp", "--stdio"]);
    }

    #[test]
    fn test_frameworks_are_declared_as_data() {
        let c: Contributions = toml::from_str(
            r#"
            [[testFrameworks]]
            id = "busted"
            name = "busted"
            testFiles = ["*_spec.lua"]
            program = "busted"
            args = ["--output=TAP"]
            report = "tap"
            [testFrameworks.scopes]
            all = []
            file = ["${file}"]
            "#,
        )
        .unwrap();
        let f = &c.test_frameworks[0];
        assert_eq!(f.report, forge_output::TestReport::Tap);
        assert_eq!(f.scopes["file"], ["${file}"]);
        assert!(f.is_test_file("spec/cart_spec.lua"));
    }

    #[test]
    fn run_configuration_types_declare_fields_and_command() {
        let c: Contributions = toml::from_str(
            r#"
            [[runConfigurationTypes]]
            id = "lua-script"
            name = "Lua Script"
            defaultInterpreter = "lua"
            interpreterSetting = "lua.interpreter"
            command = ["${interpreter}", "${interpreterOptions}", "${file}", "${arguments}"]
            [[runConfigurationTypes.fields]]
            key = "file"
            label = "File"
            kind = "path"
            required = true
            "#,
        )
        .unwrap();
        let t = &c.run_configuration_types[0];
        assert_eq!(t.default_interpreter.as_deref(), Some("lua"));
        assert_eq!(t.interpreter_setting.as_deref(), Some("lua.interpreter"));
        assert_eq!(t.command.len(), 4);
        assert_eq!(t.fields[0].kind, RunConfigurationFieldKind::Path);
        assert!(t.fields[0].required);
    }
}
