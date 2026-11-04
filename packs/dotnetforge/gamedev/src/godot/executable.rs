//! The Godot executable: Settings | .NET | Godot, the `GODOT4` / `GODOT` variables (which
//! the C# tooling uses too), the `PATH` (`godot4`, `godot`, `godot-mono`, …), the Flatpak
//! export and the usual install folders (`/Applications/Godot*.app`, `Godot_v4*`
//! executables in `~/Applications`, `~/Downloads`, `~/.local/bin`). C# projects need a
//! .NET ("mono") build, so those are preferred for them.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GodotEnv {
    /// `linux`, `macos` or `windows`.
    pub os: String,
    pub home: PathBuf,
    pub path_dirs: Vec<PathBuf>,
    /// `GODOT4`, then `GODOT`.
    pub env_vars: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GodotExecutable {
    pub path: String,
    /// `setting`, `environment`, `path` or `folder`.
    pub source: String,
    /// A .NET build (its file or folder name says `mono` or `dotnet`).
    pub dotnet: bool,
}

const PATH_NAMES: &[&str] = &[
    "godot4",
    "godot",
    "Godot",
    "godot4-mono",
    "godot-mono",
    "godot-dotnet",
    "godot4-dotnet",
];

fn is_dotnet_build(path: &str) -> bool {
    let l = path.to_ascii_lowercase();
    l.contains("mono") || l.contains("dotnet")
}

/// A downloaded Godot executable: `Godot_v4.3-stable_linux.x86_64`,
/// `Godot_v4.3-stable_mono_linux.x86_64`, `Godot_v4.3-stable_win64.exe`.
pub fn is_godot_download(name: &str) -> bool {
    let l = name.to_ascii_lowercase();
    l.starts_with("godot_v")
        && !l.ends_with(".zip")
        && !l.ends_with(".tar.gz")
        && !l.ends_with(".tpz")
        && !l.contains("console")
        && !l.ends_with(".pck")
}

/// Every candidate in search order.
pub fn candidates(
    env: &GodotEnv,
    custom: Option<&str>,
    list_dir: &dyn Fn(&Path) -> Vec<String>,
) -> Vec<(PathBuf, &'static str)> {
    let mut out: Vec<(PathBuf, &'static str)> = Vec::new();
    if let Some(c) = custom.map(str::trim).filter(|c| !c.is_empty()) {
        out.push((PathBuf::from(c), "setting"));
    }
    for v in &env.env_vars {
        if !v.trim().is_empty() {
            out.push((PathBuf::from(v.trim()), "environment"));
        }
    }
    let exe = if env.os == "windows" { ".exe" } else { "" };
    for dir in &env.path_dirs {
        for name in PATH_NAMES {
            out.push((dir.join(format!("{name}{exe}")), "path"));
        }
    }
    match env.os.as_str() {
        "macos" => {
            for base in [
                PathBuf::from("/Applications"),
                env.home.join("Applications"),
            ] {
                for app in list_dir(&base) {
                    if app.starts_with("Godot") && app.ends_with(".app") {
                        out.push((base.join(app).join("Contents/MacOS/Godot"), "folder"));
                    }
                }
            }
        }
        "linux" => {
            out.push((
                PathBuf::from("/var/lib/flatpak/exports/bin/org.godotengine.Godot"),
                "folder",
            ));
            out.push((
                env.home
                    .join(".local/share/flatpak/exports/bin/org.godotengine.Godot"),
                "folder",
            ));
        }
        _ => {}
    }
    for base in [
        env.home.join("Applications"),
        env.home.join("Downloads"),
        env.home.join(".local/bin"),
        env.home.join("Godot"),
    ] {
        let mut names = list_dir(&base);
        names.sort();
        names.reverse();
        for name in names {
            if is_godot_download(&name) {
                out.push((base.join(&name), "folder"));
            }
            if name.to_ascii_lowercase().starts_with("godot_v") {
                // An extracted .NET build: `Godot_v4.3-stable_mono_linux_x86_64/Godot_v4.3-…`.
                for inner in list_dir(&base.join(&name)) {
                    if is_godot_download(&inner) {
                        out.push((base.join(&name).join(inner), "folder"));
                    }
                }
            }
        }
    }
    out
}

/// The Godot executable for a project; `csharp` prefers .NET builds.
pub fn find_godot(
    env: &GodotEnv,
    custom: Option<&str>,
    csharp: bool,
    is_file: &dyn Fn(&Path) -> bool,
    list_dir: &dyn Fn(&Path) -> Vec<String>,
) -> Option<GodotExecutable> {
    let found: Vec<GodotExecutable> = candidates(env, custom, list_dir)
        .into_iter()
        .filter(|(p, _)| is_file(p))
        .map(|(p, source)| {
            let path = p.to_string_lossy().into_owned();
            GodotExecutable {
                dotnet: is_dotnet_build(&path),
                path,
                source: source.to_string(),
            }
        })
        .collect();
    // Settings and the environment always win.
    if let Some(first) = found
        .iter()
        .find(|e| e.source == "setting" || e.source == "environment")
    {
        return Some(first.clone());
    }
    if csharp {
        if let Some(e) = found.iter().find(|e| e.dotnet) {
            return Some(e.clone());
        }
    }
    found.into_iter().next()
}

/// `godot --version`: `4.3.stable.mono.official.77dcf97d8`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GodotVersion {
    /// `4.3`, `4.2.2`.
    pub version: String,
    /// `stable`, `rc2`, `beta1`, `dev6`.
    pub status: String,
    pub dotnet: bool,
}

