//! Installed Unity editors, found like Rider finds them: the Unity Hub's install folders
//! per OS, the Hub's secondary install path (`secondaryInstallPath.json`), editors located
//! manually in the Hub (`editors-v2.json`, older `editors.json`) and the editor of Settings.
//! The file system comes in as functions, so the search is tested with fixtures.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::version_key;

/// Where the user's machine keeps things.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EditorEnv {
    /// `linux`, `macos` or `windows`.
    pub os: String,
    pub home: PathBuf,
    /// The Hub's configuration folder: `~/.config/UnityHub`, `~/Library/Application
    /// Support/UnityHub` or `%APPDATA%\UnityHub`.
    pub hub_config: PathBuf,
    /// `C:\Program Files` (Windows).
    pub program_files: Option<PathBuf>,
}

impl EditorEnv {
    /// The environment of this machine.
    pub fn system(os: &str, home: PathBuf, appdata: Option<PathBuf>) -> Self {
        let hub_config = match os {
            "windows" => appdata
                .unwrap_or_else(|| home.join("AppData").join("Roaming"))
                .join("UnityHub"),
            "macos" => home.join("Library/Application Support/UnityHub"),
            _ => home.join(".config/UnityHub"),
        };
        Self {
            os: os.to_string(),
            program_files: (os == "windows").then(|| PathBuf::from(r"C:\Program Files")),
            home,
            hub_config,
        }
    }

    /// The Hub's default install folders.
    pub fn default_hub_dirs(&self) -> Vec<PathBuf> {
        match self.os.as_str() {
            "windows" => self
                .program_files
                .iter()
                .map(|p| p.join("Unity").join("Hub").join("Editor"))
                .collect(),
            "macos" => vec![PathBuf::from("/Applications/Unity/Hub/Editor")],
            _ => vec![self.home.join("Unity/Hub/Editor")],
        }
    }
}

/// How an editor was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EditorSource {
    Hub,
    /// Located manually in the Hub.
    HubManual,
    /// Settings | .NET | Unity.
    Custom,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnityEditor {
    pub version: String,
    /// The editor executable.
    pub path: String,
    pub source: EditorSource,
}

/// The executable below a Hub version folder.
pub fn executable_in(os: &str, version_dir: &Path) -> PathBuf {
    match os {
        "windows" => version_dir.join("Editor").join("Unity.exe"),
        "macos" => version_dir.join("Unity.app/Contents/MacOS/Unity"),
        _ => version_dir.join("Editor").join("Unity"),
    }
}

/// The version of an editor executable from its path (`…/2022.3.10f1/Editor/Unity`).
pub fn version_from_path(path: &str) -> Option<String> {
    path.split(['/', '\\'])
        .rev()
        .find(|seg| version_key(seg).is_some() && seg.contains('.'))
        .map(str::to_string)
}

fn path_string(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

/// The file system as the search sees it.
pub struct Fs<'a> {
    pub list_dirs: &'a dyn Fn(&Path) -> Vec<String>,
    pub is_file: &'a dyn Fn(&Path) -> bool,
    pub read: &'a dyn Fn(&Path) -> Option<String>,
}

