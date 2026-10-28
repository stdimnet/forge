//! Rails in the editor, on the editor's text (offsets are UTF-16, like CodeMirror's):
//!
//! - Go to Declaration on route helpers (`articles_path`), `render` / `layout` names,
//!   translation keys, Stimulus controllers, actions and targets, Turbo frame ids and the
//!   targets of `config/routes.rb`;
//! - completion of route helpers, partials and templates, translation keys (lazy lookup
//!   too), model attributes (`Article.where(`, `@article.`, `validates :`, `form.text_field :`,
//!   `permit(`), Stimulus identifiers, actions and targets, Turbo helpers and frame ids;
//! - gutter marks: action ↔ views and routes, view → action, partial → its renderers,
//!   model → table and migrations, associations → models, routes → actions, Stimulus
//!   controllers ↔ views;
//! - inspections (missing translations, unresolved partials and templates, unknown
//!   Stimulus controllers, actions and targets) and folds of `t(…)` with the translation.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::app::{self, RenderKind};
use crate::hotwire::{self, StimulusRefKind};
use crate::i18n::{self, Catalog};
use crate::inflect::{camelize, singularize, underscore};
use crate::source::{byte_to_utf16, string_around, utf16_to_byte, word_at, LineIndex};
use crate::{Location, Model, Project};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionItem {
    pub label: String,
    /// `route`, `partial`, `template`, `i18n`, `attribute`, `association`, `stimulus`,
    /// `action`, `target`, `turbo`, `frame`.
    pub kind: String,
    pub detail: String,
    /// The text to insert when it differs from the label (`title: `).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub apply: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Completions {
    /// UTF-16 range the items replace.
    pub from: u32,
    pub to: u32,
    pub items: Vec<CompletionItem>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GutterMark {
    pub line: u32,
    /// `view`, `route`, `controller`, `partial`, `table`, `model`, `action`, `stimulus`.
    pub kind: String,
    pub tooltip: String,
    pub targets: Vec<Location>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    pub line: u32,
    /// UTF-16 columns.
    pub start: u32,
    pub end: u32,
    pub message: String,
    /// `error`, `warning` or `info`.
    pub severity: String,
    pub code: String,
}

/// A translation call folded to its text.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Fold {
    /// UTF-16 document offsets.
    pub from: u32,
    pub to: u32,
    pub text: String,
    pub key: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileMarks {
    pub gutter: Vec<GutterMark>,
    pub problems: Vec<Problem>,
    pub folds: Vec<Fold>,
}

/// The name of the `def` around a byte offset.
pub fn enclosing_def(text: &str, at: usize) -> Option<String> {
    let before = &text[..at.min(text.len())];
    for line in before.lines().rev() {
        let t = crate::source::code(line).trim_start();
        if let Some(rest) = t.strip_prefix("def ") {
            let name: String = rest
                .trim_start_matches("self.")
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '?' || *c == '!')
                .collect();
            return (!name.is_empty()).then_some(name);
        }
        if t.starts_with("class ") {
            return None;
        }
    }
    None
}

fn is_erb(path: &str) -> bool {
    path.ends_with(".erb")
}

fn is_routes_file(path: &str) -> bool {
    path == "config/routes.rb" || (path.starts_with("config/routes/") && path.ends_with(".rb"))
}

/// Ruby code at `at`: everywhere in `.rb`, inside `<% %>` in ERB.
fn in_code(path: &str, text: &str, at: usize) -> bool {
    if !is_erb(path) {
        return path.ends_with(".rb") || path.ends_with(".jbuilder") || path.ends_with(".builder");
    }
    let before = &text[..at];
    match (before.rfind("<%"), before.rfind("%>")) {
        (Some(o), Some(c)) => o > c,
        (Some(_), None) => true,
        _ => false,
    }
}

fn line_bounds(text: &str, at: usize) -> (usize, usize) {
    let s = text[..at].rfind('\n').map_or(0, |i| i + 1);
    let e = text[at..].find('\n').map_or(text.len(), |i| at + i);
    (s, e)
}

fn routes_named<'a>(p: &'a Project, name: &str) -> impl Iterator<Item = &'a crate::Route> {
    let name = name.to_string();
    p.info
        .routes
        .iter()
        .filter(move |r| r.name.as_deref() == Some(name.as_str()))
}

/// `articles_path` → `articles`.
fn helper_route(word: &str) -> Option<&str> {
    word.strip_suffix("_path")
        .or_else(|| word.strip_suffix("_url"))
}

fn route_label(r: &crate::Route) -> String {
    let m = if r.methods.is_empty() {
        "ANY".to_string()
    } else {
        r.methods.join("|")
    };
    format!("{m} {}", r.path)
}

fn in_range(s: usize, e: usize, at: usize) -> bool {
    s <= at && at <= e
}

