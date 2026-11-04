//! Is a project a Minecraft plugin? Read from its build files (`build.gradle(.kts)`,
//! `pom.xml`, `gradle/libs.versions.toml`): the API dependency names the platform and the
//! Minecraft version, run-paper's Gradle plugin enables "Run Paper Server".

use serde::Serialize;

use crate::versions::Platform;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Detected {
    pub platform: Platform,
    /// From the API dependency (`1.21.8-R0.1-SNAPSHOT` → `1.21.8`); `None` for proxies or
    /// when the version is a variable.
    pub mc_version: Option<String>,
    /// The build applies run-paper (`runServer`).
    pub run_paper: bool,
}

impl Detected {
    /// The framework name shown in Forge (`Paper Plugin`).
    pub fn name(&self) -> String {
        match self.platform {
            Platform::Bungeecord => "BungeeCord Plugin".into(),
            p => format!("{} Plugin", p.label()),
        }
    }
}

/// Dependency markers, most specific first: (artifact, platform).
const MARKERS: &[(&str, Platform)] = &[
    ("folia-api", Platform::Folia),
    ("purpur-api", Platform::Purpur),
    ("paper-api", Platform::Paper),
    ("paperDevBundle", Platform::Paper),
    ("spigot-api", Platform::Spigot),
    ("org.bukkit:bukkit:", Platform::Bukkit),
    ("<artifactId>bukkit</artifactId>", Platform::Bukkit),
    ("velocity-api", Platform::Velocity),
    ("bungeecord-api", Platform::Bungeecord),
    ("waterfall-api", Platform::Bungeecord),
];

/// The Minecraft version after a dependency marker: Gradle `paper-api:1.21.8-R0.1-SNAPSHOT`,
/// `paperDevBundle("1.21.8-R0.1-SNAPSHOT")`, a version catalog's `version = "…"` on the same
/// line, or Maven's next `<version>`.
fn version_after(build: &str, at: usize) -> Option<String> {
    let rest = &build[at..];
    let line = rest.lines().next().unwrap_or("");
    let candidate = if rest.trim_start().starts_with('<') {
        // Maven: the `<version>` of the same `<dependency>`.
        let after = &rest[..rest.find("</dependency>").unwrap_or(rest.len())];
        let v = after.find("<version>")? + "<version>".len();
        let end = after[v..].find('<')?;
        after[v..v + end].to_string()
    } else {
        // The first version-looking token on the line.
        line.split(|c: char| !(c.is_ascii_alphanumeric() || c == '.' || c == '-'))
            .find(|t| t.contains("-R0.") || t.starts_with(|c: char| c.is_ascii_digit()))?
            .to_string()
    };
    // `1.21.8-R0.1-SNAPSHOT`, `26.3.build.240-stable`, `26.3.build.+`.
    let v = candidate
        .split("-R0.")
        .next()?
        .split(".build.")
        .next()?
        .trim();
    crate::versions::is_release(v).then(|| v.to_string())
}

/// Detect a Minecraft plugin from the texts of the build files.
pub fn detect(builds: &[&str]) -> Option<Detected> {
    for (marker, platform) in MARKERS {
        for build in builds {
            let Some(at) = build.find(marker) else {
                continue;
            };
            let mc_version = if platform.is_proxy() {
                None
            } else {
                version_after(build, at + marker.len())
            };
            let run_paper = builds
                .iter()
                .any(|b| b.contains("xyz.jpenilla.run-paper") || b.contains("run-paper"));
            return Some(Detected {
                platform: *platform,
                mc_version,
                run_paper,
            });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_gradle_and_maven_builds() {
        let kts = "plugins {\n    java\n    id(\"xyz.jpenilla.run-paper\") version \"2.3.1\"\n}\ndependencies {\n    compileOnly(\"io.papermc.paper:paper-api:1.21.8-R0.1-SNAPSHOT\")\n}\n";
        assert_eq!(
            detect(&[kts]),
            Some(Detected {
                platform: Platform::Paper,
                mc_version: Some("1.21.8".into()),
                run_paper: true,
            })
        );
        let pom = "<dependency>\n  <groupId>org.spigotmc</groupId>\n  <artifactId>spigot-api</artifactId>\n  <version>1.20.4-R0.1-SNAPSHOT</version>\n  <scope>provided</scope>\n</dependency>";
        let d = detect(&[pom]).unwrap();
        assert_eq!(d.platform, Platform::Spigot);
        assert_eq!(d.mc_version.as_deref(), Some("1.20.4"));
        assert!(!d.run_paper);
        assert_eq!(d.name(), "Spigot Plugin");
        let velocity = "compileOnly 'com.velocitypowered:velocity-api:3.4.0-SNAPSHOT'";
        let d = detect(&[velocity]).unwrap();
        assert_eq!(d.platform, Platform::Velocity);
        assert_eq!(d.mc_version, None);
        let purpur = "compileOnly(\"org.purpurmc.purpur:purpur-api:${mcVersion}-R0.1-SNAPSHOT\")";
        let d = detect(&[purpur]).unwrap();
        assert_eq!(d.platform, Platform::Purpur);
        assert_eq!(d.mc_version, None);
        let userdev = "paperweight.paperDevBundle(\"1.21.4-R0.1-SNAPSHOT\")";
        assert_eq!(
            detect(&[userdev]).unwrap().mc_version.as_deref(),
            Some("1.21.4")
        );
        let build26 = "compileOnly(\"io.papermc.paper:paper-api:26.3.build.240-stable\")";
        assert_eq!(
            detect(&[build26]).unwrap().mc_version.as_deref(),
            Some("26.3")
        );
        let catalog = "[libraries]\npaper-api = { module = \"io.papermc.paper:paper-api\", version = \"1.20.6-R0.1-SNAPSHOT\" }\n";
        assert_eq!(
            detect(&["", catalog]).unwrap().mc_version.as_deref(),
            Some("1.20.6")
        );
        assert_eq!(
            detect(&["implementation(\"com.google.guava:guava:33.0.0-jre\")"]),
            None
        );
    }

    #[test]
    fn detects_generated_projects() {
        use crate::generate::{generate, Options};
        for platform in Platform::ALL {
            for build in ["gradle-kts", "gradle-groovy", "maven"] {
                let o = Options {
                    platform: platform.id().into(),
                    build_system: build.into(),
                    ..Options::default()
                };
                let files = generate("demo", &o).unwrap();
                let texts: Vec<&str> = files
                    .iter()
                    .filter(|f| {
                        ["build.gradle.kts", "build.gradle", "pom.xml"].contains(&f.path.as_str())
                    })
                    .map(|f| f.contents.as_str())
                    .collect();
                let d = detect(&texts).unwrap_or_else(|| panic!("{platform:?} {build}"));
                // Folia plugins compile against paper-api, Bukkit plugins for versions
                // after 1.20.5 against spigot-api.
                let expected = match platform {
                    Platform::Folia => Platform::Paper,
                    Platform::Bukkit => Platform::Spigot,
                    p => p,
                };
                assert_eq!(d.platform, expected);
                if !platform.is_proxy() {
                    assert_eq!(
                        d.mc_version.as_deref(),
                        Some(crate::versions::DEFAULT_MC_VERSION),
                        "{platform:?} {build}"
                    );
                }
                assert_eq!(d.run_paper, build != "maven" && !platform.is_proxy());
            }
        }
    }
}
