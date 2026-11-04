//! The pinned version table: platforms, Minecraft versions with the Java release they need,
//! and the API dependency (repositories and coordinates) of each platform. "Refresh versions"
//! adds Minecraft versions read from Paper's `maven-metadata.xml`; versions that are not in
//! the table get their coordinates and Java release from the same rules.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

/// The server or proxy a plugin is written for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Platform {
    Paper,
    Purpur,
    Folia,
    Spigot,
    Bukkit,
    Velocity,
    Bungeecord,
}

impl Platform {
    pub const ALL: [Platform; 7] = [
        Platform::Paper,
        Platform::Purpur,
        Platform::Folia,
        Platform::Spigot,
        Platform::Bukkit,
        Platform::Velocity,
        Platform::Bungeecord,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Platform::Paper => "paper",
            Platform::Purpur => "purpur",
            Platform::Folia => "folia",
            Platform::Spigot => "spigot",
            Platform::Bukkit => "bukkit",
            Platform::Velocity => "velocity",
            Platform::Bungeecord => "bungeecord",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Platform::Paper => "Paper",
            Platform::Purpur => "Purpur",
            Platform::Folia => "Folia",
            Platform::Spigot => "Spigot",
            Platform::Bukkit => "Bukkit",
            Platform::Velocity => "Velocity",
            Platform::Bungeecord => "BungeeCord / Waterfall",
        }
    }

    pub fn parse(id: &str) -> Option<Platform> {
        Platform::ALL.into_iter().find(|p| p.id() == id)
    }

    /// Velocity and BungeeCord run on the proxy, not on a Minecraft server.
    pub fn is_proxy(self) -> bool {
        matches!(self, Platform::Velocity | Platform::Bungeecord)
    }

    /// Paper and its forks: the Adventure API, `paper-plugin.yml`, Brigadier commands.
    pub fn is_paper_family(self) -> bool {
        matches!(self, Platform::Paper | Platform::Purpur | Platform::Folia)
    }
}

pub const PAPER_REPO: &str = "https://repo.papermc.io/repository/maven-public/";
pub const SPIGOT_REPO: &str = "https://hub.spigotmc.org/nexus/content/repositories/snapshots/";
pub const PURPUR_REPO: &str = "https://repo.purpurmc.org/snapshots";
pub const SONATYPE_SNAPSHOTS: &str = "https://oss.sonatype.org/content/repositories/snapshots";

/// Where "Refresh versions" reads the Minecraft versions Paper publishes its API for.
pub const PAPER_METADATA_PATH: &str = "io/papermc/paper/paper-api/maven-metadata.xml";

/// The Velocity API the generator uses, and the Java release it needs.
pub const VELOCITY_API: &str = "3.4.0-SNAPSHOT";
pub const VELOCITY_JAVA: u32 = 21;
/// The BungeeCord API the generator uses, and the Java release of generated projects.
pub const BUNGEECORD_API: &str = "1.21-R0.1-SNAPSHOT";
pub const BUNGEECORD_JAVA: u32 = 17;

/// Build plugins and libraries of generated projects.
pub const RUN_PAPER: &str = "2.3.1";
pub const SHADOW: &str = "8.3.6";
pub const KOTLIN: &str = "2.2.20";
pub const MAVEN_COMPILER: &str = "3.13.0";
pub const MAVEN_SHADE: &str = "3.6.0";

/// A Minecraft version of the pinned table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct McVersion {
    pub version: &'static str,
    /// The Java release servers of this version need.
    pub java: u32,
}

/// Newest first. Each has a Paper, Spigot and Bukkit API; Purpur, Folia and
/// `paper-plugin.yml` have lower bounds (see [`check_platform`]).
pub const MC_VERSIONS: &[McVersion] = &[
    McVersion {
        version: "1.21.10",
        java: 21,
    },
    McVersion {
        version: "1.21.8",
        java: 21,
    },
    McVersion {
        version: "1.21.4",
        java: 21,
    },
    McVersion {
        version: "1.21.1",
        java: 21,
    },
    McVersion {
        version: "1.20.6",
        java: 21,
    },
    McVersion {
        version: "1.20.4",
        java: 17,
    },
    McVersion {
        version: "1.20.1",
        java: 17,
    },
    McVersion {
        version: "1.19.4",
        java: 17,
    },
    McVersion {
        version: "1.18.2",
        java: 17,
    },
    McVersion {
        version: "1.17.1",
        java: 16,
    },
    McVersion {
        version: "1.16.5",
        java: 8,
    },
];

