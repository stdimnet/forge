//! Where Forge keeps its files outside projects. Tests point every directory into a
//! temporary folder.

use std::path::PathBuf;

/// The user directories Forge writes to.
#[derive(Debug, Clone, Default)]
pub struct ForgeDirs {
    /// The user cache directory (`~/.cache` on Linux); Forge's caches live in `forge/`.
    pub cache: Option<PathBuf>,
    /// The user data directory (`~/.local/share`); Forge's data lives in `forge/`.
    pub data: Option<PathBuf>,
    /// The user config directory (`~/.config`); Forge's settings live in `forge/`.
    pub config: Option<PathBuf>,
    /// The cache of the embedded browser engine (the webview), if the shell knows it.
    pub webview_cache: Option<PathBuf>,
}

impl ForgeDirs {
    /// The directories of this machine's user.
    pub fn system(webview_cache: Option<PathBuf>) -> Self {
        Self {
            cache: ::dirs::cache_dir(),
            data: ::dirs::data_dir(),
            config: ::dirs::config_dir(),
            webview_cache,
        }
    }

    /// All directories below one root (`cache/`, `data/`, `config/`, `webview/`), for tests.
    pub fn under(root: &std::path::Path) -> Self {
        Self {
            cache: Some(root.join("cache")),
            data: Some(root.join("data")),
            config: Some(root.join("config")),
            webview_cache: Some(root.join("webview")),
        }
    }

    /// `<config dir>/forge`: settings and other configuration files.
    pub fn forge_config(&self) -> Option<PathBuf> {
        self.config.as_ref().map(|c| c.join("forge"))
    }

    /// `<data dir>/forge`.
    pub fn forge_data(&self) -> Option<PathBuf> {
        self.data.as_ref().map(|d| d.join("forge"))
    }

    /// `<cache dir>/forge`.
    pub fn forge_cache(&self) -> Option<PathBuf> {
        self.cache.as_ref().map(|d| d.join("forge"))
    }
}
