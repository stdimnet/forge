//! Auto-detection of local PHP interpreters: `PATH`, the system folders (`/usr/bin/php8.3`),
//! Homebrew (`/opt/homebrew`, `/usr/local/opt/php@8.2`), version managers (phpenv,
//! phpbrew, asdf), Laravel Herd, and Laragon / XAMPP / `C:\php` on Windows. Each candidate
//! is asked for its version with `php -v`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// A PHP executable found on this machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedInterpreter {
    pub path: String,
    pub version: String,
}

/// Where to look.
#[derive(Debug, Clone, Default)]
pub struct SearchEnv {
    /// The `PATH` variable.
    pub path_var: Option<String>,
    pub home: Option<PathBuf>,
    pub windows: bool,
    /// Folders that hold PHP installations (`/opt/homebrew/opt`): their `php*` subfolders'
    /// `bin` folders are searched.
    pub install_roots: Vec<PathBuf>,
    /// Folders searched directly (`/usr/bin`).
    pub dirs: Vec<PathBuf>,
}

impl SearchEnv {
    /// This machine's environment.
    pub fn system() -> Self {
        let windows = cfg!(windows);
        let home = std::env::var_os(if windows { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
        let mut dirs: Vec<PathBuf> = Vec::new();
        let mut install_roots: Vec<PathBuf> = Vec::new();
        if windows {
            for drive in ["C:\\", "D:\\"] {
                let d = PathBuf::from(drive);
                dirs.push(d.join("php"));
                dirs.push(d.join("xampp").join("php"));
                install_roots.push(d.join("laragon").join("bin").join("php"));
                install_roots.push(d.join("tools"));
                install_roots.push(d.to_path_buf());
            }
        } else {
            for d in ["/usr/bin", "/usr/local/bin", "/bin", "/opt/homebrew/bin", "/opt/local/bin"] {
                dirs.push(PathBuf::from(d));
            }
            for d in ["/opt/homebrew/opt", "/usr/local/opt", "/opt/remi", "/opt"] {
                install_roots.push(PathBuf::from(d));
            }
        }
        if let Some(h) = &home {
            if windows {
                dirs.push(h.join(".config").join("herd").join("bin"));
                install_roots.push(h.join(".config").join("herd").join("bin"));
            } else {
                dirs.push(h.join("Library/Application Support/Herd/bin"));
                dirs.push(h.join(".config/herd-lite/bin"));
                install_roots.push(h.join(".phpenv/versions"));
                install_roots.push(h.join(".phpbrew/php"));
                install_roots.push(h.join(".asdf/installs/php"));
                install_roots.push(h.join(".local/share/mise/installs/php"));
            }
        }
        Self {
            path_var: std::env::var("PATH").ok(),
            home,
            windows,
            install_roots,
            dirs,
        }
    }

    /// Every folder to search, in order.
    pub fn search_dirs(&self) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = Vec::new();
        if let Some(p) = &self.path_var {
            let sep = if self.windows { ';' } else { ':' };
            out.extend(p.split(sep).filter(|s| !s.is_empty()).map(PathBuf::from));
        }
        out.extend(self.dirs.iter().cloned());
        for root in &self.install_roots {
            let Ok(entries) = std::fs::read_dir(root) else {
                continue;
            };
            let mut subs: Vec<PathBuf> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .filter(|p| {
                    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    // Version managers name folders by version; others `php*`.
                    name.to_ascii_lowercase().starts_with("php")
                        || name.starts_with(|c: char| c.is_ascii_digit())
                })
                .collect();
            subs.sort();
            for sub in subs {
                out.push(sub.join("bin"));
                out.push(sub);
            }
        }
        let mut seen = HashSet::new();
        out.retain(|d| seen.insert(d.clone()));
        out
    }
}

/// `php`, `php8.3`, `php83`, `php-8.3` (`php.exe` on Windows); not `php-config`, `phpize`,
/// `php-fpm` or `php-cgi`.
pub fn is_php_name(name: &str, windows: bool) -> bool {
    if windows {
        return name.eq_ignore_ascii_case("php.exe");
    }
    let Some(rest) = name.strip_prefix("php") else {
        return false;
    };
    let rest = rest.strip_prefix('-').unwrap_or(rest);
    rest.chars().all(|c| c.is_ascii_digit() || c == '.')
}

fn is_executable(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    true
}

/// The PHP executables in the search folders, the first path of each real file.
pub fn candidates(env: &SearchEnv) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for dir in env.search_dirs() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut found: Vec<PathBuf> = entries
            .flatten()
            .filter(|e| is_php_name(&e.file_name().to_string_lossy(), env.windows))
            .map(|e| e.path())
            .filter(|p| is_executable(p))
            .collect();
        found.sort();
        for path in found {
            let real = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
            if seen.insert(real) {
                out.push(path);
            }
        }
    }
    out
}

