//! CMake profiles (CLion's Settings | Build, Execution, Deployment | CMake): a build type,
//! toolchain, generator, CMake options, build folder, build options and environment. The
//! user's profiles come from the setting `cmake.profiles`; configure presets of
//! `CMakePresets.json` / `CMakeUserPresets.json` become profiles too. This module also builds
//! the command lines of configure, build, clean and CTest.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::presets::{MacroContext, Presets};
use crate::sanitizers;

/// A CMake profile.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Profile {
    pub name: String,
    /// `Debug`, `Release`, `RelWithDebInfo`, `MinSizeRel` or a custom type.
    pub build_type: String,
    /// A toolchain id; empty: the default toolchain.
    pub toolchain: String,
    /// `Ninja`, `Unix Makefiles`, …; empty: the toolchain's default.
    pub generator: String,
    /// Extra arguments for configure (`-DSHAPES_TESTS=ON`).
    pub cmake_options: String,
    /// Relative to the project or absolute; empty: `cmake-build-<name>`.
    pub build_directory: String,
    /// Extra arguments for `cmake --build` (`-j 8`, `-- -k 0`).
    pub build_options: String,
    /// `KEY=VALUE` entries.
    pub environment: Vec<String>,
    pub enabled: bool,
    /// The configure preset it comes from (read-only in the settings).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    /// The preset's cache variables and toolchain file.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub cache_variables: BTreeMap<String, String>,
    /// Enabled sanitizers (`address`, `undefined`, `thread`, `memory`, `leak`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub sanitizers: Vec<String>,
    /// Why the sanitizers cannot be combined.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sanitizer_error: Option<String>,
}

/// One table of the `cmake.profiles` setting (setting tables hold strings).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProfileEntry {
    pub name: String,
    pub build_type: String,
    pub toolchain: String,
    pub generator: String,
    pub cmake_options: String,
    pub build_directory: String,
    pub build_options: String,
    /// `KEY=VALUE` entries separated by `;`.
    pub environment: String,
    /// `false` disables the profile; anything else enables it.
    pub enabled: String,
    /// Sanitizers separated by commas (`address,undefined`).
    pub sanitizers: String,
}

/// CLion's build folder name of a profile: `cmake-build-debug`, `cmake-build-release-clang`.
pub fn default_build_dir(name: &str) -> String {
    let mut slug = String::new();
    for c in name.trim().to_ascii_lowercase().chars() {
        let c = if c.is_ascii_alphanumeric() { c } else { '-' };
        if c != '-' || !slug.ends_with('-') {
            slug.push(c);
        }
    }
    let slug = slug.trim_matches('-').to_string();
    format!(
        "cmake-build-{}",
        if slug.is_empty() { "default" } else { &slug }
    )
}

impl Profile {
    /// CLion's first profile.
    pub fn debug() -> Self {
        Self {
            name: "Debug".into(),
            build_type: "Debug".into(),
            enabled: true,
            ..Self::default()
        }
    }

    pub fn from_entry(e: &ProfileEntry) -> Self {
        Self {
            name: e.name.trim().to_string(),
            build_type: if e.build_type.trim().is_empty() {
                "Debug".into()
            } else {
                e.build_type.trim().to_string()
            },
            toolchain: e.toolchain.trim().to_string(),
            generator: e.generator.trim().to_string(),
            cmake_options: e.cmake_options.trim().to_string(),
            build_directory: e.build_directory.trim().to_string(),
            build_options: e.build_options.trim().to_string(),
            environment: e
                .environment
                .split([';', '\n'])
                .map(str::trim)
                .filter(|v| v.contains('='))
                .map(str::to_string)
                .collect(),
            enabled: e.enabled.trim() != "false",
            preset: None,
            cache_variables: BTreeMap::new(),
            sanitizer_error: sanitizers::conflict(&sanitizers::parse_list(&e.sanitizers)),
            sanitizers: sanitizers::parse_list(&e.sanitizers),
        }
    }

    /// The build folder relative to the project (or absolute).
    pub fn build_dir(&self) -> String {
        let d = self.build_directory.trim().trim_end_matches(['/', '\\']);
        if d.is_empty() {
            default_build_dir(&self.name)
        } else {
            d.strip_prefix("./").unwrap_or(d).to_string()
        }
    }

