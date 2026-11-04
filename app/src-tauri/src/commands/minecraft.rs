//! Minecraft plugins (`javaforge-minecraft`): "Refresh versions" of the New Project wizard
//! (Paper's `maven-metadata.xml`, only when the user clicks it, cached in the user cache
//! dir), the `main` class check and navigation in plugin descriptors, and the gutter markers
//! of event handlers and the plugin's main class.

use std::path::PathBuf;

use forge_core::{ForgeError, Result};
use javaforge_minecraft::descriptor::{self, Problem, Target};
use javaforge_minecraft::markers::{self, Marker, MarkerKind};
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::commands::workspace::blocking;
use crate::state::AppState;

fn versions_cache() -> Result<PathBuf> {
    dirs::cache_dir()
        .map(|d| d.join("forge").join("minecraft").join("versions.json"))
        .ok_or_else(|| ForgeError::Other("no user cache directory".into()))
}

/// Minecraft versions of the last "Refresh versions" (empty before the first).
pub(crate) fn cached_versions() -> Vec<String> {
    versions_cache()
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Choice {
    value: String,
    label: String,
}

/// Read the Minecraft versions Paper publishes from repo.papermc.io and return the new
/// choices of the wizard's "Minecraft version" option.
#[tauri::command]
pub async fn minecraft_versions_refresh() -> Result<Vec<Choice>> {
    let found =
        javaforge_minecraft::net::fetch_mc_versions(javaforge_minecraft::net::default_repository())
            .await
            .map_err(ForgeError::Other)?;
    if let Ok(path) = versions_cache() {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(text) = serde_json::to_string(&found) {
            let _ = std::fs::write(&path, text);
        }
    }
    Ok(javaforge_minecraft::versions::version_choices(&found)
        .into_iter()
        .map(|(value, label)| Choice { value, label })
        .collect())
}

/// Problems of the `main` entry of a plugin descriptor.
#[tauri::command]
pub async fn minecraft_inspect(
    path: String,
    content: String,
    app: AppHandle,
) -> Result<Vec<Problem>> {
    blocking(move || {
        let backend = app.state::<AppState>().backend()?;
        let exists = |rel: &str| backend.read_optional(rel).ok().flatten().is_some();
        Ok(descriptor::inspect(&path, &content, &exists))
    })
    .await
}

/// Ctrl+B on the `main` value of a plugin descriptor.
#[tauri::command]
pub async fn minecraft_goto(
    path: String,
    content: String,
    pos: usize,
    app: AppHandle,
) -> Result<Vec<Target>> {
    blocking(move || {
        let backend = app.state::<AppState>().backend()?;
        let read = |rel: &str| backend.read_optional(rel).ok().flatten();
        Ok(descriptor::goto(&path, &content, pos, &read)
            .into_iter()
            .collect())
    })
    .await
}

/// Gutter markers of a Java / Kotlin source; the main class's "Run Paper Server" only when
/// the build applies run-paper.
#[tauri::command]
pub async fn minecraft_gutter(
    path: String,
    content: String,
    app: AppHandle,
) -> Result<Vec<Marker>> {
    blocking(move || {
        let state = app.state::<AppState>();
        if !crate::commands::java::flag(&state, "minecraft.gutterIcons", true) {
            return Ok(Vec::new());
        }
        let backend = state.backend()?;
        let builds: Vec<String> = ["build.gradle.kts", "build.gradle", "pom.xml"]
            .iter()
            .filter_map(|f| backend.read_optional(f).ok().flatten())
            .collect();
        let texts: Vec<&str> = builds.iter().map(String::as_str).collect();
        let run_paper = javaforge_minecraft::detect(&texts).is_some_and(|d| d.run_paper);
        let mut out = markers::markers(&content, path.ends_with(".kt"));
        out.retain(|m| m.kind != MarkerKind::Plugin || run_paper);
        Ok(out)
    })
    .await
}
