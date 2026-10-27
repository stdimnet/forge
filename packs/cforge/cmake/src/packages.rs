//! C/C++ package managers of CMake projects: vcpkg in manifest mode (`vcpkg.json`, the vcpkg
//! root, its CMake toolchain file, port names for completion) and Conan 2 (`conanfile.txt` /
//! `conanfile.py`, `conan install` per profile and the generated `conan_toolchain.cmake`).
//! Neither tool is downloaded: the user installs them.

use serde::{Deserialize, Serialize};

use crate::complete::{Completion, Item, ItemKind};
use crate::profile::Command;

/// The vcpkg manifest.
pub const VCPKG_MANIFEST: &str = "vcpkg.json";
/// Conan recipes, in the order Conan prefers them.
pub const CONANFILES: [&str; 2] = ["conanfile.py", "conanfile.txt"];

/// What the project's package managers look like.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageManagers {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vcpkg: Option<Vcpkg>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conan: Option<Conan>,
    /// The toolchain file Forge passes to CMake for the active profile (Conan's, else
    /// vcpkg's), unless the profile sets its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toolchain_file: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Vcpkg {
    /// The vcpkg installation (`VCPKG_ROOT`), when found.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toolchain_file: Option<String>,
    /// Port names of the manifest's `dependencies`.
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Conan {
    /// `conanfile.py` or `conanfile.txt`.
    pub conanfile: String,
    /// `conan_toolchain.cmake` of the active profile once `conan install` ran.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toolchain_file: Option<String>,
}

