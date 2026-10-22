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
    /// The cache folders of the embedded browser engine (the webview).
    pub webview_cache: Vec<PathBuf>,
}

impl ForgeDirs {
    /// The directories of this machine's user. `identifier` is the app's bundle identifier,
    /// which names the webview's folders: WebKit (Linux, macOS) caches in
    /// `<cache dir>/<identifier>`, WebView2 (Windows) in the `Cache` folders of
    /// `<local data dir>/<identifier>/EBWebView/Default`.
    pub fn system(identifier: &str) -> Self {
        let mut webview_cache: Vec<PathBuf> = ::dirs::cache_dir()
            .map(|c| c.join(identifier))
            .into_iter()
            .collect();
        if let Some(local) = ::dirs::data_local_dir() {
            let profile = local.join(identifier).join("EBWebView").join("Default");
            for cache in ["Cache", "Code Cache", "GPUCache"] {
                webview_cache.push(profile.join(cache));
            }
        }
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
            webview_cache: vec![root.join("webview")],
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
