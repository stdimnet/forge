//! Finding installed language servers: `PATH` plus the places package managers put
//! executables that are often missing from `PATH` in a desktop session (npm global prefix,
//! nvm/fnm/volta, `~/.local/bin` for pip/pipx, `~/.cargo/bin`, `~/go/bin`, `~/.dotnet/tools`,
//! gem user dirs and Neovim's Mason directory). The open project's `node_modules/.bin` and
//! Forge's own directory for npm servers (`<data dir>/forge/langservers`) come first.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// The environment detection depends on. [`DetectEnv::from_system`] reads the real one;
/// tests build their own.
#[derive(Debug, Clone, Default)]
pub struct DetectEnv {
    pub path: Option<OsString>,
    pub home: Option<PathBuf>,
    pub npm_prefix: Option<PathBuf>,
    pub gopath: Option<PathBuf>,
    pub gobin: Option<PathBuf>,
    pub cargo_home: Option<PathBuf>,
    pub xdg_data_home: Option<PathBuf>,
    /// `%APPDATA%` and `%LOCALAPPDATA%` on Windows.
    pub appdata: Option<PathBuf>,
    pub local_appdata: Option<PathBuf>,
    pub windows: bool,
    /// The open project; its `node_modules/.bin` is searched first.
    pub project_root: Option<PathBuf>,
    /// Where Forge installs npm language servers (`npm install --prefix <dir>`); its
    /// `node_modules/.bin` is searched right after the project's.
    pub managed_dir: Option<PathBuf>,
}

/// Forge's own directory for npm language servers: `<user data dir>/forge/langservers`.
pub fn default_managed_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("forge").join("langservers"))
}

fn env_path(key: &str) -> Option<PathBuf> {
    std::env::var_os(key)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

impl DetectEnv {
    pub fn from_system() -> Self {
        DetectEnv {
            path: std::env::var_os("PATH"),
            home: dirs::home_dir(),
            npm_prefix: env_path("NPM_CONFIG_PREFIX").or_else(|| env_path("npm_config_prefix")),
            gopath: env_path("GOPATH"),
            gobin: env_path("GOBIN"),
            cargo_home: env_path("CARGO_HOME"),
            xdg_data_home: env_path("XDG_DATA_HOME"),
            appdata: env_path("APPDATA"),
            local_appdata: env_path("LOCALAPPDATA"),
            windows: cfg!(windows),
            project_root: None,
            managed_dir: default_managed_dir(),
        }
    }

    /// Directories searched for executables, most specific first, without duplicates: the
    /// project's `node_modules/.bin`, Forge's managed servers, `PATH`, then the usual
    /// package manager directories.
    pub fn search_dirs(&self) -> Vec<PathBuf> {
        let mut dirs: Vec<PathBuf> = Vec::new();
        if let Some(root) = &self.project_root {
            dirs.push(root.join("node_modules").join(".bin"));
        }
        if let Some(managed) = &self.managed_dir {
            dirs.push(managed.join("node_modules").join(".bin"));
        }
        dirs.extend(
            self.path
                .as_ref()
                .map(|p| std::env::split_paths(p).collect::<Vec<_>>())
                .unwrap_or_default(),
        );
        let home = self.home.as_deref();
        let data_home = self
            .xdg_data_home
            .clone()
            .or_else(|| home.map(|h| h.join(".local/share")));

        // npm global prefix and Node version managers.
        if let Some(prefix) = &self.npm_prefix {
            dirs.push(if self.windows {
                prefix.clone()
            } else {
                prefix.join("bin")
            });
        }
        if let Some(appdata) = &self.appdata {
            dirs.push(appdata.join("npm"));
        }
        if let Some(h) = home {
            dirs.push(h.join(".npm-global/bin"));
            dirs.push(h.join(".npm-packages/bin"));
            dirs.push(h.join(".volta/bin"));
            dirs.push(h.join(".bun/bin"));
            dirs.extend(newest_first(&h.join(".nvm/versions/node"), "bin"));
        }
        if let Some(d) = &data_home {
            dirs.push(d.join("npm/bin"));
            dirs.push(d.join("fnm/aliases/default/bin"));
        }

        // pip --user / pipx, composer global, cargo, go, dotnet, gem.
        if let Some(h) = home {
            dirs.push(h.join(".local/bin"));
            dirs.push(h.join(".config/composer/vendor/bin"));
            dirs.push(h.join(".composer/vendor/bin"));
        }
        if let Some(appdata) = &self.appdata {
            dirs.push(appdata.join("Composer/vendor/bin"));
        }
        if let Some(cargo) = self
            .cargo_home
            .clone()
            .or_else(|| home.map(|h| h.join(".cargo")))
        {
            dirs.push(cargo.join("bin"));
        }
        if let Some(gobin) = &self.gobin {
            dirs.push(gobin.clone());
        }
        match &self.gopath {
            Some(gopath) => dirs.extend(std::env::split_paths(gopath).map(|p| p.join("bin"))),
            None => dirs.extend(home.map(|h| h.join("go/bin"))),
        }
        if let Some(h) = home {
            dirs.push(h.join(".dotnet/tools"));
            dirs.extend(newest_first(&h.join(".local/share/gem/ruby"), "bin"));
            dirs.extend(newest_first(&h.join(".gem/ruby"), "bin"));
            dirs.push(h.join(".rbenv/shims"));
            dirs.push(h.join(".asdf/shims"));
        }

        // Neovim's Mason installs most servers into one bin directory.
        if let Some(d) = &data_home {
            dirs.push(d.join("nvim/mason/bin"));
        }
        if let Some(local) = &self.local_appdata {
            dirs.push(local.join("nvim-data/mason/bin"));
        }

        if !self.windows {
            dirs.push(PathBuf::from("/usr/local/bin"));
            dirs.push(PathBuf::from("/opt/homebrew/bin"));
            dirs.push(PathBuf::from("/home/linuxbrew/.linuxbrew/bin"));
        }
        let mut seen = std::collections::HashSet::new();
        dirs.retain(|d| !d.as_os_str().is_empty() && seen.insert(d.clone()));
        dirs
    }

    /// File names tried for `command` in each directory.
    pub fn candidates(&self, command: &str) -> Vec<String> {
        if self.windows {
            ["cmd", "exe", "bat"]
                .iter()
                .map(|ext| format!("{command}.{ext}"))
                .chain(std::iter::once(command.to_string()))
                .collect()
        } else {
            vec![command.to_string()]
        }
    }
}

/// `<dir>/<version>/<sub>` for every version directory, newest (by name) first.
fn newest_first(dir: &Path, sub: &str) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return vec![];
    };
    let mut versions: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    versions.sort_by(|a, b| natural_cmp(&b.to_string_lossy(), &a.to_string_lossy()));
    versions.into_iter().map(|v| v.join(sub)).collect()
}

