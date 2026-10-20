//! The project index: every Lua file of a workspace, with lightweight type resolution
//! across files (locals, globals, table fields, `require`d modules, `---@class` /
//! `---@type` / `---@param` / `---@return` annotations), Go to Declaration and symbol
//! search for Search Everywhere.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::annotations::primary_type;
use crate::model::*;
use crate::offsets::{byte_to_utf16, utf16_to_byte};
use crate::parser::parse;
use crate::require;

/// Depth limit of value resolution (chains like `a.b.c` through requires and calls).
const MAX_DEPTH: usize = 12;

/// A table a value resolves to: where its fields are declared.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Table {
    /// A table constructor in a file.
    Local(String, usize),
    /// A global table (or a nested one, `love.graphics`) without a constructor we know.
    Global(String),
    /// A `---@class`.
    Class(String),
}

/// A place in a file, for navigation. Offsets and columns are UTF-16.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Location {
    pub path: String,
    pub offset: usize,
    pub line: usize,
    pub column: usize,
}

impl Location {
    pub fn at(path: &str, pos: Pos) -> Self {
        Location {
            path: path.to_string(),
            offset: pos.offset,
            line: pos.line,
            column: pos.column,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SearchKind {
    /// Classes, aliases and modules (Go to Class).
    #[default]
    Class,
    /// Everything (Go to Symbol).
    Symbol,
}

/// Kind of a symbol in search results and the structure view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SymbolKind {
    Class,
    Alias,
    Module,
    Function,
    Method,
    Field,
    Variable,
}

/// A Search Everywhere hit, pre-ranked.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolHit {
    pub name: String,
    pub kind: SymbolKind,
    /// Owner (`Player`, `M`) or empty.
    pub container: String,
    pub path: String,
    pub offset: usize,
    pub line: usize,
    pub column: usize,
    pub score: i32,
    pub positions: Vec<usize>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub deprecated: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexStats {
    pub files: usize,
    pub functions: usize,
    pub classes: usize,
}

/// A declaration somewhere in the project.
#[derive(Debug, Clone, Copy)]
pub struct DeclRef<'a> {
    pub path: &'a str,
    pub index: usize,
    pub decl: &'a Decl,
}

#[derive(Debug, Clone, Default)]
pub struct ProjectIndex {
    files: BTreeMap<String, FileIndex>,
    texts: BTreeMap<String, String>,
    templates: Vec<String>,
    name: String,
}

impl ProjectIndex {
    /// An empty index; `require_paths` are extra `package.path`-like templates.
    pub fn new(require_paths: &[String]) -> Self {
        ProjectIndex {
            templates: require::templates(require_paths),
            ..Self::default()
        }
    }

    /// Name of the project (shown for LÖVE run configurations of the root game).
    pub fn set_name(&mut self, name: &str) {
        self.name = name.to_string();
    }

    pub fn project_name(&self) -> &str {
        if self.name.is_empty() {
            "game"
        } else {
            &self.name
        }
    }

    pub fn set_require_paths(&mut self, require_paths: &[String]) {
        self.templates = require::templates(require_paths);
    }

    pub fn templates(&self) -> &[String] {
        &self.templates
    }

    /// Add or replace a file. Paths are workspace relative with `/` separators.
    pub fn set_file(&mut self, path: &str, text: &str) {
        if self.texts.get(path).is_some_and(|t| t == text) {
            return;
        }
        self.files.insert(path.to_string(), parse(path, text));
        self.texts.insert(path.to_string(), text.to_string());
    }

    /// Re-parse `path` when the editor sends newer content.
    pub fn update(&mut self, path: &str, content: Option<&str>) {
        if let Some(text) = content {
            self.set_file(path, text);
        }
    }

    pub fn remove_file(&mut self, path: &str) {
        self.files.remove(path);
        self.texts.remove(path);
    }

    pub fn clear(&mut self) {
        self.files.clear();
        self.texts.clear();
    }

    pub fn file(&self, path: &str) -> Option<&FileIndex> {
        self.files.get(path)
    }

    pub fn text(&self, path: &str) -> Option<&str> {
        self.texts.get(path).map(String::as_str)
    }

    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.files.keys().map(String::as_str)
    }