/// Port names of a manifest's `dependencies` (strings or `{ "name": … }` objects).
pub fn vcpkg_dependencies(manifest: &str) -> Vec<String> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(manifest) else {
        return Vec::new();
    };
    v.get("dependencies")
        .and_then(|d| d.as_array())
        .map(|deps| {
            deps.iter()
                .filter_map(|d| {
                    d.as_str()
                        .or_else(|| d.get("name").and_then(|n| n.as_str()))
                        .map(str::to_string)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// `<root>/scripts/buildsystems/vcpkg.cmake`.
pub fn vcpkg_toolchain(root: &str) -> String {
    format!(
        "{}/scripts/buildsystems/vcpkg.cmake",
        root.trim_end_matches(['/', '\\']).replace('\\', "/")
    )
}

/// The vcpkg root: the setting, `VCPKG_ROOT`, `VCPKG_INSTALLATION_ROOT` (GitHub runners,
/// Visual Studio), as long as its toolchain file exists.
pub fn vcpkg_root(
    setting: Option<&str>,
    env: &dyn Fn(&str) -> Option<String>,
    is_file: &dyn Fn(&str) -> bool,
) -> Option<String> {
    let candidates = [
        setting.map(str::to_string),
        env("VCPKG_ROOT"),
        env("VCPKG_INSTALLATION_ROOT"),
    ];
    candidates
        .into_iter()
        .flatten()
        .map(|c| c.trim().trim_end_matches(['/', '\\']).to_string())
        .filter(|c| !c.is_empty())
        .find(|c| is_file(&vcpkg_toolchain(c)))
}

/// `conan install . -of <build> -s build_type=<type> --build=missing [-pr <profile>] <extra>`.
pub fn conan_install_command(
    conan: &str,
    source_dir: &str,
    build_dir: &str,
    build_type: &str,
    conan_profile: &str,
    extra: &[String],
) -> Command {
    let mut args: Vec<String> = vec!["install".into(), ".".into(), "-of".into(), build_dir.into()];
    if !build_type.trim().is_empty() {
        args.push("-s".into());
        args.push(format!("build_type={}", build_type.trim()));
    }
    args.push("--build=missing".into());
    if !conan_profile.trim().is_empty() {
        args.push("-pr".into());
        args.push(conan_profile.trim().into());
    }
    args.extend(extra.iter().cloned());
    Command {
        program: conan.into(),
        args,
        cwd: source_dir.into(),
        env: Default::default(),
    }
}

/// Where `conan install -of <build>` writes `conan_toolchain.cmake`: the output folder itself,
/// its `generators/` folder, or `build/<type>/generators` with `cmake_layout`.
pub fn conan_toolchain_candidates(build_dir: &str, build_type: &str) -> Vec<String> {
    let b = build_dir.trim_end_matches(['/', '\\']).replace('\\', "/");
    let mut out = vec![
        format!("{b}/conan_toolchain.cmake"),
        format!("{b}/generators/conan_toolchain.cmake"),
    ];
    if !build_type.is_empty() {
        out.push(format!(
            "{b}/build/{build_type}/generators/conan_toolchain.cmake"
        ));
        out.push(format!("{b}/{build_type}/generators/conan_toolchain.cmake"));
    }
    out.push(format!("{b}/build/generators/conan_toolchain.cmake"));
    out
}

/// Port name completion in `vcpkg.json`: inside a string of the `dependencies` array (or an
/// object's `"name"` there).
pub fn complete_vcpkg(text: &str, offset: usize, ports: &[String]) -> Option<Completion> {
    let offset = offset.min(text.len());
    let before = &text[..offset];
    // Inside a string: an odd number of unescaped quotes since the last structural char.
    let quote = before.rfind('"')?;
    let in_string = before.matches('"').count() % 2 == 1;
    if !in_string {
        return None;
    }
    let prefix = &before[quote + 1..];
    if prefix.contains(['\n', '\\']) {
        return None;
    }
    // The enclosing array must be `"dependencies": [`.
    let head = &before[..quote];
    let deps = head.rfind("\"dependencies\"")?;
    let between = &head[deps..];
    let opens = between.matches('[').count();
    let closes = between.matches(']').count();
    if opens <= closes {
        return None;
    }
    // In an object only the value of "name" is a port.
    let trimmed = head.trim_end();
    if trimmed.ends_with(':') {
        let key_end = trimmed[..trimmed.len() - 1].trim_end();
        if !key_end.ends_with("\"name\"") {
            return None;
        }
    }
    let lower = prefix.to_ascii_lowercase();
    let mut items: Vec<Item> = ports
        .iter()
        .filter(|p| p.to_ascii_lowercase().starts_with(&lower))
        .take(200)
        .map(|p| Item {
            label: p.clone(),
            kind: ItemKind::Package,
            detail: "vcpkg port".into(),
            apply: p.clone(),
        })
        .collect();
    items.sort_by(|a, b| a.label.cmp(&b.label));
    Some(Completion {
        from: quote + 1,
        items,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vcpkg_manifest_root_and_toolchain() {
        let m =
            r#"{ "name": "shapes", "dependencies": ["fmt", { "name": "gtest", "host": true }] }"#;
        assert_eq!(vcpkg_dependencies(m), ["fmt", "gtest"]);
        assert!(vcpkg_dependencies("not json").is_empty());
        let env = |k: &str| (k == "VCPKG_ROOT").then(|| "/opt/vcpkg/".to_string());
        let is_file = |p: &str| p == "/opt/vcpkg/scripts/buildsystems/vcpkg.cmake";
        assert_eq!(vcpkg_root(None, &env, &is_file), Some("/opt/vcpkg".into()));
        assert_eq!(
            vcpkg_root(Some("/nowhere"), &env, &is_file),
            Some("/opt/vcpkg".into())
        );
        assert_eq!(vcpkg_root(None, &|_| None, &is_file), None);
        assert_eq!(
            vcpkg_toolchain("C:\\vcpkg\\"),
            "C:/vcpkg/scripts/buildsystems/vcpkg.cmake"
        );
    }

    #[test]
    fn conan_install_and_toolchain_files() {
        let c = conan_install_command("conan", "/p", "/p/cmake-build-debug", "Debug", "", &[]);
        assert_eq!(
            c.args.join(" "),
            "install . -of /p/cmake-build-debug -s build_type=Debug --build=missing"
        );
        assert_eq!(c.cwd, "/p");
        let c = conan_install_command(
            "conan",
            "/p",
            "b",
            "Release",
            "clang",
            &["-c".into(), "x=1".into()],
        );
        assert!(c
            .args
            .join(" ")
            .ends_with("--build=missing -pr clang -c x=1"));
        let cands = conan_toolchain_candidates("/p/b/", "Debug");
        assert_eq!(cands[0], "/p/b/conan_toolchain.cmake");
        assert!(cands.contains(&"/p/b/build/Debug/generators/conan_toolchain.cmake".to_string()));
    }

    #[test]
    fn completes_ports_in_dependencies() {
        let ports: Vec<String> = ["fmt", "flatbuffers", "gtest", "zlib"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let text = "{\n  \"name\": \"shapes\",\n  \"dependencies\": [\n    \"f";
        let c = complete_vcpkg(text, text.len(), &ports).unwrap();
        assert_eq!(c.from, text.len() - 1);
        let labels: Vec<_> = c.items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels, ["flatbuffers", "fmt"]);
        let obj = "{ \"dependencies\": [ { \"name\": \"g";
        assert_eq!(
            complete_vcpkg(obj, obj.len(), &ports).unwrap().items[0].label,
            "gtest"
        );
        let host = "{ \"dependencies\": [ { \"name\": \"gtest\", \"host\": \"t";
        assert!(complete_vcpkg(host, host.len(), &ports).is_none());
        let top = "{ \"na";
        assert!(complete_vcpkg(top, top.len(), &ports).is_none());
        let closed = "{ \"dependencies\": [\"fmt\"], \"builtin-baseline\": \"a";
        assert!(complete_vcpkg(closed, closed.len(), &ports).is_none());
    }
}