// ---- Go to Declaration ---------------------------------------------------------------------------

/// Where the reference at `offset` (UTF-16) leads.
pub fn goto(p: &Project, path: &str, text: &str, offset: usize) -> Vec<Location> {
    let at = utf16_to_byte(text, offset);
    // Translation keys.
    for r in i18n::refs(path, text) {
        if in_range(r.start, r.end, at) {
            let key = r.full.unwrap_or(r.key);
            let mut out: Vec<Location> = p
                .catalog
                .get(&key)
                .unwrap_or_default()
                .iter()
                .map(|t| t.location.clone())
                .collect();
            out.sort_by_key(|l| {
                l.path != format!("config/locales/{}.yml", p.catalog.default_locale)
            });
            return out;
        }
    }
    // render "form", render :edit, layout "admin".
    for r in app::render_refs(path, text) {
        if in_range(r.start, r.end, at) {
            return app::resolve_render(path, &r, &p.views)
                .iter()
                .map(|f| Location::file(f))
                .collect();
        }
    }
    // Stimulus.
    for r in hotwire::stimulus_refs(text) {
        if !in_range(r.start, r.end, at) {
            continue;
        }
        let Some(c) = p.stimulus(&r.identifier) else {
            return Vec::new();
        };
        let found = match r.kind {
            StimulusRefKind::Controller => None,
            StimulusRefKind::Method => c.methods.iter().find(|m| m.name == r.name),
            StimulusRefKind::Target => c.targets.iter().find(|m| m.name == r.name),
        };
        return vec![found
            .map(|n| n.location.clone())
            .unwrap_or_else(|| c.location.clone())];
    }
    // Turbo frames: the frame and the other places that target it.
    let frames = hotwire::frame_refs(text);
    if let Some(f) = frames.iter().find(|f| in_range(f.start, f.end, at)) {
        return frame_locations(p, &f.id, Some((path, f.start)));
    }
    let (ls, le) = line_bounds(text, at);
    let line = &text[ls..le];
    // routes.rb: a target string or resource symbol leads to the controller's action.
    if is_routes_file(path) && string_around(line, at - ls).is_some() {
        let n = LineIndex::new(text).position(ls).0;
        let mut out: Vec<Location> = Vec::new();
        for r in p
            .info
            .routes
            .iter()
            .filter(|r| r.location.path == path && r.location.line == n)
        {
            if let Some(l) = &r.action_location {
                if !out.contains(l) {
                    out.push(l.clone());
                }
            }
        }
        if !out.is_empty() {
            return out;
        }
    }
    // Route helpers.
    if let Some((_, _, w)) = word_at(line, at - ls) {
        if let Some(name) = helper_route(w) {
            let out: Vec<Location> = routes_named(p, name).map(|r| r.location.clone()).collect();
            if !out.is_empty() {
                return out;
            }
        }
    }
    Vec::new()
}

fn frame_locations(p: &Project, id: &str, except: Option<(&str, usize)>) -> Vec<Location> {
    let mut defs = Vec::new();
    let mut uses = Vec::new();
    for (path, text) in p.files() {
        if !(path.starts_with("app/views/") || path.starts_with("app/controllers/")) {
            continue;
        }
        let idx = LineIndex::new(text);
        for f in hotwire::frame_refs(text) {
            if f.id != id || except == Some((path, f.start)) {
                continue;
            }
            let (line, character) = idx.position(f.start);
            let loc = Location {
                path: path.to_string(),
                line,
                character,
            };
            if f.definition {
                defs.push(loc);
            } else {
                uses.push(loc);
            }
        }
    }
    defs.extend(uses);
    defs
}

// ---- Completion -----------------------------------------------------------------------------------

/// The byte index of an unclosed quote in `before` (a line up to the caret).
fn open_quote(before: &str) -> Option<usize> {
    let mut quote: Option<(char, usize)> = None;
    let mut prev = ' ';
    for (i, c) in before.char_indices() {
        match quote {
            Some((q, _)) if c == q && prev != '\\' => quote = None,
            Some(_) => {}
            None if c == '"' || c == '\'' => quote = Some((c, i)),
            None => {}
        }
        prev = c;
    }
    quote.map(|(_, i)| i)
}

/// The identifier that ends `s` (after trailing spaces / an open parenthesis): `render(`,
/// `t `, `I18n.t(`.
fn trailing_call(s: &str) -> (String, &str) {
    let t = s.trim_end().trim_end_matches('(').trim_end();
    let start = t
        .char_indices()
        .rev()
        .take_while(|(_, c)| {
            c.is_alphanumeric() || *c == '_' || *c == '.' || *c == ':' || *c == '-'
        })
        .last()
        .map(|(i, _)| i)
        .unwrap_or(t.len());
    (t[start..].to_string(), &t[..start])
}