    pub fn stats(&self) -> IndexStats {
        IndexStats {
            files: self.files.len(),
            functions: self
                .files
                .values()
                .flat_map(|f| &f.decls)
                .filter(|d| d.kind.is_function())
                .count(),
            classes: self.files.values().map(|f| f.classes.len()).sum(),
        }
    }

    /// The file a module name resolves to.
    pub fn resolve_require(&self, module: &str) -> Option<String> {
        require::resolve(module, &self.templates, |p| self.files.contains_key(p))
    }

    /// The module name of a file, as `require` would take it.
    pub fn module_name(&self, path: &str) -> Option<String> {
        require::module_name(path, &self.templates)
    }

    fn decls(&self) -> impl Iterator<Item = DeclRef<'_>> {
        self.files.iter().flat_map(|(path, f)| {
            f.decls
                .iter()
                .enumerate()
                .map(move |(index, decl)| DeclRef { path, index, decl })
        })
    }

    /// Declarations (assignments) of a global variable or function.
    pub fn search_globals(&self, name: &str) -> Vec<DeclRef<'_>> {
        self.decls()
            .filter(|d| {
                d.decl.name == name
                    && matches!(d.decl.kind, DeclKind::Global | DeclKind::GlobalFunction)
            })
            .collect()
    }

    pub fn decl(&self, path: &str, index: usize) -> Option<DeclRef<'_>> {
        let (path, f) = self.files.get_key_value(path)?;
        f.decls.get(index).map(|decl| DeclRef { path, index, decl })
    }

    pub fn class(&self, name: &str) -> Option<(&str, &ClassDef)> {
        self.files.iter().find_map(|(path, f)| {
            f.classes
                .iter()
                .find(|c| c.ann.name == name)
                .map(|c| (path.as_str(), c))
        })
    }

    pub fn alias(&self, name: &str) -> Option<(&str, &AliasDef)> {
        self.files.iter().find_map(|(path, f)| {
            f.aliases
                .iter()
                .find(|a| a.ann.name == name)
                .map(|a| (path.as_str(), a))
        })
    }

    // ---- Resolution -----------------------------------------------------------------

    /// The table of a type name (`Player`, `Player|nil`, an alias of a class).
    pub fn table_of_type(&self, ty: &str, depth: usize) -> Option<Table> {
        if depth > MAX_DEPTH {
            return None;
        }
        let name = primary_type(ty);
        if self.class(name).is_some() {
            return Some(Table::Class(name.to_string()));
        }
        let (_, alias) = self.alias(name)?;
        self.table_of_type(&alias.ann.ty, depth + 1)
    }

    /// The table a value in `path` evaluates to.
    pub fn resolve_value(&self, path: &str, value: &Value, depth: usize) -> Option<Table> {
        if depth > MAX_DEPTH {
            return None;
        }
        let file = self.files.get(path)?;
        match value {
            Value::Table(id) => {
                let class = file
                    .decls
                    .iter()
                    .find(|d| d.value == Value::Table(*id) && d.class.is_some())
                    .and_then(|d| d.class.clone());
                Some(match class {
                    Some(c) => Table::Class(c),
                    None => Table::Local(path.to_string(), *id),
                })
            }
            Value::Local(i) => {
                let d = file.decls.get(*i)?;
                self.resolve_decl(path, d, depth + 1)
            }
            Value::Global(name) => self.resolve_global(name, depth + 1),
            Value::Field(base, name) => {
                let table = self.resolve_value(path, base, depth + 1)?;
                for m in self.members(&table, Some(name), depth + 1) {
                    if let Some(t) = self.resolve_decl(m.path, m.decl, depth + 1) {
                        return Some(t);
                    }
                }
                if let Some(ty) = self.class_field_type(&table, name, depth + 1) {
                    return self.table_of_type(&ty, depth + 1);
                }
                match table {
                    Table::Global(g) => Some(Table::Global(format!("{g}.{name}"))),
                    _ => None,
                }
            }
            Value::Require(module) => {
                let target = self.resolve_require(module)?;
                let returns = self.files.get(&target)?.returns.clone()?;
                self.resolve_value(&target, &returns, depth + 1)
            }
            Value::Call(callee) => {
                if let Some(f) = self.function_of(path, callee, depth + 1) {
                    if let Some(ret) = f.decl.doc.as_ref().and_then(|d| d.returns.first()) {
                        return self.table_of_type(&ret.ty, depth + 1);
                    }
                }
                // `Class.new()` / `Class:new()` without annotations returns an instance.
                match callee.as_ref() {
                    Value::Field(base, name) if name == "new" || name == "create" => {
                        self.resolve_value(path, base, depth + 1)
                    }
                    _ => None,
                }
            }
            Value::Typed(ty) => self.table_of_type(ty, depth + 1),
            Value::Function | Value::Literal(_) | Value::Unknown => None,
        }
    }

    fn resolve_decl(&self, path: &str, d: &Decl, depth: usize) -> Option<Table> {
        if let Some(c) = &d.class {
            return Some(Table::Class(c.clone()));
        }
        if let Some(ty) = d.annotated_type() {
            if let Some(t) = self.table_of_type(ty, depth + 1) {
                return Some(t);
            }
        }
        self.resolve_value(path, &d.value, depth + 1)
    }

    fn resolve_global(&self, name: &str, depth: usize) -> Option<Table> {
        let mut nominal = true;
        for d in self.decls().filter(|d| {
            d.decl.name == name
                && matches!(d.decl.kind, DeclKind::Global | DeclKind::GlobalFunction)
        }) {
            if d.decl.kind == DeclKind::GlobalFunction {
                nominal = false;
                continue;
            }
            if let Some(t) = self.resolve_decl(d.path, d.decl, depth + 1) {
                return Some(t);
            }
        }
        if self.class(name).is_some() {
            return Some(Table::Class(name.to_string()));
        }
        nominal.then(|| Table::Global(name.to_string()))
    }

    /// The function declaration a callee value refers to.
    fn function_of(&self, path: &str, callee: &Value, depth: usize) -> Option<DeclRef<'_>> {
        match callee {
            Value::Local(i) => self.decl(path, *i).filter(|d| d.decl.kind.is_function()),
            Value::Global(name) => self
                .decls()
                .find(|d| d.decl.name == *name && d.decl.kind == DeclKind::GlobalFunction),
            Value::Field(base, name) => {
                let table = self.resolve_value(path, base, depth + 1)?;
                self.members(&table, Some(name), depth + 1)
                    .into_iter()
                    .find(|m| m.decl.kind.is_function())
            }
            _ => None,
        }
    }

    /// `---@field name type` of a class or its parents.
    fn class_field_type(&self, table: &Table, name: &str, depth: usize) -> Option<String> {
        let Table::Class(c) = table else { return None };
        let (_, class) = self.class(c)?;
        if let Some(f) = class.ann.fields.iter().find(|f| f.name == name) {
            return Some(f.ty.clone());
        }
        if depth > MAX_DEPTH {
            return None;
        }
        class.ann.parents.iter().find_map(|p| {
            self.table_of_type(p, depth + 1)
                .and_then(|t| self.class_field_type(&t, name, depth + 1))
        })
    }

    /// Field declarations of a table (optionally only those named `name`), including the
    /// members of parent classes.
    pub fn members(&self, table: &Table, name: Option<&str>, depth: usize) -> Vec<DeclRef<'_>> {
        if depth > MAX_DEPTH {
            return Vec::new();
        }
        let mut out: Vec<DeclRef<'_>> = self
            .decls()
            .filter(|d| d.decl.kind.is_field() && name.is_none_or(|n| d.decl.name == n))
            .filter(|d| {
                d.decl
                    .owner
                    .as_ref()
                    .and_then(|o| self.resolve_value(d.path, o, depth + 1))
                    .as_ref()
                    == Some(table)
            })
            .collect();
        if let Table::Class(c) = table {
            if let Some((_, class)) = self.class(c) {
                for p in &class.ann.parents {
                    if let Some(t) = self.table_of_type(p, depth + 1) {
                        if &t != table {
                            out.extend(self.members(&t, name, depth + 1));
                        }
                    }
                }
            }
        }
        out
    }

    // ---- Navigation -----------------------------------------------------------------

    /// Go to Declaration at a UTF-16 offset of `path`.
    pub fn goto(&self, path: &str, offset: usize) -> Vec<Location> {
        let (Some(file), Some(text)) = (self.files.get(path), self.texts.get(path)) else {
            return Vec::new();
        };
        let byte = utf16_to_byte(text, offset);
        if let Some(req) = file.require_at(byte) {
            return self
                .resolve_require(&req.module)
                .map(|p| {
                    vec![Location {
                        path: p,
                        offset: 0,
                        line: 0,
                        column: 0,
                    }]
                })
                .unwrap_or_default();
        }
        if file.in_doc_comment(byte) {
            let word = word_at(text, byte);
            return self.type_locations(word);
        }
        let Some(r) = file.ref_at(byte) else {
            return Vec::new();
        };
        match &r.target {
            Target::Local(i) => {
                let Some(d) = file.decls.get(*i) else {
                    return Vec::new();
                };
                // On the declaration itself: nothing to go to.
                if d.start == r.start {
                    return Vec::new();
                }
                vec![Location::at(path, d.pos)]
            }
            Target::Global(name) => {
                let mut out: Vec<Location> = self
                    .decls()
                    .filter(|d| {
                        d.decl.name == *name
                            && matches!(d.decl.kind, DeclKind::Global | DeclKind::GlobalFunction)
                    })
                    .filter(|d| !(d.path == path && d.decl.start == r.start))
                    .map(|d| Location::at(d.path, d.decl.pos))
                    .collect();
                if out.is_empty() {
                    out = self.type_locations(name);
                }
                out
            }
            Target::Field(base, name) => {
                let Some(table) = self.resolve_value(path, base, 0) else {
                    return Vec::new();
                };
                let mut out: Vec<Location> = self
                    .members(&table, Some(name), 0)
                    .into_iter()
                    .filter(|d| !(d.path == path && d.decl.start == r.start))
                    .map(|d| Location::at(d.path, d.decl.pos))
                    .collect();
                if let Some(loc) = self.class_field_location(&table, name) {
                    out.insert(0, loc);
                }
                out
            }
        }
    }

    fn class_field_location(&self, table: &Table, name: &str) -> Option<Location> {
        self.class_field_location_in(table, name, 0)
    }

    fn class_field_location_in(&self, table: &Table, name: &str, depth: usize) -> Option<Location> {
        let Table::Class(c) = table else { return None };
        let (path, class) = self.class(c)?;
        if let Some(field) = class.ann.fields.iter().find(|f| f.name == name) {
            let text = self.texts.get(path)?;
            let pos = crate::offsets::LineIndex::new(text).pos(text, field.offset);
            return Some(Location::at(path, pos));
        }
        if depth > MAX_DEPTH {
            return None;
        }
        class.ann.parents.iter().find_map(|p| {
            self.table_of_type(p, depth + 1)
                .and_then(|t| self.class_field_location_in(&t, name, depth + 1))
        })
    }

    /// Where a `---@class` or `---@alias` of that name is declared.
    pub fn type_locations(&self, name: &str) -> Vec<Location> {
        if name.is_empty() {
            return Vec::new();
        }
        if let Some((path, c)) = self.class(name) {
            return vec![Location::at(path, c.pos)];
        }
        if let Some((path, a)) = self.alias(name) {
            return vec![Location::at(path, a.pos)];
        }
        Vec::new()
    }

    // ---- Search ---------------------------------------------------------------------

    /// Classes (Go to Class) or every symbol (Go to Symbol) matching `query`.
    pub fn search(&self, query: &str, kind: SearchKind, limit: usize) -> Vec<SymbolHit> {
        let mut hits = Vec::new();
        let mut push = |name: &str,
                        kind: SymbolKind,
                        container: &str,
                        path: &str,
                        pos: Pos,
                        deprecated: bool| {
            if let Some(m) =
                forge_fuzzy::fuzzy_match_with(query, name, forge_fuzzy::MatchOptions::STRICT)
            {
                hits.push(SymbolHit {
                    name: name.to_string(),
                    kind,
                    container: container.to_string(),
                    path: path.to_string(),
                    offset: pos.offset,
                    line: pos.line,
                    column: pos.column,
                    score: m.score,
                    positions: m.positions,
                    deprecated,
                });
            }
        };
        for (path, f) in &self.files {
            for c in &f.classes {
                push(&c.ann.name, SymbolKind::Class, "", path, c.pos, false);
            }
            for a in &f.aliases {
                push(&a.ann.name, SymbolKind::Alias, "", path, a.pos, false);
            }
            // The table a module returns.
            if let Some(Value::Local(i)) = &f.returns {
                if let Some(d) = f.decls.get(*i).filter(|d| d.class.is_none()) {
                    let module = self.module_name(path).unwrap_or_default();
                    push(&d.name, SymbolKind::Module, &module, path, d.pos, false);
                }
            }
            if kind == SearchKind::Class {
                continue;
            }
            for c in &f.classes {
                let text = self.texts.get(path).map(String::as_str).unwrap_or("");
                let lines = crate::offsets::LineIndex::new(text);
                for field in &c.ann.fields {
                    push(
                        &field.name,
                        SymbolKind::Field,
                        &c.ann.name,
                        path,
                        lines.pos(text, field.offset),
                        false,
                    );
                }
            }
            for d in &f.decls {
                let deprecated = d.doc.as_ref().is_some_and(|doc| doc.deprecated.is_some());
                let kind = match d.kind {
                    DeclKind::Method => SymbolKind::Method,
                    DeclKind::FieldFunction => SymbolKind::Function,
                    DeclKind::LocalFunction | DeclKind::GlobalFunction if d.top_level => {
                        SymbolKind::Function
                    }
                    DeclKind::Global if d.top_level => SymbolKind::Variable,
                    DeclKind::Field if d.top_level && !d.container.is_empty() => SymbolKind::Field,
                    _ => continue,
                };
                push(&d.name, kind, &d.container, path, d.pos, deprecated);
            }
        }
        hits.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then(a.name.len().cmp(&b.name.len()))
                .then(a.name.cmp(&b.name))
                .then(a.path.cmp(&b.path))
        });
        hits.truncate(limit);
        hits
    }
}

