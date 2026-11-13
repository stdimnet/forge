//! "Refresh versions" (feature `net`): read the Minecraft versions Paper publishes its API
//! for, and the newest build-numbered Paper and Purpur APIs (26.1+), from the
//! `maven-metadata.xml` files of their repositories; for mods, Fabric's meta API (loader,
//! game versions, Yarn), the Fabric API, Loom, NeoForge and ModDevGradle metadata. Only runs
//! when the user asks for it.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::mods::versions::{self as mods, ModVersions};
use crate::versions::{
    self, mc_versions_from_paper_metadata, newest_builds, Builds, PAPER_METADATA_PATH,
    PURPUR_METADATA_PATH,
};

const MAX_SIZE: usize = 4 << 20;

/// What a refresh found (cached by the app).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Refreshed {
    /// Newest first.
    pub mc_versions: Vec<String>,
    pub builds: Builds,
    /// The mod generator's versions (a refresh from the Minecraft Mod wizard).
    pub mods: ModVersions,
}

/// Where the mod refresh reads: Fabric's meta API and Maven repository, NeoForge's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModSources {
    /// `https://meta.fabricmc.net/v2/versions/`.
    pub fabric_meta: String,
    pub fabric_maven: String,
    pub neoforge_maven: String,
}

impl Default for ModSources {
    fn default() -> Self {
        Self {
            fabric_meta: mods::FABRIC_META.into(),
            fabric_maven: mods::FABRIC_MAVEN.into(),
            neoforge_maven: mods::NEOFORGE_MAVEN.into(),
        }
    }
}

/// Refresh the mod versions. Fabric's game versions and the NeoForge metadata are required
/// (one of them must answer); every other source keeps its pinned values when it fails.
pub async fn refresh_mods(sources: &ModSources) -> Result<ModVersions, String> {
    let meta = |p: &str| format!("{}{p}", with_slash(&sources.fabric_meta));
    let game = fetch_url(&meta("game")).await;
    let neoforge = fetch_text(&sources.neoforge_maven, mods::NEOFORGE_METADATA_PATH).await;
    if let (Err(a), Err(b)) = (&game, &neoforge) {
        return Err(format!("{a}; {b}"));
    }
    let loader = fetch_url(&meta("loader")).await.ok();
    let yarn = fetch_url(&meta("yarn")).await.ok();
    let fabric_api = fetch_text(&sources.fabric_maven, mods::FABRIC_API_METADATA_PATH)
        .await
        .ok();
    let loom = fetch_text(&sources.fabric_maven, mods::LOOM_METADATA_PATH)
        .await
        .ok();
    let moddev = fetch_text(&sources.neoforge_maven, mods::MODDEV_METADATA_PATH)
        .await
        .ok();
    let neoforge = neoforge
        .ok()
        .map(|x| mods::neoforge_from_metadata(&x))
        .unwrap_or_default();
    let fabric_api = fabric_api
        .map(|x| mods::fabric_api_from_metadata(&x))
        .unwrap_or_default();
    // Minecraft versions: Fabric's stable releases, else the versions NeoForge has; only
    // those a loader has a build for.
    let mut mc_versions = game
        .map(|j| mods::game_versions_from_meta(&j))
        .unwrap_or_default();
    if mc_versions.is_empty() {
        mc_versions = neoforge.keys().cloned().collect();
    }
    mc_versions.retain(|v| {
        mods::check_mc(v).is_ok() && (fabric_api.contains_key(v) || neoforge.contains_key(v))
    });
    mc_versions.sort_by(|a, b| versions::compare(b, a));
    mc_versions.dedup();
    Ok(ModVersions {
        mc_versions,
        fabric_loader: loader.and_then(|j| mods::fabric_loader_from_meta(&j)),
        loom: loom.and_then(|x| mods::newest_release(&x)),
        fabric_api,
        yarn: yarn.map(|j| mods::yarn_from_meta(&j)).unwrap_or_default(),
        neoforge,
        moddev: moddev.and_then(|x| mods::newest_release(&x)),
    })
}

fn with_slash(url: &str) -> String {
    format!("{}/", url.trim_end_matches('/'))
}

/// Refresh from Paper's repository (versions and Paper builds) and Purpur's (Purpur builds;
/// a failure there keeps the pinned Purpur builds).
pub async fn refresh(paper_repo: &str, purpur_repo: &str) -> Result<Refreshed, String> {
    let paper = fetch_text(paper_repo, PAPER_METADATA_PATH).await?;
    let mc_versions = mc_versions_from_paper_metadata(&paper);
    if mc_versions.is_empty() {
        return Err(format!("{paper_repo}: no Minecraft versions found"));
    }
    let purpur = fetch_text(purpur_repo, PURPUR_METADATA_PATH)
        .await
        .map(|xml| newest_builds(&xml))
        .unwrap_or_default();
    Ok(Refreshed {
        mc_versions,
        builds: Builds {
            paper: newest_builds(&paper),
            purpur,
        },
        mods: ModVersions::default(),
    })
}

/// Fetch the Minecraft versions from `repository` (normally [`versions::PAPER_REPO`]),
/// newest first.
pub async fn fetch_mc_versions(repository: &str) -> Result<Vec<String>, String> {
    let text = fetch_text(repository, PAPER_METADATA_PATH).await?;
    let found = mc_versions_from_paper_metadata(&text);
    if found.is_empty() {
        return Err(format!("{repository}: no Minecraft versions found"));
    }
    Ok(found)
}