/// The candidates that report a PHP version through `version_of`.
pub fn detect(
    env: &SearchEnv,
    version_of: impl Fn(&Path) -> Option<String>,
) -> Vec<DetectedInterpreter> {
    candidates(env)
        .into_iter()
        .filter_map(|p| {
            let version = version_of(&p)?;
            Some(DetectedInterpreter {
                path: p.to_string_lossy().into_owned(),
                version,
            })
        })
        .collect()
}

/// `<php> -v`, parsed; `None` when it fails or takes longer than `timeout`.
pub fn version_of(php: &Path, timeout: Duration) -> Option<String> {
    let mut child = Command::new(php)
        .arg("-v")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() < timeout => std::thread::sleep(Duration::from_millis(10)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    let out = child.wait_with_output().ok()?;
    crate::probe::parse_version(&String::from_utf8_lossy(&out.stdout))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_php_executable_names() {
        for n in ["php", "php8.3", "php83", "php-8.2", "php8"] {
            assert!(is_php_name(n, false), "{n}");
        }
        for n in ["php-config", "phpize", "php-fpm8.3", "php-cgi", "phpunit", "xphp"] {
            assert!(!is_php_name(n, false), "{n}");
        }
        assert!(is_php_name("PHP.EXE", true));
        assert!(!is_php_name("php-cgi.exe", true));
    }

    #[cfg(unix)]
    fn fake_php(dir: &Path, name: &str, version: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir_all(dir).unwrap();
        let path = dir.join(name);
        std::fs::write(
            &path,
            format!("#!/bin/sh\necho 'PHP {version} (cli) (built: Jan 1 2025)'\necho 'Copyright (c) The PHP Group'\n"),
        )
        .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[cfg(unix)]
    #[test]
    fn finds_php_on_the_path_and_in_version_managers() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let bin = root.join("usr/bin");
        fake_php(&bin, "php8.3", "8.3.12");
        fake_php(&bin, "php8.2", "8.2.20");
        std::fs::write(bin.join("php-config"), "#!/bin/sh\n").unwrap();
        // Not executable: ignored.
        std::fs::write(bin.join("php7.4"), "").unwrap();
        // The same file through a symlink is listed once.
        std::os::unix::fs::symlink(bin.join("php8.3"), bin.join("php")).unwrap();
        let phpenv = root.join("home/.phpenv/versions");
        fake_php(&phpenv.join("8.1.29/bin"), "php", "8.1.29");
        // A folder that is not a PHP version.
        std::fs::create_dir_all(phpenv.join("shims")).unwrap();

        let env = SearchEnv {
            path_var: Some(format!("{}:/does/not/exist", bin.display())),
            home: Some(root.join("home")),
            windows: false,
            install_roots: vec![phpenv.clone()],
            dirs: vec![bin.clone()],
        };
        let found = detect(&env, |p| version_of(p, Duration::from_secs(5)));
        let pairs: Vec<(String, &str)> = found
            .iter()
            .map(|d| {
                (
                    Path::new(&d.path)
                        .strip_prefix(root)
                        .unwrap()
                        .display()
                        .to_string(),
                    d.version.as_str(),
                )
            })
            .collect();
        assert_eq!(
            pairs,
            [
                ("usr/bin/php".to_string(), "8.3.12"),
                ("usr/bin/php8.2".to_string(), "8.2.20"),
                ("home/.phpenv/versions/8.1.29/bin/php".to_string(), "8.1.29"),
            ]
        );
    }
}
