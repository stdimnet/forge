//! Editor-friendly views of LSP results. The UI consumes these instead of raw protocol
//! types: markup is always markdown, kinds are strings, locations carry file paths.
//! Positions stay LSP positions (0-based line, UTF-16 character), which is also what
//! CodeMirror uses for offsets inside a line.

use std::path::PathBuf;

use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionItemTag, CompletionResponse, Diagnostic,
    DiagnosticSeverity, DiagnosticTag, DocumentSymbol, DocumentSymbolResponse, Documentation,
    GotoDefinitionResponse, Hover, HoverContents, Location, MarkedString, NumberOrString,
    ParameterLabel, Range, SignatureHelp, SymbolKind, WorkspaceSymbolResponse,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::client::uri_to_path;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    Error,
    Warning,
    /// PHPStorm's "weak warning" (LSP information).
    Weak,
    /// LSP hint.
    Info,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorDiagnostic {
    pub range: Range,
    pub severity: Severity,
    pub message: String,
    pub source: Option<String>,
    pub code: Option<String>,
    /// Unused code: rendered greyed out.
    pub unnecessary: bool,
    pub deprecated: bool,
}

impl From<&Diagnostic> for EditorDiagnostic {
    fn from(d: &Diagnostic) -> Self {
        let severity = match d.severity {
            Some(DiagnosticSeverity::ERROR) | None => Severity::Error,
            Some(DiagnosticSeverity::WARNING) => Severity::Warning,
            Some(DiagnosticSeverity::INFORMATION) => Severity::Weak,
            _ => Severity::Info,
        };
        let tags = d.tags.as_deref().unwrap_or_default();
        EditorDiagnostic {
            range: d.range,
            severity,
            message: d.message.clone(),
            source: d.source.clone(),
            code: d.code.as_ref().map(|c| match c {
                NumberOrString::Number(n) => n.to_string(),
                NumberOrString::String(s) => s.clone(),
            }),
            unnecessary: tags.contains(&DiagnosticTag::UNNECESSARY),
            deprecated: tags.contains(&DiagnosticTag::DEPRECATED),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorHover {
    /// Markdown.
    pub contents: String,
    pub range: Option<Range>,
}

fn marked_to_markdown(m: &MarkedString) -> String {
    match m {
        MarkedString::String(s) => s.clone(),
        MarkedString::LanguageString(ls) => format!("```{}\n{}\n```", ls.language, ls.value),
    }
}

impl From<Hover> for EditorHover {
    fn from(h: Hover) -> Self {
        let contents = match h.contents {
            HoverContents::Scalar(m) => marked_to_markdown(&m),
            HoverContents::Array(ms) => ms
                .iter()
                .map(marked_to_markdown)
                .collect::<Vec<_>>()
                .join("\n\n---\n\n"),
            HoverContents::Markup(m) => m.value,
        };
        EditorHover {
            contents,
            range: h.range,
        }
    }
}

fn documentation_to_markdown(d: &Documentation) -> String {
    match d {
        Documentation::String(s) => s.clone(),
        Documentation::MarkupContent(m) => m.value.clone(),
    }
}

/// Lower-camel-case name of a completion kind, e.g. `"method"`, `"enumMember"`.
pub fn completion_kind_name(kind: Option<CompletionItemKind>) -> &'static str {
    match kind {
        Some(CompletionItemKind::METHOD) => "method",
        Some(CompletionItemKind::FUNCTION) => "function",
        Some(CompletionItemKind::CONSTRUCTOR) => "constructor",
        Some(CompletionItemKind::FIELD) => "field",
        Some(CompletionItemKind::VARIABLE) => "variable",
        Some(CompletionItemKind::CLASS) => "class",
        Some(CompletionItemKind::INTERFACE) => "interface",
        Some(CompletionItemKind::MODULE) => "module",
        Some(CompletionItemKind::PROPERTY) => "property",
        Some(CompletionItemKind::UNIT) => "unit",
        Some(CompletionItemKind::VALUE) => "value",
        Some(CompletionItemKind::ENUM) => "enum",
        Some(CompletionItemKind::KEYWORD) => "keyword",
        Some(CompletionItemKind::SNIPPET) => "snippet",
        Some(CompletionItemKind::COLOR) => "color",
        Some(CompletionItemKind::FILE) => "file",
        Some(CompletionItemKind::REFERENCE) => "reference",
        Some(CompletionItemKind::FOLDER) => "folder",
        Some(CompletionItemKind::ENUM_MEMBER) => "enumMember",
        Some(CompletionItemKind::CONSTANT) => "constant",
        Some(CompletionItemKind::STRUCT) => "struct",
        Some(CompletionItemKind::EVENT) => "event",
        Some(CompletionItemKind::OPERATOR) => "operator",
        Some(CompletionItemKind::TYPE_PARAMETER) => "typeParameter",
        _ => "text",
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorCompletionItem {
    pub label: String,
    pub kind: String,
    pub detail: Option<String>,
    /// Markdown.
    pub documentation: Option<String>,
    pub insert_text: Option<String>,
    /// Replace range from a `textEdit`, if the server sent one.
    pub replace_range: Option<Range>,
    pub filter_text: Option<String>,
    pub sort_text: Option<String>,
    pub deprecated: bool,
    /// Edits elsewhere in the file, applied with the completion (auto-import: the server's
    /// `import` / `use` statement). Often only filled in by `completionItem/resolve`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub additional_text_edits: Vec<lsp_types::TextEdit>,
    /// The original item, passed back to `lsp_completion_resolve`.
    pub raw: Value,
}

impl From<&CompletionItem> for EditorCompletionItem {
    #[allow(deprecated)] // `deprecated` field is still sent by many servers
    fn from(c: &CompletionItem) -> Self {
        let (insert_text, replace_range) = match &c.text_edit {
            Some(lsp_types::CompletionTextEdit::Edit(e)) => {
                (Some(e.new_text.clone()), Some(e.range))
            }
            Some(lsp_types::CompletionTextEdit::InsertAndReplace(e)) => {
                (Some(e.new_text.clone()), Some(e.replace))
            }
            None => (c.insert_text.clone(), None),
        };
        let detail = c
            .detail
            .clone()
            .or_else(|| c.label_details.as_ref().and_then(|d| d.description.clone()));
        EditorCompletionItem {
            label: c.label.clone(),
            kind: completion_kind_name(c.kind).into(),
            detail,
            documentation: c.documentation.as_ref().map(documentation_to_markdown),
            insert_text,
            replace_range,
            filter_text: c.filter_text.clone(),
            sort_text: c.sort_text.clone(),
            deprecated: c.deprecated.unwrap_or(false)
                || c.tags
                    .as_deref()
                    .unwrap_or_default()
                    .contains(&CompletionItemTag::DEPRECATED),
            additional_text_edits: c.additional_text_edits.clone().unwrap_or_default(),
            raw: serde_json::to_value(c).unwrap_or(Value::Null),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorCompletionList {
    pub is_incomplete: bool,
    pub items: Vec<EditorCompletionItem>,
}

impl From<CompletionResponse> for EditorCompletionList {
    fn from(r: CompletionResponse) -> Self {
        let (is_incomplete, items) = match r {
            CompletionResponse::Array(items) => (false, items),
            CompletionResponse::List(l) => (l.is_incomplete, l.items),
        };
        EditorCompletionList {
            is_incomplete,
            items: items.iter().map(EditorCompletionItem::from).collect(),
        }
    }
}

/// A location as an absolute file path plus range. The app rewrites `path` relative to
/// the workspace before handing it to the UI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorLocation {
    pub path: String,
    pub range: Range,
}

fn location(uri: &lsp_types::Url, range: Range) -> Option<EditorLocation> {
    Some(EditorLocation {
        path: uri_to_path(uri)?.display().to_string(),
        range,
    })
}

pub fn locations(locs: &[Location]) -> Vec<EditorLocation> {
    locs.iter()
        .filter_map(|l| location(&l.uri, l.range))
        .collect()
}

pub fn definition_locations(r: GotoDefinitionResponse) -> Vec<EditorLocation> {
    match r {
        GotoDefinitionResponse::Scalar(l) => locations(&[l]),
        GotoDefinitionResponse::Array(ls) => locations(&ls),
        GotoDefinitionResponse::Link(links) => links
            .iter()
            .filter_map(|l| location(&l.target_uri, l.target_selection_range))
            .collect(),
    }
}

pub fn symbol_kind_name(kind: SymbolKind) -> &'static str {
    match kind {
        SymbolKind::FILE => "file",
        SymbolKind::MODULE => "module",
        SymbolKind::NAMESPACE => "namespace",
        SymbolKind::PACKAGE => "package",
        SymbolKind::CLASS => "class",
        SymbolKind::METHOD => "method",
        SymbolKind::PROPERTY => "property",
        SymbolKind::FIELD => "field",
        SymbolKind::CONSTRUCTOR => "constructor",
        SymbolKind::ENUM => "enum",
        SymbolKind::INTERFACE => "interface",
        SymbolKind::FUNCTION => "function",
        SymbolKind::VARIABLE => "variable",
        SymbolKind::CONSTANT => "constant",
        SymbolKind::STRING => "string",
        SymbolKind::NUMBER => "number",
        SymbolKind::BOOLEAN => "boolean",
        SymbolKind::ARRAY => "array",
        SymbolKind::OBJECT => "object",
        SymbolKind::KEY => "key",
        SymbolKind::NULL => "null",
        SymbolKind::ENUM_MEMBER => "enumMember",
        SymbolKind::STRUCT => "struct",
        SymbolKind::EVENT => "event",
        SymbolKind::OPERATOR => "operator",
        SymbolKind::TYPE_PARAMETER => "typeParameter",
        _ => "unknown",
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorSymbol {
    pub name: String,
    pub detail: Option<String>,
    pub kind: String,
    pub range: Range,
    pub selection_range: Range,
    pub children: Vec<EditorSymbol>,
}

impl From<&DocumentSymbol> for EditorSymbol {
    fn from(s: &DocumentSymbol) -> Self {
        EditorSymbol {
            name: s.name.clone(),
            detail: s.detail.clone(),
            kind: symbol_kind_name(s.kind).into(),
            range: s.range,
            selection_range: s.selection_range,
            children: s
                .children
                .as_deref()
                .unwrap_or_default()
                .iter()
                .map(EditorSymbol::from)
                .collect(),
        }
    }
}

/// Document symbols as a tree. Flat `SymbolInformation` lists are nested by container
/// name where possible.
pub fn document_symbols(r: DocumentSymbolResponse) -> Vec<EditorSymbol> {
    match r {
        DocumentSymbolResponse::Nested(syms) => syms.iter().map(EditorSymbol::from).collect(),
        DocumentSymbolResponse::Flat(infos) => {
            let mut roots: Vec<EditorSymbol> = Vec::new();
            let mut orphans: Vec<(Option<String>, EditorSymbol)> = Vec::new();
            for info in &infos {
                let sym = EditorSymbol {
                    name: info.name.clone(),
                    detail: None,
                    kind: symbol_kind_name(info.kind).into(),
                    range: info.location.range,
                    selection_range: info.location.range,
                    children: vec![],
                };
                match &info.container_name {
                    Some(c) if !c.is_empty() => orphans.push((Some(c.clone()), sym)),
                    _ => roots.push(sym),
                }
            }
            for (container, sym) in orphans {
                let container = container.unwrap_or_default();
                let short = container
                    .rsplit('\\')
                    .next()
                    .unwrap_or(&container)
                    .to_string();
                match roots
                    .iter_mut()
                    .find(|r| r.name == container || r.name == short)
                {
                    Some(parent) => parent.children.push(sym),
                    None => roots.push(sym),
                }
            }
            roots
        }
    }
}

/// One hit of a project-wide symbol search (`workspace/symbol`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorWorkspaceSymbol {
    pub name: String,
    pub kind: String,
    /// Enclosing class, module or namespace as reported by the server.
    pub container_name: Option<String>,
    /// Absolute path; the app rewrites it relative to the workspace.
    pub path: String,
    /// `None` when the server only knows the file (resolve on demand is not supported).
    pub range: Option<Range>,
    /// Id of the server that found it.
    pub server: String,
}

/// Flatten either `workspace/symbol` result shape.
pub fn workspace_symbols(r: WorkspaceSymbolResponse, server: &str) -> Vec<EditorWorkspaceSymbol> {
    match r {
        WorkspaceSymbolResponse::Flat(infos) => infos
            .into_iter()
            .filter_map(|i| {
                Some(EditorWorkspaceSymbol {
                    name: i.name,
                    kind: symbol_kind_name(i.kind).into(),
                    container_name: i.container_name.filter(|c| !c.is_empty()),
                    path: uri_to_path(&i.location.uri)?.display().to_string(),
                    range: Some(i.location.range),
                    server: server.to_string(),
                })
            })
            .collect(),
        WorkspaceSymbolResponse::Nested(syms) => syms
            .into_iter()
            .filter_map(|s| {
                let (uri, range) = match s.location {
                    lsp_types::OneOf::Left(l) => (l.uri, Some(l.range)),
                    lsp_types::OneOf::Right(w) => (w.uri, None),
                };
                Some(EditorWorkspaceSymbol {
                    name: s.name,
                    kind: symbol_kind_name(s.kind).into(),
                    container_name: s.container_name.filter(|c| !c.is_empty()),
                    path: uri_to_path(&uri)?.display().to_string(),
                    range,
                    server: server.to_string(),
                })
            })
            .collect(),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorParameter {
    /// `[start, end)` UTF-16 offsets into the signature label.
    pub label: [u32; 2],
    pub documentation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorSignature {
    pub label: String,
    pub documentation: Option<String>,
    pub parameters: Vec<EditorParameter>,
    pub active_parameter: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorSignatureHelp {
    pub signatures: Vec<EditorSignature>,
    pub active_signature: u32,
    pub active_parameter: u32,
}

fn utf16_len(s: &str) -> u32 {
    s.encode_utf16().count() as u32
}

impl From<SignatureHelp> for EditorSignatureHelp {
    fn from(h: SignatureHelp) -> Self {
        let signatures = h
            .signatures
            .iter()
            .map(|s| {
                let mut search_from = 0usize;
                let parameters = s
                    .parameters
                    .as_deref()
                    .unwrap_or_default()
                    .iter()
                    .map(|p| {
                        let label = match &p.label {
                            ParameterLabel::LabelOffsets(o) => *o,
                            ParameterLabel::Simple(text) => {
                                match s.label[search_from..].find(text.as_str()) {
                                    Some(i) => {
                                        let start = search_from + i;
                                        search_from = start + text.len();
                                        [
                                            utf16_len(&s.label[..start]),
                                            utf16_len(&s.label[..search_from]),
                                        ]
                                    }
                                    None => [0, 0],
                                }
                            }
                        };
                        EditorParameter {
                            label,
                            documentation: p.documentation.as_ref().map(documentation_to_markdown),
                        }
                    })
                    .collect();
                EditorSignature {
                    label: s.label.clone(),
                    documentation: s.documentation.as_ref().map(documentation_to_markdown),
                    parameters,
                    active_parameter: s.active_parameter,
                }
            })
            .collect();
        EditorSignatureHelp {
            signatures,
            active_signature: h.active_signature.unwrap_or(0),
            active_parameter: h.active_parameter.unwrap_or(0),
        }
    }
}

// ---- Workspace edits and code actions ----

fn range_of(r: Range) -> forge_codeintel::Range {
    forge_codeintel::Range::new(
        forge_codeintel::Position::new(r.start.line, r.start.character),
        forge_codeintel::Position::new(r.end.line, r.end.character),
    )
}

fn text_edits(edits: &[lsp_types::TextEdit]) -> Vec<forge_codeintel::TextEdit> {
    edits
        .iter()
        .map(|e| forge_codeintel::TextEdit {
            range: range_of(e.range),
            new_text: e.new_text.clone(),
        })
        .collect()
}

fn annotated_edits(
    edits: &[lsp_types::OneOf<lsp_types::TextEdit, lsp_types::AnnotatedTextEdit>],
) -> Vec<forge_codeintel::TextEdit> {
    let plain: Vec<lsp_types::TextEdit> = edits
        .iter()
        .map(|e| match e {
            lsp_types::OneOf::Left(t) => t.clone(),
            lsp_types::OneOf::Right(a) => a.text_edit.clone(),
        })
        .collect();
    text_edits(&plain)
}

fn path_of(uri: &lsp_types::Url) -> String {
    uri_to_path(uri)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| uri.to_string())
}

/// An LSP workspace edit as Forge's model, with absolute paths (the app makes them
/// workspace-relative). `documentChanges` wins over `changes`, as the protocol says.
pub fn workspace_edit(e: &lsp_types::WorkspaceEdit) -> forge_codeintel::WorkspaceEdit {
    use forge_codeintel::DocumentChange;
    use lsp_types::{DocumentChangeOperation, DocumentChanges, ResourceOp};
    let mut out = forge_codeintel::WorkspaceEdit::default();
    match &e.document_changes {
        Some(DocumentChanges::Edits(edits)) => {
            for d in edits {
                out.add_edits(path_of(&d.text_document.uri), annotated_edits(&d.edits));
            }
        }
        Some(DocumentChanges::Operations(ops)) => {
            for op in ops {
                match op {
                    DocumentChangeOperation::Edit(d) => {
                        out.add_edits(path_of(&d.text_document.uri), annotated_edits(&d.edits))
                    }
                    DocumentChangeOperation::Op(ResourceOp::Create(c)) => {
                        out.changes.push(DocumentChange::Create {
                            path: path_of(&c.uri),
                            overwrite: c
                                .options
                                .as_ref()
                                .and_then(|o| o.overwrite)
                                .unwrap_or(false),
                        })
                    }
                    DocumentChangeOperation::Op(ResourceOp::Rename(r)) => {
                        out.changes.push(DocumentChange::Rename {
                            from: path_of(&r.old_uri),
                            to: path_of(&r.new_uri),
                        })
                    }
                    DocumentChangeOperation::Op(ResourceOp::Delete(d)) => {
                        out.changes.push(DocumentChange::Delete {
                            path: path_of(&d.uri),
                        })
                    }
                }
            }
        }
        None => {
            let mut files: Vec<(String, &Vec<lsp_types::TextEdit>)> = e
                .changes
                .iter()
                .flatten()
                .map(|(uri, edits)| (path_of(uri), edits))
                .collect();
            files.sort_by(|a, b| a.0.cmp(&b.0));
            for (path, edits) in files {
                out.add_edits(path, text_edits(edits));
            }
        }
    }
    out
}

/// A command a code action runs on the server (`workspace/executeCommand`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorCommand {
    pub title: String,
    pub command: String,
    #[serde(default)]
    pub arguments: Vec<Value>,
}

/// A quick fix or refactoring for the Alt+Enter popup.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorCodeAction {
    pub title: String,
    /// LSP kind, e.g. `quickfix`, `refactor.extract`, `source.organizeImports`.
    pub kind: Option<String>,
    pub is_preferred: bool,
    /// Why the action cannot run now (shown greyed out).
    pub disabled: Option<String>,
    /// Absolute paths until the app makes them relative.
    pub edit: Option<forge_codeintel::WorkspaceEdit>,
    pub command: Option<EditorCommand>,
    /// The edit comes from `codeAction/resolve` (send `raw` back).
    pub needs_resolve: bool,
    /// The server that offered it.
    pub server: String,
    /// The original action, for `codeAction/resolve`.
    pub raw: Value,
}

impl EditorCodeAction {
    pub fn from_lsp(a: &lsp_types::CodeActionOrCommand, server: &str, can_resolve: bool) -> Self {
        match a {
            lsp_types::CodeActionOrCommand::Command(c) => EditorCodeAction {
                title: c.title.clone(),
                kind: None,
                is_preferred: false,
                disabled: None,
                edit: None,
                command: Some(EditorCommand {
                    title: c.title.clone(),
                    command: c.command.clone(),
                    arguments: c.arguments.clone().unwrap_or_default(),
                }),
                needs_resolve: false,
                server: server.to_string(),
                raw: serde_json::to_value(c).unwrap_or(Value::Null),
            },
            lsp_types::CodeActionOrCommand::CodeAction(c) => {
                Self::from_action(c, server, can_resolve)
            }
        }
    }

    pub fn from_action(c: &lsp_types::CodeAction, server: &str, can_resolve: bool) -> Self {
        EditorCodeAction {
            title: c.title.clone(),
            kind: c.kind.as_ref().map(|k| k.as_str().to_string()),
            is_preferred: c.is_preferred.unwrap_or(false),
            disabled: c.disabled.as_ref().map(|d| d.reason.clone()),
            edit: c.edit.as_ref().map(workspace_edit),
            command: c.command.as_ref().map(|cmd| EditorCommand {
                title: cmd.title.clone(),
                command: cmd.command.clone(),
                arguments: cmd.arguments.clone().unwrap_or_default(),
            }),
            needs_resolve: can_resolve && c.edit.is_none(),
            server: server.to_string(),
            raw: serde_json::to_value(c).unwrap_or(Value::Null),
        }
    }
}

/// Answer of `textDocument/prepareRename`: the range to rename and its current text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorPrepareRename {
    pub range: Option<Range>,
    pub placeholder: Option<String>,
}

impl From<lsp_types::PrepareRenameResponse> for EditorPrepareRename {
    fn from(r: lsp_types::PrepareRenameResponse) -> Self {
        match r {
            lsp_types::PrepareRenameResponse::Range(range) => EditorPrepareRename {
                range: Some(range),
                placeholder: None,
            },
            lsp_types::PrepareRenameResponse::RangeWithPlaceholder { range, placeholder } => {
                EditorPrepareRename {
                    range: Some(range),
                    placeholder: Some(placeholder),
                }
            }
            lsp_types::PrepareRenameResponse::DefaultBehavior { .. } => EditorPrepareRename {
                range: None,
                placeholder: None,
            },
        }
    }
}

/// Absolute path behind a diagnostics URI, if it is a local file.
pub fn diagnostics_path(uri: &lsp_types::Url) -> Option<PathBuf> {
    uri_to_path(uri)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn hover_markup_and_marked_strings_become_markdown() {
        let h: Hover = serde_json::from_value(json!({
            "contents": [{"language": "php", "value": "function a()"}, "Docs"]
        }))
        .unwrap();
        let e = EditorHover::from(h);
        assert_eq!(e.contents, "```php\nfunction a()\n```\n\n---\n\nDocs");
    }

    #[test]
    fn diagnostics_map_severity_and_tags() {
        let d: Diagnostic = serde_json::from_value(json!({
            "range": {"start": {"line": 1, "character": 0}, "end": {"line": 1, "character": 4}},
            "severity": 3, "message": "Unused", "code": 1003, "tags": [1]
        }))
        .unwrap();
        let e = EditorDiagnostic::from(&d);
        assert_eq!(e.severity, Severity::Weak);
        assert!(e.unnecessary && !e.deprecated);
        assert_eq!(e.code.as_deref(), Some("1003"));
        assert_eq!(serde_json::to_value(&e).unwrap()["severity"], "weak");
    }

    #[test]
    fn completion_items_use_text_edits_and_kind_names() {
        let r: CompletionResponse = serde_json::from_value(json!({
            "isIncomplete": true,
            "items": [{
                "label": "hasMany", "kind": 2, "detail": "HasMany",
                "textEdit": {"range": {"start": {"line": 0, "character": 1}, "end": {"line": 0, "character": 3}}, "newText": "hasMany"},
                "tags": [1]
            }]
        }))
        .unwrap();
        let list = EditorCompletionList::from(r);
        assert!(list.is_incomplete);
        let item = &list.items[0];
        assert_eq!(item.kind, "method");
        assert!(item.deprecated);
        assert_eq!(item.insert_text.as_deref(), Some("hasMany"));
        assert_eq!(item.replace_range.unwrap().end.character, 3);
    }

    #[test]
    fn signature_parameters_resolve_to_utf16_offsets() {
        let h: SignatureHelp = serde_json::from_value(json!({
            "signatures": [{
                "label": "f(string $ä, int $b)",
                "parameters": [{"label": "string $ä"}, {"label": "int $b"}]
            }],
            "activeParameter": 1
        }))
        .unwrap();
        let e = EditorSignatureHelp::from(h);
        let params = &e.signatures[0].parameters;
        assert_eq!(params[0].label, [2, 11]);
        assert_eq!(params[1].label, [13, 19]);
        assert_eq!(e.active_parameter, 1);
    }

    #[test]
    fn workspace_symbols_accept_both_shapes() {
        let flat: WorkspaceSymbolResponse = serde_json::from_value(json!([
            {"name": "Order", "kind": 11, "containerName": "", "location": {"uri": "file:///p/a.ts", "range": {"start": {"line": 2, "character": 0}, "end": {"line": 6, "character": 1}}}}
        ]))
        .unwrap();
        let hits = workspace_symbols(flat, "ts");
        assert_eq!(hits[0].kind, "interface");
        assert_eq!(hits[0].container_name, None);
        assert_eq!(hits[0].range.unwrap().start.line, 2);
        assert_eq!(hits[0].server, "ts");

        let nested: WorkspaceSymbolResponse = serde_json::from_value(json!([
            {"name": "load", "kind": 12, "containerName": "app", "location": {"uri": "file:///p/a.py"}}
        ]))
        .unwrap();
        let hits = workspace_symbols(nested, "py");
        assert_eq!(hits[0].kind, "function");
        assert_eq!(hits[0].container_name.as_deref(), Some("app"));
        assert!(hits[0].range.is_none());
    }

    #[test]
    fn workspace_edits_keep_document_change_order() {
        let e: lsp_types::WorkspaceEdit = serde_json::from_value(json!({
            "documentChanges": [
                {"textDocument": {"uri": "file:///p/User.php", "version": 3},
                 "edits": [{"range": {"start": {"line": 1, "character": 6}, "end": {"line": 1, "character": 10}}, "newText": "Customer"}]},
                {"kind": "rename", "oldUri": "file:///p/User.php", "newUri": "file:///p/Customer.php"},
                {"kind": "create", "uri": "file:///p/new.php", "options": {"overwrite": true}}
            ]
        }))
        .unwrap();
        let m = workspace_edit(&e);
        assert_eq!(m.changes.len(), 3);
        assert_eq!(
            m.paths(),
            vec!["/p/User.php", "/p/Customer.php", "/p/new.php"]
        );
        assert_eq!(m.edit_count(), 1);

        let e: lsp_types::WorkspaceEdit = serde_json::from_value(json!({
            "changes": {
                "file:///p/b.php": [{"range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 1}}, "newText": "b"}],
                "file:///p/a.php": [{"range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 1}}, "newText": "a"}]
            }
        }))
        .unwrap();
        assert_eq!(workspace_edit(&e).paths(), vec!["/p/a.php", "/p/b.php"]);
    }

    #[test]
    fn code_actions_and_commands_are_normalized() {
        let fix: lsp_types::CodeActionOrCommand = serde_json::from_value(json!({
            "title": "Import 'App\\Models\\Order'", "kind": "quickfix", "isPreferred": true,
            "data": {"id": 7}
        }))
        .unwrap();
        let a = EditorCodeAction::from_lsp(&fix, "intelephense", true);
        assert_eq!(a.kind.as_deref(), Some("quickfix"));
        assert!(a.is_preferred && a.needs_resolve && a.edit.is_none());
        assert_eq!(a.raw["data"]["id"], 7);

        let cmd: lsp_types::CodeActionOrCommand = serde_json::from_value(json!({
            "title": "Organize imports", "command": "php.organizeImports", "arguments": ["x"]
        }))
        .unwrap();
        let a = EditorCodeAction::from_lsp(&cmd, "intelephense", true);
        assert_eq!(a.command.unwrap().command, "php.organizeImports");
        assert!(!a.needs_resolve);
    }

    #[test]
    fn prepare_rename_shapes() {
        let r: lsp_types::PrepareRenameResponse = serde_json::from_value(json!({
            "range": {"start": {"line": 1, "character": 2}, "end": {"line": 1, "character": 6}},
            "placeholder": "User"
        }))
        .unwrap();
        let p = EditorPrepareRename::from(r);
        assert_eq!(p.placeholder.as_deref(), Some("User"));
        let r: lsp_types::PrepareRenameResponse =
            serde_json::from_value(json!({"defaultBehavior": true})).unwrap();
        assert_eq!(EditorPrepareRename::from(r).range, None);
    }

    #[test]
    fn flat_symbols_are_nested_by_container() {
        let r: DocumentSymbolResponse = serde_json::from_value(json!([
            {"name": "User", "kind": 5, "location": {"uri": "file:///a.php", "range": {"start": {"line": 0, "character": 0}, "end": {"line": 9, "character": 0}}}},
            {"name": "orders", "kind": 6, "containerName": "App\\Models\\User", "location": {"uri": "file:///a.php", "range": {"start": {"line": 2, "character": 0}, "end": {"line": 3, "character": 0}}}}
        ]))
        .unwrap();
        let syms = document_symbols(r);
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].children[0].name, "orders");
        assert_eq!(syms[0].children[0].kind, "method");
    }
}