async fn fetch_text(repository: &str, path: &str) -> Result<String, String> {
    fetch_url(&format!("{}/{path}", repository.trim_end_matches('/'))).await
}

async fn fetch_url(url: &str) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .user_agent(concat!("Forge/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("cannot reach {url}: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("{url}: HTTP {}", response.status()));
    }
    let bytes = response.bytes().await.map_err(|e| format!("{url}: {e}"))?;
    if bytes.len() > MAX_SIZE {
        return Err(format!("{url}: the answer is too large"));
    }
    String::from_utf8(bytes.to_vec()).map_err(|_| format!("{url}: unexpected answer"))
}

/// The default repositories "Refresh versions" reads (Paper, Purpur).
pub fn default_repositories() -> (&'static str, &'static str) {
    (versions::PAPER_REPO, versions::PURPUR_REPO)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// An HTTP server on localhost answering `routes` (path, body), else 404.
    async fn serve(routes: &'static [(&'static str, &'static str)]) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move {
            while let Ok((mut sock, _)) = listener.accept().await {
                let mut buf = vec![0u8; 4096];
                let n = sock.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                let hit = routes
                    .iter()
                    .find(|(path, _)| req.starts_with(&format!("GET {path} ")));
                let (status, body) = match hit {
                    Some((_, body)) => ("200 OK", *body),
                    None => ("404 Not Found", ""),
                };
                let resp = format!(
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = sock.write_all(resp.as_bytes()).await;
            }
        });
        url
    }

    #[tokio::test]
    async fn refreshes_mod_versions() {
        let url = serve(&[
            (
                "/meta/game",
                include_str!("../tests/fixtures/fabric-meta-game.json"),
            ),
            (
                "/meta/loader",
                include_str!("../tests/fixtures/fabric-meta-loader.json"),
            ),
            (
                "/meta/yarn",
                include_str!("../tests/fixtures/fabric-meta-yarn.json"),
            ),
            (
                "/fabric/net/fabricmc/fabric-api/fabric-api/maven-metadata.xml",
                include_str!("../tests/fixtures/fabric-api-maven-metadata.xml"),
            ),
            (
                "/neo/net/neoforged/neoforge/maven-metadata.xml",
                include_str!("../tests/fixtures/neoforge-maven-metadata.xml"),
            ),
            (
                "/neo/net/neoforged/moddev/net.neoforged.moddev.gradle.plugin/maven-metadata.xml",
                include_str!("../tests/fixtures/moddev-maven-metadata.xml"),
            ),
        ])
        .await;
        let sources = ModSources {
            fabric_meta: format!("{url}/meta"),
            fabric_maven: format!("{url}/fabric/"),
            neoforge_maven: format!("{url}/neo"),
        };
        let m = refresh_mods(&sources).await.unwrap();
        assert_eq!(m.mc_versions[0], "26.3");
        assert!(m.mc_versions.contains(&"1.21.1".to_string()));
        // Fabric's 1.21.5 has neither a Fabric API nor a NeoForge build in the fixtures.
        assert!(!m.mc_versions.contains(&"1.21.5".to_string()));
        assert_eq!(m.fabric_loader.as_deref(), Some("0.20.1"));
        assert_eq!(m.fabric_api["26.3"], "0.165.2+26.3");
        assert_eq!(m.neoforge["26.3"], "26.3.0.101");
        assert_eq!(m.yarn["1.21.11"], "1.21.11+build.6");
        assert_eq!(m.moddev.as_deref(), Some("2.0.148"));
        // Loom is not served: it keeps its pinned version.
        assert_eq!(m.loom, None);
        // Nothing answers: an error.
        let none = ModSources {
            fabric_meta: format!("{url}/nowhere"),
            fabric_maven: format!("{url}/nowhere"),
            neoforge_maven: format!("{url}/nowhere"),
        };
        assert!(refresh_mods(&none).await.unwrap_err().contains("404"));
    }

    const PAPER: &str = include_str!("../tests/fixtures/paper-api-maven-metadata.xml");
    const PURPUR: &str = include_str!("../tests/fixtures/purpur-api-maven-metadata.xml");

    #[tokio::test]
    async fn fetches_versions_from_a_repository() {
        let url = serve(&[("/repo/io/papermc/paper/paper-api/maven-metadata.xml", PAPER)]).await;
        let versions = fetch_mc_versions(&format!("{url}/repo/")).await.unwrap();
        assert_eq!(versions[0], "26.3");
        let err = fetch_mc_versions(&format!("{url}/missing"))
            .await
            .unwrap_err();
        assert!(err.contains("404"), "{err}");
    }

    #[tokio::test]
    async fn refreshes_versions_and_builds() {
        let url = serve(&[
            (
                "/paper/io/papermc/paper/paper-api/maven-metadata.xml",
                PAPER,
            ),
            (
                "/purpur/org/purpurmc/purpur/purpur-api/maven-metadata.xml",
                PURPUR,
            ),
        ])
        .await;
        let r = refresh(&format!("{url}/paper"), &format!("{url}/purpur"))
            .await
            .unwrap();
        assert_eq!(r.mc_versions[0], "26.3");
        assert_eq!(r.builds.paper["26.2"], "26.2.build.150-stable");
        assert_eq!(r.builds.purpur["26.3"], "26.3.build.2655-stable");
        // Purpur unreachable: Paper's answer still counts.
        let r = refresh(&format!("{url}/paper"), &format!("{url}/nowhere"))
            .await
            .unwrap();
        assert!(r.builds.purpur.is_empty());
    }
}
