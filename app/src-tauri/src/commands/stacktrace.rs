//! Analyze External Stack Trace (Code | Analyze Stack Trace or Thread Dump…): a pasted trace
//! becomes console lines whose frames link into the project (`forge_output::stacktrace`).
//! Paths of servers and containers are mapped with the PHP servers' path mappings and the
//! bind mounts of the project's Docker Compose file; files are looked up through the
//! backend, so remote projects work too.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use forge_backend::{Backend, BackendFiles};
use forge_core::Result;
use forge_process::output::stacktrace::{self, PathMapping, TraceAnalysis, TraceContext};
use tauri::State;

use crate::commands::workspace::blocking;
use crate::state::AppState;

const COMPOSE_FILES: &[&str] = &[
    "compose.yaml",
    "compose.yml",
    "docker-compose.yml",
    "docker-compose.yaml",
];

/// Path mappings of the project: PHP servers (Settings | PHP | Servers) and the bind mounts
/// of the Compose file in the project root.
fn mappings(backend: &std::sync::Arc<dyn Backend>) -> Vec<PathMapping> {
    let mut out: Vec<PathMapping> =
        forge_debug::servers::load_servers_from(&BackendFiles(backend.clone()))
            .into_iter()
            .filter(|s| s.use_path_mappings)
            .flat_map(|s| s.mappings)
            .map(|m| PathMapping {
                local: m.local,
                remote: m.remote,
            })
            .collect();
    for name in COMPOSE_FILES {
        if let Ok(Some(text)) = backend.read_optional(name) {
            let file = forge_containers::compose::parse(name, &text, &Default::default());
            for service in &file.services {
                out.extend(stacktrace::compose_mappings("", &service.volumes));
            }
            break;
        }
    }
    out
}

/// The analyzed trace: console lines with links to project files.
#[tauri::command]
pub async fn stacktrace_analyze(text: String, state: State<'_, AppState>) -> Result<TraceAnalysis> {
    let backend = state.backend()?;
    blocking(move || {
        let ctx = TraceContext {
            root: Some(backend.info().root.to_string_lossy().replace('\\', "/")),
            mappings: mappings(&backend),
        };
        // One directory listing per folder a candidate lives in.
        let listings: Mutex<HashMap<String, HashSet<String>>> = Mutex::default();
        let exists = |rel: &str| {
            let (dir, name) = rel.rsplit_once('/').unwrap_or(("", rel));
            let mut cache = listings.lock().expect("listing cache");
            let names = cache.entry(dir.to_string()).or_insert_with(|| {
                backend
                    .list_dir(dir)
                    .map(|entries| entries.into_iter().map(|e| e.name).collect())
                    .unwrap_or_default()
            });
            names.contains(name)
        };
        Ok(stacktrace::analyze(&text, &ctx, &exists))
    })
    .await
}
