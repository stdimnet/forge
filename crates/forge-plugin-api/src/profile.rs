//! Launch profiles: which packs a Forge process loads.
//!
//! `forge` loads every pack. Started under a pack's name (`phpforge`, `webforge`, … — a
//! symlink or a copy of the binary) or with `--packs <list>`, Forge loads the core, the
//! named packs and what their plugins depend on; the plugins of the other packs are off for
//! that process. The profile is not a setting: `plugins.disabled` keeps what the user chose.
//!
//! The profile is looked up once at startup, in this order: the `--packs` flag, the
//! `FORGE_PACKS` environment variable, the program name. The process then exports
//! `FORGE_PACKS`, so the windows it starts and a restart keep the profile.

use serde::{Deserialize, Serialize};

/// `--packs phpforge,webforge`, `--packs=none`, `--packs all`.
pub const PACKS_FLAG: &str = "--packs";
/// The same list as an environment variable.
pub const PACKS_ENV: &str = "FORGE_PACKS";

/// Packs every profile loads: the core and the config and script file types.
pub const BASE_PACKS: &[&str] = &["core", "configforge", "scriptforge"];
/// Pack of installed third-party plugins (`forge_plugins::store::THIRD_PARTY_PACK`); the
/// user installed them, so every profile loads them.
const THIRD_PARTY_PACK: &str = "thirdParty";
/// Plugins a pack loads from another pack although no manifest depends on them: the
/// templates of these packs are HTML with CSS and JavaScript.
const COMPANIONS: &[(&str, &[&str])] = &[
    ("phpforge", &["forge.web"]),
    ("pyforge", &["forge.web"]),
    ("rubyforge", &["forge.web"]),
    ("javaforge", &["forge.web"]),
    ("dotnetforge", &["forge.web"]),
];

/// The packs a Forge process was asked to load.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum LaunchProfile {
    /// Every pack (plain `forge`).
    #[default]
    All,
    /// The core plus these packs; empty for the core alone (`--packs none`).
    Packs(Vec<String>),
}

impl LaunchProfile {
    /// `all` (or nothing) is every pack, `none` the core alone, otherwise a list of pack
    /// ids separated by commas. Case and spaces do not matter.
    pub fn parse(spec: &str) -> Self {
        let mut packs: Vec<String> = Vec::new();
        for name in spec.split(',') {
            let name = name.trim().to_ascii_lowercase();
            match name.as_str() {
                "" => {}
                "all" => return Self::All,
                "none" => {}
                _ if packs.contains(&name) => {}
                _ => packs.push(name),
            }
        }
        if packs.is_empty()
            && !spec
                .split(',')
                .any(|n| n.trim().eq_ignore_ascii_case("none"))
        {
            return Self::All;
        }
        Self::Packs(packs)
    }

    /// The text [`LaunchProfile::parse`] reads back (the value of `FORGE_PACKS`).
    pub fn spec(&self) -> String {
        match self {
            Self::All => "all".into(),
            Self::Packs(packs) if packs.is_empty() => "none".into(),
            Self::Packs(packs) => packs.join(","),
        }
    }

    /// The profile of a program name: `phpforge`, `PHPForge.exe` or `/usr/bin/webforge`
    /// name a pack, `forge` and anything else load every pack. [`resolve`] ignores a name
    /// that is no pack.
    pub fn from_program(argv0: &str) -> Self {
        let file = argv0.rsplit(['/', '\\']).next().unwrap_or(argv0);
        let mut name = file.to_ascii_lowercase();
        for ext in [".exe", ".appimage"] {
            if let Some(stem) = name.strip_suffix(ext) {
                name = stem.to_string();
            }
        }
        let is_pack_name = name.len() > "forge".len()
            && name.ends_with("forge")
            && name.chars().all(|c| c.is_ascii_alphanumeric());
        if is_pack_name {
            Self::Packs(vec![name])
        } else {
            Self::All
        }
    }

