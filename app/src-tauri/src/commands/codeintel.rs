//! Code intelligence actions on top of the language servers and the PHP index: Find Usages
//! (Alt+F7) / Show Usages / Find Usages in File with the Find Usages options, Highlight Usages
//! in File (and of the element at the caret), Rename (Shift+F6), Show Context Actions (Alt+Enter), Go to
//! Implementation / Type Declaration / Super Method, applying workspace edits to the files
//! that are not open in an editor, and the documentation actions of the View menu (External
//! Documentation, Quick Definition, Type Info, Context Info) and Related Symbol. Paths are workspace-relative;
//! positions are LSP positions (0-based line, UTF-16 character). File access goes through
//! the project's backend, so everything works for remote projects too.
//!
//! Events: `lsp://apply-edit` `{ label?, edit: WorkspaceEdit }` when a server applies an
//! edit itself (e.g. while running a code action's command).

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use forge_backend::Backend;
use forge_core::search::FindRequest;
use forge_core::{ForgeError, Result};
use forge_lsp::forge_codeintel::{
    self as codeintel, order_actions, preview_workspace_edit, ActionKind, AppliedFile, Bulb,
    ContextAction, EditPlan, ElementKind, FilePreview, FileRename, FileStore, GroupRequest,
    HighlightAnswer, HighlightTarget, Position, Range, RenameConflict, UsageHighlight,
    UsageLocation, UsageOptions, UsageScope, UsageTarget, UsageTree, WorkspaceEdit,
};
use forge_lsp::model::{EditorCodeAction, EditorLocation};
use forge_lsp::{lsp_types, Feature, LspManager};
use phpforge_lang_php::generate::GenerateOptions;
use phpforge_lang_php::navigate::{NavKind, NavTarget, PhpLineMarker};
use phpforge_lang_php::offsets::{byte_to_utf16, utf16_to_byte};
use phpforge_lang_php::stubs::StubTarget;
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, State};

use crate::commands::injections;
use crate::commands::library::{
    read_library, stub_path, LibraryState, LIBRARY_PREFIX, STUBS_PREFIX,
};
use crate::commands::lsp::{to_ui_path, LspState};
use crate::commands::workspace::blocking;
use crate::state::AppState;

/// Why an action found nothing: no server of the file supports it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MissingFeature {
    pub feature: String,
    /// "Rename is not supported by yaml-language-server"
    pub message: String,
}

fn missing_feature(feature: Feature, servers: &[String]) -> MissingFeature {
    MissingFeature {
        feature: feature.title().into(),
        message: format!(
            "{} is not supported by {}",
            feature.title(),
            servers.join(", ")
        ),
    }
}

pub(crate) fn is_php(path: &str) -> bool {
    let p = path.to_ascii_lowercase();
    (p.ends_with(".php") || p.ends_with(".phtml") || p.ends_with(".inc"))
        && !p.ends_with(".blade.php")
}

/// Reads and writes project files through the backend (local folder or remote host).
struct BackendStore(Arc<dyn Backend>);

impl FileStore for BackendStore {
    fn read(&mut self, path: &str) -> std::result::Result<Option<String>, String> {
        self.0.read_optional(path).map_err(|e| e.to_string())
    }
    fn write(&mut self, path: &str, text: &str) -> std::result::Result<(), String> {
        self.0.write_file(path, text).map_err(|e| e.to_string())
    }
    fn delete(&mut self, path: &str) -> std::result::Result<(), String> {
        self.0.delete(path).map_err(|e| e.to_string())
    }
}

/// Texts of project files: editor buffers first, then the backend.
struct Texts {
    backend: Arc<dyn Backend>,
    buffers: HashMap<String, String>,
}

impl Texts {
    fn load(&mut self, path: &str) {
        if !self.buffers.contains_key(path) {
            if let Ok(Some(t)) = self.backend.read_optional(path) {
                self.buffers.insert(path.to_string(), t);
            }
        }
    }
    fn get(&self, path: &str) -> Option<&str> {
        self.buffers.get(path).map(String::as_str)
    }
}

pub(crate) struct Ctx {
    pub(crate) manager: Arc<LspManager>,
    pub(crate) backend: Arc<dyn Backend>,
    pub(crate) abs: std::path::PathBuf,
}

impl Ctx {
    pub(crate) fn new(path: &str, app: &AppHandle, lsp: &LspState, ws: &AppState) -> Result<Self> {
        let manager = lsp.manager(app, ws)?;
        let backend = ws.backend()?;
        let abs = backend.resolve(path)?;
        Ok(Ctx {
            manager,
            backend,
            abs,
        })
    }

    fn ui_path(&self, abs: &str) -> String {
        to_ui_path(self.manager.root(), Path::new(abs))
    }

    pub(crate) fn relative_edit(&self, mut edit: WorkspaceEdit) -> WorkspaceEdit {
        let root = self.manager.root().to_path_buf();
        edit.map_paths(|p| to_ui_path(&root, Path::new(p)));
        edit
    }

    /// Locations inside the project, workspace-relative (usages outside it, e.g. in a
    /// server's own stubs, are not the project's).
    fn relative_locations(&self, locs: Vec<EditorLocation>) -> Vec<UsageLocation> {
        locs.into_iter()
            .filter(|l| Path::new(&l.path).starts_with(self.manager.root()))
            .map(|l| UsageLocation {
                path: self.ui_path(&l.path),
                range: lsp_range(l.range),
                kind: None,
            })
            .collect()
    }

    /// Navigation targets: project files workspace-relative, files outside the project
    /// (a server's stubs, global libraries) as read-only `library://file/…` paths when they
    /// are on this machine. Remote hosts' files are left out (the index and Forge's own
    /// stubs answer instead).
    fn nav_targets(
        &self,
        locs: Vec<EditorLocation>,
        lib: &LibraryState,
        remote: bool,
    ) -> Vec<NavTarget> {
        locs.into_iter()
            .filter_map(|l| {
                let abs = Path::new(&l.path);
                let path = if abs.starts_with(self.manager.root()) {
                    self.ui_path(&l.path)
                } else if !remote && abs.is_file() {
                    lib.grant(abs)
                } else {
                    return None;
                };
                Some(NavTarget {
                    path,
                    line: l.range.start.line,
                    column: l.range.start.character,
                    name: String::new(),
                    container: None,
                })
            })
            .collect()
    }
}

fn lsp_range(r: lsp_types::Range) -> Range {
    Range::new(
        Position::new(r.start.line, r.start.character),
        Position::new(r.end.line, r.end.character),
    )
}

/// The caret's text: the editor buffer, else the file.
pub(crate) fn current_text(
    backend: &Arc<dyn Backend>,
    path: &str,
    content: &Option<String>,
) -> String {
    content
        .clone()
        .or_else(|| backend.read_optional(path).ok().flatten())
        .unwrap_or_default()
}

// ---- Find Usages ----

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsagesAnswer {
    /// `User`, `User::orders()`, or the word at the caret.
    pub title: String,
    /// Where the answer came from: `lsp`, `index` or `none`.
    pub source: String,
    pub declaration: Option<UsageLocation>,
    pub by_type: UsageTree,
    pub by_file: UsageTree,
    /// What was searched (class, method, ...), for the Find Usages dialog.
    pub element: ElementKind,
    /// The scope's name: "Project Files", "Directory 'app'".
    pub scope: String,
}