/// Every editor found, newest first; `custom` is the editor of Settings.
pub fn find_editors(env: &EditorEnv, fs: &Fs, custom: Option<&str>) -> Vec<UnityEditor> {
    let mut out: Vec<UnityEditor> = Vec::new();
    let mut push = |e: UnityEditor| {
        if !out.iter().any(|o| o.path == e.path) {
            out.push(e);
        }
    };
    if let Some(c) = custom.map(str::trim).filter(|c| !c.is_empty()) {
        if (fs.is_file)(Path::new(c)) {
            push(UnityEditor {
                version: version_from_path(c).unwrap_or_default(),
                path: c.to_string(),
                source: EditorSource::Custom,
            });
        }
    }
    let mut dirs = env.default_hub_dirs();
    // `secondaryInstallPath.json` is a JSON string with the custom install folder.
    if let Some(text) = (fs.read)(&env.hub_config.join("secondaryInstallPath.json")) {
        if let Ok(Value::String(p)) = serde_json::from_str::<Value>(&text) {
            if !p.trim().is_empty() {
                dirs.push(PathBuf::from(p.trim()));
            }
        }
    }
    for dir in &dirs {
        let mut versions = (fs.list_dirs)(dir);
        versions.retain(|v| version_key(v).is_some());
        for v in versions {
            let exe = executable_in(&env.os, &dir.join(&v));
            if (fs.is_file)(&exe) {
                push(UnityEditor {
                    version: v,
                    path: path_string(&exe),
                    source: EditorSource::Hub,
                });
            }
        }
    }
    for file in ["editors-v2.json", "editors.json"] {
        if let Some(text) = (fs.read)(&env.hub_config.join(file)) {
            for (version, location) in manual_editors(&text) {
                if (fs.is_file)(Path::new(&location)) {
                    push(UnityEditor {
                        version,
                        path: location,
                        source: EditorSource::HubManual,
                    });
                }
            }
        }
    }
    out.sort_by(|a, b| {
        version_key(&b.version)
            .cmp(&version_key(&a.version))
            .then(a.path.cmp(&b.path))
    });
    out
}

/// Editors located manually in the Hub: `editors-v2.json` (`{ "data": [{ "version",
/// "location": [path] }] }`) and the older `editors.json` (`{ "<version>": { "location":
/// [path] } }`).
pub fn manual_editors(text: &str) -> Vec<(String, String)> {
    let Ok(v) = serde_json::from_str::<Value>(text) else {
        return Vec::new();
    };
    let entry = |e: &Value, key: Option<&str>| -> Option<(String, String)> {
        let version = e
            .get("version")
            .and_then(Value::as_str)
            .or(key)?
            .to_string();
        let location = match e.get("location")? {
            Value::Array(a) => a.first()?.as_str()?.to_string(),
            Value::String(s) => s.clone(),
            _ => return None,
        };
        Some((version, location))
    };
    if let Some(data) = v.get("data").and_then(Value::as_array) {
        return data.iter().filter_map(|e| entry(e, None)).collect();
    }
    v.as_object()
        .map(|m| {
            m.iter()
                .filter_map(|(k, e)| entry(e, Some(k.as_str())))
                .collect()
        })
        .unwrap_or_default()
}

/// The editor for a project version: the exact version, else the newest of the same
/// `major.minor` (Unity upgrades the project when it opens it, so it is only offered).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorMatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exact: Option<UnityEditor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closest: Option<UnityEditor>,
}

impl EditorMatch {
    /// The editor to use: Settings' editor, the exact version, else the closest one.
    pub fn best(&self) -> Option<&UnityEditor> {
        self.exact.as_ref().or(self.closest.as_ref())
    }
}

