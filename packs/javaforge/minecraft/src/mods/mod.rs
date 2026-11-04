//! Minecraft mods: Fabric (Loom) and NeoForge (ModDevGradle).
//!
//! - [`versions`]: the pinned table (Minecraft version → Java, Fabric API, Yarn, NeoForge,
//!   Parchment; Fabric Loader, Loom, ModDevGradle) and the parsers of the "Refresh versions"
//!   sources.
//! - [`generate`]: the "Minecraft Mod" New Project generator, with the source templates in
//!   [`sources`] and the placeholder textures of [`png`].
//! - [`editor`]: `fabric.mod.json` entrypoints and mixin configurations: checks and
//!   navigation; [`toml`]: completion in `neoforge.mods.toml`.
//! - [`detect_mod`]: a mod project from its build files.

pub mod editor;
pub mod generate;
pub mod png;
pub mod sources;
pub mod toml;
pub mod versions;

pub use generate::{generate_mod, ModOptions};
pub use versions::Loader;

use serde::Serialize;

/// A detected mod project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedMod {
    /// `Fabric Mod`, `Quilt Mod`, `NeoForge Mod` or `Forge Mod`.
    pub name: &'static str,
    /// From `gradle.properties` (`minecraft_version=`).
    pub mc_version: Option<String>,
}

/// Gradle plugin markers, most specific first.
const MOD_MARKERS: &[(&str, &str)] = &[
    ("org.quiltmc.loom", "Quilt Mod"),
    ("fabric-loom", "Fabric Mod"),
    ("net.neoforged.moddev", "NeoForge Mod"),
    ("net.neoforged.gradle", "NeoForge Mod"),
    ("net.minecraftforge.gradle", "Forge Mod"),
];

/// Detect a mod from the texts of the build files and `gradle.properties`.
pub fn detect_mod(builds: &[&str], gradle_properties: &str) -> Option<DetectedMod> {
    let name = MOD_MARKERS
        .iter()
        .find(|(m, _)| builds.iter().any(|b| b.contains(m)))?
        .1;
    let mc_version = gradle_properties.lines().find_map(|l| {
        let (k, v) = l.split_once('=')?;
        let v = v.trim();
        (k.trim() == "minecraft_version" && crate::versions::is_release(v)).then(|| v.to_string())
    });
    Some(DetectedMod { name, mc_version })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_generated_mods() {
        for loader in ["fabric", "neoforge"] {
            for mc in ["26.3", "1.21.1"] {
                let o = ModOptions {
                    loader: loader.into(),
                    mc_version: mc.into(),
                    ..ModOptions::default()
                };
                let files = generate_mod("demo", &o).unwrap();
                let get = |p: &str| {
                    files
                        .iter()
                        .find(|f| f.path == p)
                        .map(|f| f.contents.as_str())
                        .unwrap()
                };
                let d = detect_mod(&[get("build.gradle")], get("gradle.properties")).unwrap();
                assert_eq!(
                    d.name,
                    if loader == "fabric" {
                        "Fabric Mod"
                    } else {
                        "NeoForge Mod"
                    }
                );
                assert_eq!(d.mc_version.as_deref(), Some(mc));
            }
        }
        let forge = detect_mod(
            &["plugins { id 'net.minecraftforge.gradle' version '[6.0,6.2)' }"],
            "minecraft_version=${mc}\n",
        )
        .unwrap();
        assert_eq!(forge.name, "Forge Mod");
        assert_eq!(forge.mc_version, None);
        assert_eq!(detect_mod(&["plugins { id 'java' }"], ""), None);
    }
}