/// Find Usages / Show Usages / Find Usages in File: references from the language servers,
/// from the PHP index when they have none, filtered by the Find Usages options and scope,
/// with text occurrences on request, grouped by usage type and by file. `buffers` are
/// unsaved editor contents by path.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn code_find_usages(
    path: String,
    line: u32,
    character: u32,
    content: Option<String>,
    buffers: Option<HashMap<String, String>>,
    options: Option<UsageOptions>,
    scope: Option<UsageScope>,
    app: AppHandle,
    lsp: State<'_, LspState>,
    ws: State<'_, AppState>,
) -> Result<UsagesAnswer> {
    let options = options.unwrap_or_default();
    let mut scope = scope.unwrap_or_default();
    if scope.current_file.is_empty() {
        scope.current_file = path.clone();
    }
    let c = Ctx::new(&path, &app, &lsp, &ws)?;
    let pos = lsp_types::Position::new(line, character);
    let refs = c.manager.references(&c.abs, pos).await.unwrap_or_default();
    let mut declaration = None;
    let (mut usages, mut source) = (c.relative_locations(refs), "lsp");
    if !usages.is_empty() {
        declaration = c
            .manager
            .definition(&c.abs, pos)
            .await
            .ok()
            .and_then(|d| c.relative_locations(d).into_iter().next());
    }
    let backend = c.backend.clone();
    blocking(move || {
        let text = current_text(&backend, &path, &content);
        let word = codeintel::word_range(&text, Position::new(line, character))
            .map(|(_, w)| w)
            .unwrap_or_default();
        let mut title = word.clone();
        let mut element = ElementKind::Symbol;
        if !usages.is_empty() {
            // The PHP index knows what the servers' symbol is (cheap: one file).
            if is_php(&path) {
                let offset = codeintel::utf16_offset(&text, Position::new(line, character));
                if let Ok(Some(h)) = backend.php().highlights(&path, offset, Some(text.clone())) {
                    element = h.element;
                }
            }
            if !options.effective(element).usages {
                usages.clear();
            }
        } else if is_php(&path) {
            let offset = codeintel::utf16_offset(&text, Position::new(line, character));
            if let Some(found) =
                backend
                    .php()
                    .usages(&path, offset, options.clone(), Some(text.clone()))?
            {
                title = found.title;
                declaration = found.declaration;
                usages = found.usages;
                element = found.element;
                source = "index";
            }
        }
        let effective = options.effective(element);
        let mut buffers = buffers.unwrap_or_default();
        buffers.insert(path.clone(), text);
        let mut texts = Texts { backend, buffers };

        // Text occurrences: the files containing the word, searched where they live.
        if (effective.comments_and_strings || effective.text_occurrences)
            && !word.is_empty()
            && element != ElementKind::Variable
        {
            let bare = word.trim_start_matches('$').to_string();
            let found = texts.backend.find_in_files(&FindRequest {
                query: bare.clone(),
                case_sensitive: true,
                whole_words: true,
                limit: 5000,
                ..FindRequest::default()
            });
            if let Ok(found) = found {
                let paths: Vec<String> = found
                    .files
                    .into_iter()
                    .map(|f| f.path)
                    .filter(|p| scope.contains(p))
                    .collect();
                for p in &paths {
                    texts.load(p);
                }
                let files = paths
                    .iter()
                    .filter_map(|p| texts.get(p).map(|t| (p.as_str(), t)));
                let extra = codeintel::text_occurrences(&bare, &path, files, &usages, &effective);
                usages.extend(extra);
            }
        }

        let mut usages = scope.filter(usages);
        for u in &usages {
            texts.load(&u.path);
        }
        usages = codeintel::apply_options(usages, &effective, |u| {
            let line = texts
                .get(&u.path)
                .and_then(|t| t.split('\n').nth(u.range.start.line as usize))
                .unwrap_or("");
            let end = if u.range.end.line == u.range.start.line {
                u.range.end.character
            } else {
                u.range.start.character
            };
            codeintel::classify_usage(line, u.range.start.character, end)
        });
        if usages.is_empty() {
            source = "none";
        }
        let mut req = GroupRequest {
            usages,
            declaration: declaration.clone(),
            group_by_type: true,
        };
        let by_type = codeintel::group_usages(&req, |p| texts.get(p));
        req.group_by_type = false;
        let by_file = codeintel::group_usages(&req, |p| texts.get(p));
        Ok(UsagesAnswer {
            title,
            source: source.into(),
            declaration,
            by_type,
            by_file,
            element,
            scope: scope.label(),
        })
    })
    .await
}

/// Highlight Usages in File (Ctrl+Shift+F7) and the highlighting of the element at the
/// caret: exit points on `return` / `throw`, the implementing / overriding methods on
/// `implements` / `extends` (PHP index), the servers' `documentHighlight`, the PHP index,
/// else the lexical fallback. `None` when there is nothing to highlight.
#[tauri::command]
pub async fn code_highlight_usages(
    path: String,
    line: u32,
    character: u32,
    content: Option<String>,
    app: AppHandle,
    lsp: State<'_, LspState>,
    ws: State<'_, AppState>,
) -> Result<Option<HighlightAnswer>> {
    let c = Ctx::new(&path, &app, &lsp, &ws)?;
    let backend = c.backend.clone();
    let p = path.clone();
    let text = blocking(move || Ok(current_text(&backend, &p, &content))).await?;
    let at = Position::new(line, character);
    if let Some(exits) = codeintel::exit_points(&text, &path, at) {
        return Ok(Some(exits));
    }
    let word = codeintel::word_range(&text, at);
    let on_parents = word
        .as_ref()
        .is_some_and(|(_, w)| w == "implements" || w == "extends");
    if !on_parents {
        let pos = lsp_types::Position::new(line, character);
        let found = c
            .manager
            .document_highlights(&c.abs, pos)
            .await
            .unwrap_or_default();
        if !found.is_empty() {
            let highlights = found
                .into_iter()
                .map(|(r, kind)| UsageHighlight {
                    range: lsp_range(r),
                    kind: codeintel::lsp_highlight_kind(kind),
                })
                .collect();
            let title = word.map(|(_, w)| w).unwrap_or_default();
            return Ok(Some(HighlightAnswer::new(
                HighlightTarget::Usages,
                title,
                highlights,
                "lsp",
            )));
        }
    }
    let backend = c.backend.clone();
    blocking(move || {
        if is_php(&path) {
            let offset = codeintel::utf16_offset(&text, at);
            if let Some(found) = backend
                .php()
                .highlights(&path, offset, Some(text.clone()))?
            {
                return Ok(Some(found));
            }
        }
        if on_parents {
            return Ok(None);
        }
        Ok(codeintel::lexical_highlights(&text, &path, at))
    })
    .await
}

/// What the Find Usages dialog (Ctrl+Shift+Alt+F7) shows for the element at the caret.
#[tauri::command]
pub async fn code_usage_target(
    path: String,
    line: u32,
    character: u32,
    content: Option<String>,
    ws: State<'_, AppState>,
) -> Result<Option<UsageTarget>> {
    let backend = ws.backend()?;
    blocking(move || {
        let text = current_text(&backend, &path, &content);
        let at = Position::new(line, character);
        let Some((_, word)) = codeintel::word_range(&text, at) else {
            return Ok(None);
        };
        let mut element = ElementKind::Symbol;
        let mut title = word;
        if is_php(&path) {
            let offset = codeintel::utf16_offset(&text, at);
            if let Ok(Some(h)) = backend.php().highlights(&path, offset, Some(text.clone())) {
                if h.target == HighlightTarget::Usages {
                    element = h.element;
                    title = h.title;
                }
            }
        } else if let Some(h) = codeintel::lexical_highlights(&text, &path, at) {
            element = h.element;
        }
        Ok(Some(UsageTarget::new(element, title)))
    })
    .await
}

// ---- Navigate: Implementation, Type Declaration, Super Method ----

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NavigateAnswer {
    pub targets: Vec<NavTarget>,
    /// Set when the language server lacks the feature (and the index found nothing).
    pub missing: Option<MissingFeature>,
}