    /// The environment as a map.
    pub fn env(&self) -> BTreeMap<String, String> {
        self.environment
            .iter()
            .filter_map(|e| e.split_once('='))
            .map(|(k, v)| (k.trim().to_string(), v.to_string()))
            .collect()
    }
}

/// The profiles of a project: the user's (or a Debug profile when there are none) and the
/// project's configure presets. Presets with a `binaryDir` outside the project keep it.
pub fn profiles(entries: &[ProfileEntry], presets: &Presets, ctx: &MacroContext) -> Vec<Profile> {
    let mut out: Vec<Profile> = entries
        .iter()
        .filter(|e| !e.name.trim().is_empty())
        .map(Profile::from_entry)
        .collect();
    if out.is_empty() {
        out.push(Profile::debug());
    }
    let source = ctx.source_dir.trim_end_matches(['/', '\\']);
    for p in presets.visible_configure_presets(ctx) {
        if out
            .iter()
            .any(|o| o.name == p.display_name || o.preset.as_deref() == Some(&p.name))
        {
            continue;
        }
        let build = p
            .binary_dir
            .clone()
            .map(|d| {
                d.strip_prefix(source)
                    .map(|r| r.trim_start_matches(['/', '\\']).to_string())
                    .unwrap_or(d)
            })
            .unwrap_or_default();
        let mut cache = p.cache_variables.clone();
        if let Some(t) = &p.toolchain_file {
            cache.insert("CMAKE_TOOLCHAIN_FILE".into(), t.clone());
        }
        out.push(Profile {
            name: p.display_name.clone(),
            build_type: p.build_type().unwrap_or("Debug").to_string(),
            toolchain: String::new(),
            generator: p.generator.clone().unwrap_or_default(),
            cmake_options: String::new(),
            build_directory: build,
            build_options: String::new(),
            environment: p
                .environment
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect(),
            enabled: true,
            preset: Some(p.name.clone()),
            cache_variables: cache,
            sanitizers: Vec::new(),
            sanitizer_error: None,
        });
    }
    out
}

/// Split an options string like a shell (quotes group, backslashes escape).
pub fn split_args(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut any = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (None, '"' | '\'') => {
                quote = Some(c);
                any = true;
            }
            (None, c) if c.is_whitespace() => {
                if any || !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                    any = false;
                }
            }
            (q, '\\') if q != Some('\'') => match chars.peek() {
                Some(&n) if n == '"' || n == '\\' || (q.is_none() && n.is_whitespace()) => {
                    cur.push(n);
                    chars.next();
                }
                _ => cur.push('\\'),
            },
            (_, c) => cur.push(c),
        }
    }
    if any || !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// A program with arguments.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Command {
    pub program: String,
    pub args: Vec<String>,
    /// Absolute.
    pub cwd: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
}

/// Everything a configure needs besides the profile.
#[derive(Debug, Clone, Default)]
pub struct ConfigureContext {
    pub cmake: String,
    /// Absolute project folder.
    pub source_dir: String,
    /// The toolchain's generator when the profile has none.
    pub default_generator: String,
    /// `CMAKE_C_COMPILER`, `CMAKE_CXX_COMPILER`, `CMAKE_MAKE_PROGRAM` of the toolchain.
    pub toolchain_variables: BTreeMap<String, String>,
    /// A package manager's toolchain file (Conan's `conan_toolchain.cmake`, vcpkg's
    /// `vcpkg.cmake`), passed unless the profile names its own `CMAKE_TOOLCHAIN_FILE`.
    pub package_toolchain: Option<String>,
}

/// `-DNAME=value` / `-DNAME:TYPE=value` definitions of an options string.
fn definitions(args: &[String]) -> BTreeMap<String, String> {
    args.iter()
        .filter_map(|a| a.strip_prefix("-D"))
        .filter_map(|d| d.split_once('='))
        .map(|(k, v)| (k.split(':').next().unwrap_or(k).to_string(), v.to_string()))
        .collect()
}