pub const DEFAULT_MC_VERSION: &str = "1.21.8";

/// The oldest Minecraft version the generator supports.
pub const OLDEST: &str = "1.16.5";

/// Compare dotted versions numerically (`1.21.10` > `1.21.8`); non-numeric parts count as 0.
pub fn compare(a: &str, b: &str) -> Ordering {
    let parts = |s: &str| -> Vec<u64> {
        s.split('.')
            .map(|p| {
                p.chars()
                    .take_while(char::is_ascii_digit)
                    .collect::<String>()
                    .parse()
                    .unwrap_or(0)
            })
            .collect()
    };
    let (a, b) = (parts(a), parts(b));
    for i in 0..a.len().max(b.len()) {
        match a.get(i).unwrap_or(&0).cmp(b.get(i).unwrap_or(&0)) {
            Ordering::Equal => {}
            other => return other,
        }
    }
    Ordering::Equal
}

fn at_least(mc: &str, min: &str) -> bool {
    compare(mc, min) != Ordering::Less
}

/// A release version: `1.21.8`, `1.20`, `26.1` (digits and dots, two or three parts).
pub fn is_release(v: &str) -> bool {
    let parts: Vec<_> = v.split('.').collect();
    (2..=3).contains(&parts.len())
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

/// The Java release servers of `mc` need: the table, else Mojang's rules (1.17: 16,
/// 1.18: 17, 1.20.5: 21; the year-numbered versions from 26.1 on: 25).
pub fn java_for(mc: &str) -> u32 {
    if let Some(v) = MC_VERSIONS.iter().find(|v| v.version == mc) {
        return v.java;
    }
    if at_least(mc, "26") {
        25
    } else if at_least(mc, "1.20.5") {
        21
    } else if at_least(mc, "1.18") {
        17
    } else if at_least(mc, "1.17") {
        16
    } else {
        8
    }
}

/// `api-version` of `plugin.yml` / `paper-plugin.yml`: the major version (`1.21`).
pub fn api_version(mc: &str) -> String {
    mc.split('.').take(2).collect::<Vec<_>>().join(".")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Repository {
    /// Maven repository id (`papermc`).
    pub id: &'static str,
    pub url: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Coordinates {
    pub group: &'static str,
    pub artifact: &'static str,
    pub version: String,
}

impl Coordinates {
    pub fn gradle(&self) -> String {
        format!("{}:{}:{}", self.group, self.artifact, self.version)
    }
}

/// What a platform's API dependency needs: the repositories (besides Maven Central), the
/// `compileOnly` / `provided` dependency and the Java release.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiSpec {
    pub repositories: Vec<Repository>,
    pub dependency: Coordinates,
    pub java: u32,
}

const PAPERMC: Repository = Repository {
    id: "papermc",
    url: PAPER_REPO,
};
const SPIGOTMC: Repository = Repository {
    id: "spigotmc",
    url: SPIGOT_REPO,
};
const PURPURMC: Repository = Repository {
    id: "purpurmc",
    url: PURPUR_REPO,
};
const SONATYPE: Repository = Repository {
    id: "sonatype-snapshots",
    url: SONATYPE_SNAPSHOTS,
};

/// Why `platform` has no API for `mc`, if it has none.
pub fn check_platform(platform: Platform, mc: &str) -> Result<(), String> {
    if platform.is_proxy() {
        return Ok(());
    }
    if !is_release(mc) {
        return Err(format!("'{mc}' is not a Minecraft release version"));
    }
    if !at_least(mc, OLDEST) {
        return Err(format!(
            "The generator supports Minecraft {OLDEST} and newer"
        ));
    }
    match platform {
        Platform::Purpur if !at_least(mc, "1.18") => {
            Err("The Purpur API is available for Minecraft 1.18 and newer".into())
        }
        Platform::Folia if !at_least(mc, "1.20") => {
            Err("Folia plugins need Minecraft 1.20 or newer".into())
        }
        _ => Ok(()),
    }
}

/// The API dependency of `platform` for Minecraft `mc` (ignored by proxies).
pub fn api_spec(platform: Platform, mc: &str) -> Result<ApiSpec, String> {
    check_platform(platform, mc)?;
    let snapshot = format!("{mc}-R0.1-SNAPSHOT");
    let java = java_for(mc);
    let spec = |repositories: Vec<Repository>, group, artifact, version: String, java| ApiSpec {
        repositories,
        dependency: Coordinates {
            group,
            artifact,
            version,
        },
        java,
    };
    Ok(match platform {
        // Folia plugins compile against the Paper API (it has the region schedulers) and
        // declare `folia-supported: true`.
        Platform::Paper | Platform::Folia if at_least(mc, "1.17") => spec(
            vec![PAPERMC],
            "io.papermc.paper",
            "paper-api",
            snapshot,
            java,
        ),
        Platform::Paper | Platform::Folia => spec(
            vec![PAPERMC],
            "com.destroystokyo.paper",
            "paper-api",
            snapshot,
            java,
        ),
        // purpur-api depends on Paper's artifacts, so it needs both repositories.
        Platform::Purpur => spec(
            vec![PAPERMC, PURPURMC],
            "org.purpurmc.purpur",
            "purpur-api",
            snapshot,
            java,
        ),
        Platform::Spigot => spec(vec![SPIGOTMC], "org.spigotmc", "spigot-api", snapshot, java),
        Platform::Bukkit => spec(vec![SPIGOTMC], "org.bukkit", "bukkit", snapshot, java),
        Platform::Velocity => spec(
            vec![PAPERMC],
            "com.velocitypowered",
            "velocity-api",
            VELOCITY_API.into(),
            VELOCITY_JAVA,
        ),
        Platform::Bungeecord => spec(
            vec![SONATYPE],
            "net.md-5",
            "bungeecord-api",
            BUNGEECORD_API.into(),
            BUNGEECORD_JAVA,
        ),
    })
}

/// `paper-plugin.yml` with Brigadier commands (the lifecycle API) needs Paper 1.20.6+.
pub const PAPER_PLUGIN_YML_MIN: &str = "1.20.6";

pub fn supports_paper_plugin_yml(mc: &str) -> bool {
    at_least(mc, PAPER_PLUGIN_YML_MIN)
}

/// The `<version>` values of a `maven-metadata.xml`.
pub fn parse_maven_metadata(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find("<version>") {
        let after = &rest[start + "<version>".len()..];
        let Some(end) = after.find("</version>") else {
            break;
        };
        let v = after[..end].trim();
        if !v.is_empty() && !out.iter().any(|x| x == v) {
            out.push(v.to_string());
        }
        rest = &after[end..];
    }
    out
}

/// Minecraft release versions Paper publishes an API for, from paper-api's
/// `maven-metadata.xml` (`1.21.8-R0.1-SNAPSHOT` → `1.21.8`), newest first. Pre-releases,
/// release candidates and versions older than [`OLDEST`] are skipped.
pub fn mc_versions_from_paper_metadata(xml: &str) -> Vec<String> {
    let mut out: Vec<String> = parse_maven_metadata(xml)
        .into_iter()
        .filter_map(|v| v.strip_suffix("-R0.1-SNAPSHOT").map(str::to_string))
        .filter(|v| is_release(v) && at_least(v, OLDEST))
        .collect();
    out.sort_by(|a, b| compare(b, a));
    out.dedup();
    out
}

/// The choices of the "Minecraft version" option: the pinned table plus `extra` (refreshed)
/// versions, newest first, as (value, label) pairs.
pub fn version_choices(extra: &[String]) -> Vec<(String, String)> {
    let mut all: Vec<String> = MC_VERSIONS.iter().map(|v| v.version.to_string()).collect();
    for v in extra {
        if is_release(v) && at_least(v, OLDEST) && !all.contains(v) {
            all.push(v.clone());
        }
    }
    all.sort_by(|a, b| compare(b, a));
    all.into_iter()
        .map(|v| {
            let label = format!("{v} (Java {})", java_for(&v));
            (v, label)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions_numerically() {
        assert_eq!(compare("1.21.10", "1.21.8"), Ordering::Greater);
        assert_eq!(compare("1.21", "1.21.0"), Ordering::Equal);
        assert_eq!(compare("1.20.4", "1.20.5"), Ordering::Less);
        assert_eq!(compare("26.1", "1.21.10"), Ordering::Greater);
    }

    #[test]
    fn java_release_follows_the_minecraft_version() {
        assert_eq!(java_for("1.21.8"), 21);
        assert_eq!(java_for("1.20.6"), 21);
        assert_eq!(java_for("1.20.5"), 21);
        assert_eq!(java_for("1.20.4"), 17);
        assert_eq!(java_for("1.18"), 17);
        assert_eq!(java_for("1.17"), 16);
        assert_eq!(java_for("1.16.5"), 8);
        assert_eq!(java_for("1.21.11"), 21);
        assert_eq!(java_for("26.1"), 25);
        for v in MC_VERSIONS {
            assert!(is_release(v.version));
            assert_eq!(java_for(v.version), v.java, "{}", v.version);
        }
    }

    #[test]
    fn table_is_newest_first_and_has_the_default() {
        assert!(MC_VERSIONS
            .windows(2)
            .all(|w| compare(w[0].version, w[1].version) == Ordering::Greater));
        assert!(MC_VERSIONS.iter().any(|v| v.version == DEFAULT_MC_VERSION));
        assert_eq!(MC_VERSIONS.last().unwrap().version, OLDEST);
    }

    #[test]
    fn api_coordinates_per_platform() {
        let paper = api_spec(Platform::Paper, "1.21.8").unwrap();
        assert_eq!(
            paper.dependency.gradle(),
            "io.papermc.paper:paper-api:1.21.8-R0.1-SNAPSHOT"
        );
        assert_eq!(paper.repositories[0].url, PAPER_REPO);
        assert_eq!(paper.java, 21);
        assert_eq!(
            api_spec(Platform::Paper, "1.16.5")
                .unwrap()
                .dependency
                .gradle(),
            "com.destroystokyo.paper:paper-api:1.16.5-R0.1-SNAPSHOT"
        );
        let purpur = api_spec(Platform::Purpur, "1.21.4").unwrap();
        assert_eq!(
            purpur.dependency.gradle(),
            "org.purpurmc.purpur:purpur-api:1.21.4-R0.1-SNAPSHOT"
        );
        assert_eq!(purpur.repositories.len(), 2);
        assert_eq!(
            api_spec(Platform::Spigot, "1.20.4")
                .unwrap()
                .dependency
                .gradle(),
            "org.spigotmc:spigot-api:1.20.4-R0.1-SNAPSHOT"
        );
        assert_eq!(
            api_spec(Platform::Bukkit, "1.20.4")
                .unwrap()
                .dependency
                .gradle(),
            "org.bukkit:bukkit:1.20.4-R0.1-SNAPSHOT"
        );
        assert_eq!(
            api_spec(Platform::Folia, "1.21.4")
                .unwrap()
                .dependency
                .artifact,
            "paper-api"
        );
        let velocity = api_spec(Platform::Velocity, "ignored").unwrap();
        assert_eq!(
            velocity.dependency.gradle(),
            "com.velocitypowered:velocity-api:3.4.0-SNAPSHOT"
        );
        assert_eq!(velocity.java, 21);
        assert_eq!(
            api_spec(Platform::Bungeecord, "")
                .unwrap()
                .dependency
                .gradle(),
            "net.md-5:bungeecord-api:1.21-R0.1-SNAPSHOT"
        );
    }

    #[test]
    fn platform_lower_bounds() {
        assert!(check_platform(Platform::Purpur, "1.17.1").is_err());
        assert!(check_platform(Platform::Purpur, "1.18.2").is_ok());
        assert!(check_platform(Platform::Folia, "1.19.4").is_err());
        assert!(check_platform(Platform::Paper, "1.12.2").is_err());
        assert!(check_platform(Platform::Paper, "1.21-pre1").is_err());
        assert!(supports_paper_plugin_yml("1.20.6"));
        assert!(!supports_paper_plugin_yml("1.20.4"));
        assert_eq!(api_version("1.21.8"), "1.21");
        assert_eq!(api_version("1.20"), "1.20");
    }

    #[test]
    fn reads_paper_versions_from_maven_metadata() {
        let xml = include_str!("../tests/fixtures/paper-api-maven-metadata.xml");
        let versions = mc_versions_from_paper_metadata(xml);
        assert_eq!(versions.first().map(String::as_str), Some("1.21.11"));
        assert!(versions.contains(&"1.21.10".to_string()));
        assert!(!versions
            .iter()
            .any(|v| v.contains("pre") || v.contains("rc")));
        assert!(versions.contains(&"1.17".to_string()));
        assert!(versions.contains(&"1.21".to_string()));

        let choices = version_choices(&versions);
        assert_eq!(choices[0], ("1.21.11".into(), "1.21.11 (Java 21)".into()));
        assert!(choices.iter().any(|(v, _)| v == "1.16.5"));
        let mut values: Vec<_> = choices.iter().map(|c| c.0.clone()).collect();
        values.dedup();
        assert_eq!(values.len(), choices.len());
    }
}