/// Go to Implementation (`implementation`), Type Declaration (`typeDeclaration`) and Super
/// Method (`superMethod`, PHP index only): the language servers first, then the PHP index.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn code_navigate(
    path: String,
    line: u32,
    character: u32,
    kind: NavKind,
    content: Option<String>,
    app: AppHandle,
    lsp: State<'_, LspState>,
    ws: State<'_, AppState>,
    lib: State<'_, LibraryState>,
) -> Result<NavigateAnswer> {
    let c = Ctx::new(&path, &app, &lsp, &ws)?;
    let pos = lsp_types::Position::new(line, character);
    let feature = match kind {
        NavKind::Implementation => Some(Feature::Implementation),
        NavKind::TypeDeclaration => Some(Feature::TypeDefinition),
        NavKind::SuperMethod => None,
    };
    let mut targets: Vec<NavTarget> = Vec::new();
    let mut missing = None;
    if let Some(feature) = feature {
        let locs = match feature {
            Feature::Implementation => c.manager.implementation(&c.abs, pos).await,
            _ => c.manager.type_definition(&c.abs, pos).await,
        }
        .unwrap_or_default();
        targets = c.nav_targets(locs, &lib, ws.is_remote());
        missing = c
            .manager
            .missing_feature(&c.abs, feature)
            .await
            .map(|servers| missing_feature(feature, &servers));
    }
    let backend = c.backend.clone();
    blocking(move || {
        if targets.is_empty() && is_php(&path) {
            let text = current_text(&backend, &path, &content);
            let offset = codeintel::utf16_offset(&text, Position::new(line, character));
            targets = backend.php().navigate(&path, offset, kind, Some(text))?;
        }
        Ok(NavigateAnswer {
            missing: if targets.is_empty() { missing } else { None },
            targets,
        })
    })
    .await
}

// ---- Go to Declaration ----

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeclarationAnswer {
    /// The caret is on a declaration: the UI shows its usages (PHPStorm's "Declaration or
    /// Usages").
    pub on_declaration: bool,
    /// `User`, `Throwable`, `User::orders()`.
    pub title: Option<String>,
    /// Workspace-relative paths, or `library://…` for read-only library files.
    pub targets: Vec<NavTarget>,
    /// `lsp`, `index`, `stubs` or `none`.
    pub source: String,
    /// Why nothing was found ("Cannot find declaration to go to", a stub download error).
    pub message: Option<String>,
}

fn stub_targets(found: Vec<StubTarget>) -> Vec<NavTarget> {
    found
        .into_iter()
        .map(|t| NavTarget {
            path: stub_path(&t.path),
            line: t.line,
            column: t.column,
            name: t.name,
            container: Some(t.extension),
        })
        .collect()
}

/// Go to Declaration in a library file: PHP stubs resolve their names in the stubs.
async fn library_declaration(
    path: &str,
    line: u32,
    character: u32,
    content: Option<String>,
    app: &AppHandle,
    ws: &AppState,
    lib: &LibraryState,
) -> DeclarationAnswer {
    let not_found = || DeclarationAnswer {
        source: "none".into(),
        message: Some("Cannot find declaration to go to".into()),
        ..Default::default()
    };
    let (Some(rel), Some(text)) = (path.strip_prefix(STUBS_PREFIX), content) else {
        return not_found();
    };
    let Ok(index) = lib.stubs(app, ws, false).await else {
        return not_found();
    };
    let offset = utf16_to_byte(
        &text,
        codeintel::utf16_offset(&text, Position::new(line, character)),
    );
    match index.definition_in(rel, &text, offset) {
        Some((_, found)) if !found.is_empty() => DeclarationAnswer {
            title: found.first().map(|t| t.name.clone()),
            targets: stub_targets(found),
            source: "stubs".into(),
            ..Default::default()
        },
        _ => not_found(),
    }
}

/// Go to Declaration (Ctrl+B, Ctrl+Click, middle click): the language servers' definition
/// first, the PHP index when they have none, and PHP's own symbols in the PHP stubs
/// (downloaded on first use). On a declaration the answer says so, and the UI shows the
/// usages instead.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn code_goto_declaration(
    path: String,
    line: u32,
    character: u32,
    content: Option<String>,
    app: AppHandle,
    lsp: State<'_, LspState>,
    ws: State<'_, AppState>,
    lib: State<'_, LibraryState>,
) -> Result<DeclarationAnswer> {
    resolve_declaration(path, line, character, content, &app, &lsp, &ws, &lib).await
}

/// The declaration at a position (Go to Declaration, Quick Definition, External
/// Documentation): language servers, the PHP index, the PHP stubs.
#[allow(clippy::too_many_arguments)]
async fn resolve_declaration(
    path: String,
    line: u32,
    character: u32,
    content: Option<String>,
    app: &AppHandle,
    lsp: &LspState,
    ws: &AppState,
    lib: &LibraryState,
) -> Result<DeclarationAnswer> {
    if path.starts_with(LIBRARY_PREFIX) {
        return Ok(library_declaration(&path, line, character, content, app, ws, lib).await);
    }
    let c = Ctx::new(&path, app, lsp, ws)?;
    let pos = lsp_types::Position::new(line, character);
    let locs = c.manager.definition(&c.abs, pos).await.unwrap_or_default();
    let on_declaration = locs.iter().any(|l| {
        Path::new(&l.path) == c.abs
            && (l.range.start.line, l.range.start.character) <= (line, character)
            && (line, character) <= (l.range.end.line, l.range.end.character)
    });
    if on_declaration {
        return Ok(DeclarationAnswer {
            on_declaration: true,
            source: "lsp".into(),
            ..Default::default()
        });
    }
    let targets = c.nav_targets(locs, lib, ws.is_remote());
    if !targets.is_empty() {
        return Ok(DeclarationAnswer {
            targets,
            source: "lsp".into(),
            ..Default::default()
        });
    }
    let mut answer = DeclarationAnswer {
        source: "none".into(),
        message: Some("Cannot find declaration to go to".into()),
        ..Default::default()
    };
    if !is_php(&path) {
        return Ok(answer);
    }
    let backend = c.backend.clone();
    let def = blocking(move || {
        let text = current_text(&backend, &path, &content);
        let offset = codeintel::utf16_offset(&text, Position::new(line, character));
        backend.php().definition(&path, offset, Some(text))
    })
    .await?;
    answer.title = def.title.clone();
    if def.on_declaration {
        answer.on_declaration = true;
        answer.source = "index".into();
        answer.message = None;
    } else if !def.targets.is_empty() {
        answer.targets = def.targets;
        // Include path files open as read-only library files.
        for t in &mut answer.targets {
            t.path = lib.ui_path(&t.path);
        }
        answer.source = "index".into();
        answer.message = None;
    } else if let Some(external) = def.external {
        match lib.stubs(app, ws, true).await {
            Ok(index) => {
                let found = index.lookup(&external);
                if !found.is_empty() {
                    answer.targets = stub_targets(found);
                    answer.source = "stubs".into();
                    answer.message = None;
                }
            }
            Err(e) => answer.message = Some(e),
        }
    }
    Ok(answer)
}

/// The tooltip's first line for a stub declaration: its class, else where it comes from
/// (`PHP Runtime · Core`).
fn stub_container(t: &StubTarget) -> String {
    if t.info.container.is_empty() {
        format!("PHP Runtime · {}", t.extension)
    } else {
        t.info.container.clone()
    }
}

/// What Ctrl+hover shows: the reference to underline and a short description.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolHover {
    pub range: Range,
    /// Namespace or class; empty for globals.
    pub container: String,
    /// The declaration's signature; empty when only the name is known.
    pub signature: String,
}

/// The Ctrl+hover link at a position: from the PHP index (and the loaded PHP stubs), so it
/// answers quickly; `null` where nothing is navigable. Other languages answer `null` and
/// the UI underlines the word for the language server.
#[tauri::command]
pub async fn code_symbol_info(
    path: String,
    line: u32,
    character: u32,
    content: Option<String>,
    ws: State<'_, AppState>,
    lib: State<'_, LibraryState>,
) -> Result<Option<SymbolHover>> {
    let stubs = lib.stubs_ready();
    if let Some(rel) = path.strip_prefix(STUBS_PREFIX) {
        let (Some(index), Some(text)) = (stubs, content) else {
            return Ok(None);
        };
        let offset = utf16_to_byte(
            &text,
            codeintel::utf16_offset(&text, Position::new(line, character)),
        );
        return Ok(index
            .definition_in(rel, &text, offset)
            .and_then(|((s, e), t)| {
                let t = t.into_iter().next()?;
                Some(SymbolHover {
                    range: Range::new(
                        codeintel::position_at(&text, byte_to_utf16(&text, s)),
                        codeintel::position_at(&text, byte_to_utf16(&text, e)),
                    ),
                    container: stub_container(&t),
                    signature: t.info.signature,
                })
            }));
    }
    if !is_php(&path) {
        return Ok(None);
    }
    let backend = ws.backend()?;
    blocking(move || {
        let text = current_text(&backend, &path, &content);
        let offset = codeintel::utf16_offset(&text, Position::new(line, character));
        let def = backend.php().definition(&path, offset, Some(text))?;
        let Some(range) = def.range else {
            return Ok(None);
        };
        if let Some(info) = def.info {
            return Ok(Some(SymbolHover {
                range,
                container: info.container,
                signature: info.signature,
            }));
        }
        let Some(external) = def.external else {
            return Ok(None);
        };
        // PHP's own symbols: underline only what the stubs know (once they are loaded).
        Ok(match stubs {
            Some(index) => index
                .lookup(&external)
                .into_iter()
                .next()
                .map(|t| SymbolHover {
                    range,
                    container: stub_container(&t),
                    signature: t.info.signature,
                }),
            None => Some(SymbolHover {
                range,
                container: String::new(),
                signature: String::new(),
            }),
        })
    })
    .await
}

