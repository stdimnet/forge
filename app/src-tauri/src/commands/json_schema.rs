//! JSON Schema for JSON and YAML files (`forge-jsonschema`): problems, completion, hover and
//! Go to Declaration in the editor, the schema shown in the status bar, and read-only
//! schema files for navigation.
//!
//! Schemas come from (see `forge_jsonschema::mapping`): the user's mappings
//! (`jsonSchema.mappings`), the file's `$schema`, the bundled built-ins and the SchemaStore
//! catalog. Project files are read through the backend, so remote projects work; URLs are
//! downloaded only with `jsonSchema.remote.enabled`, into `<cache dir>/forge/json-schemas`.
//!
//! IPC: `json_schema_validate`, `json_schema_complete`, `json_schema_hover`,
//! `json_schema_goto` (`{ path, language, content, offset? }`), `json_schema_read({ location })`,
//! `json_schema_builtins()`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use forge_core::{ForgeError, Result};
use forge_jsonschema::mapping::{self, Catalog, Selection, Source, UserMapping};
use forge_jsonschema::remote::{RemoteCache, CATALOG_URL};
use forge_jsonschema::{bundled, uri, DocKind, Registry, SchemaDoc};
use serde::Serialize;
use tauri::State;

use crate::commands::workspace::blocking;
use crate::state::AppState;

pub const MAPPINGS: &str = "jsonSchema.mappings";
pub const REMOTE_ENABLED: &str = "jsonSchema.remote.enabled";
pub const SCHEMA_STORE: &str = "jsonSchema.remote.schemaStore";

const REMOTE_PREFIX: &str = "json-schema://remote/";
const FILE_PREFIX: &str = "json-schema://file/";
/// A failed download is not retried before this.
const RETRY_AFTER: Duration = Duration::from_secs(300);
/// At most this many schema files per file (refs included).
const MAX_DOCS: usize = 40;

#[derive(Default)]
pub struct JsonSchemaState {
    /// Parsed project schemas by URI, with the text they were parsed from.
    project: Mutex<HashMap<String, (String, Arc<SchemaDoc>)>>,
    /// Downloaded and local (`file://`) schemas by URI.
    loaded: Mutex<HashMap<String, Arc<SchemaDoc>>>,
    failed: Mutex<HashMap<String, (Instant, String)>>,
    catalog: tokio::sync::Mutex<Option<(Instant, Arc<Catalog>)>>,
    cache: Mutex<Option<Arc<RemoteCache>>>,
}

struct Options {
    mappings: Vec<UserMapping>,
    remote: bool,
    schema_store: bool,
}

fn options(state: &AppState) -> Options {
    let s = state.read_settings();
    Options {
        mappings: s.get_as::<Vec<UserMapping>>(MAPPINGS).unwrap_or_default(),
        remote: s.get_as::<bool>(REMOTE_ENABLED).unwrap_or(false),
        schema_store: s.get_as::<bool>(SCHEMA_STORE).unwrap_or(true),
    }
}

/// The schema of a file, as the status bar shows it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaInfo {
    pub name: String,
    /// Empty when a mapping turned schemas off for the file.
    pub uri: String,
    pub source: Source,
    /// What Go to Declaration opens for the schema file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    /// Why the schema could not be used.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidateAnswer {
    pub schema: Option<SchemaInfo>,
    pub problems: Vec<forge_jsonschema::ProblemOut>,
}

impl JsonSchemaState {
    fn cache(&self) -> std::result::Result<Arc<RemoteCache>, String> {
        let mut slot = self.cache.lock().expect("cache lock");
        if let Some(c) = slot.as_ref() {
            return Ok(c.clone());
        }
        let dir = dirs::cache_dir()
            .map(|d| d.join("forge").join("json-schemas"))
            .ok_or("no cache folder for JSON schemas")?;
        let c = Arc::new(RemoteCache::new(dir)?);
        *slot = Some(c.clone());
        Ok(c)
    }

    /// SchemaStore's catalog (downloaded at most daily, cached on disk).
    async fn catalog(&self) -> Option<Arc<Catalog>> {
        let mut slot = self.catalog.lock().await;
        if let Some((at, c)) = slot.as_ref() {
            if at.elapsed() < Duration::from_secs(24 * 60 * 60) {
                return Some(c.clone());
            }
        }
        if self.recently_failed(CATALOG_URL).is_some() {
            return slot.as_ref().map(|(_, c)| c.clone());
        }
        let cache = self.cache().ok()?;
        match cache
            .fetch(CATALOG_URL)
            .await
            .and_then(|t| Catalog::parse(&t))
        {
            Ok(c) => {
                let c = Arc::new(c);
                *slot = Some((Instant::now(), c.clone()));
                Some(c)
            }
            Err(e) => {
                self.fail(CATALOG_URL, e);
                None
            }
        }
    }

