//! The plugin registry: which plugins are installed, which are enabled, and lookups over
//! the contributions of the enabled ones.

use forge_core::{ForgeError, Result};
use serde::{Deserialize, Serialize};

use crate::contributions::*;
use crate::{Plugin, PluginManifest};

/// A plugin as declared in a manifest file: metadata plus contributions.
///
/// In TOML the manifest fields sit at the top level and contributions under
/// `[contributions]`, e.g. `id = "forge.php"` … `[[contributions.settings]]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginDescriptor {
    #[serde(flatten)]
    pub manifest: PluginManifest,
    #[serde(default)]
    pub contributions: Contributions,
}

impl PluginDescriptor {
    pub fn from_toml(src: &str) -> Result<Self> {
        toml::from_str(src).map_err(|e| ForgeError::Other(format!("invalid plugin manifest: {e}")))
    }

    /// Parse a manifest file holding either one plugin, or several as `[[plugin]]` tables.
    pub fn parse_many(src: &str) -> Result<Vec<Self>> {
        #[derive(Deserialize)]
        struct Many {
            plugin: Vec<PluginDescriptor>,
        }
        let table: toml::Table = toml::from_str(src)
            .map_err(|e| ForgeError::Other(format!("invalid plugin manifest: {e}")))?;
        if table.contains_key("plugin") {
            let many: Many = table
                .try_into()
                .map_err(|e| ForgeError::Other(format!("invalid plugin manifest: {e}")))?;
            Ok(many.plugin)
        } else {
            Self::from_toml(src).map(|d| vec![d])
        }
    }
}

/// A registered plugin with its enablement state, as listed in the Plugins settings page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginInfo {
    #[serde(flatten)]
    pub manifest: PluginManifest,
    pub enabled: bool,
    pub contributions: Contributions,
}

#[derive(Default)]
pub struct PluginRegistry {
    /// Plugins in registration order (the order the UI lists them in).
    plugins: Vec<PluginInfo>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a plugin implemented in Rust.
    pub fn register(&mut self, plugin: &dyn Plugin) -> Result<()> {
        self.register_descriptor(PluginDescriptor {
            manifest: plugin.manifest(),
            contributions: plugin.contributions(),
        })
    }

    /// Register a declarative plugin. Plugins start enabled; ids must be unique.
    pub fn register_descriptor(&mut self, d: PluginDescriptor) -> Result<()> {
        if self.get(&d.manifest.id).is_some() {
            return Err(ForgeError::Other(format!(
                "plugin `{}` is already registered",
                d.manifest.id
            )));
        }
        self.plugins.push(PluginInfo {
            manifest: d.manifest,
            enabled: true,
            contributions: d.contributions,
        });
        Ok(())
    }

    pub fn list(&self) -> &[PluginInfo] {
        &self.plugins
    }

    pub fn get(&self, id: &str) -> Option<&PluginInfo> {
        self.plugins.iter().find(|p| p.manifest.id == id)
    }

    pub fn is_enabled(&self, id: &str) -> bool {
        self.get(id).is_some_and(|p| p.enabled)
    }

    /// Enable or disable a plugin. Enabling also enables its dependencies; disabling also
    /// disables plugins that depend on it (transitively). Required plugins cannot be
    /// disabled. Returns the ids whose state changed.
    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> Result<Vec<String>> {
        let plugin = self
            .get(id)
            .ok_or_else(|| ForgeError::Other(format!("unknown plugin `{id}`")))?;
        if !enabled && plugin.manifest.required {
            return Err(ForgeError::Other(format!(
                "plugin `{id}` is required and cannot be disabled"
            )));
        }
        let affected = if enabled {
            self.closure(id, |p| p.manifest.depends.clone())
        } else {
            self.closure(id, |p| self.dependents_of(&p.manifest.id))
        };
        if !enabled {
            if let Some(req) = affected
                .iter()
                .find(|a| self.get(a).is_some_and(|p| p.manifest.required))
            {
                return Err(ForgeError::Other(format!(
                    "plugin `{req}` is required and depends on `{id}`"
                )));
            }
        }
        let mut changed = Vec::new();
        for p in &mut self.plugins {
            if affected.contains(&p.manifest.id) && p.enabled != enabled {
                p.enabled = enabled;
                changed.push(p.manifest.id.clone());
            }
        }
        Ok(changed)
    }

    fn dependents_of(&self, id: &str) -> Vec<String> {
        self.plugins
            .iter()
            .filter(|p| p.manifest.depends.iter().any(|d| d == id))
            .map(|p| p.manifest.id.clone())
            .collect()
    }

    /// `id` plus everything reachable through `next`, ignoring unknown ids.
    fn closure(&self, id: &str, next: impl Fn(&PluginInfo) -> Vec<String>) -> Vec<String> {
        let mut out = vec![id.to_string()];
        let mut i = 0;
        while i < out.len() {
            if let Some(p) = self.get(&out[i]) {
                for n in next(p) {
                    if !out.contains(&n) && self.get(&n).is_some() {
                        out.push(n);
                    }
                }
            }
            i += 1;
        }
        out
    }

