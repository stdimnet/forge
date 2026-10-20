//! The Structure tool window (Alt+7) of a Lua file: classes with their fields and methods,
//! module tables with their functions, top-level functions and variables.

use std::collections::HashSet;

use serde::Serialize;

use crate::model::*;
use crate::offsets::LineIndex;
use crate::project::{ProjectIndex, SymbolKind, Table};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StructureNode {
    pub name: String,
    pub kind: SymbolKind,
    pub detail: String,
    /// UTF-16 offset of the name.
    pub offset: usize,
    /// 0-based line.
    pub line: usize,
    /// UTF-16 column of the name.
    pub column: usize,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub deprecated: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<StructureNode>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileStructure {
    pub path: String,
    pub nodes: Vec<StructureNode>,
}

const MAX_NESTING: usize = 6;

struct Builder<'a> {
    index: &'a ProjectIndex,
    path: &'a str,
    file: &'a FileIndex,
    used: HashSet<usize>,
}

fn function_detail(d: &Decl) -> String {
    let mut s = format!("({})", d.params.join(", "));
    if let Some(doc) = d.doc.as_ref().filter(|doc| !doc.returns.is_empty()) {
        let rets: Vec<&str> = doc.returns.iter().map(|r| r.ty.as_str()).collect();
        s.push_str(&format!(": {}", rets.join(", ")));
    }
    s
}

impl Builder<'_> {
    fn node(&self, d: &Decl, kind: SymbolKind, detail: String) -> StructureNode {
        StructureNode {
            name: d.name.clone(),
            kind,
            detail,
            offset: d.pos.offset,
            line: d.pos.line,
            column: d.pos.column,
            deprecated: d.doc.as_ref().is_some_and(|doc| doc.deprecated.is_some()),
            children: Vec::new(),
        }
    }

    fn value_detail(&self, d: &Decl) -> String {
        if let Some(t) = d.annotated_type() {
            return format!(": {t}");
        }
        match &d.value {
            Value::Require(m) => format!("require(\"{m}\")"),
            Value::Literal(l) => format!(": {l}"),
            Value::Typed(t) => format!(": {t}"),
            _ => String::new(),
        }
    }

    /// Members of a table declared in this file, nested tables included.
    fn members(&mut self, table: &Table, depth: usize) -> Vec<StructureNode> {
        if depth > MAX_NESTING {
            return Vec::new();
        }
        let mut seen: HashSet<String> = HashSet::new();
        let mut out = Vec::new();
        for (i, d) in self.file.decls.iter().enumerate() {
            if !d.kind.is_field() || self.used.contains(&i) {
                continue;
            }
            let owner = d
                .owner
                .as_ref()
                .and_then(|o| self.index.resolve_value(self.path, o, 0));
            if owner.as_ref() != Some(table) {
                continue;
            }
            self.used.insert(i);
            if !seen.insert(d.name.clone()) {
                continue;
            }
            let node = match d.kind {
                DeclKind::Method => self.node(d, SymbolKind::Method, function_detail(d)),
                DeclKind::FieldFunction => self.node(d, SymbolKind::Function, function_detail(d)),
                _ => {
                    let mut n = self.node(d, SymbolKind::Field, self.value_detail(d));
                    if let Value::Table(id) = d.value {
                        let inner = Table::Local(self.path.to_string(), id);
                        n.children = self.members(&inner, depth + 1);
                    }
                    n
                }
            };
            out.push(node);
        }
        out
    }
}