fn truncate(s: &str, n: usize) -> String {
    let one = s.replace('\n', " ");
    if one.chars().count() <= n {
        one
    } else {
        format!("{}…", one.chars().take(n).collect::<String>())
    }
}

fn item(label: impl Into<String>, kind: &str, detail: impl Into<String>) -> CompletionItem {
    CompletionItem {
        label: label.into(),
        kind: kind.into(),
        detail: detail.into(),
        apply: None,
    }
}

/// The completions at `offset` (UTF-16).
pub fn complete(p: &Project, path: &str, text: &str, offset: usize) -> Option<Completions> {
    let at = utf16_to_byte(text, offset);
    let (ls, le) = line_bounds(text, at);
    let before = &text[ls..at];
    if let Some(q) = open_quote(before) {
        let typed = &before[q + 1..];
        let quote = before[q..].chars().next().unwrap_or('"');
        let to = text[at..le].find(quote).map_or(at, |i| at + i);
        let head = &before[..q];
        let items = string_items(p, path, text, at, head, typed);
        let (items, from) = items?;
        return Some(Completions {
            from: byte_to_utf16(text, ls + q + 1 + from),
            to: byte_to_utf16(text, to),
            items,
        });
    }
    if !in_code(path, text, at) {
        return None;
    }
    // The word being typed (with a leading `:` for symbols).
    let mut ws = before.len();
    for (i, c) in before.char_indices().rev() {
        if c.is_alphanumeric() || c == '_' {
            ws = i;
        } else {
            break;
        }
    }
    let symbol = before[..ws].ends_with(':') && !before[..ws].ends_with("::");
    let head = if symbol {
        &before[..ws - 1]
    } else {
        &before[..ws]
    };
    let typed = &before[ws..];
    let from = ls + if symbol { ws - 1 } else { ws };
    let items = code_items(p, path, text, at, head, typed, symbol)?;
    Some(Completions {
        from: byte_to_utf16(text, from),
        to: byte_to_utf16(text, at),
        items,
    })
}

/// Items inside a string: (items, byte offset in the string where they start).
fn string_items(
    p: &Project,
    path: &str,
    text: &str,
    at: usize,
    head: &str,
    typed: &str,
) -> Option<(Vec<CompletionItem>, usize)> {
    let (call, _) = trailing_call(head);
    // Translation keys.
    if matches!(
        call.as_str(),
        "t" | "translate" | "I18n.t" | "I18n.translate"
    ) {
        let items = if let Some(rel) = typed.strip_prefix('.') {
            let _ = rel;
            let scope = i18n::lazy_scope(path, text, at)?;
            let prefix = format!("{scope}.");
            p.catalog
                .keys()
                .filter_map(|(k, v)| {
                    k.strip_prefix(&prefix)
                        .map(|r| item(format!(".{r}"), "i18n", truncate(v, 40)))
                })
                .collect()
        } else {
            p.catalog
                .keys()
                .map(|(k, v)| item(k, "i18n", truncate(v, 40)))
                .collect()
        };
        return Some((items, 0));
    }
    let head_t = head.trim_end();
    // render "…", render partial: "…", layout "…".
    let render_head = matches!(call.as_str(), "render")
        || head_t.ends_with("partial:")
        || head_t.ends_with("template:")
        || (head_t.ends_with("layout:") && head.contains("render"))
        || (call == "layout" && app::controller_name(path).is_some());
    if render_head && (path.starts_with("app/views/") || app::controller_name(path).is_some()) {
        return Some((render_items(p, path, head_t), 0));
    }
    // Stimulus and Turbo attributes: data-x="…" or data: { x: "…" }.
    let attr = if let Some(h) = head_t.strip_suffix('=') {
        let (name, _) = trailing_call(h);
        name.strip_prefix("data-").map(str::to_string)
    } else if let Some(h) = head_t.strip_suffix(':') {
        let (name, _) = trailing_call(h);
        head.contains("data:").then(|| name.replace('_', "-"))
    } else {
        None
    };
    // The word of a space-separated value being typed.
    let word_from = typed.rfind(' ').map_or(0, |i| i + 1);
    if let Some(attr) = attr {
        if attr == "controller" {
            let items = p
                .info
                .stimulus
                .iter()
                .map(|c| item(&c.identifier, "stimulus", &c.path))
                .collect();
            return Some((items, word_from));
        }
        if attr == "action" {
            let word = &typed[word_from..];
            let start = word.find("->").map_or(word_from, |i| word_from + i + 2);
            let mut items = Vec::new();
            for c in &p.info.stimulus {
                for m in &c.methods {
                    if matches!(m.name.as_str(), "connect" | "disconnect" | "initialize") {
                        continue;
                    }
                    items.push(item(
                        format!("{}#{}", c.identifier, m.name),
                        "action",
                        &c.path,
                    ));
                }
            }
            return Some((items, start));
        }
        if let Some(id) = attr.strip_suffix("-target") {
            let c = p.stimulus(id)?;
            let items = c
                .targets
                .iter()
                .map(|t| item(&t.name, "target", &c.identifier))
                .collect();
            return Some((items, word_from));
        }
        if attr == "turbo-frame" {
            return Some((frame_items(p), 0));
        }
        return None;
    }
    // turbo_stream.replace "…", turbo_frame_tag "…", target: "…".
    if call.starts_with("turbo_stream.") || call == "turbo_frame_tag" || head_t.ends_with("target:")
    {
        return Some((frame_items(p), 0));
    }
    // routes.rb: to: "controller#action".
    if is_routes_file(path) && (head_t.ends_with("to:") || head_t.ends_with("=>") || call == "root")
    {
        let mut items = Vec::new();
        for c in p.info.controllers.iter().filter(|c| c.kind == "controller") {
            for a in &c.actions {
                items.push(item(format!("{}#{}", c.name, a.name), "action", &c.class));
            }
        }
        return Some((items, 0));
    }
    None
}