/// The implemented / overridden gutter markers of a PHP file (PHP index).
#[tauri::command]
pub async fn code_line_markers(
    path: String,
    content: Option<String>,
    ws: State<'_, AppState>,
) -> Result<Vec<PhpLineMarker>> {
    if !is_php(&path) {
        return Ok(vec![]);
    }
    let backend = ws.backend()?;
    blocking(move || backend.php().line_markers(&path, content)).await
}

// ---- Rename ----

/// "Cannot perform refactoring" when the caret is not on something renameable.
const NOT_RENAMEABLE: &str =
    "Cannot perform refactoring. Caret should be positioned at the symbol to be renamed";

/// Whether the language servers rename this file: `Ok(None)` when they can, the missing
/// feature when they cannot, `Err` when no server serves
/// the file at all.
async fn lsp_rename_support(c: &Ctx) -> std::result::Result<Option<MissingFeature>, ()> {
    if let Some(servers) = c.manager.missing_feature(&c.abs, Feature::Rename).await {
        return Ok(Some(missing_feature(Feature::Rename, &servers)));
    }
    if c.manager.clients_for(&c.abs).await.is_empty() {
        return Err(());
    }
    Ok(None)
}

/// The text of `r` (one line) in `text`.
fn text_of_range(text: &str, r: Range) -> Option<String> {
    let line = text.split('\n').nth(r.start.line as usize)?;
    let s = codeintel::utf16_offset(line, Position::new(0, r.start.character));
    let e = codeintel::utf16_offset(line, Position::new(0, r.end.character));
    let units: Vec<u16> = line.encode_utf16().collect();
    String::from_utf16(units.get(s..e)?).ok()
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareRenameAnswer {
    /// What to rename: the server's range, else the word at the caret.
    pub range: Option<Range>,
    pub placeholder: Option<String>,
    /// Nothing can be renamed here.
    pub invalid: bool,
    /// Why nothing can be renamed ("Cannot perform refactoring ...").
    pub message: Option<String>,
    /// The language server cannot rename; the PHP index renames instead when `source` is
    /// `index`.
    pub missing: Option<MissingFeature>,
    /// `lsp` or `index`: who computes the rename.
    pub source: String,
    /// Occurrences in this file, edited live by the in-place rename (includes `range`).
    pub occurrences: Vec<Range>,
    /// "Rename class 'User' and its usages to:" (PHP index), for the Rename dialog.
    pub label: Option<String>,
    /// The class file renamed along with the class (PHPStorm's "Rename file" option).
    pub class_file: Option<String>,
    /// The name starts with `$` (PHP variables and properties), which is not part of
    /// `range` and `occurrences`.
    pub sigil: bool,
}

/// Start an in-place rename (Shift+F6): the range to edit, its occurrences in this file and
/// who renames. The language servers rename when they can; PHP files fall back to the PHP
/// index (a server without rename, or no server), with `missing` explaining why.
#[tauri::command]
pub async fn code_prepare_rename(
    path: String,
    line: u32,
    character: u32,
    content: Option<String>,
    app: AppHandle,
    lsp: State<'_, LspState>,
    ws: State<'_, AppState>,
) -> Result<PrepareRenameAnswer> {
    let c = Ctx::new(&path, &app, &lsp, &ws)?;
    let text = current_text(&c.backend, &path, &content);
    let word = codeintel::word_range(&text, Position::new(line, character));
    let php = is_php(&path);
    let support = lsp_rename_support(&c).await;
    let mut answer = PrepareRenameAnswer {
        source: "lsp".into(),
        ..Default::default()
    };
    match support {
        Ok(None) => {
            let pos = lsp_types::Position::new(line, character);
            let Some(p) = c.manager.prepare_rename(&c.abs, pos).await? else {
                answer.invalid = true;
                answer.message = Some(NOT_RENAMEABLE.into());
                return Ok(answer);
            };
            answer.range = p.range.map(lsp_range).or(word.as_ref().map(|w| w.0));
            answer.placeholder = p
                .placeholder
                .or_else(|| answer.range.and_then(|r| text_of_range(&text, r)));
            // The occurrences in this file: what renaming to the same name would touch.
            if let Some(name) = answer.placeholder.clone() {
                if let Ok(Some(edit)) = c.manager.rename(&c.abs, pos, &name).await {
                    answer.occurrences = codeintel::ranges_in(&c.relative_edit(edit), &path);
                }
            }
        }
        Ok(Some(missing)) => {
            answer.missing = Some(missing);
            answer.source = "index".into();
        }
        Err(()) => answer.source = "index".into(),
    }
    if answer.source == "index" && !php {
        return match answer.missing {
            Some(m) => Ok(PrepareRenameAnswer {
                invalid: true,
                message: Some(m.message.clone()),
                missing: Some(m),
                ..answer
            }),
            None => Err(ForgeError::Other(
                "Rename needs a language server for this file".into(),
            )),
        };
    }
    let backend = c.backend.clone();
    blocking(move || {
        if answer.source == "index" {
            answer.range = word.as_ref().map(|w| w.0);
            answer.placeholder = word.map(|w| w.1);
        }
        if php {
            let offset = codeintel::utf16_offset(&text, Position::new(line, character));
            match backend.php().usages(
                &path,
                offset,
                UsageOptions::default(),
                Some(text.clone()),
            )? {
                Some(found) => {
                    answer.label = Some(phpforge_lang_php::rename::rename_label(&found.symbol));
                    if let (phpforge_lang_php::navigate::SymbolRef::Class { fqn }, Some(decl)) =
                        (&found.symbol, &found.declaration)
                    {
                        let short = phpforge_lang_php::short_name(fqn).to_string();
                        let decl_text = if decl.path == path {
                            Some(text.clone())
                        } else {
                            backend.read_optional(&decl.path).ok().flatten()
                        };
                        if decl_text
                            .is_some_and(|t| codeintel::is_class_file(&decl.path, &t, &short))
                        {
                            answer.class_file = Some(decl.path.clone());
                        }
                    }
                    if answer.occurrences.is_empty() {
                        let mut all = found.usages;
                        all.extend(found.declaration);
                        let local: Vec<UsageLocation> =
                            all.into_iter().filter(|u| u.path == path).collect();
                        answer.occurrences =
                            codeintel::edit_from_locations(&local, "x", |_| Some(text.clone()))
                                .edit
                                .edits_of(&path)
                                .into_iter()
                                .map(|e| e.range)
                                .collect();
                    }
                }
                None if answer.source == "index" => {
                    answer.invalid = true;
                    answer.message = Some(NOT_RENAMEABLE.into());
                }
                None => {}
            }
        } else if let Some(name) = &answer.placeholder {
            if codeintel::is_class_file(&path, &text, name) {
                answer.class_file = Some(path.clone());
            }
        }
        if let Some(r) = answer.range {
            // The server's / index's occurrences use their own extent of the name (`$x`
            // vs `x`); the edited range is the one that contains the caret.
            match answer.occurrences.iter().find(|o| o.contains(r.start)) {
                Some(o) => answer.range = Some(*o),
                None => answer.occurrences.push(r),
            }
            answer.occurrences.sort();
            answer.occurrences.dedup();
        }
        if answer.range.is_none() && !answer.invalid {
            answer.invalid = true;
            answer.message = Some(NOT_RENAMEABLE.into());
        }
        if let (Some(r), true) = (answer.range, answer.placeholder.is_none()) {
            answer.placeholder = text_of_range(&text, r);
        }
        // Like PHPStorm, `$` stays outside the edited name; `sigil` tells the UI to put it
        // back for the server.
        let sigil =
            |r: &Range| text_of_range(&text, *r).is_some_and(|t| t.len() > 1 && t.starts_with('$'));
        let skip_sigil = |r: Range| {
            if sigil(&r) {
                Range::new(Position::new(r.start.line, r.start.character + 1), r.end)
            } else {
                r
            }
        };
        answer.sigil = answer.range.as_ref().is_some_and(sigil);
        answer.occurrences = answer.occurrences.iter().map(|r| skip_sigil(*r)).collect();
        answer.range = answer.range.map(skip_sigil);
        answer.placeholder = answer
            .placeholder
            .map(|p| p.trim_start_matches('$').to_string());
        Ok(answer)
    })
    .await
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameAnswer {
    pub edit: WorkspaceEdit,
    /// Affected lines per file, for the Refactoring Preview.
    pub preview: Vec<FilePreview>,
    /// Problems to confirm before refactoring (PHPStorm's "Problems Detected").
    pub conflicts: Vec<RenameConflict>,
    /// `lsp` or `index`.
    pub source: String,
    /// Set when the language server cannot rename and the index did.
    pub missing: Option<MissingFeature>,
    /// The class file renamed with the class (included in `edit` when `renameFile`).
    pub file_rename: Option<FileRename>,
    /// Dynamic usages that were not renamed.
    pub skipped: usize,
}

/// Rename the symbol at the position to `new_name`: the language servers' edit, else the
/// PHP index's (every usage and the declaration). Renaming a class also renames its file
/// when it is named after it (unless `renameFile` is false). Nothing is applied; the UI
/// shows conflicts and applies the edit (open editors itself, the rest with
/// `code_apply_edit`). `buffers` are unsaved editor contents by path.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn code_rename(
    path: String,
    line: u32,
    character: u32,
    new_name: String,
    buffers: Option<HashMap<String, String>>,
    rename_file: Option<bool>,
    app: AppHandle,
    lsp: State<'_, LspState>,
    ws: State<'_, AppState>,
) -> Result<RenameAnswer> {
    if let Some(problem) = codeintel::validate_identifier(&new_name) {
        return Err(ForgeError::Other(problem));
    }
    let c = Ctx::new(&path, &app, &lsp, &ws)?;
    let support = lsp_rename_support(&c).await;
    let php = is_php(&path);
    let mut edit = None;
    let mut missing = None;
    match support {
        Ok(None) => {
            let pos = lsp_types::Position::new(line, character);
            let renamed = c.manager.rename(&c.abs, pos, &new_name).await;
            match renamed {
                Ok(e) => edit = e.map(|e| c.relative_edit(e)),
                Err(e) if !php => return Err(e),
                Err(_) => {}
            }
        }
        Ok(Some(m)) if php => missing = Some(m),
        Ok(Some(m)) => return Err(ForgeError::Other(m.message)),
        Err(()) if php => {}
        Err(()) => {
            return Err(ForgeError::Other(
                "Rename needs a language server for this file".into(),
            ))
        }
    }
    let backend = c.backend.clone();
    blocking(move || {
        let mut buffers = buffers.unwrap_or_default();
        let text = buffers
            .get(&path)
            .cloned()
            .unwrap_or_else(|| current_text(&backend, &path, &None));
        buffers.insert(path.clone(), text.clone());
        let mut texts = Texts {
            backend: backend.clone(),
            buffers,
        };
        let old = codeintel::word_range(&text, Position::new(line, character))
            .map(|(_, w)| w)
            .unwrap_or_default();
        let mut source = "lsp";
        let mut symbol = None;
        let mut skipped = 0;
        let mut declaration = None;
        if edit.is_none() && php {
            let offset = codeintel::utf16_offset(&text, Position::new(line, character));
            if let Some(found) =
                backend
                    .php()
                    .usages(&path, offset, UsageOptions::default(), Some(text.clone()))?
            {
                let mut all = found.declaration.iter().cloned().collect::<Vec<_>>();
                all.extend(found.usages);
                let built = codeintel::edit_from_locations(&all, &new_name, |p| {
                    texts.load(p);
                    texts.get(p).map(str::to_string)
                });
                skipped = built.skipped;
                declaration = found.declaration;
                edit = Some(built.edit);
                symbol = Some(found.symbol);
                source = "index";
            }
        } else if php {
            // A language server renamed it: the index still knows the symbol, so name clashes
            // are reported like for an index rename.
            let offset = codeintel::utf16_offset(&text, Position::new(line, character));
            symbol = backend
                .php()
                .usages(&path, offset, UsageOptions::default(), Some(text.clone()))
                .ok()
                .flatten()
                .map(|found| found.symbol);
        }
        let mut edit = edit
            .filter(|e| !e.is_empty())
            .ok_or_else(|| ForgeError::Other(NOT_RENAMEABLE.into()))?;
        // PHPStorm renames `User.php` with `class User`.
        let bare_old = old.trim_start_matches('$');
        let bare_new = new_name.trim_start_matches('$');
        let mut candidates = edit.paths();
        if let Some(d) = &declaration {
            candidates.insert(0, d.path.clone());
        }
        let file_rename = codeintel::suggest_file_rename(&candidates, bare_old, bare_new, |p| {
            texts.load(p);
            texts.get(p).map(str::to_string)
        })
        .filter(|f| f.from != f.to);
        if let (Some(f), true) = (&file_rename, rename_file.unwrap_or(true)) {
            edit.changes.push(codeintel::DocumentChange::Rename {
                from: f.from.clone(),
                to: f.to.clone(),
            });
        }
        let mut conflicts = codeintel::edit_conflicts(&edit, |p| {
            texts.get(p).is_some() || backend.read_optional(p).ok().flatten().is_some()
        });
        if let Some(symbol) = &symbol {
            let hits = backend
                .php()
                .search(
                    bare_new,
                    phpforge_lang_php::index::SearchKind::Symbol,
                    200,
                    true,
                )
                .unwrap_or_default();
            conflicts.extend(phpforge_lang_php::rename::clashes(symbol, &new_name, &hits));
        }
        let preview = preview_workspace_edit(&edit, |p| {
            texts.load(p);
            texts.get(p).map(str::to_string)
        });
        Ok(RenameAnswer {
            edit,
            preview,
            conflicts,
            source: source.into(),
            missing,
            file_rename,
            skipped,
        })
    })
    .await
}

/// How the UI applies `edit`: the text edits of the `open` files go to their editors (one
/// undoable step each), the rest to `code_apply_edit`; renamed open files are saved first
/// and reopened at their new path.
#[tauri::command]
pub fn code_plan_edit(edit: WorkspaceEdit, open: Vec<String>) -> EditPlan {
    codeintel::plan_edit(&edit, &open)
}

// ---- Workspace edits ----

/// Apply `edit` to the files through the backend, except the text edits of `skip` (files
/// open in an editor, which the UI edits itself so they stay undoable there). All or
/// nothing; the answer is what `code_revert_edit` needs to undo it.
#[tauri::command]
pub async fn code_apply_edit(
    edit: WorkspaceEdit,
    skip: Option<Vec<String>>,
    state: State<'_, AppState>,
    lh: State<'_, crate::commands::local_history::LocalHistoryState>,
) -> Result<Vec<AppliedFile>> {
    let backend = state.backend()?;
    let applied = blocking(move || {
        let edit = edit.without_text_edits_of(&skip.unwrap_or_default());
        codeintel::apply_workspace_edit(&edit, &mut BackendStore(backend))
            .map_err(|e| ForgeError::Other(e.to_string()))
    })
    .await?;
    // Joins the refactoring's change set when the UI started one.
    crate::commands::local_history::edited(
        &lh,
        forge_localhistory::ChangeKind::Refactoring,
        "Refactoring",
        applied
            .iter()
            .map(|f| (f.path.as_str(), f.before.as_deref(), f.after.as_deref())),
    );
    Ok(applied)
}

/// Undo [`code_apply_edit`]; refused when a file changed since.
#[tauri::command]
pub async fn code_revert_edit(files: Vec<AppliedFile>, state: State<'_, AppState>) -> Result<()> {
    let backend = state.backend()?;
    blocking(move || {
        codeintel::revert_applied(&files, &mut BackendStore(backend))
            .map_err(|e| ForgeError::Other(e.to_string()))
    })
    .await
}

// ---- Show Context Actions ----

fn from_lsp_action(a: EditorCodeAction, c: &Ctx) -> ContextAction {
    ContextAction {
        kind: ActionKind::of_lsp(a.kind.as_deref()),
        title: codeintel::phpstorm_title(a.kind.as_deref(), &a.title).unwrap_or(a.title),
        source: "lsp".into(),
        is_preferred: a.is_preferred,
        disabled: a.disabled,
        edit: a.edit.map(|e| c.relative_edit(e)),
        command: a.command.and_then(|cmd| serde_json::to_value(cmd).ok()),
        needs_resolve: a.needs_resolve,
        server: Some(a.server),
        raw: Some(a.raw),
        action: None,
        children: vec![],
        caret: None,
        data: None,
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextActionsAnswer {
    pub actions: Vec<ContextAction>,
    /// The light bulb to show at the caret line (none when only links like "Generate..."
    /// are available).
    pub bulb: Option<Bulb>,
    /// The language server offers no code actions.
    pub missing: Option<MissingFeature>,
}

/// The Alt+Enter popup: the language servers' quick fixes and refactorings for the caret
/// (or selection) and, in PHP files, Forge's own intentions, in PHPStorm's order ("fix
/// all" variants in the sub-menu of their quick fix). Also answers the light bulb.
///
/// Create from usage ("Create method 'foo'") is left out when a language server offers to
/// create the same symbol, or when the PHP stubs declare it (a PHP function or class).
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn code_context_actions(
    path: String,
    line: u32,
    character: u32,
    end_line: Option<u32>,
    end_character: Option<u32>,
    content: Option<String>,
    app: AppHandle,
    lsp: State<'_, LspState>,
    ws: State<'_, AppState>,
    lib: State<'_, LibraryState>,
) -> Result<ContextActionsAnswer> {
    let c = Ctx::new(&path, &app, &lsp, &ws)?;
    let missing = c
        .manager
        .missing_feature(&c.abs, Feature::CodeAction)
        .await
        .map(|servers| missing_feature(Feature::CodeAction, &servers));
    let start = lsp_types::Position::new(line, character);
    let end =
        lsp_types::Position::new(end_line.unwrap_or(line), end_character.unwrap_or(character));
    let lsp_actions = c
        .manager
        .code_actions(&c.abs, lsp_types::Range::new(start, end))
        .await
        .unwrap_or_default();
    let mut actions: Vec<ContextAction> = lsp_actions
        .into_iter()
        .map(|a| from_lsp_action(a, &c))
        .collect();
    if is_php(&path) {
        let options = GenerateOptions {
            style: crate::commands::php::php_code_style(&ws),
            code_templates: crate::commands::templates::code_templates(&ws),
            ..Default::default()
        };
        let backend = c.backend.clone();
        let p = path.clone();
        let content = content.clone();
        let (text, intentions) = blocking(move || {
            let text = current_text(&backend, &p, &content);
            let offset = codeintel::utf16_offset(&text, Position::new(line, character));
            let found = backend
                .php()
                .intentions(&p, offset, options, Some(text.clone()))?;
            Ok((text, found))
        })
        .await?;
        let stubs = if intentions.iter().any(|i| i.unresolved.is_some()) {
            lib.stubs(&app, &ws, false).await.ok()
        } else {
            None
        };
        let lsp_titles: Vec<String> = actions.iter().map(|a| a.title.clone()).collect();
        let lsp_titles: Vec<&str> = lsp_titles.iter().map(String::as_str).collect();
        for i in intentions {
            if let Some(symbol) = &i.unresolved {
                let declared = match (&symbol.member, &stubs) {
                    // A member of a class with ancestors outside the index: only when they
                    // are all PHP's own classes and none declares it (a library parent
                    // such as Eloquent's Model may).
                    (Some(_), Some(s)) => {
                        !s.lookup(symbol).is_empty()
                            || symbol.classes.iter().any(|c| s.class(c).is_none())
                    }
                    (Some(_), None) => true,
                    (None, Some(s)) => !s.lookup(symbol).is_empty(),
                    (None, None) => false,
                };
                let name = i.title.split('\'').nth(1).unwrap_or_default();
                if declared || codeintel::lsp_creates(&lsp_titles, name) {
                    continue;
                }
            }
            actions.push(php_action(&path, &text, i));
        }
        // "Import class" on an unresolved class name: candidates from the index (project
        // and vendor) and PHP's own classes, a chooser when there are several.
        let style = crate::commands::imports::import_style(&ws);
        let list = crate::commands::imports::unresolved_with_stubs(
            &c.backend, &path, &text, &app, &ws, &lib,
        )
        .await;
        let offset = codeintel::utf16_offset(&text, Position::new(line, character));
        if let Some(fix) =
            phpforge_lang_php::imports::auto::import_class_fix(&text, &list, offset, &style)
        {
            actions.push(php_action(&path, &text, fix));
        }
        // The server's twins of Forge's intentions (php-lsp's "Generate getter for $name",
        // "Import 'App\Models\User'"): Forge's own are listed once, in PHPStorm's wording.
        let own: Vec<String> = actions
            .iter()
            .filter(|a| a.source != "lsp")
            .map(|a| a.title.clone())
            .collect();
        actions.retain(|a| {
            a.source != "lsp"
                || !own
                    .iter()
                    .any(|o| phpforge_lang_php::intentions::is_lsp_twin(o, &a.title))
        });
    }
    // Extract Component on whole Vue template or JSX elements.
    if is_web_script(&path) && end != start {
        let backend = c.backend.clone();
        let (p, content) = (path.clone(), content.clone());
        let (s, e) = (
            Position::new(start.line, start.character),
            Position::new(end.line, end.character),
        );
        let applies = blocking(move || {
            let text = current_text(&backend, &p, &content);
            Ok(crate::commands::extract_component::applies(&p, &text, s, e))
        })
        .await?;
        if applies {
            actions.push(ContextAction {
                title: "Extract Component\u{2026}".into(),
                kind: ActionKind::Refactor,
                source: "webforge".into(),
                is_preferred: false,
                disabled: None,
                edit: None,
                command: None,
                needs_resolve: false,
                server: None,
                raw: None,
                action: Some(EXTRACT_COMPONENT.into()),
                children: vec![],
                caret: None,
                data: None,
            });
        }
    }
    // Language injections: Inject language, Un-inject, Edit Fragment.
    if forge_injections::Host::of_path(&path).is_some() {
        let injection_settings = injections::settings_of(&ws.read_settings());
        let backend = c.backend.clone();
        let p = path.clone();
        actions.extend(
            blocking(move || {
                let text = current_text(&backend, &p, &content);
                let offset = codeintel::utf16_offset(&text, Position::new(line, character));
                Ok(injections::context_actions(
                    &p,
                    &text,
                    offset,
                    injection_settings,
                ))
            })
            .await?,
        );
    }
    let actions = order_actions(actions);
    Ok(ContextActionsAnswer {
        bulb: codeintel::bulb_for(&actions),
        actions,
        missing,
    })
}

/// The language servers' refactorings for a Refactor menu action in files Forge has no
/// refactoring of its own for (typescript-language-server's "Extract to function in module
/// scope", "Extract to constant in enclosing scope", "Inline variable"). The UI lets the user
/// choose one and applies it like a context action.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn code_refactor_actions(
    path: String,
    line: u32,
    character: u32,
    end_line: u32,
    end_character: u32,
    refactoring: String,
    app: AppHandle,
    lsp: State<'_, LspState>,
    ws: State<'_, AppState>,
) -> Result<Vec<ContextAction>> {
    let c = Ctx::new(&path, &app, &lsp, &ws)?;
    let range = lsp_types::Range::new(
        lsp_types::Position::new(line, character),
        lsp_types::Position::new(end_line, end_character),
    );
    let actions = c
        .manager
        .code_actions(&c.abs, range)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|a| from_lsp_action(a, &c))
        .collect();
    Ok(codeintel::refactor_actions(actions, &refactoring))
}

/// Refactor This (Ctrl+Alt+Shift+T): the refactorings that apply at the caret or to the
/// selection.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefactorThisAnswer {
    /// Forge's refactorings (action ids, PHPStorm's order); `None` when Forge has no
    /// analysis of its own for the file (the UI offers every enabled refactoring).
    pub actions: Option<Vec<String>>,
    /// The language servers' refactorings (JS/TS), worded like PHPStorm.
    pub lsp: Vec<ContextAction>,
}

