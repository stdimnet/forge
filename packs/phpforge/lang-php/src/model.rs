//! Symbol model produced by the indexer. Everything is serializable so it can be cached on
//! disk and sent over IPC unchanged (camelCase field names for the UI).

use serde::{Deserialize, Serialize};

use crate::docblock::{AnnotationArg, DocBlock};

/// Byte range in a source file plus the 0-based line/column (in bytes) of its start.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Range {
    pub start: usize,
    pub end: usize,
    pub line: u32,
    pub column: u32,
}

impl Range {
    pub fn contains(&self, offset: usize) -> bool {
        self.start <= offset && offset <= self.end
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SymbolKind {
    Namespace,
    Class,
    Interface,
    Trait,
    Enum,
    EnumCase,
    Method,
    Property,
    Constant,
    Function,
}

impl SymbolKind {
    pub fn is_class_like(self) -> bool {
        matches!(
            self,
            Self::Class | Self::Interface | Self::Trait | Self::Enum
        )
    }

    /// PHP keyword used in declarations (`class`, `interface`, ...).
    pub fn keyword(self) -> &'static str {
        match self {
            Self::Namespace => "namespace",
            Self::Class => "class",
            Self::Interface => "interface",
            Self::Trait => "trait",
            Self::Enum => "enum",
            Self::EnumCase => "case",
            Self::Method | Self::Function => "function",
            Self::Property => "",
            Self::Constant => "const",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Visibility {
    #[default]
    Public,
    Protected,
    Private,
}

impl Visibility {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Protected => "protected",
            Self::Private => "private",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "public" => Some(Self::Public),
            "protected" => Some(Self::Protected),
            "private" => Some(Self::Private),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Modifiers {
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_static: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_abstract: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_final: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_readonly: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// A PHP 8 attribute (`#[Route('/x', methods: ['GET'])]`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Attribute {
    /// Name as written (`Route`, `ORM\Column`).
    pub name: String,
    /// Fully qualified name resolved through the file's use-imports.
    pub fqn: String,
    pub args: Vec<AnnotationArg>,
    pub range: Range,
}

impl Attribute {
    /// Source-like rendering: `#[Route('/x', methods: ['GET'])]`.
    pub fn render(&self) -> String {
        if self.args.is_empty() {
            return format!("#[{}]", self.name);
        }
        let args: Vec<String> = self
            .args
            .iter()
            .map(|a| match &a.name {
                Some(n) => format!("{n}: {}", a.value),
                None => a.value.clone(),
            })
            .collect();
        format!("#[{}({})]", self.name, args.join(", "))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Param {
    /// Name without `$`.
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub type_hint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub variadic: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub by_ref: bool,
    /// Set for constructor-promoted parameters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub promoted: Option<Visibility>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub readonly: bool,
}

impl Param {
    /// `int ...$ids = []` style rendering (without promotion modifiers).
    pub fn render(&self) -> String {
        let mut s = String::new();
        if let Some(t) = &self.type_hint {
            s.push_str(t);
            s.push(' ');
        }
        if self.by_ref {
            s.push('&');
        }
        if self.variadic {
            s.push_str("...");
        }
        s.push('$');
        s.push_str(&self.name);
        if let Some(d) = &self.default {
            s.push_str(" = ");
            s.push_str(d);
        }
        s
    }
}

/// A member of a class-like: method, property, constant or enum case.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Member {
    pub kind: SymbolKind,
    /// Name without `$` for properties.
    pub name: String,
    pub range: Range,
    pub name_range: Range,
    pub visibility: Visibility,
    #[serde(default)]
    pub modifiers: Modifiers,
    /// Property/constant type, or method return type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub type_hint: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<Param>,
    /// Default value of a property, value of a constant or backed enum case.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc: Option<DocBlock>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<Attribute>,
    /// Property declared through constructor promotion.
    #[serde(default, skip_serializing_if = "is_false")]
    pub promoted: bool,
    /// Byte offset just after the method's opening `{`, if it has a body.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_start: Option<usize>,
}

impl Member {
    /// Effective type: declared type, else the `@var` / `@return` docblock type.
    pub fn effective_type(&self) -> Option<String> {
        if let Some(t) = &self.type_hint {
            return Some(t.clone());
        }
        let doc = self.doc.as_ref()?;
        match self.kind {
            SymbolKind::Property => doc.var.as_ref().map(|v| v.type_name.clone()),
            SymbolKind::Method => doc.returns.as_ref().map(|r| r.type_name.clone()),
            _ => None,
        }
    }

    /// One-line signature as PHP source, e.g. `public static function make(int $a): self`.
    pub fn signature(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if self.modifiers.is_final {
            parts.push("final".into());
        }
        if self.modifiers.is_abstract {
            parts.push("abstract".into());
        }
        if self.kind != SymbolKind::EnumCase {
            parts.push(self.visibility.as_str().into());
        }
        if self.modifiers.is_static {
            parts.push("static".into());
        }
        if self.modifiers.is_readonly {
            parts.push("readonly".into());
        }
        let head = parts.join(" ");
        match self.kind {
            SymbolKind::Method => {
                let params: Vec<String> = self.params.iter().map(Param::render).collect();
                let ret = self
                    .type_hint
                    .as_ref()
                    .map(|t| format!(": {t}"))
                    .unwrap_or_default();
                format!("{head} function {}({}){ret}", self.name, params.join(", "))
            }
            SymbolKind::Property => {
                let ty = self
                    .type_hint
                    .as_ref()
                    .map(|t| format!("{t} "))
                    .unwrap_or_default();
                let val = self
                    .value
                    .as_ref()
                    .map(|v| format!(" = {v}"))
                    .unwrap_or_default();
                format!("{head} {ty}${}{val}", self.name)
            }
            SymbolKind::Constant => {
                let ty = self
                    .type_hint
                    .as_ref()
                    .map(|t| format!("{t} "))
                    .unwrap_or_default();
                let val = self
                    .value
                    .as_ref()
                    .map(|v| format!(" = {v}"))
                    .unwrap_or_default();
                format!("{head} const {ty}{}{val}", self.name)
            }
            SymbolKind::EnumCase => match &self.value {
                Some(v) => format!("case {} = {v}", self.name),
                None => format!("case {}", self.name),
            },
            _ => self.name.clone(),
        }
    }
}

/// Class, interface, trait or enum.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassLike {
    pub kind: SymbolKind,
    pub name: String,
    pub fqn: String,
    pub namespace: String,
    pub range: Range,
    pub name_range: Range,
    /// Range of the `{ ... }` body including both braces.
    pub body_range: Range,
    #[serde(default)]
    pub modifiers: Modifiers,
    /// Parent class (classes) or parent interfaces (interfaces), resolved to FQNs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extends: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub implements: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub traits: Vec<String>,
    /// Backing type of an enum (`string`, `int`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backing_type: Option<String>,
    pub members: Vec<Member>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc: Option<DocBlock>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<Attribute>,
}

impl ClassLike {
    pub fn member(&self, kind: SymbolKind, name: &str) -> Option<&Member> {
        self.members.iter().find(|m| {
            m.kind == kind
                && if kind == SymbolKind::Method {
                    m.name.eq_ignore_ascii_case(name)
                } else {
                    m.name == name
                }
        })
    }

    pub fn method(&self, name: &str) -> Option<&Member> {
        self.member(SymbolKind::Method, name)
    }

    pub fn properties(&self) -> impl Iterator<Item = &Member> {
        self.members
            .iter()
            .filter(|m| m.kind == SymbolKind::Property)
    }

    pub fn methods(&self) -> impl Iterator<Item = &Member> {
        self.members.iter().filter(|m| m.kind == SymbolKind::Method)
    }

    /// Declaration head, e.g. `final class OrderController extends Controller`.
    pub fn signature(&self) -> String {
        let mut s = String::new();
        if self.modifiers.is_final {
            s.push_str("final ");
        }
        if self.modifiers.is_abstract && self.kind == SymbolKind::Class {
            s.push_str("abstract ");
        }
        if self.modifiers.is_readonly {
            s.push_str("readonly ");
        }
        s.push_str(self.kind.keyword());
        s.push(' ');
        s.push_str(&self.name);
        if let Some(b) = &self.backing_type {
            s.push_str(": ");
            s.push_str(b);
        }
        if !self.extends.is_empty() {
            s.push_str(" extends ");
            s.push_str(
                &self
                    .extends
                    .iter()
                    .map(|e| short_name(e))
                    .collect::<Vec<_>>()
                    .join(", "),
            );
        }
        if !self.implements.is_empty() {
            s.push_str(" implements ");
            s.push_str(
                &self
                    .implements
                    .iter()
                    .map(|e| short_name(e))
                    .collect::<Vec<_>>()
                    .join(", "),
            );
        }
        s
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionDecl {
    pub name: String,
    pub fqn: String,
    pub namespace: String,
    pub range: Range,
    pub name_range: Range,
    pub params: Vec<Param>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub return_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc: Option<DocBlock>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<Attribute>,
}

impl FunctionDecl {
    pub fn signature(&self) -> String {
        let params: Vec<String> = self.params.iter().map(Param::render).collect();
        let ret = self
            .return_type
            .as_ref()
            .map(|t| format!(": {t}"))
            .unwrap_or_default();
        format!("function {}({}){ret}", self.name, params.join(", "))
    }
}

/// A top-level `const X = ...;` (namespace constant).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConstantDecl {
    pub name: String,
    pub fqn: String,
    pub range: Range,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc: Option<DocBlock>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UseKind {
    #[default]
    Class,
    Function,
    Const,
}

/// `use Foo\Bar as Baz;`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UseImport {
    pub kind: UseKind,
    /// Imported fully qualified name, without leading backslash.
    pub fqn: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
    pub range: Range,
}

impl UseImport {
    /// The local name this import introduces.
    pub fn local_name(&self) -> &str {
        self.alias
            .as_deref()
            .unwrap_or_else(|| short_name(&self.fqn))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NamespaceDecl {
    pub name: String,
    pub range: Range,
}

/// Everything the indexer knows about one file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileIndex {
    pub path: String,
    pub namespaces: Vec<NamespaceDecl>,
    pub uses: Vec<UseImport>,
    pub classes: Vec<ClassLike>,
    pub functions: Vec<FunctionDecl>,
    pub constants: Vec<ConstantDecl>,
}

impl FileIndex {
    /// Innermost class-like whose range contains `offset`.
    pub fn class_at(&self, offset: usize) -> Option<&ClassLike> {
        self.classes
            .iter()
            .filter(|c| c.range.contains(offset))
            .min_by_key(|c| c.range.end - c.range.start)
    }

    /// Namespace in effect at `offset` (empty string for the global namespace).
    pub fn namespace_at(&self, offset: usize) -> &str {
        self.namespaces
            .iter()
            .rfind(|n| n.range.start <= offset)
            .map(|n| n.name.as_str())
            .unwrap_or("")
    }

    /// Resolve a class name as written in this file to a fully qualified name.
    pub fn resolve_class(&self, name: &str, offset: usize) -> String {
        crate::resolve::resolve_class_name(name, self.namespace_at(offset), &self.uses)
    }
}

/// Last segment of a namespaced name (`App\Models\User` -> `User`).
pub fn short_name(fqn: &str) -> &str {
    fqn.rsplit('\\').next().unwrap_or(fqn)
}