fn frame_items(p: &Project) -> Vec<CompletionItem> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for (path, text) in p.files() {
        if !path.starts_with("app/views/") {
            continue;
        }
        for f in hotwire::frame_refs(text) {
            if f.definition && seen.insert(f.id.clone()) {
                out.push(item(f.id, "frame", path));
            }
        }
    }
    out
}

fn render_items(p: &Project, path: &str, head: &str) -> Vec<CompletionItem> {
    let dir = app::view_dir(path).unwrap_or_default();
    let layouts =
        head.ends_with("layout") || head.trim_end_matches('(').trim_end().ends_with("layout");
    let partials = path.starts_with("app/views/") || head.ends_with("partial:");
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for v in &p.views {
        let (label, kind) = if layouts {
            if v.dir != "layouts" || v.partial {
                continue;
            }
            (v.name.clone(), "template")
        } else if partials {
            if !v.partial {
                continue;
            }
            if v.dir == dir {
                (v.name.clone(), "partial")
            } else {
                (format!("{}/{}", v.dir, v.name), "partial")
            }
        } else {
            if v.partial || v.dir == "layouts" {
                continue;
            }
            if v.dir == dir {
                (v.name.clone(), "template")
            } else {
                (format!("{}/{}", v.dir, v.name), "template")
            }
        };
        if seen.insert(label.clone()) {
            out.push(item(label, kind, &v.path));
        }
    }
    out
}

const KEY_METHODS: &[&str] = &[
    "where",
    "not",
    "rewhere",
    "find_by",
    "find_by!",
    "find_or_create_by",
    "find_or_create_by!",
    "find_or_initialize_by",
    "create",
    "create!",
    "new",
    "build",
    "update",
    "update!",
    "update_columns",
    "assign_attributes",
    "exists?",
];

const SYMBOL_METHODS: &[&str] = &[
    "order",
    "reorder",
    "pluck",
    "pick",
    "select",
    "group",
    "distinct_on",
    "sum",
    "average",
    "minimum",
    "maximum",
    "count",
    "includes",
    "preload",
    "eager_load",
    "joins",
    "left_joins",
    "permit",
];

const FORM_FIELDS: &[&str] = &[
    "text_field",
    "text_area",
    "textarea",
    "label",
    "select",
    "check_box",
    "checkbox",
    "radio_button",
    "collection_select",
    "collection_check_boxes",
    "date_field",
    "date_select",
    "datetime_field",
    "datetime_local_field",
    "datetime_select",
    "time_field",
    "time_select",
    "hidden_field",
    "file_field",
    "number_field",
    "email_field",
    "password_field",
    "rich_text_area",
    "rich_textarea",
    "url_field",
    "telephone_field",
    "phone_field",
    "color_field",
    "search_field",
    "range_field",
];

const VALIDATIONS: &[&str] = &[
    "validates",
    "validates_presence_of",
    "validates_uniqueness_of",
    "validates_length_of",
    "validates_format_of",
    "validates_numericality_of",
    "validates_inclusion_of",
    "validate",
    "attr_readonly",
    "normalizes",
    "encrypts",
    "has_secure_token",
    "store_accessor",
];