/// Refactor This: in PHP files Rust decides which refactorings apply (a class, a method, an
/// expression or a selection each have their own).
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn code_refactor_this(
    path: String,
    line: u32,
    character: u32,
    end_line: u32,
    end_character: u32,
    content: Option<String>,
    app: AppHandle,
    lsp: State<'_, LspState>,
    ws: State<'_, AppState>,
) -> Result<RefactorThisAnswer> {
    let start = Position::new(line, character);
    let end = Position::new(end_line, end_character);
    if is_php(&path) {
        let backend = ws.backend()?;
        let actions = blocking(move || {
            let text = current_text(&backend, &path, &content);
            Ok(crate::commands::refactor_hierarchy::php_refactorings(
                &text, start, end,
            ))
        })
        .await?;
        return Ok(RefactorThisAnswer {
            actions: Some(actions),
            lsp: vec![],
        });
    }
    if !is_web_script(&path) {
        return Ok(RefactorThisAnswer {
            actions: None,
            lsp: vec![],
        });
    }
    // JS/TS/Vue: Rename and the file refactorings, Extract Component for whole elements, and
    // the language servers' refactorings.
    let backend = ws.backend()?;
    let (p, c) = (path.clone(), content.clone());
    let component = blocking(move || {
        let text = current_text(&backend, &p, &c);
        Ok(crate::commands::extract_component::applies(
            &p, &text, start, end,
        ))
    })
    .await?;
    let mut actions = vec!["RenameElement".to_string()];
    if component {
        actions.push(EXTRACT_COMPONENT.to_string());
    }
    actions.extend(["Move".to_string(), "CopyElement".to_string()]);
    let lsp_actions = match Ctx::new(&path, &app, &lsp, &ws) {
        Ok(c) => {
            let range = lsp_types::Range::new(
                lsp_types::Position::new(line, character),
                lsp_types::Position::new(end_line, end_character),
            );
            let found = c
                .manager
                .code_actions(&c.abs, range)
                .await
                .unwrap_or_default()
                .into_iter()
                .map(|a| from_lsp_action(a, &c))
                .collect();
            codeintel::refactor_this_actions(found)
        }
        Err(_) => vec![],
    };
    Ok(RefactorThisAnswer {
        actions: Some(actions),
        lsp: lsp_actions,
    })
}

