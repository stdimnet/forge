//! The contract between Forge and its plugins.
//!
//! A plugin (a language pack like PHPForge, or a feature like Git) describes itself with a
//! [`PluginManifest`] and contributes capabilities through [`Contributions`]. The full
//! registry and extension points live here so packs never depend on app internals.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginManifest {
    /// Reverse-DNS style id, e.g. `forge.php` or `forge.git`.
    pub id: String,
    pub name: String,
    pub version: String,
    /// Pack this plugin belongs to (`phpforge`, `luaforge`, `webforge`, `aiforge`, or `core`).
    pub pack: String,
    #[serde(default)]
    pub description: String,
}

/// Everything a plugin adds to the IDE. Grows as extension points are added.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Contributions {
    /// File extensions this plugin provides language support for (without dot).
    #[serde(default)]
    pub languages: Vec<String>,
}

pub trait Plugin: Send + Sync {
    fn manifest(&self) -> PluginManifest;
    fn contributions(&self) -> Contributions {
        Contributions::default()
    }
}