    /// The profile of a launch: the `--packs` flag in `args` (the command line with the
    /// program name first), else `env` (the value of `FORGE_PACKS`), else the program
    /// name. `argv0` replaces the program name where the command line does not carry the
    /// name Forge was started under (an AppImage's `ARGV0`).
    pub fn detect(args: &[String], env: Option<&str>, argv0: Option<&str>) -> Self {
        if let Some(spec) = flag_value(args) {
            return Self::parse(&spec);
        }
        if let Some(spec) = env.filter(|s| !s.trim().is_empty()) {
            return Self::parse(spec);
        }
        argv0
            .or(args.first().map(String::as_str))
            .map(Self::from_program)
            .unwrap_or_default()
    }
}

/// The value of the last `--packs <list>` / `--packs=<list>` in `args`.
fn flag_value(args: &[String]) -> Option<String> {
    let mut value = None;
    let mut iter = args.iter().skip(1);
    while let Some(arg) = iter.next() {
        if arg == PACKS_FLAG {
            value = Some(iter.next().cloned().unwrap_or_default());
        } else if let Some(v) = arg
            .strip_prefix(PACKS_FLAG)
            .and_then(|r| r.strip_prefix('='))
        {
            value = Some(v.to_string());
        }
    }
    value
}

/// `args` without the `--packs` flag and its value, for the code that reads the files and
/// folders to open from the command line.
pub fn strip_args<I, S>(args: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut out = Vec::new();
    let mut skip_value = false;
    for arg in args {
        let arg: String = arg.into();
        if skip_value {
            skip_value = false;
        } else if arg == PACKS_FLAG {
            skip_value = true;
        } else if !arg
            .strip_prefix(PACKS_FLAG)
            .is_some_and(|r| r.starts_with('='))
        {
            out.push(arg);
        }
    }
    out
}

/// What [`resolve`] needs to know about a plugin.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginFacts {
    pub id: String,
    pub pack: String,
    #[serde(default)]
    pub depends: Vec<String>,
    #[serde(default)]
    pub required: bool,
}

/// A profile applied to the registered plugins (the `launch_profile` IPC command).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileView {
    /// Every pack is loaded; `packs` and `hidden` are empty.
    pub all: bool,
    /// The packs asked for that exist, without the base packs.
    pub packs: Vec<String>,
    /// Ids of the plugins that are off for this process.
    pub hidden: Vec<String>,
    /// Names asked for that are no pack.
    pub unknown: Vec<String>,
}

impl Default for ProfileView {
    fn default() -> Self {
        Self {
            all: true,
            packs: Vec::new(),
            hidden: Vec::new(),
            unknown: Vec::new(),
        }
    }
}