/// WebForge's Extract Component action.
pub(crate) const EXTRACT_COMPONENT: &str = "WebForge.ExtractComponent";

/// Files the TypeScript language server and Extract Component work on.
pub(crate) fn is_web_script(path: &str) -> bool {
    let p = path.to_ascii_lowercase();
    [
        ".js", ".jsx", ".ts", ".tsx", ".mjs", ".cjs", ".mts", ".cts", ".vue",
    ]
    .iter()
    .any(|e| p.ends_with(e))
        && !p.ends_with(".d.ts")
}

/// A PHP intention as a context action: its edits of the current file (UTF-16 offsets) and
/// of other files in one workspace edit.
fn php_action(
    path: &str,
    text: &str,
    i: phpforge_lang_php::intentions::Intention,
) -> ContextAction {
    let mut edit = WorkspaceEdit::default();
    if !i.edits.is_empty() {
        edit.add_edits(
            path.to_string(),
            i.edits
                .iter()
                .map(|e| codeintel::TextEdit {
                    range: Range::new(
                        codeintel::position_at(text, e.start),
                        codeintel::position_at(text, e.end),
                    ),
                    new_text: e.new_text.clone(),
                })
                .collect(),
        );
    }
    if let Some(other) = i.workspace_edit {
        edit.changes.extend(other.changes);
    }
    ContextAction {
        title: i.title,
        kind: match i.kind {
            phpforge_lang_php::intentions::IntentionKind::QuickFix => ActionKind::QuickFix,
            phpforge_lang_php::intentions::IntentionKind::Intention => ActionKind::Intention,
        },
        source: "php".into(),
        is_preferred: false,
        disabled: None,
        edit: (!edit.is_empty()).then_some(edit),
        command: None,
        needs_resolve: false,
        server: None,
        raw: None,
        action: i.action,
        children: vec![],
        caret: i.caret,
        data: i.data,
    }
}

