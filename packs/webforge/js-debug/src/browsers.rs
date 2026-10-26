//! Chromium browsers on this machine for "JavaScript Debug" (js-debug launches them with
//! remote debugging): Google Chrome, Microsoft Edge and Chromium, at their usual install
//! paths or on the `PATH`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BrowserKind {
    Chrome,
    Edge,
    Chromium,
}

impl BrowserKind {
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "chrome" => Some(Self::Chrome),
            "edge" => Some(Self::Edge),
            "chromium" => Some(Self::Chromium),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Chrome => "Google Chrome",
            Self::Edge => "Microsoft Edge",
            Self::Chromium => "Chromium",
        }
    }

    /// The js-debug adapter type that launches it.
    pub fn adapter(self) -> &'static str {
        match self {
            Self::Edge => "pwa-msedge",
            _ => "pwa-chrome",
        }
    }
}

/// An installed browser.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Browser {
    pub kind: BrowserKind,
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Linux,
    MacOs,
    Windows,
}

impl Os {
    pub fn current() -> Self {
        if cfg!(windows) {
            Os::Windows
        } else if cfg!(target_os = "macos") {
            Os::MacOs
        } else {
            Os::Linux
        }
    }
}

/// Where to look: absolute paths and program names searched on the `PATH`.
fn candidates(kind: BrowserKind, os: Os, env: &dyn Fn(&str) -> Option<String>) -> Vec<String> {
    match os {
        Os::Linux => match kind {
            BrowserKind::Chrome => vec![
                "google-chrome".into(),
                "google-chrome-stable".into(),
                "/opt/google/chrome/chrome".into(),
            ],
            BrowserKind::Edge => vec![
                "microsoft-edge".into(),
                "microsoft-edge-stable".into(),
                "/opt/microsoft/msedge/msedge".into(),
            ],
            BrowserKind::Chromium => vec![
                "chromium".into(),
                "chromium-browser".into(),
                "/snap/bin/chromium".into(),
            ],
        },
        Os::MacOs => {
            let app = match kind {
                BrowserKind::Chrome => "Google Chrome.app/Contents/MacOS/Google Chrome",
                BrowserKind::Edge => "Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
                BrowserKind::Chromium => "Chromium.app/Contents/MacOS/Chromium",
            };
            let mut out = vec![format!("/Applications/{app}")];
            if let Some(home) = env("HOME") {
                out.push(format!("{home}/Applications/{app}"));
            }
            out
        }
        Os::Windows => {
            let rel = match kind {
                BrowserKind::Chrome => "Google\\Chrome\\Application\\chrome.exe",
                BrowserKind::Edge => "Microsoft\\Edge\\Application\\msedge.exe",
                BrowserKind::Chromium => "Chromium\\Application\\chrome.exe",
            };
            ["LOCALAPPDATA", "PROGRAMFILES", "PROGRAMFILES(X86)"]
                .iter()
                .filter_map(|v| env(v))
                .map(|base| format!("{base}\\{rel}"))
                .collect()
        }
    }
}

/// The installed browsers, Chrome first. `exists` checks a file, `env` reads environment
/// variables (`PATH`, `HOME`, `LOCALAPPDATA`, …).
pub fn find_browsers(
    os: Os,
    exists: &dyn Fn(&Path) -> bool,
    env: &dyn Fn(&str) -> Option<String>,
) -> Vec<Browser> {
    let sep = if os == Os::Windows { ';' } else { ':' };
    let path_dirs: Vec<String> = env("PATH")
        .map(|p| {
            p.split(sep)
                .filter(|d| !d.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let mut out = Vec::new();
    for kind in [
        BrowserKind::Chrome,
        BrowserKind::Edge,
        BrowserKind::Chromium,
    ] {
        let found = candidates(kind, os, env).into_iter().find_map(|c| {
            let absolute = c.starts_with('/') || c.contains('\\');
            if absolute {
                return exists(Path::new(&c)).then_some(c);
            }
            path_dirs.iter().find_map(|d| {
                let p = PathBuf::from(d).join(&c);
                exists(&p).then(|| p.to_string_lossy().into_owned())
            })
        });
        if let Some(path) = found {
            out.push(Browser {
                kind,
                name: kind.label().into(),
                path,
            });
        }
    }
    out
}

/// The browsers of this machine.
pub fn system_browsers() -> Vec<Browser> {
    find_browsers(Os::current(), &|p| p.is_file(), &|v| std::env::var(v).ok())
}

/// The browser for a choice (`auto`, `chrome`, `edge`, `chromium` or a path).
pub fn pick<'a>(browsers: &'a [Browser], choice: &str) -> Option<Browser> {
    match choice.trim() {
        "" | "auto" => browsers.first().cloned(),
        id => match BrowserKind::from_id(id) {
            Some(kind) => browsers.iter().find(|b| b.kind == kind).cloned(),
            // A path to a browser executable: Edge when it says so.
            None => Some(Browser {
                kind: if id.to_ascii_lowercase().contains("edge") {
                    BrowserKind::Edge
                } else {
                    BrowserKind::Chrome
                },
                name: Path::new(id)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| id.to_string()),
                path: id.to_string(),
            }),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    fn env(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = vars
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |k| map.get(k).cloned()
    }

    #[test]
    fn finds_browsers_on_the_path_and_at_install_paths() {
        let files: HashSet<PathBuf> = [
            PathBuf::from("/usr/bin/chromium"),
            PathBuf::from("/opt/microsoft/msedge/msedge"),
        ]
        .into();
        let exists = |p: &Path| files.contains(p);
        let found = find_browsers(
            Os::Linux,
            &exists,
            &env(&[("PATH", "/usr/local/bin:/usr/bin")]),
        );
        let rows: Vec<_> = found.iter().map(|b| (b.kind, b.path.as_str())).collect();
        assert_eq!(
            rows,
            [
                (BrowserKind::Edge, "/opt/microsoft/msedge/msedge"),
                (BrowserKind::Chromium, "/usr/bin/chromium")
            ]
        );
        assert_eq!(pick(&found, "auto").unwrap().kind, BrowserKind::Edge);
        assert_eq!(pick(&found, "chromium").unwrap().path, "/usr/bin/chromium");
        assert!(pick(&found, "chrome").is_none());
        assert_eq!(
            pick(&found, "/opt/edge-dev/msedge").unwrap().kind,
            BrowserKind::Edge
        );
    }

    #[test]
    fn windows_and_macos_install_paths() {
        let win = "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe";
        let found = find_browsers(
            Os::Windows,
            &|p: &Path| p.to_string_lossy() == win,
            &env(&[("PROGRAMFILES", "C:\\Program Files")]),
        );
        assert_eq!(found[0].path, win);
        let mac = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
        let found = find_browsers(Os::MacOs, &|p: &Path| p.to_string_lossy() == mac, &env(&[]));
        assert_eq!(
            (found[0].kind, found[0].path.as_str()),
            (BrowserKind::Chrome, mac)
        );
        assert_eq!(BrowserKind::Edge.adapter(), "pwa-msedge");
    }
}