/// Compare strings treating digit runs as numbers (`v20.1` > `v9.11`).
fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let key = |s: &str| -> Vec<(u64, String)> {
        let mut out = Vec::new();
        let mut num = String::new();
        let mut text = String::new();
        for ch in s.chars() {
            if ch.is_ascii_digit() {
                if !text.is_empty() {
                    out.push((0, std::mem::take(&mut text)));
                }
                num.push(ch);
            } else {
                if !num.is_empty() {
                    out.push((num.parse().unwrap_or(0), String::new()));
                    num.clear();
                }
                text.push(ch);
            }
        }
        if !num.is_empty() {
            out.push((num.parse().unwrap_or(0), String::new()));
        }
        if !text.is_empty() {
            out.push((0, text));
        }
        out
    };
    key(a).cmp(&key(b))
}

/// Looks up executables in a fixed list of directories.
#[derive(Debug, Clone)]
pub struct Detector {
    env: DetectEnv,
    dirs: Vec<PathBuf>,
}

impl Detector {
    pub fn new(env: DetectEnv) -> Self {
        let dirs = env.search_dirs();
        Detector { env, dirs }
    }

    pub fn system() -> Self {
        Self::new(DetectEnv::from_system())
    }

    /// Like [`system`](Self::system), also looking in the project's `node_modules/.bin`.
    pub fn for_project(root: &Path) -> Self {
        Self::new(DetectEnv {
            project_root: Some(root.to_path_buf()),
            ..DetectEnv::from_system()
        })
    }

    /// Forge's own directory for npm servers, the `--prefix` of managed installs.
    pub fn managed_dir(&self) -> Option<&Path> {
        self.env.managed_dir.as_deref()
    }

    /// `node_modules` of the managed directory (packages next to the managed servers).
    pub fn managed_node_modules(&self) -> Option<PathBuf> {
        self.managed_dir().map(|d| d.join("node_modules"))
    }

    pub fn dirs(&self) -> &[PathBuf] {
        &self.dirs
    }

    pub fn env(&self) -> &DetectEnv {
        &self.env
    }

