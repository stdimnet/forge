//! Go SDKs on this machine, found without running anything: the `go` on the PATH, `GOROOT`,
//! the usual install folders (`/usr/local/go`, `/usr/lib/go`, Homebrew, `C:\Program Files\Go`),
//! SDKs downloaded with `golang.org/dl` (`~/sdk/go1.x`), by GoLand (`~/go/go1.x`) or by
//! Forge (`<data dir>/forge/go-sdks`). The version comes from the SDK's `VERSION` file.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoSdk {
    /// The SDK folder (GOROOT).
    pub root: PathBuf,
    /// `1.24.7`.
    pub version: String,
    /// Where it was found: `PATH`, `GOROOT`, `Homebrew`, `Forge`, `~/sdk`, a folder.
    pub source: String,
}

impl GoSdk {
    pub fn go(&self) -> PathBuf {
        go_executable(&self.root, cfg!(windows))
    }
}

pub fn go_executable(root: &Path, windows: bool) -> PathBuf {
    root.join("bin").join(if windows { "go.exe" } else { "go" })
}

/// Where to look; [`DetectEnv::system`] fills it from the machine.
#[derive(Debug, Clone, Default)]
pub struct DetectEnv {
    pub path: Vec<PathBuf>,
    pub home: Option<PathBuf>,
    pub goroot: Option<PathBuf>,
    /// Forge's SDK downloads.
    pub forge_sdks: Option<PathBuf>,
    pub windows: bool,
    /// Install folders with their labels: `/usr/local/go`, Homebrew, `C:\\Program Files\\Go`.
    pub folders: Vec<(PathBuf, String)>,
}

/// `<data dir>/forge/go-sdks`.
pub fn default_sdks_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("forge").join("go-sdks"))
}

impl DetectEnv {
    pub fn system() -> Self {
        let var = |k: &str| {
            std::env::var_os(k)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        let mut folders: Vec<(PathBuf, String)> = [
            "/usr/local/go",
            "/usr/lib/go",
            "/usr/lib/golang",
            "/opt/go",
            "/snap/go/current",
        ]
        .iter()
        .map(|f| (PathBuf::from(f), f.to_string()))
        .collect();
        for brew in [
            "/opt/homebrew/opt/go/libexec",
            "/usr/local/opt/go/libexec",
            "/home/linuxbrew/.linuxbrew/opt/go/libexec",
        ] {
            folders.push((PathBuf::from(brew), "Homebrew".into()));
        }
        for k in ["ProgramFiles", "ProgramFiles(x86)"] {
            if let Some(p) = var(k) {
                folders.push((p.join("Go"), p.join("Go").to_string_lossy().into_owned()));
            }
        }
        if let Some(p) = var("LOCALAPPDATA") {
            let go = p.join("Programs").join("Go");
            folders.push((go.clone(), go.to_string_lossy().into_owned()));
        }
        DetectEnv {
            path: std::env::var_os("PATH")
                .map(|p| std::env::split_paths(&p).collect())
                .unwrap_or_default(),
            home: dirs::home_dir(),
            goroot: var("GOROOT"),
            forge_sdks: default_sdks_dir(),
            windows: cfg!(windows),
            folders,
        }
    }
}

/// The version of the SDK at `root` (`VERSION` starts with `go1.24.7`).
pub fn sdk_version(root: &Path) -> Option<String> {
    let text = std::fs::read_to_string(root.join("VERSION")).ok()?;
    let first = text.lines().next()?.trim();
    let v = first.strip_prefix("go")?;
    (!v.is_empty() && v.starts_with(|c: char| c.is_ascii_digit())).then(|| v.to_string())
}

/// `(1, 24, 7)` of `1.24.7`, `1.25rc1` → `(1, 25, 0)`.
pub fn version_tuple(v: &str) -> (u32, u32, u32) {
    let v = v.trim_start_matches("go");
    let mut parts = v.split('.').map(|p| {
        p.chars()
            .take_while(|c| c.is_ascii_digit())
            .collect::<String>()
            .parse()
            .unwrap_or(0)
    });
    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    )
}

fn sdk_at(root: &Path, source: &str, windows: bool) -> Option<GoSdk> {
    if !go_executable(root, windows).is_file() {
        return None;
    }
    let version = sdk_version(root)?;
    Some(GoSdk {
        root: root.to_path_buf(),
        version,
        source: source.to_string(),
    })
}

/// Folders below `dir` whose name starts with `go` (`~/sdk/go1.22.5`).
fn versioned(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_dir()
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("go"))
        })
        .collect();
    out.sort();
    out
}