pub fn match_editor(editors: &[UnityEditor], version: &str) -> EditorMatch {
    if let Some(custom) = editors.iter().find(|e| e.source == EditorSource::Custom) {
        return EditorMatch {
            exact: Some(custom.clone()),
            closest: None,
        };
    }
    let exact = editors.iter().find(|e| e.version == version).cloned();
    let closest = if exact.is_some() {
        None
    } else {
        let want = version_key(version);
        editors
            .iter()
            .filter(|e| {
                let k = version_key(&e.version);
                matches!((k, want), (Some(k), Some(w)) if (k.0, k.1) == (w.0, w.1))
            })
            .max_by_key(|e| version_key(&e.version))
            .cloned()
    };
    EditorMatch { exact, closest }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};

    struct FakeFs {
        dirs: BTreeMap<PathBuf, Vec<String>>,
        files: BTreeSet<PathBuf>,
        texts: BTreeMap<PathBuf, String>,
    }

    impl FakeFs {
        fn search(&self, env: &EditorEnv, custom: Option<&str>) -> Vec<UnityEditor> {
            let list = |p: &Path| self.dirs.get(p).cloned().unwrap_or_default();
            let is_file = |p: &Path| self.files.contains(p);
            let read = |p: &Path| self.texts.get(p).cloned();
            find_editors(
                env,
                &Fs {
                    list_dirs: &list,
                    is_file: &is_file,
                    read: &read,
                },
                custom,
            )
        }
    }

    #[test]
    fn finds_hub_editors_per_os() {
        let home = PathBuf::from("/home/sam");
        let env = EditorEnv::system("linux", home.clone(), None);
        let hub = home.join("Unity/Hub/Editor");
        let fs = FakeFs {
            dirs: BTreeMap::from([
                (
                    hub.clone(),
                    vec!["2022.3.10f1".into(), "6000.0.23f1".into(), "notes".into()],
                ),
                (PathBuf::from("/data/unity"), vec!["2022.3.21f1".into()]),
            ]),
            files: BTreeSet::from([
                hub.join("2022.3.10f1/Editor/Unity"),
                hub.join("6000.0.23f1/Editor/Unity"),
                PathBuf::from("/data/unity/2022.3.21f1/Editor/Unity"),
                PathBuf::from("/opt/unity/2021.3.5f1/Editor/Unity"),
            ]),
            texts: BTreeMap::from([
                (
                    home.join(".config/UnityHub/secondaryInstallPath.json"),
                    "\"/data/unity\"".into(),
                ),
                (
                    home.join(".config/UnityHub/editors-v2.json"),
                    r#"{"schema_version":"v2","data":[{"version":"2021.3.5f1","location":["/opt/unity/2021.3.5f1/Editor/Unity"],"manual":true}]}"#.into(),
                ),
            ]),
        };
        let found = fs.search(&env, None);
        let versions: Vec<&str> = found.iter().map(|e| e.version.as_str()).collect();
        assert_eq!(
            versions,
            vec!["6000.0.23f1", "2022.3.21f1", "2022.3.10f1", "2021.3.5f1"]
        );
        assert_eq!(found[3].source, EditorSource::HubManual);

        let m = match_editor(&found, "2022.3.10f1");
        assert_eq!(
            m.exact.unwrap().path,
            "/home/sam/Unity/Hub/Editor/2022.3.10f1/Editor/Unity"
        );
        let m = match_editor(&found, "2022.3.15f1");
        assert!(m.exact.is_none());
        assert_eq!(m.best().unwrap().version, "2022.3.21f1");
        assert!(match_editor(&found, "2020.1.0f1").best().is_none());

        // Settings' editor wins.
        let custom = "/opt/unity/2021.3.5f1/Editor/Unity";
        let found = fs.search(&env, Some(custom));
        let m = match_editor(&found, "2022.3.10f1");
        assert_eq!(m.best().unwrap().source, EditorSource::Custom);
        assert_eq!(m.best().unwrap().version, "2021.3.5f1");
    }

    #[test]
    fn executables_and_hub_folders() {
        let mac = EditorEnv::system("macos", PathBuf::from("/Users/sam"), None);
        assert_eq!(
            mac.default_hub_dirs(),
            vec![PathBuf::from("/Applications/Unity/Hub/Editor")]
        );
        assert_eq!(
            executable_in(
                "macos",
                Path::new("/Applications/Unity/Hub/Editor/2022.3.10f1")
            ),
            PathBuf::from(
                "/Applications/Unity/Hub/Editor/2022.3.10f1/Unity.app/Contents/MacOS/Unity"
            )
        );
        let win = EditorEnv::system(
            "windows",
            PathBuf::from(r"C:\Users\sam"),
            Some(PathBuf::from(r"C:\Users\sam\AppData\Roaming")),
        );
        assert_eq!(
            win.hub_config,
            PathBuf::from(r"C:\Users\sam\AppData\Roaming").join("UnityHub")
        );
        assert!(
            path_string(&executable_in("windows", Path::new("D:/Hub/2022.3.10f1")))
                .ends_with("Unity.exe")
        );
        assert_eq!(
            version_from_path(r"C:\Program Files\Unity\Hub\Editor\2022.3.10f1\Editor\Unity.exe")
                .as_deref(),
            Some("2022.3.10f1")
        );
        assert_eq!(
            manual_editors(
                r#"{"2019.4.1f1": {"version": "2019.4.1f1", "location": ["/u/Unity"], "manual": true}}"#
            ),
            vec![("2019.4.1f1".to_string(), "/u/Unity".to_string())]
        );
    }
}