    /// Full path of `command`. Absolute or relative paths are checked as they are.
    pub fn find(&self, command: &str) -> Option<PathBuf> {
        let as_path = Path::new(command);
        if as_path.components().count() > 1 {
            return is_executable(as_path).then(|| as_path.to_path_buf());
        }
        let names = self.env.candidates(command);
        self.dirs
            .iter()
            .flat_map(|d| names.iter().map(move |n| d.join(n)))
            .find(|p| is_executable(p))
    }

    /// The executable to use: an existing custom path wins over detection.
    pub fn resolve(&self, command: &str, custom: Option<&Path>) -> Option<PathBuf> {
        match custom.filter(|p| !p.as_os_str().is_empty()) {
            Some(p) => is_executable(p).then(|| p.to_path_buf()),
            None => self.find(command),
        }
    }

    /// The global `node_modules` directory next to an npm-installed executable
    /// (`<prefix>/bin/x` → `<prefix>/lib/node_modules`, Windows `<prefix>/x.cmd` →
    /// `<prefix>/node_modules`).
    pub fn global_node_modules(&self, executable: &Path) -> Option<PathBuf> {
        let bin = executable.parent()?;
        // `<dir>/node_modules/.bin/x` (project or managed install) → `<dir>/node_modules`.
        if bin.file_name().is_some_and(|n| n == ".bin") {
            if let Some(modules) = bin.parent().filter(|p| p.is_dir()) {
                return Some(modules.to_path_buf());
            }
        }
        let candidates = [
            bin.parent().map(|p| p.join("lib/node_modules")),
            Some(bin.join("node_modules")),
            bin.parent().map(|p| p.join("node_modules")),
        ];
        candidates.into_iter().flatten().find(|p| p.is_dir())
    }
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Create an executable file (and its parent directories).
    pub fn fake_bin(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
    }

    fn env(home: &Path, path: &[&Path]) -> DetectEnv {
        DetectEnv {
            path: Some(std::env::join_paths(path).unwrap()),
            home: Some(home.to_path_buf()),
            ..Default::default()
        }
    }

    #[test]
    fn finds_servers_on_path_and_in_user_bins() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let usr = tmp.path().join("usr/bin");
        fake_bin(&usr.join("clangd"));
        fake_bin(&home.join(".npm-global/bin/typescript-language-server"));
        fake_bin(&home.join(".local/bin/basedpyright-langserver"));
        fake_bin(&home.join(".cargo/bin/rust-analyzer"));
        fake_bin(&home.join("go/bin/gopls"));
        fake_bin(&home.join(".local/share/nvim/mason/bin/lua-language-server"));
        fake_bin(&home.join(".dotnet/tools/csharp-ls"));
        fake_bin(&home.join(".nvm/versions/node/v9.11.0/bin/vtsls"));
        fake_bin(&home.join(".nvm/versions/node/v20.1.0/bin/vtsls"));