/// Create class: the edit importing the new class into the file that used it, when the
/// Create New PHP Class dialog put it into another namespace (UTF-16 offsets).
#[tauri::command]
pub fn php_import_class(
    path: String,
    content: String,
    fqn: String,
    ws: State<'_, AppState>,
) -> Vec<phpforge_lang_php::generate::TextEdit> {
    let style = crate::commands::imports::import_style(&ws);
    phpforge_lang_php::imports::with_style(style, || {
        phpforge_lang_php::service::import_class(&path, &content, &fqn)
    })
}

/// Fill in a code action's edit (`codeAction/resolve`).
#[tauri::command]
pub async fn code_resolve_action(
    path: String,
    server: String,
    raw: Value,
    app: AppHandle,
    lsp: State<'_, LspState>,
    ws: State<'_, AppState>,
) -> Result<ContextAction> {
    let c = Ctx::new(&path, &app, &lsp, &ws)?;
    let resolved = c.manager.resolve_code_action(&c.abs, &server, raw).await?;
    Ok(from_lsp_action(resolved, &c))
}

/// Run a code action's server command; edits it makes arrive as `lsp://apply-edit`.
#[tauri::command]
pub async fn code_execute_command(
    path: String,
    server: String,
    command: String,
    arguments: Option<Vec<Value>>,
    app: AppHandle,
    lsp: State<'_, LspState>,
    ws: State<'_, AppState>,
) -> Result<Option<Value>> {
    let c = Ctx::new(&path, &app, &lsp, &ws)?;
    c.manager
        .execute_command(&c.abs, &server, &command, arguments.unwrap_or_default())
        .await
}

// ---- Documentation (View menu) ----

/// External Documentation (Shift+F1): the page to open in the browser.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalDocAnswer {
    pub url: Option<String>,
    /// The symbol's name for the "No external documentation found for …" hint.
    pub name: Option<String>,
}

/// The first target as External Documentation sees it.
fn doc_target(answer: &DeclarationAnswer) -> Option<codeintel::DocTarget> {
    answer.targets.first().map(|t| codeintel::DocTarget {
        path: t.path.clone(),
        name: t.name.clone(),
        container: t.container.clone(),
    })
}

/// External Documentation: php.net for PHP's own symbols, the Laravel API, MDN, the Vue and
/// React references and the Lua manual (see `forge_codeintel::docs`).
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn code_external_doc(
    path: String,
    line: u32,
    character: u32,
    content: Option<String>,
    app: AppHandle,
    lsp: State<'_, LspState>,
    ws: State<'_, AppState>,
    lib: State<'_, LibraryState>,
) -> Result<ExternalDocAnswer> {
    let language = codeintel::language_of(&path);
    let text = match &content {
        Some(t) => t.clone(),
        None => {
            let backend = ws.backend()?;
            let p = path.clone();
            blocking(move || Ok(current_text(&backend, &p, &None))).await?
        }
    };
    let Some(mut symbol) = codeintel::symbol_at(&text, Position::new(line, character), language)
    else {
        return Ok(ExternalDocAnswer::default());
    };
    if language == "php" {
        let answer =
            resolve_declaration(path, line, character, content, &app, &lsp, &ws, &lib).await?;
        symbol.declaration = doc_target(&answer);
    }
    Ok(ExternalDocAnswer {
        url: codeintel::external_doc_url(&symbol),
        name: Some(symbol.word),
    })
}