/// Every Go SDK found, newest first; the first one on the PATH leads when versions tie.
pub fn detect(env: &DetectEnv) -> Vec<GoSdk> {
    let mut candidates: Vec<(PathBuf, String)> = Vec::new();
    let exe = if env.windows { "go.exe" } else { "go" };
    for dir in &env.path {
        let go = dir.join(exe);
        if go.is_file() {
            let real = std::fs::canonicalize(&go).unwrap_or(go);
            if let Some(root) = real.parent().and_then(Path::parent) {
                candidates.push((root.to_path_buf(), "PATH".into()));
            }
        }
    }
    if let Some(r) = &env.goroot {
        candidates.push((r.clone(), "GOROOT".into()));
    }
    candidates.extend(env.folders.iter().cloned());
    if let Some(home) = &env.home {
        for d in versioned(&home.join("sdk")) {
            candidates.push((d, "~/sdk".into()));
        }
        for d in versioned(&home.join("go")) {
            candidates.push((d, "~/go".into()));
        }
    }
    if let Some(f) = &env.forge_sdks {
        for d in versioned(f) {
            candidates.push((d, "Forge".into()));
        }
    }
    let mut out: Vec<GoSdk> = Vec::new();
    for (root, source) in candidates {
        let key = std::fs::canonicalize(&root).unwrap_or_else(|_| root.clone());
        if out
            .iter()
            .any(|s| std::fs::canonicalize(&s.root).unwrap_or_else(|_| s.root.clone()) == key)
        {
            continue;
        }
        if let Some(sdk) = sdk_at(&root, &source, env.windows) {
            out.push(sdk);
        }
    }
    // Stable sort: newest version first, the PATH order among equal versions.
    out.sort_by_key(|s| std::cmp::Reverse(version_tuple(&s.version)));
    out
}

/// The SDK at a folder the user chose (Settings | Go | GOROOT).
pub fn sdk_from_folder(root: &Path) -> Result<GoSdk, String> {
    sdk_at(root, "custom", cfg!(windows)).ok_or_else(|| {
        format!(
            "{} is not a Go SDK: it needs bin/go and a VERSION file",
            root.display()
        )
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn fake_sdk(root: &Path, version: &str) {
        std::fs::create_dir_all(root.join("bin")).unwrap();
        std::fs::write(root.join("bin/go"), "#!/bin/sh\n").unwrap();
        std::fs::write(
            root.join("VERSION"),
            format!("go{version}\ntime 2025-01-01T00:00:00Z\n"),
        )
        .unwrap();
    }

    #[test]
    fn finds_sdks_newest_first() {
        let t = tempfile::tempdir().unwrap();
        let home = t.path().join("home");
        fake_sdk(&home.join("sdk/go1.22.5"), "1.22.5");
        fake_sdk(&home.join("go/go1.25.1"), "1.25.1");
        fake_sdk(&t.path().join("opt/go"), "1.24.7");
        fake_sdk(&t.path().join("forge/go1.23.0"), "1.23.0");
        std::fs::create_dir_all(home.join("sdk/gopls")).unwrap();
        let env = DetectEnv {
            path: vec![t.path().join("opt/go/bin"), t.path().join("nothing")],
            home: Some(home.clone()),
            goroot: Some(t.path().join("opt/go")),
            forge_sdks: Some(t.path().join("forge")),
            windows: false,
            folders: vec![(t.path().join("missing"), "/usr/local/go".into())],
        };
        let found = detect(&env);
        let rows: Vec<_> = found
            .iter()
            .map(|s| (s.version.as_str(), s.source.as_str()))
            .collect();
        assert_eq!(
            rows,
            [
                ("1.25.1", "~/go"),
                ("1.24.7", "PATH"),
                ("1.23.0", "Forge"),
                ("1.22.5", "~/sdk"),
            ]
        );
        assert!(found[1].go().ends_with("bin/go"));
        assert!(sdk_from_folder(&t.path().join("forge/go1.23.0")).is_ok());
        assert!(sdk_from_folder(t.path()).is_err());
    }

    #[test]
    fn versions() {
        assert_eq!(version_tuple("1.24.7"), (1, 24, 7));
        assert_eq!(version_tuple("go1.25rc1"), (1, 25, 0));
        assert_eq!(version_tuple("1.22"), (1, 22, 0));
        let t = tempfile::tempdir().unwrap();
        std::fs::write(t.path().join("VERSION"), "devel go1.26-abc\n").unwrap();
        assert_eq!(sdk_version(t.path()), None);
    }
}
