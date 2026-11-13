//! What the parser extracts from one Lua file.

use serde::Serialize;

use crate::annotations::{AliasAnn, ClassAnn, DocBlock};

/// A position in a file: UTF-16 offset plus 0-based line and UTF-16 column.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Pos {
    pub offset: usize,
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DeclKind {
    /// `local x`.
    Local,
    /// `local function f`.
    LocalFunction,
    /// A function parameter (also the implicit `self` of `:` methods).
    Param,
    /// `for i = …` / `for k, v in …`.
    ForVar,
    /// `X = …` where `X` is not a local.
    Global,
    /// `function f()` where `f` is not a local.
    GlobalFunction,
    /// `t.x = …` or `{ x = … }`.
    Field,
    /// `function t.f()` or `t.f = function() end`.
    FieldFunction,
    /// `function t:m()`.
    Method,
}

impl DeclKind {
    pub fn is_function(self) -> bool {
        matches!(
            self,
            DeclKind::LocalFunction
                | DeclKind::GlobalFunction
                | DeclKind::FieldFunction
                | DeclKind::Method
        )
    }

    pub fn is_field(self) -> bool {
        matches!(
            self,
            DeclKind::Field | DeclKind::FieldFunction | DeclKind::Method
        )
    }

    pub fn is_local(self) -> bool {
        matches!(
            self,
            DeclKind::Local | DeclKind::LocalFunction | DeclKind::Param | DeclKind::ForVar
        )
    }
}

/// What an expression evaluates to, as far as a structural parser can tell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Unknown,
    /// A table constructor, by id within its file.
    Table(usize),
    /// A function expression or declaration.
    Function,
    /// `require("a.b")`.
    Require(String),
    /// The value of a local declaration (index into [`FileIndex::decls`]).
    Local(usize),
    /// The value of a global variable.
    Global(String),
    /// `base.name`.
    Field(Box<Value>, String),
    /// The result of calling a function.
    Call(Box<Value>),
    /// A value of an annotated type (`---@type T`, `---@param p T`).
    Typed(String),
    /// A literal: `string`, `number`, `boolean` or `nil`.
    Literal(&'static str),
}

/// A declaration: local, parameter, global, field or function.
#[derive(Debug, Clone)]
pub struct Decl {
    pub name: String,
    pub kind: DeclKind,
    /// Byte range of the name.
    pub start: usize,
    pub end: usize,
    pub pos: Pos,
    /// For locals: byte range where the name is visible.
    pub scope: (usize, usize),
    /// For fields: the table they belong to.
    pub owner: Option<Value>,
    /// Source text of the owner (`M`, `Player`, `love.graphics`).
    pub container: String,
    pub value: Value,
    /// Parameter names of functions.
    pub params: Vec<String>,
    pub doc: Option<DocBlock>,
    /// `---@class` bound to this declaration (`local Player = {}`).
    pub class: Option<String>,
    /// Declared outside any function body.
    pub top_level: bool,
    /// For `local` declarations with `<const>` / `<close>`.
    pub attrib: Option<String>,
}

impl Decl {
    /// `---@type` of the declaration.
    pub fn annotated_type(&self) -> Option<&str> {
        self.doc.as_ref().and_then(|d| d.ty.as_deref())
    }
}

/// What a name or field occurrence refers to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Local(usize),
    Global(String),
    Field(Value, String),
}

/// An occurrence of a name in an expression.
#[derive(Debug, Clone)]
pub struct Ref {
    pub start: usize,
    pub end: usize,
    pub target: Target,
}

/// A `require("a.b")` call: the byte range of the string literal and the module name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequireRef {
    pub start: usize,
    pub end: usize,
    pub module: String,
}

/// A `---@class` with where it is declared.
#[derive(Debug, Clone)]
pub struct ClassDef {
    pub ann: ClassAnn,
    pub pos: Pos,
    /// The table declaration it is bound to, if any.
    pub decl: Option<usize>,
    /// Description of the whole doc block.
    pub doc: String,
}

/// A `---@alias`.
#[derive(Debug, Clone)]
pub struct AliasDef {
    pub ann: AliasAnn,
    pub pos: Pos,
}

/// Everything indexed from one file.
#[derive(Debug, Clone, Default)]
pub struct FileIndex {
    pub path: String,
    pub decls: Vec<Decl>,
    pub refs: Vec<Ref>,
    pub requires: Vec<RequireRef>,
    pub classes: Vec<ClassDef>,
    pub aliases: Vec<AliasDef>,
    /// Byte ranges of doc comments, for "type name under the caret" lookups.
    pub doc_ranges: Vec<(usize, usize)>,
    /// Value of the main chunk's `return`, when there is one.
    pub returns: Option<Value>,
    /// Byte offset of the first statement (the gutter run icon goes there).
    pub first_statement: Option<usize>,
}

impl FileIndex {
    /// The innermost reference covering a byte offset.
    pub fn ref_at(&self, byte: usize) -> Option<&Ref> {
        self.refs
            .iter()
            .filter(|r| r.start <= byte && byte <= r.end)
            .min_by_key(|r| r.end - r.start)
    }

    /// The declaration whose name covers a byte offset.
    pub fn decl_at(&self, byte: usize) -> Option<(usize, &Decl)> {
        self.decls
            .iter()
            .enumerate()
            .find(|(_, d)| d.start <= byte && byte <= d.end)
    }

    pub fn require_at(&self, byte: usize) -> Option<&RequireRef> {
        self.requires
            .iter()
            .find(|r| r.start <= byte && byte <= r.end)
    }

    pub fn in_doc_comment(&self, byte: usize) -> bool {
        self.doc_ranges.iter().any(|&(s, e)| s <= byte && byte <= e)
    }
}
