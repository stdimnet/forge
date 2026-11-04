//! Installed third-party plugins in the user data directory, never in a project:
//!
//! ```text
//! <data dir>/forge/plugins/installed.json     state: versions, enablement, granted permissions
//! <data dir>/forge/plugins/<id>/              the unpacked package
//! <data dir>/forge/plugin-data/<id>/          the plugin's own data (`${pluginData}`)
//! ```

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use forge_plugin_api::{Contributions, PluginDescriptor};

use crate::manifest::{is_package_path, Permissions, PluginManifest};
use crate::package::Package;

const STATE_FILE: &str = "installed.json";
const STATE_FORMAT: u32 = 1;
/// The pack third-party plugins are listed under in Settings | Plugins.
pub const THIRD_PARTY_PACK: &str = "thirdParty";

/// Where an installed plugin came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Source {
    /// Install Plugin from Disk.
    Disk { file: String },
    /// The marketplace whose index listed it.
    Marketplace { index: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledPlugin {
    pub manifest: PluginManifest,
    pub enabled: bool,
    /// The permissions the user accepted on install (the manifest's at that time).
    pub granted: Permissions,
    pub source: Source,
    /// SHA-256 of the installed package.
    pub sha256: String,
    /// Seconds since the Unix epoch.
    pub installed_at: u64,
}

/// A plugin listed by imported settings that is not installed here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginRef {
    pub id: String,
    pub version: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub marketplace: bool,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct State {
    format: u32,
    #[serde(default)]
    plugins: Vec<InstalledPlugin>,
    /// Plugins imported settings asked for (shown in the Marketplace tab).
    #[serde(default)]
    wanted: Vec<PluginRef>,
}

pub struct PluginStore {
    root: PathBuf,
    data_root: PathBuf,
}

fn io(path: &Path, e: std::io::Error) -> String {
    format!("{}: {e}", path.display())
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

impl PluginStore {
    /// A store below `base` (`<base>/plugins`, `<base>/plugin-data`).
    pub fn under(base: &Path) -> Self {
        Self {
            root: base.join("plugins"),
            data_root: base.join("plugin-data"),
        }
    }

    /// The store in the user data directory (`<data dir>/forge`).
    pub fn default_location() -> Option<Self> {
        dirs::data_dir().map(|d| Self::under(&d.join("forge")))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn load(&self) -> Result<State, String> {
        let path = self.root.join(STATE_FILE);
        match std::fs::read(&path) {
            Ok(bytes) => {
                serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(State {
                format: STATE_FORMAT,
                ..State::default()
            }),
            Err(e) => Err(io(&path, e)),
        }
    }

    fn save(&self, state: &State) -> Result<(), String> {
        std::fs::create_dir_all(&self.root).map_err(|e| io(&self.root, e))?;
        let path = self.root.join(STATE_FILE);
        let tmp = self.root.join(format!("{STATE_FILE}.tmp"));
        let json = serde_json::to_vec_pretty(state).map_err(|e| e.to_string())?;
        std::fs::write(&tmp, json).map_err(|e| io(&tmp, e))?;
        std::fs::rename(&tmp, &path).map_err(|e| io(&path, e))
    }

    pub fn list(&self) -> Result<Vec<InstalledPlugin>, String> {
        Ok(self.load()?.plugins)
    }

    pub fn get(&self, id: &str) -> Result<Option<InstalledPlugin>, String> {
        Ok(self.list()?.into_iter().find(|p| p.manifest.id == id))
    }

    fn dir_of(&self, id: &str) -> PathBuf {
        self.root.join(id)
    }

    /// The plugin's `${pluginData}` folder (created on demand).
    pub fn data_dir(&self, id: &str) -> PathBuf {
        self.data_root.join(id)
    }

    /// Install (or update) a package, granting the permissions its manifest declares. The
    /// files are unpacked next to the old version and swapped in, so a failed install
    /// leaves the old version working. An update keeps the enablement.
    pub fn install(&self, package: &Package, source: Source) -> Result<InstalledPlugin, String> {
        let id = package.manifest.id.clone();
        std::fs::create_dir_all(&self.root).map_err(|e| io(&self.root, e))?;
        let staging = self.root.join(format!(".staging-{id}"));
        let _ = std::fs::remove_dir_all(&staging);
        for (name, data) in &package.files {
            if !is_package_path(name) {
                return Err(format!("unsafe path in package: {name}"));
            }
            let target = staging.join(name);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|e| io(parent, e))?;
            }
            std::fs::write(&target, data).map_err(|e| io(&target, e))?;
        }
        let dir = self.dir_of(&id);
        let old = self.root.join(format!(".old-{id}"));
        let _ = std::fs::remove_dir_all(&old);
        if dir.exists() {
            std::fs::rename(&dir, &old).map_err(|e| io(&dir, e))?;
        }
        if let Err(e) = std::fs::rename(&staging, &dir) {
            let _ = std::fs::rename(&old, &dir);
            return Err(io(&dir, e));
        }
        let _ = std::fs::remove_dir_all(&old);
        let mut state = self.load()?;
        let enabled = state
            .plugins
            .iter()
            .find(|p| p.manifest.id == id)
            .is_none_or(|p| p.enabled);
        state.plugins.retain(|p| p.manifest.id != id);
        state.wanted.retain(|w| w.id != id);
        let installed = InstalledPlugin {
            manifest: package.manifest.clone(),
            enabled,
            granted: package.manifest.permissions.clone(),
            source,
            sha256: package.sha256.clone(),
            installed_at: now(),
        };
        state.plugins.push(installed.clone());
        state
            .plugins
            .sort_by(|a, b| a.manifest.name.cmp(&b.manifest.name));
        self.save(&state)?;
        Ok(installed)
    }

    /// Remove a plugin with its data folder.
    pub fn uninstall(&self, id: &str) -> Result<(), String> {
        let mut state = self.load()?;
        let before = state.plugins.len();
        state.plugins.retain(|p| p.manifest.id != id);
        if state.plugins.len() == before {
            return Err(format!("plugin `{id}` is not installed"));
        }
        self.save(&state)?;
        for dir in [self.dir_of(id), self.data_dir(id)] {
            if dir.exists() {
                std::fs::remove_dir_all(&dir).map_err(|e| io(&dir, e))?;
            }
        }
        Ok(())
    }

    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<(), String> {
        let mut state = self.load()?;
        let plugin = state
            .plugins
            .iter_mut()
            .find(|p| p.manifest.id == id)
            .ok_or_else(|| format!("plugin `{id}` is not installed"))?;
        plugin.enabled = enabled;
        self.save(&state)
    }

    /// A file of an installed plugin (its `main` bundle, README, icon).
    pub fn read_file(&self, id: &str, path: &str) -> Result<Vec<u8>, String> {
        if !is_package_path(path) {
            return Err(format!("unsafe path {path}"));
        }
        let file = self.dir_of(id).join(path);
        std::fs::read(&file).map_err(|e| io(&file, e))
    }

    /// The installed plugins as a list for exported settings.
    pub fn refs(&self) -> Result<Vec<PluginRef>, String> {
        Ok(self
            .list()?
            .into_iter()
            .map(|p| PluginRef {
                marketplace: matches!(p.source, Source::Marketplace { .. }),
                id: p.manifest.id,
                version: p.manifest.version,
                name: p.manifest.name,
                enabled: p.enabled,
            })
            .collect())
    }

    /// Plugins imported settings listed that are not installed.
    pub fn wanted(&self) -> Result<Vec<PluginRef>, String> {
        Ok(self.load()?.wanted)
    }

    /// Remember the plugins of imported settings that are missing here; the enablement of
    /// installed ones follows the import. Returns the missing ones.
    pub fn import_refs(&self, refs: &[PluginRef]) -> Result<Vec<PluginRef>, String> {
        let mut state = self.load()?;
        for r in refs {
            if let Some(p) = state.plugins.iter_mut().find(|p| p.manifest.id == r.id) {
                p.enabled = r.enabled;
            }
        }
        let missing: Vec<PluginRef> = refs
            .iter()
            .filter(|r| !state.plugins.iter().any(|p| p.manifest.id == r.id))
            .cloned()
            .collect();
        state.wanted = missing.clone();
        self.save(&state)?;
        Ok(missing)
    }

    pub fn dismiss_wanted(&self) -> Result<(), String> {
        let mut state = self.load()?;
        state.wanted.clear();
        self.save(&state)
    }
}

/// The registry entry of an installed plugin: its declarative contributions, so Settings |
/// Plugins lists it and its settings have a schema.
pub fn descriptor(p: &InstalledPlugin) -> PluginDescriptor {
    let m = &p.manifest;
    PluginDescriptor {
        manifest: forge_plugin_api::PluginManifest {
            id: m.id.clone(),
            name: m.name.clone(),
            version: m.version.clone(),
            pack: THIRD_PARTY_PACK.into(),
            description: m.description.clone(),
            vendor: m.publisher.clone(),
            depends: Vec::new(),
            required: false,
            project_types: Vec::new(),
            l10n: m.l10n.clone(),
        },
        contributions: Contributions {
            commands: m.contributes.commands.clone(),
            keybindings: m.contributes.keybindings.clone(),
            tool_windows: m.contributes.tool_windows.clone(),
            settings: m.contributes.settings.clone(),
            file_types: m.contributes.languages.clone(),
            ..Contributions::default()
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::package::tests::word_count_package;

    fn package(version: &str) -> Package {
        Package::read(&word_count_package(version), "0.1.0").unwrap()
    }

    #[test]
    fn install_update_disable_uninstall() {
        let dir = tempfile::tempdir().unwrap();
        let store = PluginStore::under(dir.path());
        assert!(store.list().unwrap().is_empty());
        let disk = Source::Disk {
            file: "/tmp/wc.forgeplugin".into(),
        };
        let p = store.install(&package("1.0.0"), disk.clone()).unwrap();
        assert!(p.enabled);
        assert_eq!(p.granted, p.manifest.permissions);
        assert_eq!(
            store
                .read_file("example.word-count", "dist/plugin.js")
                .unwrap(),
            b"forge.log('hi');"
        );
        assert!(store
            .read_file("example.word-count", "../installed.json")
            .is_err());

        store.set_enabled("example.word-count", false).unwrap();
        std::fs::create_dir_all(store.data_dir("example.word-count")).unwrap();
        let updated = store.install(&package("1.1.0"), disk).unwrap();
        assert!(!updated.enabled, "an update keeps the enablement");
        assert_eq!(store.list().unwrap().len(), 1);
        assert_eq!(
            store
                .get("example.word-count")
                .unwrap()
                .unwrap()
                .manifest
                .version,
            "1.1.0"
        );
        // No staging or backup folders are left behind.
        let names: Vec<String> = std::fs::read_dir(store.root())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names.len(), 2, "{names:?}");

        let d = descriptor(&updated);
        assert_eq!(d.manifest.pack, THIRD_PARTY_PACK);
        assert_eq!(d.contributions.commands.len(), 1);
        assert_eq!(
            d.contributions.settings[0].key,
            "example.word-count.includeCode"
        );

        store.uninstall("example.word-count").unwrap();
        assert!(store.list().unwrap().is_empty());
        assert!(!store.data_dir("example.word-count").exists());
        assert!(store.uninstall("example.word-count").is_err());
    }

    #[test]
    fn imported_plugin_lists() {
        let dir = tempfile::tempdir().unwrap();
        let store = PluginStore::under(dir.path());
        store
            .install(&package("1.0.0"), Source::Marketplace { index: "x".into() })
            .unwrap();
        let refs = store.refs().unwrap();
        assert!(refs[0].marketplace && refs[0].enabled);
        let other = PluginRef {
            id: "acme.other".into(),
            version: "2.0.0".into(),
            name: "Other".into(),
            enabled: true,
            marketplace: true,
        };
        let mut wc = refs[0].clone();
        wc.enabled = false;
        let missing = store.import_refs(&[wc, other.clone()]).unwrap();
        assert_eq!(missing, vec![other]);
        assert_eq!(store.wanted().unwrap().len(), 1);
        assert!(!store.get("example.word-count").unwrap().unwrap().enabled);
        store.dismiss_wanted().unwrap();
        assert!(store.wanted().unwrap().is_empty());
    }
}