pub fn parse_version(output: &str) -> Option<GodotVersion> {
    let line = output
        .lines()
        .map(str::trim)
        .find(|l| l.chars().next().is_some_and(|c| c.is_ascii_digit()))?;
    let parts: Vec<&str> = line.split('.').collect();
    let mut version = Vec::new();
    let mut rest = parts.iter();
    for p in rest.by_ref() {
        if p.chars().all(|c| c.is_ascii_digit()) && !p.is_empty() {
            version.push(*p);
        } else {
            let status = p.to_string();
            let tail: Vec<&str> = rest.copied().collect();
            return Some(GodotVersion {
                version: version.join("."),
                dotnet: tail.contains(&"mono") || tail.contains(&"dotnet"),
                status,
            });
        }
    }
    (!version.is_empty()).then(|| GodotVersion {
        version: version.join("."),
        status: String::new(),
        dotnet: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};

    #[test]
    fn finds_godot() {
        let home = PathBuf::from("/home/sam");
        let env = GodotEnv {
            os: "linux".into(),
            home: home.clone(),
            path_dirs: vec![PathBuf::from("/usr/bin")],
            env_vars: vec![],
        };
        let dirs: BTreeMap<PathBuf, Vec<String>> = BTreeMap::from([
            (
                home.join("Downloads"),
                vec![
                    "Godot_v4.3-stable_linux.x86_64".to_string(),
                    "Godot_v4.3-stable_linux.x86_64.zip".into(),
                    "Godot_v4.3-stable_mono_linux_x86_64".into(),
                ],
            ),
            (
                home.join("Downloads/Godot_v4.3-stable_mono_linux_x86_64"),
                vec![
                    "Godot_v4.3-stable_mono_linux.x86_64".to_string(),
                    "GodotSharp".into(),
                ],
            ),
        ]);
        let files: BTreeSet<PathBuf> = BTreeSet::from([
            PathBuf::from("/usr/bin/godot"),
            home.join("Downloads/Godot_v4.3-stable_linux.x86_64"),
            home.join(
                "Downloads/Godot_v4.3-stable_mono_linux_x86_64/Godot_v4.3-stable_mono_linux.x86_64",
            ),
        ]);
        let list = |p: &Path| dirs.get(p).cloned().unwrap_or_default();
        let is_file = |p: &Path| files.contains(p);
        let gd = find_godot(&env, None, false, &is_file, &list).unwrap();
        assert_eq!(
            (gd.path.as_str(), gd.source.as_str()),
            ("/usr/bin/godot", "path")
        );
        let cs = find_godot(&env, None, true, &is_file, &list).unwrap();
        assert!(cs.dotnet);
        assert!(cs.path.ends_with("Godot_v4.3-stable_mono_linux.x86_64"));
        let custom = find_godot(
            &env,
            Some("/home/sam/Downloads/Godot_v4.3-stable_linux.x86_64"),
            true,
            &is_file,
            &list,
        )
        .unwrap();
        assert_eq!(custom.source, "setting");
        let env_var = GodotEnv {
            env_vars: vec!["/usr/bin/godot".into()],
            ..env.clone()
        };
        assert_eq!(
            find_godot(&env_var, None, true, &is_file, &list)
                .unwrap()
                .source,
            "environment"
        );
        assert!(find_godot(&env, None, false, &|_| false, &list).is_none());
    }

    #[test]
    fn mac_apps_and_versions() {
        let env = GodotEnv {
            os: "macos".into(),
            home: PathBuf::from("/Users/sam"),
            path_dirs: vec![],
            env_vars: vec![],
        };
        let list = |p: &Path| {
            if p == Path::new("/Applications") {
                vec!["Godot_mono.app".to_string(), "Safari.app".into()]
            } else {
                vec![]
            }
        };
        let c = candidates(&env, None, &list);
        assert!(c
            .iter()
            .any(|(p, _)| p == Path::new("/Applications/Godot_mono.app/Contents/MacOS/Godot")));
        assert_eq!(
            parse_version("4.3.stable.mono.official.77dcf97d8\n").unwrap(),
            GodotVersion {
                version: "4.3".into(),
                status: "stable".into(),
                dotnet: true
            }
        );
        assert_eq!(
            parse_version("Godot Engine v4.2\n4.2.2.rc1.official.abc")
                .unwrap()
                .version,
            "4.2.2"
        );
        assert!(parse_version("command not found").is_none());
    }
}
