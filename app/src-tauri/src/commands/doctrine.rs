//! Doctrine IPC: entity mappings, DQL completion/navigation/diagnostics inside PHP strings
//! and entity/repository gutter marks. Logic lives in `phpforge-orm-doctrine`.
//! Offsets crossing the boundary are UTF-16 (JS strings).

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use forge_core::{ForgeError, Result};
use phpforge_lang_php::offsets::{byte_to_utf16, utf16_to_byte};
use phpforge_orm_doctrine::dql::Item;
use phpforge_orm_doctrine::php::{self, GutterMark, SourceDiagnostic};
use phpforge_orm_doctrine::{Config, Entity, Location, Snapshot};
use serde::Serialize;
use tauri::State;

use crate::state::AppState;

const SNAPSHOT_TTL: Duration = Duration::from_secs(10);

#[derive(Default)]
pub struct DoctrineState {
    snapshot: Mutex<Option<(PathBuf, Instant, Arc<Snapshot>)>>,
}

fn err(e: impl std::fmt::Display) -> ForgeError {
    ForgeError::Other(e.to_string())
}

fn config(state: &AppState) -> Config {
    let s = state.read_settings();
    let d = Config::default();
    Config {
        source_directories: s
            .get_as("doctrine.sourceDirectories")
            .unwrap_or(d.source_directories),
        mapping_directories: s
            .get_as("doctrine.mappingDirectories")
            .unwrap_or(d.mapping_directories),
        annotations: s.get_as("doctrine.annotations").unwrap_or(d.annotations),
    }
}

async fn snapshot(state: &AppState, ds: &DoctrineState, force: bool) -> Result<Arc<Snapshot>> {
    let root = state.with_workspace(|ws| Ok(ws.root().to_path_buf()))?;
    if !force {
        if let Some((r, at, snap)) = ds.snapshot.lock().map_err(err)?.as_ref() {
            if *r == root && at.elapsed() < SNAPSHOT_TTL {
                return Ok(snap.clone());
            }
        }
    }
    let cfg = config(state);
    let scan_root = root.clone();
    let snap =
        tauri::async_runtime::spawn_blocking(move || phpforge_orm_doctrine::scan(&scan_root, &cfg))
            .await
            .map_err(err)?;
    let snap = Arc::new(snap);
    *ds.snapshot.lock().map_err(err)? = Some((root, Instant::now(), snap.clone()));
    Ok(snap)
}

/// Mapped entities, embeddables and mapped superclasses (rescanned with `refresh`).
#[tauri::command]
pub async fn doctrine_entities(
    refresh: Option<bool>,
    state: State<'_, AppState>,
    ds: State<'_, DoctrineState>,
) -> Result<Vec<Entity>> {
    Ok(snapshot(&state, &ds, refresh.unwrap_or(false))
        .await?
        .entities
        .clone())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DqlCompletion {
    pub from: usize,
    pub items: Vec<Item>,
}

/// DQL completion inside `createQuery()` and QueryBuilder strings.
#[tauri::command]
pub async fn doctrine_complete(
    path: String,
    content: String,
    offset: usize,
    state: State<'_, AppState>,
    ds: State<'_, DoctrineState>,
) -> Result<Option<DqlCompletion>> {
    let snap = snapshot(&state, &ds, false).await?;
    let byte = utf16_to_byte(&content, offset);
    Ok(
        php::complete(&snap, &path, &content, byte).map(|(from, c)| DqlCompletion {
            from: byte_to_utf16(&content, from),
            items: c.items,
        }),
    )
}

/// Entity, field or association declaration under the cursor in a DQL string.
#[tauri::command]
pub async fn doctrine_goto(
    path: String,
    content: String,
    offset: usize,
    state: State<'_, AppState>,
    ds: State<'_, DoctrineState>,
) -> Result<Vec<Location>> {
    let snap = snapshot(&state, &ds, false).await?;
    Ok(php::goto(
        &snap,
        &path,
        &content,
        utf16_to_byte(&content, offset),
    ))
}

/// Unknown entities, aliases and fields in the file's DQL.
#[tauri::command]
pub async fn doctrine_diagnostics(
    path: String,
    content: String,
    state: State<'_, AppState>,
    ds: State<'_, DoctrineState>,
) -> Result<Vec<SourceDiagnostic>> {
    let snap = snapshot(&state, &ds, false).await?;
    Ok(php::diagnostics(&snap, &path, &content)
        .into_iter()
        .map(|mut d| {
            d.start = byte_to_utf16(&content, d.start);
            d.end = byte_to_utf16(&content, d.end);
            d
        })
        .collect())
}

/// Entity <-> repository line markers.
#[tauri::command]
pub async fn doctrine_gutter(
    path: String,
    content: String,
    state: State<'_, AppState>,
    ds: State<'_, DoctrineState>,
) -> Result<Vec<GutterMark>> {
    let snap = snapshot(&state, &ds, false).await?;
    Ok(php::gutter(&snap, &path, &content))
}