/// Which of `plugins` a profile turns off. A profile keeps the plugins of the base packs
/// and of its own packs, third-party and required plugins, the companions of its packs
/// and everything those depend on. Names that are no pack are ignored; a profile that
/// names only such packs loads everything.
pub fn resolve(profile: &LaunchProfile, plugins: &[PluginFacts]) -> ProfileView {
    let LaunchProfile::Packs(asked) = profile else {
        return ProfileView::default();
    };
    let (packs, unknown): (Vec<String>, Vec<String>) = asked
        .iter()
        .cloned()
        .partition(|name| plugins.iter().any(|p| &p.pack == name));
    if packs.is_empty() && !unknown.is_empty() {
        return ProfileView {
            unknown,
            ..ProfileView::default()
        };
    }
    let companions: Vec<&str> = COMPANIONS
        .iter()
        .filter(|(pack, _)| packs.iter().any(|p| p == pack))
        .flat_map(|(_, ids)| ids.iter().copied())
        .collect();
    let mut keep: Vec<&str> = plugins
        .iter()
        .filter(|p| {
            p.required
                || p.pack == THIRD_PARTY_PACK
                || BASE_PACKS.contains(&p.pack.as_str())
                || packs.contains(&p.pack)
                || companions.contains(&p.id.as_str())
        })
        .map(|p| p.id.as_str())
        .collect();
    let mut i = 0;
    while i < keep.len() {
        if let Some(p) = plugins.iter().find(|p| p.id == keep[i]) {
            for dep in &p.depends {
                if !keep.contains(&dep.as_str()) && plugins.iter().any(|q| &q.id == dep) {
                    keep.push(dep);
                }
            }
        }
        i += 1;
    }
    let hidden = plugins
        .iter()
        .filter(|p| !keep.contains(&p.id.as_str()))
        .map(|p| p.id.clone())
        .collect();
    let packs = packs
        .into_iter()
        .filter(|p| !BASE_PACKS.contains(&p.as_str()))
        .collect();
    ProfileView {
        all: false,
        packs,
        hidden,
        unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn plugin(id: &str, pack: &str, depends: &[&str]) -> PluginFacts {
        PluginFacts {
            id: id.into(),
            pack: pack.into(),
            depends: depends.iter().map(|s| s.to_string()).collect(),
            required: false,
        }
    }

    fn plugins() -> Vec<PluginFacts> {
        vec![
            PluginFacts {
                required: true,
                ..plugin("forge.core", "core", &[])
            },
            plugin("forge.debugger", "core", &[]),
            plugin("forge.yaml", "configforge", &[]),
            plugin("forge.bash", "scriptforge", &[]),
            plugin("forge.php", "phpforge", &[]),
            plugin("forge.laravel", "phpforge", &["forge.php"]),
            plugin("forge.web", "webforge", &[]),
            plugin("forge.react", "webforge", &["forge.web"]),
            plugin("forge.java", "javaforge", &[]),
            plugin("forge.spring", "javaforge", &["forge.java"]),
            plugin("forge.kotlin", "kotlinforge", &["forge.java"]),
            plugin("forge.lua", "luaforge", &[]),
            plugin("acme.icons", "thirdParty", &[]),
        ]
    }

    #[test]
    fn parses_all_none_and_lists() {
        assert_eq!(LaunchProfile::parse(""), LaunchProfile::All);
        assert_eq!(LaunchProfile::parse("all"), LaunchProfile::All);
        assert_eq!(LaunchProfile::parse("phpforge,ALL"), LaunchProfile::All);
        assert_eq!(LaunchProfile::parse("none"), LaunchProfile::Packs(vec![]));
        assert_eq!(
            LaunchProfile::parse(" PHPForge, webforge ,phpforge,"),
            LaunchProfile::Packs(vec!["phpforge".into(), "webforge".into()])
        );
        assert_eq!(
            LaunchProfile::parse("none,luaforge"),
            LaunchProfile::Packs(vec!["luaforge".into()])
        );
    }

    #[test]
    fn spec_reads_back() {
        for spec in ["all", "none", "phpforge", "phpforge,webforge"] {
            assert_eq!(LaunchProfile::parse(spec).spec(), spec);
        }
    }

    #[test]
    fn program_names_select_a_pack() {
        let pack = |name: &str| LaunchProfile::Packs(vec![name.into()]);
        assert_eq!(LaunchProfile::from_program("phpforge"), pack("phpforge"));
        assert_eq!(
            LaunchProfile::from_program("/usr/local/bin/webforge"),
            pack("webforge")
        );
        assert_eq!(
            LaunchProfile::from_program(r"C:\Tools\PHPForge.exe"),
            pack("phpforge")
        );
        assert_eq!(
            LaunchProfile::from_program("./LuaForge.AppImage"),
            pack("luaforge")
        );
        for all in [
            "forge",
            "/usr/bin/forge",
            "Forge.exe",
            "forge-app",
            "Forge_0.1.1_amd64.AppImage",
            "",
        ] {
            assert_eq!(
                LaunchProfile::from_program(all),
                LaunchProfile::All,
                "{all}"
            );
        }
    }

    #[test]
    fn the_flag_wins_over_the_environment_and_the_name() {
        let php = LaunchProfile::Packs(vec!["phpforge".into()]);
        let lua = LaunchProfile::Packs(vec!["luaforge".into()]);
        assert_eq!(
            LaunchProfile::detect(
                &args(&["webforge", "--packs", "luaforge"]),
                Some("phpforge"),
                None
            ),
            lua
        );
        assert_eq!(
            LaunchProfile::detect(
                &args(&["webforge", "--packs=luaforge", "/project"]),
                None,
                None
            ),
            lua
        );
        assert_eq!(
            LaunchProfile::detect(&args(&["webforge", "/project"]), Some("phpforge"), None),
            php
        );
        assert_eq!(
            LaunchProfile::detect(&args(&["phpforge", "/project"]), None, None),
            php
        );
        assert_eq!(
            LaunchProfile::detect(
                &args(&["phpforge", "--packs", "all"]),
                Some("luaforge"),
                None
            ),
            LaunchProfile::All
        );
        // An AppImage runs `forge` inside its mount; `ARGV0` has the name it was started as.
        assert_eq!(
            LaunchProfile::detect(
                &args(&["/tmp/.mount/usr/bin/forge"]),
                None,
                Some("phpforge")
            ),
            php
        );
        assert_eq!(
            LaunchProfile::detect(&args(&["forge"]), Some("  "), None),
            LaunchProfile::All
        );
        assert_eq!(LaunchProfile::detect(&[], None, None), LaunchProfile::All);
    }

    #[test]
    fn strips_the_flag_from_launch_arguments() {
        assert_eq!(
            strip_args([
                "--packs",
                "phpforge",
                "/project",
                "--packs=none",
                "file.php"
            ]),
            ["/project", "file.php"]
        );
        assert_eq!(strip_args(["/project", "--packs"]), ["/project"]);
        assert_eq!(strip_args(["--packsaddle", "x"]), ["--packsaddle", "x"]);
    }

    #[test]
    fn every_pack_hides_nothing() {
        assert_eq!(
            resolve(&LaunchProfile::All, &plugins()),
            ProfileView::default()
        );
    }

    #[test]
    fn a_pack_keeps_the_base_packs_its_companions_and_third_party_plugins() {
        let view = resolve(&LaunchProfile::parse("phpforge"), &plugins());
        assert!(!view.all);
        assert_eq!(view.packs, ["phpforge"]);
        assert_eq!(
            view.hidden,
            [
                "forge.react",
                "forge.java",
                "forge.spring",
                "forge.kotlin",
                "forge.lua"
            ]
        );
        assert!(view.unknown.is_empty());
    }

    #[test]
    fn dependencies_in_other_packs_are_kept() {
        let view = resolve(&LaunchProfile::parse("kotlinforge"), &plugins());
        assert!(!view.hidden.contains(&"forge.java".to_string()));
        assert!(view.hidden.contains(&"forge.spring".to_string()));
        assert!(view.hidden.contains(&"forge.web".to_string()));
    }

    #[test]
    fn none_keeps_only_the_base_packs() {
        let view = resolve(&LaunchProfile::parse("none"), &plugins());
        assert!(!view.all);
        assert!(view.packs.is_empty());
        let kept: Vec<String> = plugins()
            .into_iter()
            .map(|p| p.id)
            .filter(|id| !view.hidden.contains(id))
            .collect();
        assert_eq!(
            kept,
            [
                "forge.core",
                "forge.debugger",
                "forge.yaml",
                "forge.bash",
                "acme.icons"
            ]
        );
    }

    #[test]
    fn unknown_names_are_ignored() {
        let view = resolve(&LaunchProfile::parse("luaforge,nopeforge"), &plugins());
        assert_eq!(view.packs, ["luaforge"]);
        assert_eq!(view.unknown, ["nopeforge"]);
        assert!(!view.all);

        // A name that is no pack at all (a renamed binary) loads everything.
        let view = resolve(&LaunchProfile::from_program("superforge"), &plugins());
        assert!(view.all);
        assert!(view.hidden.is_empty());
        assert_eq!(view.unknown, ["superforge"]);
    }

    #[test]
    fn base_packs_can_be_named() {
        let view = resolve(&LaunchProfile::parse("core"), &plugins());
        assert!(!view.all);
        assert!(view.packs.is_empty());
        assert_eq!(
            view,
            ProfileView {
                unknown: vec![],
                ..resolve(&LaunchProfile::parse("none"), &plugins())
            }
        );
    }
}