/// A model named by an expression: `Article`, `@article`, `articles`, `current_user.articles`.
fn model_of<'a>(p: &'a Project, path: &str, expr: &str) -> Option<&'a Model> {
    let expr = expr.trim();
    let parts: Vec<&str> = expr.split('.').filter(|s| !s.is_empty()).collect();
    // The first constant of a chain: `Article.published.where`.
    if let Some(c) = parts
        .iter()
        .find(|s| s.starts_with(|c: char| c.is_uppercase()))
    {
        if let Some(m) = p.model(c) {
            return Some(m);
        }
    }
    let last = parts.last()?.trim_start_matches('@');
    if last == "self" {
        return p.model_at(path);
    }
    // An association of the receiver: `current_user.articles`.
    if parts.len() >= 2 {
        let owner = model_of(p, path, &parts[..parts.len() - 1].join("."));
        if let Some(a) = owner.and_then(|o| o.associations.iter().find(|a| a.name == last)) {
            return p.model(&a.class_name);
        }
    }
    // A variable named after a model: `article`, `@articles`, `line_item`.
    let class = camelize(&singularize(last));
    p.info
        .models
        .iter()
        .find(|m| m.name == class || m.name.ends_with(&format!("::{class}")))
}

fn attribute_items(m: &Model, style: &str) -> Vec<CompletionItem> {
    let mut out = Vec::new();
    let label = |n: &str| match style {
        "key" => (format!("{n}:"), Some(format!("{n}: "))),
        "symbol" => (format!(":{n}"), None),
        _ => (n.to_string(), None),
    };
    for c in &m.columns {
        let (l, apply) = label(&c.name);
        out.push(CompletionItem {
            label: l,
            kind: "attribute".into(),
            detail: format!("{} · {}", c.kind, m.name),
            apply,
        });
    }
    for a in &m.associations {
        if style == "key" && a.kind != "belongs_to" {
            continue;
        }
        let (l, apply) = label(&a.name);
        out.push(CompletionItem {
            label: l,
            kind: "association".into(),
            detail: format!("{} {}", a.kind, a.class_name),
            apply,
        });
    }
    out
}

/// The innermost unclosed call of a line: (receiver expression, method).
fn open_call(head: &str) -> Option<(String, String)> {
    let mut depth = 0;
    for (i, c) in head.char_indices().rev() {
        match c {
            ')' | ']' | '}' => depth += 1,
            '(' | '[' | '{' if depth > 0 => depth -= 1,
            '(' => {
                let (call, _) = trailing_call(&head[..i]);
                let (recv, method) = call.rsplit_once('.').unwrap_or(("", &call));
                return Some((recv.to_string(), method.to_string()));
            }
            _ => {}
        }
    }
    None
}

/// The model of a form builder variable: `form_with(model: @article) do |form|`,
/// `form_for @article do |f|`, or the folder of a `_form` partial.
fn form_model<'a>(
    p: &'a Project,
    path: &str,
    text: &str,
    at: usize,
    var: &str,
) -> Option<&'a Model> {
    let before = &text[..at];
    let needle = format!("|{var}|");
    if let Some(i) = before.rfind(&needle) {
        let line_start = before[..i].rfind('\n').map_or(0, |n| n + 1);
        let line = &before[line_start..i];
        if let Some(m) = line.find("model:") {
            let expr: String = line[m + 6..]
                .trim_start()
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '@' || *c == '.')
                .collect();
            if let Some(model) = model_of(p, path, &expr) {
                return Some(model);
            }
        }
        if let Some(m) = line.find("form_for").or_else(|| line.find("fields_for")) {
            let rest = line[m..].split_once([' ', '(']).map(|x| x.1).unwrap_or("");
            let expr: String = rest
                .trim_start()
                .trim_start_matches([':', '['])
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '@')
                .collect();
            if let Some(model) = model_of(p, path, &expr) {
                return Some(model);
            }
        }
    }
    let v = app::view(path)?;
    let last = v.dir.rsplit('/').next().unwrap_or(&v.dir);
    model_of(p, path, last)
}

