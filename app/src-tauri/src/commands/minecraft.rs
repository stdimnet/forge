//! Minecraft plugins and mods (`javaforge-minecraft`): "Refresh versions" of the New Project
//! wizard (Paper's `maven-metadata.xml`; for mods Fabric's meta API and the Fabric / NeoForge
//! Maven metadata; only when the user clicks it, cached in the user cache dir), the class
//! checks and navigation in plugin descriptors, `fabric.mod.json` and mixin configurations,
//! `neoforge.mods.toml` completion, and the gutter markers of event handlers and the plugin's
//! main class.

use std::path::PathBuf;

use forge_core::{ForgeError, Result};
use javaforge_minecraft::descriptor::{self, Problem, Target};
use javaforge_minecraft::markers::{self, Marker, MarkerKind};
use javaforge_minecraft::mods::toml::{self as mods_toml, TomlCompletion};
use javaforge_minecraft::mods::{editor as mod_editor, versions as mod_versions};
use javaforge_minecraft::net::Refreshed;
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::commands::workspace::blocking;
use crate::state::AppState;

fn versions_cache() -> Result<PathBuf> {
    dirs::cache_dir()
        .map(|d| d.join("forge").join("minecraft").join("versions.json"))
        .ok_or_else(|| ForgeError::Other("no user cache directory".into()))
}

/// What the last "Refresh versions" found (empty before the first). Also hands the
/// refreshed Paper / Purpur builds (26.1+) and mod versions to the generators.
pub(crate) fn cached_refresh() -> Refreshed {
    let refreshed: Refreshed = versions_cache()
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    javaforge_minecraft::versions::remember_builds(refreshed.builds.clone());
    mod_versions::remember(refreshed.mods.clone());
    refreshed
}

/// Minecraft versions of the last "Refresh versions" (empty before the first): the plugin
/// generator's and the mod generator's.
pub(crate) fn cached_versions() -> (Vec<String>, Vec<String>) {
    let r = cached_refresh();
    (r.mc_versions, r.mods.mc_versions)
}

fn save_refresh(found: &Refreshed) {
    if let Ok(path) = versions_cache() {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(text) = serde_json::to_string(found) {
            let _ = std::fs::write(&path, text);
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Choice {
    value: String,
    label: String,
}

/// Read the Minecraft versions and the newest Paper / Purpur API builds from repo.papermc.io
/// and repo.purpurmc.org (the Minecraft Plugin wizard), or, for `generator`
/// `minecraft-mod`, the Fabric and NeoForge versions from meta.fabricmc.net,
/// maven.fabricmc.net and maven.neoforged.net, and return the new choices of the wizard's
/// "Minecraft" option.
#[tauri::command]
pub async fn minecraft_versions_refresh(generator: Option<String>) -> Result<Vec<Choice>> {
    let mut cached = cached_refresh();
    let choices = if generator.as_deref() == Some("minecraft-mod") {
        let sources = javaforge_minecraft::net::ModSources::default();
        let found = javaforge_minecraft::net::refresh_mods(&sources)
            .await
            .map_err(ForgeError::Other)?;
        mod_versions::remember(found.clone());
        let choices = mod_versions::mod_version_choices(&found.mc_versions);
        cached.mods = found;
        choices
    } else {
        let (paper, purpur) = javaforge_minecraft::net::default_repositories();
        let found = javaforge_minecraft::net::refresh(paper, purpur)
            .await
            .map_err(ForgeError::Other)?;
        javaforge_minecraft::versions::remember_builds(found.builds.clone());
        let choices = javaforge_minecraft::versions::version_choices(&found.mc_versions);
        cached.mc_versions = found.mc_versions;
        cached.builds = found.builds;
        choices
    };
    save_refresh(&cached);
    Ok(choices
        .into_iter()
        .map(|(value, label)| Choice { value, label })
        .collect())
}

/// Problems of the `main` entry of a plugin descriptor, and of the classes and mixin
/// configurations of `fabric.mod.json` and mixin configurations.
#[tauri::command]
pub async fn minecraft_inspect(
    path: String,
    content: String,
    app: AppHandle,
) -> Result<Vec<Problem>> {
    blocking(move || {
        let backend = app.state::<AppState>().backend()?;
        let exists = |rel: &str| backend.read_optional(rel).ok().flatten().is_some();
        let mut out = descriptor::inspect(&path, &content, &exists);
        out.extend(mod_editor::inspect(&path, &content, &exists));
        Ok(out)
    })
    .await
}

/// Completion in `neoforge.mods.toml` at `pos` (UTF-16).
#[tauri::command]
pub async fn minecraft_complete(
    path: String,
    content: String,
    pos: usize,
) -> Result<Option<TomlCompletion>> {
    Ok(if mods_toml::is_mods_toml(&path) {
        mods_toml::complete(&content, pos)
    } else {
        None
    })
}

/// Ctrl+B on the `main` value of a plugin descriptor, an entrypoint or mixin configuration of
/// `fabric.mod.json`, or a mixin class of a mixin configuration.
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
            .or_else(|| mod_editor::goto(&path, &content, pos, &read))
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