        let d = Detector::new(env(&home, &[&usr]));
        assert_eq!(d.find("clangd"), Some(usr.join("clangd")));
        for (cmd, rel) in [
            ("typescript-language-server", ".npm-global/bin"),
            ("basedpyright-langserver", ".local/bin"),
            ("rust-analyzer", ".cargo/bin"),
            ("gopls", "go/bin"),
            ("lua-language-server", ".local/share/nvim/mason/bin"),
            ("csharp-ls", ".dotnet/tools"),
            ("vtsls", ".nvm/versions/node/v20.1.0/bin"),
        ] {
            assert_eq!(d.find(cmd), Some(home.join(rel).join(cmd)), "{cmd}");
        }
        assert_eq!(d.find("zls"), None);
        // PATH wins over the fallbacks.
        assert_eq!(d.dirs()[0], usr);
    }

    #[test]
    fn honours_env_overrides_and_custom_paths() {
        let tmp = tempfile::tempdir().unwrap();
        let prefix = tmp.path().join("npm");
        let gopath = tmp.path().join("gp");
        let cargo = tmp.path().join("cargo");
        fake_bin(&prefix.join("bin/yaml-language-server"));
        fake_bin(&gopath.join("bin/sqls"));
        fake_bin(&cargo.join("bin/taplo"));
        let d = Detector::new(DetectEnv {
            npm_prefix: Some(prefix.clone()),
            gopath: Some(gopath.clone()),
            cargo_home: Some(cargo.clone()),
            ..Default::default()
        });
        assert_eq!(
            d.find("yaml-language-server"),
            Some(prefix.join("bin/yaml-language-server"))
        );
        assert_eq!(d.find("sqls"), Some(gopath.join("bin/sqls")));
        assert_eq!(d.find("taplo"), Some(cargo.join("bin/taplo")));

        let custom = tmp.path().join("opt/my-taplo");
        assert_eq!(
            d.resolve("taplo", Some(&custom)),
            None,
            "missing custom path"
        );
        fake_bin(&custom);
        assert_eq!(d.resolve("taplo", Some(&custom)), Some(custom.clone()));
        assert_eq!(
            d.resolve("taplo", Some(Path::new(""))),
            Some(cargo.join("bin/taplo"))
        );
        std::fs::create_dir_all(prefix.join("lib/node_modules")).unwrap();
        assert_eq!(
            d.global_node_modules(&prefix.join("bin/yaml-language-server")),
            Some(prefix.join("lib/node_modules"))
        );
    }

    #[test]
    fn windows_tries_cmd_shims() {
        let tmp = tempfile::tempdir().unwrap();
        let appdata = tmp.path().join("AppData/Roaming");
        fake_bin(&appdata.join("npm/intelephense.cmd"));
        let local = tmp.path().join("AppData/Local");
        fake_bin(&local.join("nvim-data/mason/bin/gopls.exe"));
        let d = Detector::new(DetectEnv {
            appdata: Some(appdata.clone()),
            local_appdata: Some(local.clone()),
            windows: true,
            ..Default::default()
        });
        assert_eq!(
            d.find("intelephense"),
            Some(appdata.join("npm/intelephense.cmd"))
        );
        assert_eq!(
            d.find("gopls"),
            Some(local.join("nvim-data/mason/bin/gopls.exe"))
        );
        assert!(!d.dirs().contains(&PathBuf::from("/usr/local/bin")));
    }

    #[test]
    fn project_then_managed_then_path() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().join("project");
        let managed = tmp.path().join("data/forge/langservers");
        let usr = tmp.path().join("usr/bin");
        for dir in [
            project.join("node_modules/.bin"),
            managed.join("node_modules/.bin"),
            usr.clone(),
        ] {
            fake_bin(&dir.join("yaml-language-server"));
        }
        fake_bin(&managed.join("node_modules/.bin/vscode-json-language-server"));
        fake_bin(&usr.join("vscode-json-language-server"));
        fake_bin(&usr.join("gopls"));

        let d = Detector::new(DetectEnv {
            path: Some(std::env::join_paths([&usr]).unwrap()),
            project_root: Some(project.clone()),
            managed_dir: Some(managed.clone()),
            ..Default::default()
        });
        assert_eq!(
            d.find("yaml-language-server"),
            Some(project.join("node_modules/.bin/yaml-language-server")),
            "the project's own server wins"
        );
        assert_eq!(
            d.find("vscode-json-language-server"),
            Some(managed.join("node_modules/.bin/vscode-json-language-server")),
            "Forge's managed install beats PATH"
        );
        assert_eq!(d.find("gopls"), Some(usr.join("gopls")));
        assert_eq!(d.managed_dir(), Some(managed.as_path()));
        assert_eq!(d.managed_node_modules(), Some(managed.join("node_modules")));

        // Without a project or managed directory only PATH and the fallbacks remain.
        let plain = Detector::new(DetectEnv {
            path: Some(std::env::join_paths([&usr]).unwrap()),
            ..Default::default()
        });
        assert_eq!(
            plain.find("yaml-language-server"),
            Some(usr.join("yaml-language-server"))
        );
    }

    #[test]
    fn node_modules_next_to_a_local_bin_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let bin = tmp.path().join("langservers/node_modules/.bin");
        fake_bin(&bin.join("pyright"));
        let d = Detector::new(DetectEnv::default());
        assert_eq!(
            d.global_node_modules(&bin.join("pyright")),
            Some(tmp.path().join("langservers/node_modules"))
        );
    }

    #[test]
    fn natural_order_for_versions() {
        assert_eq!(
            natural_cmp("v20.1.0", "v9.11.0"),
            std::cmp::Ordering::Greater
        );
        assert_eq!(natural_cmp("3.2.0", "3.10.0"), std::cmp::Ordering::Less);
    }

    #[test]
    fn non_executable_files_are_ignored() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("zls"), "").unwrap();
        let d = Detector::new(env(tmp.path(), &[tmp.path()]));
        #[cfg(unix)]
        assert_eq!(d.find("zls"), None);
        #[cfg(not(unix))]
        assert!(d.find("zls").is_some());
    }
}