fn code_items(
    p: &Project,
    path: &str,
    text: &str,
    at: usize,
    head: &str,
    typed: &str,
    symbol: bool,
) -> Option<Vec<CompletionItem>> {
    let head_t = head.trim_end();
    // turbo_stream.
    if head.ends_with("turbo_stream.") {
        return Some(
            hotwire::TURBO_STREAM_ACTIONS
                .iter()
                .map(|(a, d)| item(*a, "turbo", *d))
                .collect(),
        );
    }
    // `@article.` / `article.` / `Article.`: attributes and associations, scopes.
    if let Some(recv) = head.strip_suffix('.') {
        let (expr, _) = trailing_call(recv);
        let is_class = expr
            .rsplit('.')
            .next()
            .is_some_and(|s| s.starts_with(|c: char| c.is_uppercase()));
        let m = model_of(p, path, &expr)?;
        if is_class {
            return Some(
                m.scopes
                    .iter()
                    .map(|s| item(&s.name, "scope", format!("scope · {}", m.name)))
                    .collect(),
            );
        }
        return Some(attribute_items(m, "plain"));
    }
    // Inside a query or params call: `Article.where(ti`, `order(:ti`, `permit(:ti`.
    if let Some((recv, method)) = open_call(head) {
        if method == "permit" {
            let m = text[..at]
                .rfind("require(:")
                .map(|i| &text[i + 9..])
                .and_then(|r| {
                    model_of(
                        p,
                        path,
                        &r.chars()
                            .take_while(|c| c.is_alphanumeric() || *c == '_')
                            .collect::<String>(),
                    )
                })
                .or_else(|| {
                    let (name, _) = app::controller_name(path)?;
                    model_of(p, path, name.rsplit('/').next()?)
                })?;
            return Some(attribute_items(m, "symbol"));
        }
        let recv = if recv.is_empty() {
            "self".to_string()
        } else {
            recv
        };
        let recv = recv
            .strip_suffix(".where")
            .map(str::to_string)
            .unwrap_or(recv);
        if KEY_METHODS.contains(&method.as_str()) && !symbol {
            if let Some(m) = model_of(p, path, &recv) {
                return Some(attribute_items(m, "key"));
            }
        }
        if SYMBOL_METHODS.contains(&method.as_str())
            || (KEY_METHODS.contains(&method.as_str()) && symbol)
        {
            if let Some(m) = model_of(p, path, &recv) {
                return Some(attribute_items(m, "symbol"));
            }
        }
    }
    let (call, rest) = trailing_call(head_t);
    // validates :title, …
    if (symbol || head_t.ends_with(',')) && p.model_at(path).is_some() {
        let first = head_t.split_whitespace().next().unwrap_or("");
        if VALIDATIONS.contains(&first) {
            return Some(attribute_items(p.model_at(path)?, "symbol"));
        }
    }
    // form.text_field :title
    if symbol || head.ends_with(' ') || head.ends_with('(') {
        if let Some((var, method)) = call.rsplit_once('.') {
            if FORM_FIELDS.contains(&method) {
                let m = form_model(p, path, text, at, var)?;
                return Some(attribute_items(m, "symbol"));
            }
        }
    }
    let _ = rest;
    if symbol || typed.len() < 2 {
        return None;
    }
    // Route helpers and Turbo helpers.
    if head.ends_with('.') || head.ends_with("::") || head.ends_with('@') {
        return None;
    }
    let mut items: Vec<CompletionItem> = Vec::new();
    let mut seen = BTreeSet::new();
    for r in &p.info.routes {
        let Some(name) = &r.name else { continue };
        if !seen.insert(name.clone()) {
            continue;
        }
        let detail = format!("{} → {}", route_label(r), r.handler());
        items.push(item(format!("{name}_path"), "route", &detail));
        items.push(item(format!("{name}_url"), "route", &detail));
    }
    if is_erb(path) && "turbo".starts_with(&typed[..typed.len().min(5)]) {
        items.extend(
            hotwire::TURBO_HELPERS
                .iter()
                .map(|(n, d)| item(*n, "turbo", *d)),
        );
    }
    let lower = typed.to_ascii_lowercase();
    items.retain(|i| {
        let l = i.label.to_ascii_lowercase();
        l.starts_with(&lower) || l.contains(&format!("_{lower}"))
    });
    (!items.is_empty()).then_some(items)
}

// ---- Gutter marks, inspections and folds ----------------------------------------------------------

