//! The contract between Forge and its plugins.
//!
//! A plugin (a language pack like PHPForge, or a feature like Git) describes itself with a
//! [`PluginManifest`] and contributes capabilities through [`Contributions`]: commands,
//! keybindings, tool windows, typed settings, language servers, run configuration types and
//! AI providers/agents/generators. The [`PluginRegistry`] tracks installed plugins, their
//! enablement, and answers lookups over the contributions of enabled plugins. Packs depend
//! only on this crate, never on app internals.

pub mod contributions;
pub mod l10n;
pub mod profile;
pub mod registry;
pub mod settings_model;

/// The project lifecycle packs subscribe to (re-exported from `forge_core::lifecycle`): a pack
/// crate implements [`lifecycle::ProjectLifecycle`] and the desktop app subscribes it to the
/// window's [`lifecycle::ProjectLifecycleBus`] (`app/src-tauri/src/lifecycle.rs`).
pub mod lifecycle {
    pub use forge_core::lifecycle::{
        LifecycleEvent, Objection, PhaseReport, ProjectLifecycle, ProjectLifecycleBus,
        ProjectPhase, ProjectRef, SubscriptionId, LIFECYCLE_EVENT,
    };
}

use serde::{Deserialize, Serialize};

pub use contributions::*;
pub use profile::{LaunchProfile, ProfileView};
pub use registry::{PluginDescriptor, PluginInfo, PluginRegistry};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PluginManifest {
    /// Reverse-DNS style id, e.g. `forge.php` or `forge.git`.
    pub id: String,
    pub name: String,
    pub version: String,
    /// Pack this plugin belongs to (`phpforge`, `luaforge`, `webforge`, or `core`).
    pub pack: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub vendor: String,
    /// Ids of plugins this one needs; they are enabled with it and it is disabled with them.
    #[serde(default)]
    pub depends: Vec<String>,
    /// Required plugins (the IDE core) cannot be disabled.
    #[serde(default)]
    pub required: bool,
    /// Project types the plugin applies to: language, framework, pack or tool ids from project
    /// detection (`php`, `laravel`, `luaforge`, ...). The plugin is active in a project that
    /// uses any of them; empty means always active.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub project_types: Vec<String>,
    /// Translations of the manifest's own text by locale (`[l10n.de]`), keyed like the
    /// bundled catalogs (see [`l10n`]).
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub l10n: std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>,
}

pub trait Plugin: Send + Sync {
    fn manifest(&self) -> PluginManifest;
    fn contributions(&self) -> Contributions {
        Contributions::default()
    }
}