/// The structure of an indexed file.
pub fn file_structure(index: &ProjectIndex, path: &str) -> Option<FileStructure> {
    let file = index.file(path)?;
    let text = index.text(path).unwrap_or("");
    let lines = LineIndex::new(text);
    let mut b = Builder {
        index,
        path,
        file,
        used: HashSet::new(),
    };
    let mut nodes = Vec::new();

    // Classes: annotated fields, then what the file declares on the class table.
    for class in &file.classes {
        let mut children: Vec<StructureNode> = class
            .ann
            .fields
            .iter()
            .map(|f| {
                let pos = lines.pos(text, f.offset);
                StructureNode {
                    name: f.name.clone(),
                    kind: SymbolKind::Field,
                    detail: format!(": {}", f.ty),
                    offset: pos.offset,
                    line: pos.line,
                    column: pos.column,
                    deprecated: false,
                    children: Vec::new(),
                }
            })
            .collect();
        let table = Table::Class(class.ann.name.clone());
        let members = b.members(&table, 0);
        children.extend(
            members
                .into_iter()
                .filter(|m| !class.ann.fields.iter().any(|f| f.name == m.name)),
        );
        if let Some(d) = class.decl {
            b.used.insert(d);
        }
        nodes.push(StructureNode {
            name: class.ann.name.clone(),
            kind: SymbolKind::Class,
            detail: if class.ann.parents.is_empty() {
                String::new()
            } else {
                format!(": {}", class.ann.parents.join(", "))
            },
            offset: class.pos.offset,
            line: class.pos.line,
            column: class.pos.column,
            deprecated: false,
            children,
        });
    }
    for alias in &file.aliases {
        nodes.push(StructureNode {
            name: alias.ann.name.clone(),
            kind: SymbolKind::Alias,
            detail: format!(" = {}", alias.ann.ty),
            offset: alias.pos.offset,
            line: alias.pos.line,
            column: alias.pos.column,
            deprecated: false,
            children: Vec::new(),
        });
    }

    // Top-level declarations in source order.
    let returned = match &file.returns {
        Some(Value::Local(i)) => Some(*i),
        _ => None,
    };
    for (i, d) in file.decls.iter().enumerate() {
        if !d.top_level || b.used.contains(&i) {
            continue;
        }
        match d.kind {
            DeclKind::LocalFunction | DeclKind::GlobalFunction => {
                b.used.insert(i);
                nodes.push(b.node(d, SymbolKind::Function, function_detail(d)));
            }
            DeclKind::Local | DeclKind::Global => {
                if d.kind == DeclKind::Global
                    && nodes
                        .iter()
                        .any(|n| n.name == d.name && n.line < d.pos.line)
                {
                    continue;
                }
                b.used.insert(i);
                let table = match d.value {
                    Value::Table(_) | Value::Local(_) | Value::Global(_) => index
                        .resolve_value(path, &Value::Local(i), 0)
                        .or_else(|| match &d.value {
                            Value::Global(g) => Some(Table::Global(g.clone())),
                            _ => None,
                        }),
                    _ => None,
                };
                let table = if d.kind == DeclKind::Global {
                    match d.value {
                        Value::Table(id) => Some(Table::Local(path.to_string(), id)),
                        _ => table,
                    }
                } else {
                    table
                };
                let children = table.map(|t| b.members(&t, 0)).unwrap_or_default();
                let is_table = matches!(d.value, Value::Table(_));
                let kind = if (is_table || !children.is_empty())
                    && (returned == Some(i) || d.kind == DeclKind::Global || !children.is_empty())
                {
                    SymbolKind::Module
                } else {
                    SymbolKind::Variable
                };
                let mut n = b.node(d, kind, b.value_detail(d));
                n.children = children;
                nodes.push(n);
            }
            _ => {}
        }
    }

    // Functions on tables this file does not declare (`function love.draw()`), grouped by
    // their table expression.
    let mut groups: Vec<(String, StructureNode)> = Vec::new();
    for (i, d) in file.decls.iter().enumerate() {
        if !d.top_level || b.used.contains(&i) || !d.kind.is_field() || d.container.is_empty() {
            continue;
        }
        if matches!(d.owner, Some(Value::Table(_))) {
            continue;
        }
        let kind = match d.kind {
            DeclKind::Method => SymbolKind::Method,
            DeclKind::FieldFunction => SymbolKind::Function,
            _ => SymbolKind::Field,
        };
        let detail = if d.kind.is_function() {
            function_detail(d)
        } else {
            b.value_detail(d)
        };
        let child = b.node(d, kind, detail);
        match groups.iter_mut().find(|(c, _)| *c == d.container) {
            Some((_, g)) => {
                if !g.children.iter().any(|c| c.name == child.name) {
                    g.children.push(child);
                }
            }
            None => groups.push((
                d.container.clone(),
                StructureNode {
                    name: d.container.clone(),
                    kind: SymbolKind::Module,
                    detail: String::new(),
                    offset: d.pos.offset,
                    line: d.pos.line,
                    column: d.pos.column,
                    deprecated: false,
                    children: vec![child],
                },
            )),
        }
    }
    nodes.extend(groups.into_iter().map(|(_, n)| n));
    nodes.sort_by_key(|n| n.offset);
    Some(FileStructure {
        path: path.to_string(),
        nodes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(nodes: &[StructureNode], depth: usize, out: &mut Vec<String>) {
        for n in nodes {
            out.push(format!(
                "{}{:?} {}{}",
                "  ".repeat(depth),
                n.kind,
                n.name,
                n.detail
            ));
            tree(&n.children, depth + 1, out);
        }
    }

    fn structure(src: &str) -> Vec<String> {
        let mut p = ProjectIndex::new(&[]);
        p.set_file("a.lua", src);
        let s = file_structure(&p, "a.lua").unwrap();
        let mut out = Vec::new();
        tree(&s.nodes, 0, &mut out);
        out
    }

    #[test]
    fn classes_modules_functions_and_variables() {
        let src = r#"local json = require("json")
local SPEED = 120

---@class Player : Entity
---@field name string
local Player = {}

---@param dt number
---@return boolean
function Player:update(dt) self.timer = 0 end
function Player.new() end

local M = { version = "1.0", conf = { width = 800 } }
function M.start() end

local function helper(a, b) end

function love.load() end
function love.draw() end

return M
"#;
        assert_eq!(
            structure(src),
            vec![
                "Variable jsonrequire(\"json\")",
                "Variable SPEED: number",
                "Class Player: Entity",
                "  Field name: string",
                "  Method update(dt): boolean",
                "  Field timer: number",
                "  Function new()",
                "Module M",
                "  Field version: string",
                "  Field conf",
                "    Field width: number",
                "  Function start()",
                "Function helper(a, b)",
                "Module love",
                "  Function load()",
                "  Function draw()",
            ]
        );
    }
}