fn short(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

#[allow(clippy::too_many_arguments)]
fn push_problem(
    out: &mut Vec<Problem>,
    text: &str,
    idx: &LineIndex,
    s: usize,
    e: usize,
    message: String,
    severity: &str,
    code: &str,
) {
    let (line, start) = idx.position(s);
    let (_, end) = idx.position(e.max(s).min(text.len()));
    out.push(Problem {
        line,
        start,
        end: end.max(start + 1),
        message,
        severity: severity.into(),
        code: code.into(),
    });
}

/// Gutter marks, problems and folds of a file.
pub fn marks(p: &Project, path: &str, text: &str) -> FileMarks {
    let mut m = FileMarks::default();
    let idx = LineIndex::new(text);
    let line_of = |byte: usize| idx.position(byte).0;
    // A controller's actions: views and routes.
    if let Some((name, kind)) = app::controller_name(path) {
        let c = app::controller(path, text);
        for a in c.iter().flat_map(|c| c.actions.iter()) {
            let views: Vec<&app::View> = p
                .views
                .iter()
                .filter(|v| v.dir == name && v.name == a.name && !v.partial)
                .collect();
            if !views.is_empty() {
                m.gutter.push(GutterMark {
                    line: a.location.line,
                    kind: "view".into(),
                    tooltip: format!(
                        "Go to view: {}",
                        views
                            .iter()
                            .map(|v| short(&v.path))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    targets: views.iter().map(|v| Location::file(&v.path)).collect(),
                });
            }
            if kind == "controller" {
                let routes: Vec<&crate::Route> = p
                    .info
                    .routes
                    .iter()
                    .filter(|r| r.controller == name && r.action == a.name)
                    .collect();
                if !routes.is_empty() {
                    let mut labels: Vec<String> = routes.iter().map(|r| route_label(r)).collect();
                    labels.dedup();
                    m.gutter.push(GutterMark {
                        line: a.location.line,
                        kind: "route".into(),
                        tooltip: format!("Routes: {}", labels.join(", ")),
                        targets: routes.iter().map(|r| r.location.clone()).collect(),
                    });
                }
            }
        }
    }
    // A view: its controller action; a partial: the files that render it.
    if let Some(v) = app::view(path) {
        if !v.partial {
            if let Some(c) = p.controller(&v.dir) {
                let (target, label) = match c.action(&v.name) {
                    Some(a) => (a.location.clone(), format!("{}#{}", c.class, a.name)),
                    None => (c.location.clone(), c.class.clone()),
                };
                m.gutter.push(GutterMark {
                    line: 0,
                    kind: "controller".into(),
                    tooltip: format!("Go to {label}"),
                    targets: vec![target],
                });
            }
        } else {
            let mut targets = Vec::new();
            for (other, t) in p.files() {
                if other == path
                    || !(other.starts_with("app/views/") || other.starts_with("app/controllers/"))
                {
                    continue;
                }
                if !t.contains("render") {
                    continue;
                }
                let oidx = LineIndex::new(t);
                for r in app::render_refs(other, t) {
                    if r.kind == RenderKind::Partial
                        && app::resolve_render(other, &r, &p.views)
                            .iter()
                            .any(|f| f == path)
                    {
                        let (line, character) = oidx.position(r.start);
                        targets.push(Location {
                            path: other.to_string(),
                            line,
                            character,
                        });
                    }
                }
            }
            if !targets.is_empty() {
                m.gutter.push(GutterMark {
                    line: 0,
                    kind: "partial".into(),
                    tooltip: format!(
                        "Rendered in {} place{}",
                        targets.len(),
                        if targets.len() == 1 { "" } else { "s" }
                    ),
                    targets,
                });
            }
        }
    }
    // A model: its table and migrations; associations: their models.
    if let Some(model) = p.model_at(path) {
        if let Some(t) = p.info.tables.iter().find(|t| t.name == model.table) {
            let mut targets = vec![t.location.clone()];
            targets.extend(t.migrations.iter().filter(|l| **l != t.location).cloned());
            let cols: Vec<&str> = t.columns.iter().map(|c| c.name.as_str()).take(8).collect();
            m.gutter.push(GutterMark {
                line: model.location.line,
                kind: "table".into(),
                tooltip: format!(
                    "Table {}: {}{}",
                    t.name,
                    cols.join(", "),
                    if t.columns.len() > 8 { ", …" } else { "" }
                ),
                targets,
            });
        }
        for a in &model.associations {
            if let Some(other) = p.model(&a.class_name) {
                m.gutter.push(GutterMark {
                    line: a.location.line,
                    kind: "model".into(),
                    tooltip: format!("{} {}", a.kind, other.name),
                    targets: vec![other.location.clone()],
                });
            }
        }
    }
    // Routes: the actions they lead to.
    if is_routes_file(path) {
        let mut lines: Vec<u32> = p
            .info
            .routes
            .iter()
            .filter(|r| r.location.path == path)
            .map(|r| r.location.line)
            .collect();
        lines.dedup();
        for line in lines {
            let rs: Vec<&crate::Route> = p
                .info
                .routes
                .iter()
                .filter(|r| r.location.path == path && r.location.line == line)
                .collect();
            let mut targets: Vec<Location> = Vec::new();
            for r in &rs {
                if let Some(l) = &r.action_location {
                    if !targets.contains(l) {
                        targets.push(l.clone());
                    }
                }
            }
            if targets.is_empty() {
                continue;
            }
            let handlers: Vec<String> = rs.iter().map(|r| r.handler()).take(4).collect();
            m.gutter.push(GutterMark {
                line,
                kind: "action".into(),
                tooltip: format!(
                    "{}{}",
                    handlers.join(", "),
                    if rs.len() > 4 { ", …" } else { "" }
                ),
                targets,
            });
        }
    }
    // A Stimulus controller: the views that use it.
    if let Some(id) = hotwire::identifier(path) {
        let mut targets = Vec::new();
        for (other, t) in p.files() {
            if !other.starts_with("app/views/") || !t.contains(&id) {
                continue;
            }
            let oidx = LineIndex::new(t);
            for r in hotwire::stimulus_refs(t) {
                if r.kind == StimulusRefKind::Controller && r.identifier == id {
                    let (line, character) = oidx.position(r.start);
                    targets.push(Location {
                        path: other.to_string(),
                        line,
                        character,
                    });
                }
            }
        }
        let line = p.stimulus(&id).map_or(0, |c| c.location.line);
        if !targets.is_empty() {
            m.gutter.push(GutterMark {
                line,
                kind: "stimulus".into(),
                tooltip: format!(
                    "Used in {} place{}",
                    targets.len(),
                    if targets.len() == 1 { "" } else { "s" }
                ),
                targets,
            });
        }
    }
    let view_like = path.starts_with("app/views/") || path.ends_with(".html") || is_erb(path);
    // Views: Stimulus controllers per line, unknown controllers, actions and targets.
    if view_like {
        let mut marked = BTreeSet::new();
        let known = !p.info.stimulus.is_empty();
        for r in hotwire::stimulus_refs(text) {
            let c = p.stimulus(&r.identifier);
            match (&r.kind, c) {
                (StimulusRefKind::Controller, Some(c)) => {
                    let line = line_of(r.start);
                    if marked.insert(line) {
                        m.gutter.push(GutterMark {
                            line,
                            kind: "stimulus".into(),
                            tooltip: format!("Stimulus controller {}", c.identifier),
                            targets: vec![c.location.clone()],
                        });
                    }
                }
                (StimulusRefKind::Controller, None) if known => push_problem(
                    &mut m.problems,
                    text,
                    &idx,
                    r.start,
                    r.end,
                    format!("Unknown Stimulus controller '{}'", r.identifier),
                    "info",
                    "RailsStimulus",
                ),
                (StimulusRefKind::Method, Some(c))
                    if !c.methods.iter().any(|x| x.name == r.name) =>
                {
                    push_problem(
                        &mut m.problems,
                        text,
                        &idx,
                        r.start,
                        r.end,
                        format!(
                            "Unknown action '{}' of Stimulus controller '{}'",
                            r.name, c.identifier
                        ),
                        "warning",
                        "RailsStimulus",
                    )
                }
                (StimulusRefKind::Target, Some(c))
                    if !c.targets.iter().any(|x| x.name == r.name) =>
                {
                    push_problem(
                        &mut m.problems,
                        text,
                        &idx,
                        r.start,
                        r.end,
                        format!(
                            "Unknown target '{}' of Stimulus controller '{}'",
                            r.name, c.identifier
                        ),
                        "warning",
                        "RailsStimulus",
                    )
                }
                _ => {}
            }
        }
    }
    // Unresolved partials and templates.
    for r in app::render_refs(path, text) {
        if !r.literal
            || r.name.contains("#{")
            || !app::resolve_render(path, &r, &p.views).is_empty()
        {
            continue;
        }
        let what = match r.kind {
            RenderKind::Partial => "partial",
            RenderKind::Template => "template",
            RenderKind::Layout => "layout",
        };
        push_problem(
            &mut m.problems,
            text,
            &idx,
            r.start,
            r.end,
            format!("Cannot find {what} '{}'", r.name),
            "warning",
            "RailsMissingView",
        );
    }
    // Translations: missing keys and folds.
    let has_catalog = !p.catalog.locales.is_empty();
    for r in i18n::refs(path, text) {
        let Some(full) = r.full.clone() else { continue };
        if let Some(value) = p.catalog.text(&full) {
            m.folds.push(Fold {
                from: byte_to_utf16(text, r.fold_start),
                to: byte_to_utf16(text, r.fold_end),
                text: truncate(value, 60),
                key: full.clone(),
            });
            let missing = p.catalog.missing_locales(&full);
            if !missing.is_empty() {
                push_problem(
                    &mut m.problems,
                    text,
                    &idx,
                    r.start,
                    r.end,
                    format!("Translation '{full}' is missing for {}", missing.join(", ")),
                    "info",
                    "RailsI18nMissing",
                );
            }
        } else if has_catalog
            && !p.catalog.is_namespace(&full)
            && !Catalog::is_framework_key(&full)
            && !full.ends_with('.')
        {
            push_problem(
                &mut m.problems,
                text,
                &idx,
                r.start,
                r.end,
                format!("Missing translation '{full}'"),
                "warning",
                "RailsI18nMissing",
            );
        }
    }
    m.gutter.sort_by_key(|g| g.line);
    m
}

/// `Article` for `app/controllers/articles_controller.rb` and its views (used by the UI).
pub fn model_for_path(p: &Project, path: &str) -> Option<String> {
    if let Some(m) = p.model_at(path) {
        return Some(m.name.clone());
    }
    let dir = app::view_dir(path)?;
    let last = dir.rsplit('/').next()?;
    let class = camelize(&singularize(last));
    p.model(&class).map(|m| m.name.clone()).or_else(|| {
        p.info
            .models
            .iter()
            .find(|m| underscore(&m.name).ends_with(&singularize(last)))
            .map(|m| m.name.clone())
    })
}