    fn recently_failed(&self, uri: &str) -> Option<String> {
        let failed = self.failed.lock().expect("failed lock");
        failed
            .get(uri)
            .filter(|(at, _)| at.elapsed() < RETRY_AFTER)
            .map(|(_, e)| e.clone())
    }

    fn fail(&self, uri: &str, error: String) {
        self.failed
            .lock()
            .expect("failed lock")
            .insert(uri.to_string(), (Instant::now(), error));
    }

    /// Load one schema document (not a bundled one).
    async fn load(
        &self,
        state: &AppState,
        uri: &str,
        remote: bool,
    ) -> std::result::Result<Arc<SchemaDoc>, String> {
        if let Some(path) = uri::project_path(uri) {
            let backend = state.backend().map_err(|e| e.to_string())?;
            let rel = path.to_string();
            let text = blocking(move || backend.read_file(&rel))
                .await
                .map_err(|e| format!("Cannot read {path}: {e}"))?;
            let mut project = self.project.lock().expect("project lock");
            if let Some((old, doc)) = project.get(uri) {
                if *old == text {
                    return Ok(doc.clone());
                }
            }
            let doc = Arc::new(
                SchemaDoc::parse(uri, path, text.clone()).map_err(|e| format!("{path}: {e}"))?,
            );
            project.insert(uri.to_string(), (text, doc.clone()));
            return Ok(doc);
        }
        if let Some(doc) = self.loaded.lock().expect("loaded lock").get(uri) {
            return Ok(doc.clone());
        }
        if let Some(e) = self.recently_failed(uri) {
            return Err(e);
        }
        let result = if let Some(abs) = uri.strip_prefix("file://") {
            let path = PathBuf::from(abs);
            std::fs::read_to_string(&path)
                .map_err(|e| format!("Cannot read {abs}: {e}"))
                .and_then(|text| SchemaDoc::parse(uri, &format!("{FILE_PREFIX}{abs}"), text))
        } else if uri::is_remote(uri) {
            if !remote {
                return Err(
                    "Downloading remote schemas is turned off (Settings | Languages & Frameworks | Schemas and DTDs | Remote JSON Schemas)"
                        .into(),
                );
            }
            match self.cache()?.fetch(uri).await {
                Ok(text) => SchemaDoc::parse(uri, &format!("{REMOTE_PREFIX}{uri}"), text),
                Err(e) => Err(e),
            }
        } else {
            Err(format!("Unknown schema location: {uri}"))
        };
        match result {
            Ok(doc) => {
                let doc = Arc::new(doc);
                self.loaded
                    .lock()
                    .expect("loaded lock")
                    .insert(uri.to_string(), doc.clone());
                Ok(doc)
            }
            Err(e) => {
                self.fail(uri, e.clone());
                Err(e)
            }
        }
    }

    /// The registry with the schema of `uri` and everything its `$ref`s need.
    async fn registry(
        &self,
        state: &AppState,
        root: &str,
        remote: bool,
    ) -> (Registry, Option<String>) {
        let mut reg = Registry::new();
        let mut queue = vec![uri::document(root).to_string()];
        let mut count = 0;
        let mut root_error = None;
        while let Some(u) = queue.pop() {
            if reg.contains(&u) || reg.failure(&u).is_some() {
                continue;
            }
            count += 1;
            if count > MAX_DOCS {
                break;
            }
            if forge_jsonschema::load_bundled(&mut reg, &u) {
                if !reg.contains(&u) {
                    reg.mark_failed(&u, "not bundled".into());
                }
            } else {
                match self.load(state, &u, remote).await {
                    Ok(doc) => {
                        let loaded = doc.uri.clone();
                        reg.insert(doc);
                        reg.alias(&u, &loaded);
                    }
                    Err(e) => {
                        if count == 1 {
                            root_error = Some(e.clone());
                        }
                        reg.mark_failed(&u, e);
                    }
                }
            }
            queue.extend(reg.missing());
        }
        (reg, root_error)
    }

    /// The schema of a file, ready to use.
    async fn prepare(
        &self,
        state: &AppState,
        path: &str,
        kind: DocKind,
        content: &str,
    ) -> Option<(Selection, Registry, SchemaInfo)> {
        let opts = options(state);
        let mut selection = mapping::select(path, content, kind, &opts.mappings, None);
        if selection.is_none() && opts.remote && opts.schema_store {
            let catalog = self.catalog().await;
            selection = mapping::select(path, content, kind, &opts.mappings, catalog.as_deref());
        }
        let selection = selection?;
        let mut info = SchemaInfo {
            name: selection.name.clone(),
            uri: selection.uri.clone(),
            source: selection.source,
            location: None,
            error: None,
        };
        if selection.uri.is_empty() {
            return Some((selection, Registry::new(), info));
        }
        let (mut reg, error) = self.registry(state, &selection.uri, opts.remote).await;
        reg.draft = selection.version;
        info.location = reg.doc(&selection.uri).map(|d| d.location.clone());
        info.error = error;
        Some((selection, reg, info))
    }
}