fn absolute(source: &str, dir: &str) -> String {
    if dir.starts_with('/') || dir.get(1..2) == Some(":") {
        dir.to_string()
    } else {
        format!("{}/{}", source.trim_end_matches(['/', '\\']), dir)
    }
}

/// `cmake -S <src> -B <build> -G <generator> -DCMAKE_BUILD_TYPE=… <toolchain> <options>`, or
/// `cmake --preset <name>` for preset profiles.
pub fn configure_command(p: &Profile, ctx: &ConfigureContext) -> Command {
    let build = absolute(&ctx.source_dir, &p.build_dir());
    let mut args: Vec<String> = Vec::new();
    let options = split_args(&p.cmake_options);
    let mut defined = p.cache_variables.clone();
    defined.extend(definitions(&options));
    if let Some(preset) = &p.preset {
        args.push(format!("--preset={preset}"));
        args.push("-B".into());
        args.push(build);
    } else {
        args.extend(["-S".into(), ctx.source_dir.clone(), "-B".into(), build]);
        let generator = if p.generator.is_empty() {
            &ctx.default_generator
        } else {
            &p.generator
        };
        if !generator.is_empty() {
            args.push("-G".into());
            args.push(generator.clone());
        }
        if !p.build_type.is_empty() {
            args.push(format!("-DCMAKE_BUILD_TYPE={}", p.build_type));
        }
        for (k, v) in &ctx.toolchain_variables {
            args.push(format!("-D{k}={v}"));
        }
        args.push("-DCMAKE_EXPORT_COMPILE_COMMANDS=ON".into());
        // The toolchain names a C compiler for C++-only projects too.
        args.push("--no-warn-unused-cli".into());
    }
    if let Some(file) = ctx.package_toolchain.as_deref().filter(|f| !f.is_empty()) {
        if !defined.contains_key("CMAKE_TOOLCHAIN_FILE") {
            args.push(format!("-DCMAKE_TOOLCHAIN_FILE={file}"));
        }
    }
    // Sanitizer flags extend the flags the options set (those definitions are dropped).
    let sanitizer_defs = sanitizers::cmake_definitions(&p.sanitizers, &defined);
    let replaced: Vec<String> = sanitizer_defs
        .iter()
        .filter_map(|d| {
            d.strip_prefix("-D")?
                .split_once('=')
                .map(|(k, _)| k.to_string())
        })
        .collect();
    args.extend(options.into_iter().filter(|o| {
        let name = o
            .strip_prefix("-D")
            .and_then(|d| d.split_once('='))
            .map(|(k, _)| k.split(':').next().unwrap_or(k));
        !name.is_some_and(|n| replaced.iter().any(|r| r == n))
    }));
    args.extend(sanitizer_defs);
    Command {
        program: ctx.cmake.clone(),
        args,
        cwd: ctx.source_dir.clone(),
        env: p.env(),
    }
}

/// What a build action does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BuildAction {
    Build,
    /// Clean, then build (`--clean-first`).
    Rebuild,
    Clean,
}

/// `cmake --build <build> [--config <type>] [--target <t>] [--clean-first] <build options>`.
/// `multi_config`: the generator builds several configurations (Visual Studio, Xcode,
/// Ninja Multi-Config), which need `--config`.
pub fn build_command(
    p: &Profile,
    cmake: &str,
    source_dir: &str,
    targets: &[String],
    action: BuildAction,
    multi_config: bool,
) -> Command {
    let build = absolute(source_dir, &p.build_dir());
    let mut args: Vec<String> = vec!["--build".into(), build];
    if multi_config && !p.build_type.is_empty() {
        args.push("--config".into());
        args.push(p.build_type.clone());
    }
    match action {
        BuildAction::Clean => {
            args.push("--target".into());
            args.push("clean".into());
        }
        BuildAction::Build | BuildAction::Rebuild => {
            if !targets.is_empty() {
                args.push("--target".into());
                args.extend(targets.iter().cloned());
            }
            if action == BuildAction::Rebuild {
                args.push("--clean-first".into());
            }
        }
    }
    // Build options may pass native tool flags after `--`.
    args.extend(split_args(&p.build_options));
    Command {
        program: cmake.to_string(),
        args,
        cwd: source_dir.to_string(),
        env: p.env(),
    }
}