/// One declaration in the Quick Definition popup.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickDefinition {
    pub path: String,
    /// 0-based line and UTF-16 column of the name.
    pub line: u32,
    pub column: u32,
    pub name: String,
    pub container: Option<String>,
    /// The first line of `text` in the file.
    pub start_line: u32,
    pub text: String,
    pub truncated: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickDefinitionAnswer {
    pub title: Option<String>,
    pub definitions: Vec<QuickDefinition>,
    /// Why nothing was found.
    pub message: Option<String>,
}

/// The most declarations Quick Definition shows (←/→ switch between them).
const MAX_QUICK_DEFINITIONS: usize = 20;

/// Quick Definition (Ctrl+Shift+I): the declaration's source, with its docblock, from the
/// project, the PHP stubs or a library file. On a declaration it shows the declaration.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn code_quick_definition(
    path: String,
    line: u32,
    character: u32,
    content: Option<String>,
    app: AppHandle,
    lsp: State<'_, LspState>,
    ws: State<'_, AppState>,
    lib: State<'_, LibraryState>,
) -> Result<QuickDefinitionAnswer> {
    let answer = resolve_declaration(
        path.clone(),
        line,
        character,
        content.clone(),
        &app,
        &lsp,
        &ws,
        &lib,
    )
    .await?;
    let targets = if answer.on_declaration {
        vec![NavTarget {
            path: path.clone(),
            line,
            column: character,
            name: answer.title.clone().unwrap_or_default(),
            container: None,
        }]
    } else {
        answer.targets
    };
    let mut definitions = Vec::new();
    for t in targets.into_iter().take(MAX_QUICK_DEFINITIONS) {
        let text = if t.path == path && content.is_some() {
            content.clone()
        } else if t.path.starts_with(LIBRARY_PREFIX) {
            read_library(&t.path, &app, &ws, &lib)
                .await
                .ok()
                .map(|f| f.text)
        } else {
            let backend = ws.backend()?;
            let p = t.path.clone();
            blocking(move || Ok(backend.read_optional(&p).ok().flatten())).await?
        };
        let Some(text) = text else { continue };
        let range = codeintel::definition_snippet(&text, t.line, codeintel::language_of(&t.path));
        let snippet = text
            .lines()
            .skip(range.start_line as usize)
            .take((range.end_line - range.start_line + 1) as usize)
            .collect::<Vec<_>>()
            .join("\n");
        definitions.push(QuickDefinition {
            path: t.path,
            line: t.line,
            column: t.column,
            name: t.name,
            container: t.container,
            start_line: range.start_line,
            text: snippet,
            truncated: range.truncated,
        });
    }
    let message = definitions
        .is_empty()
        .then(|| "No definition found".to_string());
    Ok(QuickDefinitionAnswer {
        title: answer.title,
        definitions,
        message,
    })
}

/// Type Info (Ctrl+Shift+P): the type of the expression at the caret, from the language
/// servers' hover, else from the PHP index's signature. `null` when unknown.
#[tauri::command]
pub async fn code_type_info(
    path: String,
    line: u32,
    character: u32,
    content: Option<String>,
    app: AppHandle,
    lsp: State<'_, LspState>,
    ws: State<'_, AppState>,
) -> Result<Option<String>> {
    if !path.starts_with(LIBRARY_PREFIX) {
        let c = Ctx::new(&path, &app, &lsp, &ws)?;
        let hover = c
            .manager
            .hover(&c.abs, lsp_types::Position::new(line, character))
            .await
            .ok()
            .flatten();
        if let Some(ty) = hover.and_then(|h| codeintel::type_from_hover(&h.contents)) {
            return Ok(Some(ty));
        }
    }
    if !is_php(&path) {
        return Ok(None);
    }
    let backend = ws.backend()?;
    blocking(move || {
        let text = current_text(&backend, &path, &content);
        let offset = codeintel::utf16_offset(&text, Position::new(line, character));
        let def = backend.php().definition(&path, offset, Some(text))?;
        Ok(def
            .info
            .and_then(|i| codeintel::type_from_signature(&i.signature)))
    })
    .await
}

/// Context Info (Alt+Q): the headers of the classes, functions and rules around `line`.
#[tauri::command]
pub async fn code_context_info(
    path: String,
    line: u32,
    content: Option<String>,
    ws: State<'_, AppState>,
) -> Result<Vec<codeintel::ContextLine>> {
    let text = match content {
        Some(t) => t,
        None => {
            let backend = ws.backend()?;
            let p = path.clone();
            blocking(move || Ok(current_text(&backend, &p, &None))).await?
        }
    };
    Ok(codeintel::context_headers(
        &text,
        line,
        codeintel::language_of(&path),
    ))
}

/// Related Symbol (Ctrl+Alt+Home): tests, views, templates, routes, migrations and other
/// files that belong to `path`. Computed next to the files (in the agent for a remote
/// project), see `forge_codeintel::related`.
#[tauri::command]
pub async fn code_related(
    path: String,
    content: Option<String>,
    ws: State<'_, AppState>,
) -> Result<Vec<codeintel::RelatedItem>> {
    let backend = ws.backend()?;
    blocking(move || {
        forge_backend::frameworks::framework_call(
            backend.as_ref(),
            forge_backend::frameworks::FrameworkCall::Related { path, content },
        )
    })
    .await
}

/// Navigate | Test / Test Subject (Ctrl+Shift+T): the tests of `path`, or the subjects of a
/// test. Computed next to the files, see `forge_codeintel::test_nav`.
#[tauri::command]
pub async fn code_test_targets(
    path: String,
    content: Option<String>,
    ws: State<'_, AppState>,
) -> Result<codeintel::TestTargets> {
    let backend = ws.backend()?;
    blocking(move || {
        forge_backend::frameworks::framework_call(
            backend.as_ref(),
            forge_backend::frameworks::FrameworkCall::TestTargets { path, content },
        )
    })
    .await
}

/// Create New Test: the dialog's initial values for the subject `path` (`None` for a test or
/// an unsupported file). The PSR-4 rules come from Settings | PHP | Composer's composer.json.
#[tauri::command]
pub async fn code_new_test_plan(
    path: String,
    content: Option<String>,
    ws: State<'_, AppState>,
) -> Result<Option<codeintel::NewTestPlan>> {
    let composer_path = ws
        .read_settings()
        .get_as::<String>("php.composer.json")
        .unwrap_or_default();
    let backend = ws.backend()?;
    blocking(move || {
        forge_backend::frameworks::framework_call(
            backend.as_ref(),
            forge_backend::frameworks::FrameworkCall::NewTestPlan {
                path,
                content,
                composer_path,
            },
        )
    })
    .await
}

/// Create New Test: render the test and create its file (never replaces one).
#[tauri::command]
pub async fn code_create_test(
    request: codeintel::TestSourceRequest,
    ws: State<'_, AppState>,
    lh: State<'_, crate::commands::local_history::LocalHistoryState>,
) -> Result<codeintel::TestSource> {
    let file = codeintel::test_source(&request).map_err(ForgeError::Other)?;
    let backend = ws.backend()?;
    let (path, text) = (file.path.clone(), file.text.clone());
    blocking(move || {
        backend.create_file(&path)?;
        backend.write_file(&path, &text)
    })
    .await?;
    crate::commands::local_history::created(
        &lh,
        &file.path,
        forge_localhistory::Content::Bytes(file.text.clone().into_bytes()),
    );
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_features_name_the_servers() {
        let m = missing_feature(Feature::Implementation, &["yaml-language-server".into()]);
        assert_eq!(
            m.message,
            "Go to Implementation is not supported by yaml-language-server"
        );
    }

    #[test]
    fn php_files_are_recognized() {
        assert!(is_php("app/Models/User.php"));
        assert!(!is_php("resources/views/a.blade.php"));
        assert!(!is_php("src/app.ts"));
    }
}
