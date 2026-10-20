//! Doctrine IPC: entity mappings, DQL completion/navigation/diagnostics inside PHP strings
//! and entity/repository gutter marks. Logic lives in `phpforge-orm-doctrine`; it runs
//! where the project's files are (`forge_backend::frameworks`).
//! Offsets crossing the boundary are UTF-16 (JS strings).

use forge_backend::frameworks::DoctrineQuery;
use forge_backend::{framework_call, FrameworkCall};
use forge_core::Result;
use phpforge_dql::editor::DqlCompletion;
use phpforge_orm_doctrine::php::{GutterMark, SourceDiagnostic, SourceHighlight};
use phpforge_orm_doctrine::{Config, Entity, Location};
use serde::de::DeserializeOwned;
use tauri::State;

use crate::commands::workspace::blocking;
use crate::state::AppState;

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

/// Ask the project's backend about the mapping snapshot (cached there for 10 s).
async fn query<T: DeserializeOwned + Send + 'static>(
    state: &AppState,
    query: DoctrineQuery,
) -> Result<T> {
    let backend = state.backend()?;
    let call = FrameworkCall::Doctrine {
        config: config(state),
        query,
    };
    blocking(move || framework_call(backend.as_ref(), call)).await
}

/// Mapped entities, embeddables and mapped superclasses (rescanned with `refresh`).
#[tauri::command]
pub async fn doctrine_entities(
    refresh: Option<bool>,
    state: State<'_, AppState>,
) -> Result<Vec<Entity>> {
    query(
        &state,
        DoctrineQuery::Entities {
            refresh: refresh.unwrap_or(false),
        },
    )
    .await
}

/// DQL completion inside `createQuery()` and QueryBuilder strings.
#[tauri::command]
pub async fn doctrine_complete(
    path: String,
    content: String,
    offset: usize,
    state: State<'_, AppState>,
) -> Result<Option<DqlCompletion>> {
    query(
        &state,
        DoctrineQuery::Complete {
            path,
            content,
            offset,
        },
    )
    .await
}

/// Entity, field or association declaration under the cursor in a DQL string.
#[tauri::command]
pub async fn doctrine_goto(
    path: String,
    content: String,
    offset: usize,
    state: State<'_, AppState>,
) -> Result<Vec<Location>> {
    query(
        &state,
        DoctrineQuery::Goto {
            path,
            content,
            offset,
        },
    )
    .await
}

/// Unknown entities, aliases and fields in the file's DQL.
#[tauri::command]
pub async fn doctrine_diagnostics(
    path: String,
    content: String,
    state: State<'_, AppState>,
) -> Result<Vec<SourceDiagnostic>> {
    query(&state, DoctrineQuery::Diagnostics { path, content }).await
}

/// DQL injection highlighting of a PHP file: the DQL string pieces and semantic spans.
#[tauri::command]
pub async fn doctrine_highlight(
    path: String,
    content: String,
    state: State<'_, AppState>,
) -> Result<SourceHighlight> {
    query(&state, DoctrineQuery::Highlight { path, content }).await
}

/// Entity <-> repository line markers.
#[tauri::command]
pub async fn doctrine_gutter(
    path: String,
    content: String,
    state: State<'_, AppState>,
) -> Result<Vec<GutterMark>> {
    query(&state, DoctrineQuery::Gutter { path, content }).await
}