/// The CTest next to a CMake (`/usr/bin/cmake` → `/usr/bin/ctest`).
pub fn ctest_next_to(cmake: &str) -> String {
    let (dir, file) = match cmake.rfind(['/', '\\']) {
        Some(i) => (&cmake[..=i], &cmake[i + 1..]),
        None => ("", cmake),
    };
    let exe = if file.to_ascii_lowercase().ends_with(".exe") {
        ".exe"
    } else {
        ""
    };
    format!("{dir}ctest{exe}")
}

/// The absolute build folder of a profile.
pub fn build_dir_of(p: &Profile, source_dir: &str) -> String {
    absolute(source_dir, &p.build_dir())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presets::load;

    #[test]
    fn profiles_from_settings_and_presets() {
        let presets = load(&|f| {
            (f == crate::presets::PROJECT_FILE).then(|| {
                r#"{"version": 6, "configurePresets": [
                    {"name": "ninja-release", "displayName": "Release (Ninja)", "generator": "Ninja",
                     "binaryDir": "${sourceDir}/build/${presetName}", "toolchainFile": "/opt/tc.cmake",
                     "cacheVariables": {"CMAKE_BUILD_TYPE": "Release"}}]}"#
                    .to_string()
            })
        });
        let ctx = MacroContext {
            source_dir: "/home/dev/shapes".into(),
            host_system_name: "Linux".into(),
            env: BTreeMap::new(),
        };
        let none = profiles(&[], &presets, &ctx);
        assert_eq!(none[0], Profile::debug());
        assert_eq!(none[0].build_dir(), "cmake-build-debug");
        let release = &none[1];
        assert_eq!(release.name, "Release (Ninja)");
        assert_eq!(release.build_type, "Release");
        assert_eq!(release.build_dir(), "build/ninja-release");
        assert_eq!(release.preset.as_deref(), Some("ninja-release"));
        assert_eq!(
            release.cache_variables["CMAKE_TOOLCHAIN_FILE"],
            "/opt/tc.cmake"
        );
        let entries = [ProfileEntry {
            name: "Release Clang".into(),
            build_type: "Release".into(),
            toolchain: "system-clang".into(),
            cmake_options: "-DSHAPES_TESTS=OFF \"-DCMAKE_CXX_FLAGS=-O3 -march=native\"".into(),
            environment: "CCACHE_DISABLE=1; ASAN_OPTIONS=detect_leaks=0".into(),
            enabled: "false".into(),
            ..ProfileEntry::default()
        }];
        let list = profiles(&entries, &presets, &ctx);
        assert_eq!(list.len(), 2);
        let p = &list[0];
        assert!(!p.enabled);
        assert_eq!(p.build_dir(), "cmake-build-release-clang");
        assert_eq!(p.env()["ASAN_OPTIONS"], "detect_leaks=0");
    }

    #[test]
    fn configure_and_build_commands() {
        let mut p = Profile::debug();
        p.cmake_options = "-DSHAPES_TESTS=ON \"-DCMAKE_CXX_FLAGS=-Wall -Wextra\"".into();
        p.build_options = "-j 8".into();
        let ctx = ConfigureContext {
            cmake: "/usr/bin/cmake".into(),
            source_dir: "/home/dev/shapes".into(),
            default_generator: "Ninja".into(),
            toolchain_variables: [("CMAKE_CXX_COMPILER".to_string(), "/usr/bin/g++".to_string())]
                .into(),
            package_toolchain: None,
        };
        let c = configure_command(&p, &ctx);
        assert_eq!(
            c.args,
            [
                "-S",
                "/home/dev/shapes",
                "-B",
                "/home/dev/shapes/cmake-build-debug",
                "-G",
                "Ninja",
                "-DCMAKE_BUILD_TYPE=Debug",
                "-DCMAKE_CXX_COMPILER=/usr/bin/g++",
                "-DCMAKE_EXPORT_COMPILE_COMMANDS=ON",
                "--no-warn-unused-cli",
                "-DSHAPES_TESTS=ON",
                "-DCMAKE_CXX_FLAGS=-Wall -Wextra",
            ]
        );
        let b = build_command(
            &p,
            "/usr/bin/cmake",
            "/home/dev/shapes",
            &["shapes_app".into()],
            BuildAction::Rebuild,
            false,
        );
        assert_eq!(
            b.args,
            [
                "--build",
                "/home/dev/shapes/cmake-build-debug",
                "--target",
                "shapes_app",
                "--clean-first",
                "-j",
                "8"
            ]
        );
        let clean = build_command(&p, "cmake", "/p", &[], BuildAction::Clean, true);
        assert_eq!(
            clean.args,
            [
                "--build",
                "/p/cmake-build-debug",
                "--config",
                "Debug",
                "--target",
                "clean",
                "-j",
                "8"
            ]
        );
        let mut preset = Profile::debug();
        preset.preset = Some("debug".into());
        preset.build_directory = "build/debug".into();
        assert_eq!(
            configure_command(&preset, &ctx).args,
            ["--preset=debug", "-B", "/home/dev/shapes/build/debug"]
        );
        assert_eq!(ctest_next_to("/usr/bin/cmake"), "/usr/bin/ctest");
        assert_eq!(
            ctest_next_to("C:\\CMake\\bin\\cmake.exe"),
            "C:\\CMake\\bin\\ctest.exe"
        );
        assert_eq!(ctest_next_to("cmake"), "ctest");
        assert_eq!(build_dir_of(&p, "/x"), "/x/cmake-build-debug");
    }

    #[test]
    fn sanitizers_and_package_toolchains() {
        let entry = ProfileEntry {
            name: "ASan".into(),
            sanitizers: "address, undefined".into(),
            cmake_options: "\"-DCMAKE_CXX_FLAGS=-Wall\" -DX=1".into(),
            ..ProfileEntry::default()
        };
        let p = Profile::from_entry(&entry);
        assert_eq!(p.sanitizers, ["address", "undefined"]);
        assert_eq!(p.sanitizer_error, None);
        let ctx = ConfigureContext {
            cmake: "cmake".into(),
            source_dir: "/p".into(),
            default_generator: String::new(),
            toolchain_variables: BTreeMap::new(),
            package_toolchain: Some("/p/cmake-build-asan/conan_toolchain.cmake".into()),
        };
        let args = configure_command(&p, &ctx).args;
        assert!(args.contains(
            &"-DCMAKE_TOOLCHAIN_FILE=/p/cmake-build-asan/conan_toolchain.cmake".to_string()
        ));
        assert!(args.contains(&"-DX=1".to_string()));
        assert!(!args.contains(&"-DCMAKE_CXX_FLAGS=-Wall".to_string()));
        assert!(args.contains(
            &"-DCMAKE_CXX_FLAGS=-Wall -fsanitize=address,undefined -fno-omit-frame-pointer"
                .to_string()
        ));
        assert!(args.contains(&"-DCMAKE_EXE_LINKER_FLAGS=-fsanitize=address,undefined".to_string()));
        // The profile's own toolchain file wins.
        let mut own = p.clone();
        own.cmake_options = "-DCMAKE_TOOLCHAIN_FILE:FILEPATH=/opt/arm.cmake".into();
        let args = configure_command(&own, &ctx).args;
        assert_eq!(
            args.iter()
                .filter(|a| a.contains("CMAKE_TOOLCHAIN_FILE"))
                .count(),
            1
        );
        let bad = Profile::from_entry(&ProfileEntry {
            name: "x".into(),
            sanitizers: "thread,address".into(),
            ..ProfileEntry::default()
        });
        assert!(bad.sanitizer_error.is_some());
    }

    #[test]
    fn shell_like_splitting() {
        assert_eq!(
            split_args(r#"a "b c" 'd e' f\ g "" h"#),
            ["a", "b c", "d e", "f g", "", "h"]
        );
        assert!(split_args("   ").is_empty());
        assert_eq!(
            default_build_dir("Release (Clang)"),
            "cmake-build-release-clang"
        );
    }
}