    /// Contributions of enabled plugins, with the manifest that contributed them.
    pub fn enabled(&self) -> impl Iterator<Item = &PluginInfo> {
        self.plugins.iter().filter(|p| p.enabled)
    }

    fn collect<'a, T: 'a>(
        &'a self,
        pick: impl Fn(&'a Contributions) -> &'a [T] + 'a,
    ) -> impl Iterator<Item = &'a T> + 'a {
        self.enabled().flat_map(move |p| pick(&p.contributions))
    }

    pub fn commands(&self) -> impl Iterator<Item = &CommandContribution> {
        self.collect(|c| &c.commands)
    }

    pub fn keybindings(&self) -> impl Iterator<Item = &KeybindingContribution> {
        self.collect(|c| &c.keybindings)
    }

    pub fn tool_windows(&self) -> impl Iterator<Item = &ToolWindowContribution> {
        self.collect(|c| &c.tool_windows)
    }

    /// The settings schema of all enabled plugins.
    pub fn settings_schema(&self) -> impl Iterator<Item = &SettingSpec> {
        self.collect(|c| &c.settings)
    }

    /// Schema of every installed plugin, enabled or not. Values of disabled plugins are
    /// kept so re-enabling a plugin restores its configuration.
    pub fn all_settings(&self) -> impl Iterator<Item = &SettingSpec> {
        self.plugins.iter().flat_map(|p| &p.contributions.settings)
    }

    pub fn lsp_servers(&self) -> impl Iterator<Item = &LspServerSpec> {
        self.collect(|c| &c.lsp_servers)
    }

    pub fn run_configuration_types(&self) -> impl Iterator<Item = &RunConfigurationType> {
        self.collect(|c| &c.run_configuration_types)
    }

    pub fn debug_adapters(&self) -> impl Iterator<Item = &DebugAdapterContribution> {
        self.collect(|c| &c.debug_adapters)
    }

    pub fn test_frameworks(&self) -> impl Iterator<Item = &TestFrameworkContribution> {
        self.collect(|c| &c.test_frameworks)
    }

    pub fn ai_providers(&self) -> impl Iterator<Item = &AiProviderContribution> {
        self.collect(|c| &c.ai_providers)
    }

    pub fn chat_agents(&self) -> impl Iterator<Item = &ChatAgentContribution> {
        self.collect(|c| &c.chat_agents)
    }

    pub fn file_types(&self) -> impl Iterator<Item = &FileTypeContribution> {
        self.collect(|c| &c.file_types)
    }

    /// Bundled file types plus the file types of *all* plugins (callers pass the inactive
    /// ones per request, as the UI knows which plugins the open project uses).
    pub fn file_type_registry(&self) -> forge_filetypes::FileTypeRegistry {
        let mut registry = forge_filetypes::FileTypeRegistry::with_builtins();
        for p in &self.plugins {
            for t in &p.contributions.file_types {
                registry.register(t.clone(), Some(&p.manifest.id));
            }
        }
        registry
    }

    pub fn code_generators(&self) -> impl Iterator<Item = &CodeGeneratorContribution> {
        self.collect(|c| &c.code_generators)
    }

    pub fn find_command(&self, id: &str) -> Option<&CommandContribution> {
        self.commands().find(|c| c.id == id)
    }

    pub fn find_setting(&self, key: &str) -> Option<&SettingSpec> {
        self.settings_schema().find(|s| s.key == key)
    }

    /// The language server for a file extension (without dot), first registered wins.
    pub fn lsp_server_for_extension(&self, ext: &str) -> Option<&LspServerSpec> {
        self.lsp_servers().find(|s| {
            s.file_extensions
                .iter()
                .any(|e| e.eq_ignore_ascii_case(ext))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn descriptor(id: &str, depends: &[&str], toml_contrib: &str) -> PluginDescriptor {
        let mut d = PluginDescriptor::from_toml(&format!(
            "id = \"{id}\"\nname = \"{id}\"\nversion = \"1.0\"\npack = \"phpforge\"\n\
             depends = {depends:?}\n{toml_contrib}"
        ))
        .unwrap();
        d.manifest.description = format!("{id} plugin");
        d
    }

    fn registry() -> PluginRegistry {
        let mut r = PluginRegistry::new();
        r.register_descriptor(descriptor(
            "forge.php",
            &[],
            r#"[[contributions.lspServers]]
               id = "intelephense"
               languageId = "php"
               command = "intelephense"
               fileExtensions = ["php", "phtml"]
               [[contributions.commands]]
               id = "php.run"
               title = "Run"
               [[contributions.settings]]
               key = "php.languageLevel"
               title = "Language level"
               type = "string"
               default = "8.3"
               group = "lang.php""#,
        ))
        .unwrap();
        r.register_descriptor(descriptor("forge.laravel", &["forge.php"], ""))
            .unwrap();
        r.register_descriptor(descriptor("forge.lua", &[], ""))
            .unwrap();
        r
    }

    #[test]
    fn parses_debug_adapters() {
        let d = descriptor(
            "forge.native",
            &[],
            r#"[[contributions.debugAdapters]]
               type = "lldb"
               label = "LLDB"
               command = "lldb-dap"
               commandSetting = "debugger.lldb.path"
               languages = ["c", "cpp"]
               launch = { program = "${program}", args = "${args}" }
               attach = { pid = "${pid}" }
               [[contributions.debugAdapters]]
               type = "go"
               label = "Delve"
               command = "dlv"
               args = ["dap", "--listen", "127.0.0.1:${port}"]
               transport = "connect""#,
        );
        let mut r = PluginRegistry::new();
        r.register_descriptor(d).unwrap();
        let adapters: Vec<_> = r.debug_adapters().collect();
        assert_eq!(adapters.len(), 2);
        assert_eq!(adapters[0].r#type, "lldb");
        assert_eq!(adapters[0].transport, DebugAdapterTransport::Stdio);
        assert_eq!(adapters[0].launch["program"], "${program}");
        assert!(adapters[0].core_dump.is_null());
        assert_eq!(adapters[1].transport, DebugAdapterTransport::Connect);
    }

    #[test]
    fn rejects_duplicate_ids() {
        let mut r = registry();
        assert!(r
            .register_descriptor(descriptor("forge.php", &[], ""))
            .is_err());
        assert_eq!(r.list().len(), 3);
    }

    #[test]
    fn looks_up_contributions_of_enabled_plugins_only() {
        let mut r = registry();
        assert_eq!(
            r.lsp_server_for_extension("PHTML").map(|s| s.id.as_str()),
            Some("intelephense")
        );
        assert!(r.find_command("php.run").is_some());
        assert!(r.find_setting("php.languageLevel").is_some());

        r.set_enabled("forge.php", false).unwrap();
        assert!(r.lsp_server_for_extension("php").is_none());
        assert!(r.find_command("php.run").is_none());
        assert_eq!(r.settings_schema().count(), 0);
        assert_eq!(r.all_settings().count(), 1);
    }

    #[test]
    fn disabling_cascades_to_dependents_and_enabling_to_dependencies() {
        let mut r = registry();
        let changed = r.set_enabled("forge.php", false).unwrap();
        assert_eq!(changed, vec!["forge.php", "forge.laravel"]);
        assert!(!r.is_enabled("forge.laravel"));
        assert!(r.is_enabled("forge.lua"));

        let changed = r.set_enabled("forge.laravel", true).unwrap();
        assert_eq!(changed, vec!["forge.php", "forge.laravel"]);
        assert!(r.is_enabled("forge.php"));
    }

    #[test]
    fn required_plugins_cannot_be_disabled() {
        let mut r = registry();
        let mut core = descriptor("forge.core", &[], "");
        core.manifest.required = true;
        r.register_descriptor(core).unwrap();
        assert!(r.set_enabled("forge.core", false).is_err());
        assert!(r.set_enabled("forge.unknown", true).is_err());
    }

    #[test]
    fn parses_project_types() {
        let p = PluginDescriptor::parse_many(
            "id = \"a\"\nname = \"A\"\nversion = \"1\"\npack = \"phpforge\"\nprojectTypes = [\"laravel\"]",
        )
        .unwrap();
        assert_eq!(p[0].manifest.project_types, vec!["laravel"]);
        let json = serde_json::to_value(&p[0].manifest).unwrap();
        assert_eq!(json["projectTypes"][0], "laravel");
    }

    #[test]
    fn parses_single_and_multi_plugin_files() {
        let one = PluginDescriptor::parse_many(
            "id = \"a\"\nname = \"A\"\nversion = \"1\"\npack = \"core\"",
        )
        .unwrap();
        assert_eq!(one.len(), 1);
        let many = PluginDescriptor::parse_many(
            "[[plugin]]\nid = \"a\"\nname = \"A\"\nversion = \"1\"\npack = \"core\"\n\
             [[plugin.contributions.commands]]\nid = \"x\"\ntitle = \"X\"\n\
             [[plugin]]\nid = \"b\"\nname = \"B\"\nversion = \"1\"\npack = \"core\"",
        )
        .unwrap();
        assert_eq!(many.len(), 2);
        assert_eq!(many[0].contributions.commands[0].id, "x");
    }

    struct RustPlugin;
    impl Plugin for RustPlugin {
        fn manifest(&self) -> PluginManifest {
            PluginManifest {
                id: "forge.rust-plugin".into(),
                name: "Rust plugin".into(),
                version: "0.1.0".into(),
                pack: "core".into(),
                ..Default::default()
            }
        }
    }

    #[test]
    fn registers_rust_plugins_and_serializes_for_the_ui() {
        let mut r = PluginRegistry::new();
        r.register(&RustPlugin).unwrap();
        let json = serde_json::to_value(r.list()).unwrap();
        assert_eq!(json[0]["id"], "forge.rust-plugin");
        assert_eq!(json[0]["enabled"], true);
        assert!(json[0]["contributions"]["settings"].is_array());
    }
}