/// The identifier around a byte offset.
pub fn word_at(text: &str, byte: usize) -> &str {
    let is_word = |c: char| c.is_alphanumeric() || c == '_' || c == '.';
    let byte = byte.min(text.len());
    let start = text[..byte]
        .char_indices()
        .rev()
        .find(|(_, c)| !is_word(*c))
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(0);
    let end = text[byte..]
        .char_indices()
        .find(|(_, c)| !is_word(*c))
        .map(|(i, _)| byte + i)
        .unwrap_or(text.len());
    text[start..end].trim_matches('.')
}

/// UTF-16 offset of a byte offset in the indexed text of `path`.
pub fn utf16(index: &ProjectIndex, path: &str, byte: usize) -> usize {
    index
        .text(path)
        .map(|t| byte_to_utf16(t, byte))
        .unwrap_or(byte)
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn project(files: &[(&str, &str)]) -> ProjectIndex {
        let mut p = ProjectIndex::new(&[]);
        for (path, text) in files {
            p.set_file(path, text);
        }
        p
    }

    fn at(text: &str, needle: &str, nth: usize) -> usize {
        text.match_indices(needle)
            .nth(nth)
            .map(|(i, _)| i + 1)
            .unwrap()
    }

    const VECTOR: &str = r#"---@class Vec2
---@field x number Horizontal
---@field y number
local Vec2 = {}
Vec2.__index = Vec2

--- New vector.
---@param x number
---@param y number
---@return Vec2
function Vec2.new(x, y) return setmetatable({ x = x, y = y }, Vec2) end

function Vec2:len() return math.sqrt(self.x * self.x + self.y * self.y) end

return Vec2
"#;

    const MAIN: &str = r#"local Vec2 = require("lib.vector")
local util = require "util"

local p = Vec2.new(1, 2)
print(p:len(), p.x)
util.clamp(1, 2, 3)
Game = { score = 0 }
function Game.reset() Game.score = 0 end
Game.reset()
"#;

    const UTIL: &str = "local M = {}\nfunction M.clamp(v, lo, hi) return v end\nreturn M\n";

    #[test]
    fn goto_through_require_and_annotations() {
        let p = project(&[
            ("lib/vector.lua", VECTOR),
            ("main.lua", MAIN),
            ("util.lua", UTIL),
        ]);
        // The require string opens the module.
        let g = p.goto("main.lua", at(MAIN, "\"lib.vector\"", 0) + 2);
        assert_eq!(g[0].path, "lib/vector.lua");
        // `Vec2.new` goes to the function in the module.
        let g = p.goto("main.lua", at(MAIN, ".new", 0) + 1);
        assert_eq!((g[0].path.as_str(), g[0].line), ("lib/vector.lua", 10));
        // `p:len()`: p is a Vec2 by `---@return Vec2`.
        let g = p.goto("main.lua", at(MAIN, ":len", 0) + 1);
        assert_eq!((g[0].path.as_str(), g[0].line), ("lib/vector.lua", 12));
        // `p.x` goes to the `---@field` first.
        let g = p.goto("main.lua", at(MAIN, "p.x", 0) + 2);
        assert_eq!((g[0].path.as_str(), g[0].line), ("lib/vector.lua", 1));
        // A module without annotations.
        let g = p.goto("main.lua", at(MAIN, ".clamp", 0) + 1);
        assert_eq!((g[0].path.as_str(), g[0].line), ("util.lua", 1));
        // Locals.
        let g = p.goto("main.lua", at(MAIN, "print(p", 0) + 6);
        assert_eq!((g[0].path.as_str(), g[0].line), ("main.lua", 3));
    }

    #[test]
    fn goto_globals_and_types_in_comments() {
        let p = project(&[("lib/vector.lua", VECTOR), ("main.lua", MAIN)]);
        let g = p.goto("main.lua", at(MAIN, "\nGame.reset()", 0) + 6);
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].line, 7);
        let g = p.goto("main.lua", at(MAIN, "\nGame.reset()", 0));
        assert_eq!(g[0].line, 6);
        let g = p.goto("lib/vector.lua", at(VECTOR, "@return Vec2", 0) + 9);
        assert_eq!((g[0].line, g[0].column), (0, 10));
    }

    #[test]
    fn search_classes_and_symbols() {
        let p = project(&[
            ("lib/vector.lua", VECTOR),
            ("main.lua", MAIN),
            ("util.lua", UTIL),
        ]);
        let classes = p.search("vec", SearchKind::Class, 10);
        assert_eq!(classes[0].name, "Vec2");
        assert_eq!(classes[0].kind, SymbolKind::Class);
        assert!(p
            .search("M", SearchKind::Class, 10)
            .iter()
            .any(|h| h.kind == SymbolKind::Module && h.container == "util"));
        let symbols = p.search("cla", SearchKind::Symbol, 10);
        assert_eq!(symbols[0].name, "clamp");
        assert_eq!(symbols[0].container, "M");
        assert!(p
            .search("len", SearchKind::Symbol, 10)
            .iter()
            .any(|h| h.kind == SymbolKind::Method && h.container == "Vec2"));
    }

    #[test]
    fn class_inheritance_and_alias_types() {
        let base = "---@class Entity\n---@field id integer\nlocal Entity = {}\nfunction Entity:destroy() end\nreturn Entity\n";
        let player = "---@class Player : Entity\nlocal Player = {}\n---@alias Hero Player\n---@type Hero\nlocal hero = nil\nhero:destroy()\nprint(hero.id)\n";
        let p = project(&[("entity.lua", base), ("player.lua", player)]);
        let g = p.goto("player.lua", at(player, ":destroy", 0) + 1);
        assert_eq!((g[0].path.as_str(), g[0].line), ("entity.lua", 3));
        let g = p.goto("player.lua", at(player, ".id", 0) + 1);
        assert_eq!((g[0].path.as_str(), g[0].line), ("entity.lua", 1));
    }

    #[test]
    fn word_under_caret() {
        assert_eq!(word_at("---@param p Player|nil", 14), "Player");
        assert_eq!(word_at("x", 1), "x");
    }

    #[test]
    fn stats_count_files_functions_classes() {
        let p = project(&[("lib/vector.lua", VECTOR), ("util.lua", UTIL)]);
        let s = p.stats();
        assert_eq!((s.files, s.functions, s.classes), (2, 3, 1));
    }
}