fn kind_of(language: &str, path: &str) -> Option<DocKind> {
    DocKind::from_language(language).or_else(|| DocKind::from_path(path))
}

#[tauri::command]
pub async fn json_schema_validate(
    path: String,
    language: String,
    content: String,
    state: State<'_, AppState>,
    js: State<'_, JsonSchemaState>,
) -> Result<ValidateAnswer> {
    let Some(kind) = kind_of(&language, &path) else {
        return Ok(ValidateAnswer {
            schema: None,
            problems: Vec::new(),
        });
    };
    let Some((sel, reg, info)) = js.prepare(&state, &path, kind, &content).await else {
        return Ok(ValidateAnswer {
            schema: None,
            problems: Vec::new(),
        });
    };
    if sel.uri.is_empty() || info.error.is_some() {
        return Ok(ValidateAnswer {
            schema: Some(info),
            problems: Vec::new(),
        });
    }
    let uri = sel.uri.clone();
    let problems =
        blocking(move || Ok(forge_jsonschema::validate(&reg, &uri, kind, &content))).await?;
    Ok(ValidateAnswer {
        schema: Some(info),
        problems,
    })
}

#[tauri::command]
pub async fn json_schema_complete(
    path: String,
    language: String,
    content: String,
    offset: usize,
    state: State<'_, AppState>,
    js: State<'_, JsonSchemaState>,
) -> Result<Option<forge_jsonschema::CompletionOut>> {
    let Some(kind) = kind_of(&language, &path) else {
        return Ok(None);
    };
    let Some((sel, reg, _)) = js.prepare(&state, &path, kind, &content).await else {
        return Ok(None);
    };
    Ok(forge_jsonschema::complete(
        &reg, &sel.uri, kind, &content, offset,
    ))
}

#[tauri::command]
pub async fn json_schema_hover(
    path: String,
    language: String,
    content: String,
    offset: usize,
    state: State<'_, AppState>,
    js: State<'_, JsonSchemaState>,
) -> Result<Option<forge_jsonschema::HoverOut>> {
    let Some(kind) = kind_of(&language, &path) else {
        return Ok(None);
    };
    let Some((sel, reg, _)) = js.prepare(&state, &path, kind, &content).await else {
        return Ok(None);
    };
    Ok(forge_jsonschema::hover(
        &reg, &sel.uri, kind, &content, offset,
    ))
}

#[tauri::command]
pub async fn json_schema_goto(
    path: String,
    language: String,
    content: String,
    offset: usize,
    state: State<'_, AppState>,
    js: State<'_, JsonSchemaState>,
) -> Result<Vec<forge_jsonschema::Target>> {
    let Some(kind) = kind_of(&language, &path) else {
        return Ok(Vec::new());
    };
    let Some((sel, reg, _)) = js.prepare(&state, &path, kind, &content).await else {
        return Ok(Vec::new());
    };
    Ok(forge_jsonschema::goto(
        &reg, &sel.uri, kind, &content, offset,
    ))
}

/// A read-only schema file opened by Go to Declaration.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaFile {
    pub text: String,
    /// The banner: "Bundled JSON schema (Apache-2.0, SchemaStore)", the URL, the path.
    pub detail: String,
}

/// The text of a `json-schema://` location: a bundled schema, or one loaded before.
pub fn read_location(js: &JsonSchemaState, location: &str) -> Result<SchemaFile> {
    if let Some(b) = bundled::by_location(location) {
        return Ok(SchemaFile {
            text: b.text(),
            detail: format!("Bundled with Forge · {}", b.uri()),
        });
    }
    let loaded = js.loaded.lock().expect("loaded lock");
    let doc = loaded
        .values()
        .find(|d| d.location == location)
        .ok_or_else(|| ForgeError::Other(format!("not a loaded schema: {location}")))?;
    Ok(SchemaFile {
        text: doc.text.clone(),
        detail: doc.uri.clone(),
    })
}

#[tauri::command]
pub async fn json_schema_read(
    location: String,
    js: State<'_, JsonSchemaState>,
) -> Result<SchemaFile> {
    read_location(&js, &location)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuiltinSchema {
    pub name: String,
    pub uri: String,
    pub files: Vec<String>,
}

/// The built-in mappings (the status bar's schema list).
#[tauri::command]
pub fn json_schema_builtins() -> Vec<BuiltinSchema> {
    mapping::BUILTINS
        .iter()
        .map(|b| BuiltinSchema {
            name: b.name.to_string(),
            uri: b.uri.to_string(),
            files: b.files.iter().map(|f| f.to_string()).collect(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_bundled_locations() {
        let js = JsonSchemaState::default();
        let f = read_location(&js, "json-schema://bundled/composer.json").unwrap();
        assert!(f.text.contains("\"properties\""));
        assert!(f.detail.contains("getcomposer.org"));
        assert!(read_location(&js, "json-schema://remote/https://x.org/s.json").is_err());
        assert_eq!(json_schema_builtins()[0].name, "composer.json");
    }
}
