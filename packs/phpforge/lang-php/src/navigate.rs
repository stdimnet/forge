//! Code navigation on the PHP index, used when no language server answers (none installed,
//! or free Intelephense without implementation / type definition): the symbol under the
//! caret, Find Usages, Go to Implementation, Go to Super Method and Go to Type Declaration.
//!
//! Offsets inside are UTF-8 bytes; positions handed out are LSP positions (0-based line,
//! UTF-16 character), like the language servers' answers they stand in for.

use std::collections::HashMap;

use forge_codeintel::{classify_usage, Position, Range, UsageKind, UsageLocation};
use serde::{Deserialize, Serialize};

use crate::hover::{find_member, word_at};
use crate::index::ProjectIndex;
use crate::model::{short_name, ClassLike, FileIndex, Member, SymbolKind, UseKind};
use crate::parser;
use crate::resolve::is_builtin_type;

/// A project symbol found under the caret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SymbolRef {
    Class {
        fqn: String,
    },
    /// A member, identified by the class that declares it.
    Member {
        class: String,
        member: SymbolKind,
        name: String,
    },
    Function {
        fqn: String,
    },
}

impl SymbolRef {
    /// The text that has to appear where the symbol is used.
    pub fn search_word(&self) -> &str {
        match self {
            SymbolRef::Class { fqn } | SymbolRef::Function { fqn } => short_name(fqn),
            SymbolRef::Member { name, .. } => name,
        }
    }

    /// How PHPStorm names it in titles: `User`, `User::orders()`, `User::$email`, `money()`.
    pub fn display(&self) -> String {
        match self {
            SymbolRef::Class { fqn } => short_name(fqn).to_string(),
            SymbolRef::Function { fqn } => format!("{}()", short_name(fqn)),
            SymbolRef::Member {
                class,
                member,
                name,
            } => match member {
                SymbolKind::Method => format!("{}::{name}()", short_name(class)),
                SymbolKind::Property => format!("{}::${name}", short_name(class)),
                _ => format!("{}::{name}", short_name(class)),
            },
        }
    }

    /// Lowercase kind for the UI (`class`, `method`, `property`, `constant`, `function`).
    pub fn kind_name(&self) -> &'static str {
        match self {
            SymbolRef::Class { .. } => "class",
            SymbolRef::Function { .. } => "function",
            SymbolRef::Member { member, .. } => match member {
                SymbolKind::Method => "method",
                SymbolKind::Property => "property",
                SymbolKind::EnumCase => "enumCase",
                _ => "constant",
            },
        }
    }

    fn same(&self, other: &SymbolRef) -> bool {
        match (self, other) {
            (SymbolRef::Class { fqn: a }, SymbolRef::Class { fqn: b })
            | (SymbolRef::Function { fqn: a }, SymbolRef::Function { fqn: b }) => {
                a.eq_ignore_ascii_case(b)
            }
            (
                SymbolRef::Member {
                    class: c1,
                    member: k1,
                    name: n1,
                },
                SymbolRef::Member {
                    class: c2,
                    member: k2,
                    name: n2,
                },
            ) => {
                k1 == k2
                    && c1.eq_ignore_ascii_case(c2)
                    && if *k1 == SymbolKind::Method {
                        n1.eq_ignore_ascii_case(n2)
                    } else {
                        n1 == n2
                    }
            }
            _ => false,
        }
    }
}

/// What a word in the source refers to.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Resolved {
    Symbol(SymbolRef),
    /// The declaration of the symbol itself.
    Declaration(SymbolRef),
    /// A member accessed on an expression the index cannot type (`$x->save()`).
    Dynamic {
        kind: SymbolKind,
        name: String,
    },
    Nothing,
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '\\' || c == '$'
}

fn ident_start(src: &str, end: usize) -> usize {
    src[..end]
        .char_indices()
        .rev()
        .take_while(|(_, c)| is_ident(*c))
        .last()
        .map(|(i, _)| i)
        .unwrap_or(end)
}

/// Class names inside a type as written (`?Order`, `Collection<int, Order>`, `A|B`,
/// `Order[]`), resolved in `file` at `offset`; builtin types are left out and `self` /
/// `static` / `$this` mean `owner`.
fn type_classes(ty: &str, file: &FileIndex, offset: usize, owner: Option<&str>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for part in ty.split(['|', '&', '<', '>', ',', '(', ')', ' ']) {
        let name = part.trim().trim_start_matches('?').trim_end_matches("[]");
        if name.is_empty() || (name.starts_with('$') && name != "$this") {
            continue;
        }
        let fqn = match name.to_ascii_lowercase().as_str() {
            "self" | "static" | "$this" => match owner {
                Some(o) => o.to_string(),
                None => continue,
            },
            n if is_builtin_type(n) || n == "mixed" || n == "void" || n == "never" => continue,
            _ => file.resolve_class(name, offset),
        };
        if !out.iter().any(|o| o.eq_ignore_ascii_case(&fqn)) {
            out.push(fqn);
        }
    }
    out
}

/// The file declaring `class` (for resolving its member types), the given one if it is
/// declared there.
fn declaring_file(index: &ProjectIndex, file: &FileIndex, class: &str) -> Option<FileIndex> {
    if file
        .classes
        .iter()
        .any(|c| c.fqn.eq_ignore_ascii_case(class))
    {
        return Some(file.clone());
    }
    index.class_path(class).and_then(|p| index.file(p)).cloned()
}

/// Classes of a member's type (property type or method return type).
fn member_type_classes(
    index: &ProjectIndex,
    file: &FileIndex,
    owner: &ClassLike,
    m: &Member,
) -> Vec<String> {
    let Some(ty) = m.effective_type() else {
        return vec![];
    };
    let ctx = declaring_file(index, file, &owner.fqn).unwrap_or_else(|| file.clone());
    type_classes(&ty, &ctx, m.range.start, Some(&owner.fqn))
}

fn find_class(index: &mut ProjectIndex, file: &FileIndex, fqn: &str) -> Option<ClassLike> {
    index.resolve_class(fqn).or_else(|| {
        file.classes
            .iter()
            .find(|c| c.fqn.eq_ignore_ascii_case(fqn))
            .cloned()
    })
}

/// The method or function whose range contains `offset`: its params and range.
fn enclosing_callable(
    file: &FileIndex,
    offset: usize,
) -> Option<(Vec<crate::model::Param>, usize, usize)> {
    if let Some(m) = file.class_at(offset).and_then(|c| {
        c.members
            .iter()
            .find(|m| m.kind == SymbolKind::Method && m.range.contains(offset))
    }) {
        return Some((m.params.clone(), m.range.start, m.range.end));
    }
    file.functions
        .iter()
        .find(|f| f.range.contains(offset))
        .map(|f| (f.params.clone(), f.range.start, f.range.end))
}

/// The class type of a local variable at `offset`: a typed parameter, `$x = new X(...)` or
/// an inline `/** @var X $x */` earlier in the same method or function.
fn variable_classes(file: &FileIndex, src: &str, offset: usize, var: &str) -> Vec<String> {
    let name = var.trim_start_matches('$');
    let owner = file.class_at(offset).map(|c| c.fqn.clone());
    if name == "this" {
        return owner.into_iter().collect();
    }
    let Some((params, start, _)) = enclosing_callable(file, offset) else {
        return vec![];
    };
    if let Some(ty) = params
        .iter()
        .find(|p| p.name == name)
        .and_then(|p| p.type_hint.clone())
    {
        return type_classes(&ty, file, offset, owner.as_deref());
    }
    let body = &src[start.min(offset)..offset];
    let assign = format!("${name} = new ");
    if let Some(i) = body.rfind(&assign) {
        let rest = &body[i + assign.len()..];
        let class: String = rest
            .chars()
            .take_while(|c| is_ident(*c) && *c != '$')
            .collect();
        if !class.is_empty() {
            return type_classes(&class, file, start + i, owner.as_deref());
        }
    }
    let var_tag = format!(" ${name}");
    for line in body.lines().rev() {
        if let Some(rest) = line
            .trim()
            .strip_prefix("/** @var ")
            .or_else(|| line.trim().strip_prefix("* @var "))
        {
            if let Some(ty) = rest.trim_end_matches("*/").trim().strip_suffix(&var_tag) {
                return type_classes(ty, file, start, owner.as_deref());
            }
        }
    }
    vec![]
}

/// The class of the expression ending at `end` (just before `->` or `::`).
fn receiver_class(
    index: &mut ProjectIndex,
    file: &FileIndex,
    src: &str,
    end: usize,
    depth: u8,
) -> Option<ClassLike> {
    let start = ident_start(src, end);
    let recv = &src[start..end];
    let enclosing = file.class_at(end).cloned();
    match recv.to_ascii_lowercase().as_str() {
        "" => None,
        "$this" | "self" | "static" => enclosing,
        "parent" => {
            let parent = enclosing?.extends.first().cloned()?;
            find_class(index, file, &parent)
        }
        r if r.starts_with('$') => {
            let fqn = variable_classes(file, src, end, recv).into_iter().next()?;
            find_class(index, file, &fqn)
        }
        _ => {
            // `$this->invoices->settle()`: the type of a property in a chain.
            let before = src[..start].trim_end();
            let chained = before
                .strip_suffix("?->")
                .or_else(|| before.strip_suffix("->"))
                .map(str::len);
            if let Some(chain_end) = chained {
                if depth >= 3 {
                    return None;
                }
                let owner = receiver_class(index, file, src, chain_end, depth + 1)?;
                let (decl, m) = find_member(index, &owner, recv, &[SymbolKind::Property])?;
                let fqn = member_type_classes(index, file, &decl, &m)
                    .into_iter()
                    .next()?;
                return find_class(index, file, &fqn);
            }
            let fqn = file.resolve_class(recv, start);
            find_class(index, file, &fqn)
        }
    }
}

/// Resolve the word at `start..end` of `src` (the content of `file`).
fn resolve_word(
    index: &mut ProjectIndex,
    file: &FileIndex,
    src: &str,
    start: usize,
    end: usize,
) -> Resolved {
    let word = &src[start..end];
    // 1. Declarations.
    for c in &file.classes {
        if c.name_range.start <= start && end <= c.name_range.end {
            return Resolved::Declaration(SymbolRef::Class { fqn: c.fqn.clone() });
        }
        for m in &c.members {
            if m.name_range.start <= start && end <= m.name_range.end {
                return Resolved::Declaration(SymbolRef::Member {
                    class: c.fqn.clone(),
                    member: m.kind,
                    name: m.name.clone(),
                });
            }
        }
    }
    for f in &file.functions {
        if f.name_range.start <= start && end <= f.name_range.end {
            return Resolved::Declaration(SymbolRef::Function { fqn: f.fqn.clone() });
        }
    }

    let before = src[..start].trim_end();
    let after = src[end..].trim_start();
    let is_call = after.starts_with('(');

    // 2. Member access.
    let access = if let Some(b) = before.strip_suffix("?->") {
        Some((b.len(), "->"))
    } else if let Some(b) = before.strip_suffix("->") {
        Some((b.len(), "->"))
    } else {
        before.strip_suffix("::").map(|b| (b.len(), "::"))
    };
    if let Some((recv_end, op)) = access {
        let name = word.trim_start_matches('$');
        let kinds: &[SymbolKind] = match (op, is_call) {
            (_, true) => &[SymbolKind::Method],
            ("->", false) => &[SymbolKind::Property],
            _ if word.starts_with('$') => &[SymbolKind::Property],
            _ if word.eq_ignore_ascii_case("class") => &[],
            _ => &[SymbolKind::Constant, SymbolKind::EnumCase],
        };
        let Some(class) = receiver_class(index, file, src, recv_end, 0) else {
            return if op == "->" {
                Resolved::Dynamic {
                    kind: kinds.first().copied().unwrap_or(SymbolKind::Property),
                    name: name.to_string(),
                }
            } else {
                Resolved::Nothing
            };
        };
        if kinds.is_empty() {
            return Resolved::Symbol(SymbolRef::Class { fqn: class.fqn });
        }
        return match find_member(index, &class, name, kinds) {
            Some((owner, m)) => Resolved::Symbol(SymbolRef::Member {
                class: owner.fqn,
                member: m.kind,
                name: m.name,
            }),
            None => Resolved::Nothing,
        };
    }
    if word.starts_with('$') {
        return Resolved::Nothing;
    }

    // 3. Function call.
    let prev_word = before
        .rsplit(|c: char| !(c.is_alphanumeric() || c == '_'))
        .next()
        .unwrap_or("");
    if is_call
        && !prev_word.eq_ignore_ascii_case("new")
        && !prev_word.eq_ignore_ascii_case("function")
    {
        let ns = file.namespace_at(start);
        let bare = word.trim_start_matches('\\');
        let imported = file
            .uses
            .iter()
            .find(|u| u.kind == UseKind::Function && u.local_name().eq_ignore_ascii_case(bare))
            .map(|u| u.fqn.clone());
        let qualified = if ns.is_empty() || word.starts_with('\\') {
            bare.to_string()
        } else {
            format!("{ns}\\{bare}")
        };
        for cand in imported.into_iter().chain([qualified, bare.to_string()]) {
            if let Some(f) = index
                .files()
                .flat_map(|f| f.functions.iter())
                .find(|f| f.fqn.eq_ignore_ascii_case(&cand))
            {
                return Resolved::Symbol(SymbolRef::Function { fqn: f.fqn.clone() });
            }
        }
        return Resolved::Nothing;
    }

    // 4. Class reference. Names in `use` imports are fully qualified.
    let line_start = src[..start].rfind('\n').map_or(0, |i| i + 1);
    let fqn = if src[line_start..start].trim_start().starts_with("use ") && !word.starts_with('\\')
    {
        word.to_string()
    } else {
        file.resolve_class(word, start)
    };
    match find_class(index, file, &fqn) {
        Some(c) => Resolved::Symbol(SymbolRef::Class { fqn: c.fqn }),
        None => Resolved::Nothing,
    }
}

fn resolve_at(index: &mut ProjectIndex, file: &FileIndex, src: &str, offset: usize) -> Resolved {
    match word_at(src, offset) {
        Some((start, end)) => resolve_word(index, file, src, start, end),
        None => Resolved::Nothing,
    }
}

/// The project symbol at `offset` (bytes) of `src`, the content of `file`.
pub fn symbol_at(
    index: &mut ProjectIndex,
    file: &FileIndex,
    src: &str,
    offset: usize,
) -> Option<SymbolRef> {
    match resolve_at(index, file, src, offset) {
        Resolved::Symbol(s) | Resolved::Declaration(s) => Some(s),
        _ => None,
    }
}

/// Line starts of a text, to turn byte offsets into LSP positions.
struct Lines<'a> {
    text: &'a str,
    starts: Vec<usize>,
}

impl<'a> Lines<'a> {
    fn new(text: &'a str) -> Self {
        let mut starts = vec![0];
        starts.extend(text.match_indices('\n').map(|(i, _)| i + 1));
        Lines { text, starts }
    }

    fn position(&self, byte: usize) -> Position {
        let byte = byte.min(self.text.len());
        let line = self.starts.partition_point(|&s| s <= byte) - 1;
        let start = self.starts[line];
        let character = self.text[start..byte].encode_utf16().count() as u32;
        Position::new(line as u32, character)
    }

    fn range(&self, start: usize, end: usize) -> Range {
        Range::new(self.position(start), self.position(end))
    }
}

/// Find Usages from the index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhpUsages {
    pub symbol: SymbolRef,
    /// `User`, `User::orders()`, ...
    pub title: String,
    pub declaration: Option<UsageLocation>,
    pub usages: Vec<UsageLocation>,
}

/// Contents of the project's PHP files: the editor buffer of `rel`, the rest from disk.
struct Sources<'a> {
    root: std::path::PathBuf,
    rel: &'a str,
    src: &'a str,
    cache: HashMap<String, Option<String>>,
}

impl<'a> Sources<'a> {
    fn get(&mut self, path: &str) -> Option<String> {
        if path == self.rel {
            return Some(self.src.to_string());
        }
        let root = &self.root;
        self.cache
            .entry(path.to_string())
            .or_insert_with(|| std::fs::read_to_string(root.join(path)).ok())
            .clone()
    }
}

fn is_vendor(path: &str) -> bool {
    path.starts_with("vendor/") || path.contains("/vendor/")
}

/// Byte offsets of `word` in `text` at identifier boundaries (a namespace separator may
/// come before it: `App\Models\User`).
fn occurrences(text: &str, word: &str) -> Vec<usize> {
    let ident = |c: char| c.is_alphanumeric() || c == '_';
    text.match_indices(word)
        .filter(|(i, _)| {
            let before = text[..*i].chars().next_back();
            let after = text[i + word.len()..].chars().next();
            !before.is_some_and(ident) && !after.is_some_and(ident)
        })
        .map(|(i, _)| i)
        .collect()
}

/// Where `symbol` is declared.
fn declaration_of(
    index: &mut ProjectIndex,
    sources: &mut Sources,
    symbol: &SymbolRef,
) -> Option<UsageLocation> {
    let (path, range) = match symbol {
        SymbolRef::Class { fqn } => {
            let path = index.class_path(fqn)?.to_string();
            let c = index.find_class(fqn)?;
            (path, c.name_range)
        }
        SymbolRef::Member {
            class,
            member,
            name,
        } => {
            let path = index.class_path(class)?.to_string();
            let c = index.find_class(class)?;
            (path, c.member(*member, name)?.name_range)
        }
        SymbolRef::Function { fqn } => index.files().find_map(|f| {
            f.functions
                .iter()
                .find(|x| x.fqn.eq_ignore_ascii_case(fqn))
                .map(|x| (f.path.clone(), x.name_range))
        })?,
    };
    let text = sources.get(&path)?;
    Some(UsageLocation {
        range: Lines::new(&text).range(range.start, range.end),
        path,
        kind: None,
    })
}

/// Usages of the symbol at `offset` in the project's own files (vendor excluded). Members
/// used on expressions the index cannot type are reported as dynamic usages.
pub fn find_usages(
    index: &mut ProjectIndex,
    rel: &str,
    src: &str,
    offset: usize,
) -> Option<PhpUsages> {
    let current = parser::index_source(rel, src);
    let symbol = symbol_at(index, &current, src, offset)?;
    let mut sources = Sources {
        root: index.root().to_path_buf(),
        rel,
        src,
        cache: HashMap::new(),
    };
    let declaration = declaration_of(index, &mut sources, &symbol);
    let word = symbol.search_word().to_string();
    let mut paths: Vec<String> = index
        .files()
        .map(|f| f.path.clone())
        .filter(|p| !is_vendor(p) && p != rel)
        .collect();
    paths.sort();
    paths.insert(0, rel.to_string());

    let mut usages = Vec::new();
    for path in paths {
        let Some(text) = sources.get(&path) else {
            continue;
        };
        let hits = occurrences(&text, &word);
        if hits.is_empty() {
            continue;
        }
        let file = if path == rel {
            current.clone()
        } else {
            match index.file(&path) {
                Some(f) => f.clone(),
                None => parser::index_source(&path, &text),
            }
        };
        let lines = Lines::new(&text);
        for at in hits {
            let end = at + word.len();
            let range = lines.range(at, end);
            let line_start = text[..at].rfind('\n').map_or(0, |i| i + 1);
            let line_end = text[at..].find('\n').map_or(text.len(), |i| at + i);
            let line = &text[line_start..line_end];
            let start_col = text[line_start..at].encode_utf16().count() as u32;
            let classified = classify_usage(
                line,
                start_col,
                start_col + word.encode_utf16().count() as u32,
            );
            if matches!(classified, UsageKind::Comment | UsageKind::StringLiteral) {
                continue;
            }
            let kind = match resolve_at(index, &file, &text, at) {
                Resolved::Symbol(s) if s.same(&symbol) => None,
                Resolved::Dynamic { kind, name }
                    if matches!(&symbol, SymbolRef::Member { member, name: n, .. }
                        if *member == kind && n.eq_ignore_ascii_case(&name)) =>
                {
                    Some(UsageKind::Dynamic)
                }
                // PHPDoc mentions of a class (`@return User`, `@var User[]`).
                Resolved::Nothing
                    if classified == UsageKind::Doc
                        && matches!(&symbol, SymbolRef::Class { fqn }
                            if file.resolve_class(&word, at).eq_ignore_ascii_case(fqn)) =>
                {
                    None
                }
                _ => continue,
            };
            usages.push(UsageLocation {
                path: path.clone(),
                range,
                kind,
            });
        }
    }
    Some(PhpUsages {
        title: symbol.display(),
        symbol,
        declaration,
        usages,
    })
}

/// The index-based Navigate actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NavKind {
    Implementation,
    SuperMethod,
    TypeDeclaration,
}

/// Targets of `kind` for the caret at `offset` (bytes).
pub fn navigate(
    index: &mut ProjectIndex,
    rel: &str,
    src: &str,
    offset: usize,
    kind: NavKind,
) -> Vec<NavTarget> {
    match kind {
        NavKind::Implementation => implementations(index, rel, src, offset),
        NavKind::SuperMethod => super_targets(index, rel, src, offset),
        NavKind::TypeDeclaration => type_targets(index, rel, src, offset),
    }
}

/// A navigation target for Go to Implementation / Super Method / Type Declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NavTarget {
    pub path: String,
    /// 0-based line and UTF-16 column of the name.
    pub line: u32,
    pub column: u32,
    /// `OrderController`, `Model::save()`.
    pub name: String,
    /// Namespace or declaring class, shown greyed out in the chooser.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container: Option<String>,
}

fn target(
    sources: &mut Sources,
    path: &str,
    at: crate::model::Range,
    name: String,
    container: Option<String>,
) -> Option<NavTarget> {
    let text = sources.get(path)?;
    let p = Lines::new(&text).position(at.start);
    Some(NavTarget {
        path: path.to_string(),
        line: p.line,
        column: p.character,
        name,
        container,
    })
}

fn class_target(index: &mut ProjectIndex, sources: &mut Sources, fqn: &str) -> Option<NavTarget> {
    let c = index.resolve_class(fqn)?;
    let path = index.class_path(&c.fqn)?.to_string();
    let ns = (!c.namespace.is_empty()).then(|| c.namespace.clone());
    target(sources, &path, c.name_range, c.name.clone(), ns)
}

fn member_target(
    index: &ProjectIndex,
    sources: &mut Sources,
    owner: &ClassLike,
    m: &Member,
) -> Option<NavTarget> {
    let path = index.class_path(&owner.fqn)?.to_string();
    let name = match m.kind {
        SymbolKind::Method => format!("{}::{}()", owner.name, m.name),
        SymbolKind::Property => format!("{}::${}", owner.name, m.name),
        _ => format!("{}::{}", owner.name, m.name),
    };
    let ns = (!owner.namespace.is_empty()).then(|| owner.namespace.clone());
    target(sources, &path, m.name_range, name, ns)
}

/// Every indexed class that extends or implements `fqn` (directly or not).
fn subclasses(index: &mut ProjectIndex, fqn: &str) -> Vec<ClassLike> {
    let all: Vec<ClassLike> = index
        .files()
        .filter(|f| !is_vendor(&f.path))
        .flat_map(|f| f.classes.iter().cloned())
        .collect();
    let mut out: Vec<ClassLike> = all
        .into_iter()
        .filter(|c| !c.fqn.eq_ignore_ascii_case(fqn))
        .filter(|c| {
            index
                .ancestors(c)
                .iter()
                .any(|a| a.fqn.eq_ignore_ascii_case(fqn))
        })
        .collect();
    out.sort_by(|a, b| a.fqn.cmp(&b.fqn));
    out
}

fn sources<'a>(index: &ProjectIndex, rel: &'a str, src: &'a str) -> Sources<'a> {
    Sources {
        root: index.root().to_path_buf(),
        rel,
        src,
        cache: HashMap::new(),
    }
}

/// Go to Implementation: classes extending / implementing the class or interface at the
/// caret, or the methods overriding the method at the caret.
pub fn implementations(
    index: &mut ProjectIndex,
    rel: &str,
    src: &str,
    offset: usize,
) -> Vec<NavTarget> {
    let file = parser::index_source(rel, src);
    let mut sources = sources(index, rel, src);
    match symbol_at(index, &file, src, offset) {
        Some(SymbolRef::Class { fqn }) => subclasses(index, &fqn)
            .iter()
            .filter_map(|c| class_target(index, &mut sources, &c.fqn))
            .collect(),
        Some(SymbolRef::Member {
            class,
            member: SymbolKind::Method,
            name,
        }) => subclasses(index, &class)
            .iter()
            .filter_map(|c| {
                let m = c.method(&name)?;
                member_target(index, &mut sources, c, m)
            })
            .collect(),
        _ => vec![],
    }
}

/// Go to Super Method: the methods the method at (or around) the caret overrides or
/// implements; on a class, its parent class and interfaces.
pub fn super_targets(
    index: &mut ProjectIndex,
    rel: &str,
    src: &str,
    offset: usize,
) -> Vec<NavTarget> {
    let file = parser::index_source(rel, src);
    let mut sources = sources(index, rel, src);
    let Some(class) = file.class_at(offset).cloned() else {
        return vec![];
    };
    let method = match symbol_at(index, &file, src, offset) {
        Some(SymbolRef::Member {
            member: SymbolKind::Method,
            name,
            class: owner,
        }) if owner.eq_ignore_ascii_case(&class.fqn) => Some(name),
        Some(SymbolRef::Class { fqn }) if fqn.eq_ignore_ascii_case(&class.fqn) => None,
        _ => class
            .members
            .iter()
            .find(|m| m.kind == SymbolKind::Method && m.range.contains(offset))
            .map(|m| m.name.clone()),
    };
    match method {
        Some(name) => {
            let mut out: Vec<NavTarget> = Vec::new();
            for anc in index.ancestors(&class) {
                if let Some(m) = anc.method(&name) {
                    if let Some(t) = member_target(index, &mut sources, &anc, m) {
                        if !out.contains(&t) {
                            out.push(t);
                        }
                    }
                }
            }
            out
        }
        None => class
            .extends
            .iter()
            .chain(&class.implements)
            .filter_map(|fqn| class_target(index, &mut sources, fqn))
            .collect(),
    }
}

/// Go to Type Declaration: the classes of the variable, property, method or function at
/// the caret.
pub fn type_targets(
    index: &mut ProjectIndex,
    rel: &str,
    src: &str,
    offset: usize,
) -> Vec<NavTarget> {
    let file = parser::index_source(rel, src);
    let mut sources = sources(index, rel, src);
    let Some((start, end)) = word_at(src, offset) else {
        return vec![];
    };
    let word = &src[start..end];
    let before = src[..start].trim_end();
    let classes: Vec<String> = if word.starts_with('$') && !before.ends_with("::") {
        variable_classes(&file, src, start, word)
    } else {
        match symbol_at(index, &file, src, offset) {
            Some(SymbolRef::Class { fqn }) => vec![fqn],
            Some(SymbolRef::Member {
                class,
                member,
                name,
            }) => match find_class(index, &file, &class) {
                Some(owner) => match owner.member(member, &name).cloned() {
                    Some(m) if matches!(member, SymbolKind::Method | SymbolKind::Property) => {
                        member_type_classes(index, &file, &owner, &m)
                    }
                    Some(_) => vec![owner.fqn],
                    None => vec![],
                },
                None => vec![],
            },
            Some(SymbolRef::Function { fqn }) => index
                .files()
                .flat_map(|f| f.functions.iter().map(move |x| (f, x)))
                .find(|(_, x)| x.fqn.eq_ignore_ascii_case(&fqn))
                .and_then(|(f, x)| {
                    let ty = x.return_type.clone()?;
                    Some(type_classes(&ty, f, x.range.start, None))
                })
                .unwrap_or_default(),
            None => vec![],
        }
    };
    classes
        .iter()
        .filter_map(|fqn| class_target(index, &mut sources, fqn))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::IndexConfig;
    use std::fs;

    const MODEL: &str = "<?php\nnamespace App\\Models;\n\nabstract class Model\n{\n    public function save(): bool {}\n}\n";
    const USER: &str = "<?php\nnamespace App\\Models;\n\n/**\n * @property int $id\n */\nclass User extends Model implements Contact\n{\n    public ?Address $address;\n\n    public function save(): bool { return true; }\n\n    public function orders(): Collection\n    {\n        return $this->hasMany(Order::class);\n    }\n}\n";
    const CONTACT: &str = "<?php\nnamespace App\\Models;\n\ninterface Contact\n{\n    public function save(): bool;\n}\n";
    const ADDRESS: &str = "<?php\nnamespace App\\Models;\n\nfinal class Address\n{\n    public string $city = '';\n}\n";
    const SERVICE: &str = "<?php\nnamespace App\\Services;\n\nuse App\\Models\\User;\n\n// User is mentioned in a comment\nfinal class Billing\n{\n    public function __construct(private readonly User $owner) {}\n\n    /** @return User */\n    public function bill(User $user, $any): User\n    {\n        $copy = new User();\n        $user->save();\n        $this->owner->save();\n        $any->save();\n        echo $user->address->city;\n        return $copy;\n    }\n}\n";

    fn setup() -> (tempfile::TempDir, ProjectIndex) {
        let dir = tempfile::tempdir().unwrap();
        let r = dir.path();
        fs::create_dir_all(r.join("app/Models")).unwrap();
        fs::create_dir_all(r.join("app/Services")).unwrap();
        for (p, t) in [
            ("app/Models/Model.php", MODEL),
            ("app/Models/User.php", USER),
            ("app/Models/Contact.php", CONTACT),
            ("app/Models/Address.php", ADDRESS),
            ("app/Services/Billing.php", SERVICE),
        ] {
            fs::write(r.join(p), t).unwrap();
        }
        let mut idx = ProjectIndex::new(
            r,
            IndexConfig {
                cache_file: Some(r.join("cache.json")),
                ..Default::default()
            },
        );
        idx.index_workspace(|_, _| {});
        (dir, idx)
    }

    fn at(src: &str, needle: &str, nth: usize) -> usize {
        src.match_indices(needle).nth(nth).expect("needle").0 + 1
    }

    #[test]
    fn resolves_symbols_like_hover() {
        let (_d, mut idx) = setup();
        let file = parser::index_source("app/Services/Billing.php", SERVICE);
        let sym = |idx: &mut ProjectIndex, o| symbol_at(idx, &file, SERVICE, o);
        assert_eq!(
            sym(&mut idx, at(SERVICE, "User;", 0)),
            Some(SymbolRef::Class {
                fqn: "App\\Models\\User".into()
            })
        );
        // `$user->save()` through the typed parameter; the declaration is User's own.
        let save = sym(&mut idx, at(SERVICE, "save();", 0)).unwrap();
        assert_eq!(save.display(), "User::save()");
        // `$this->owner->save()` through the promoted property's type.
        assert_eq!(sym(&mut idx, at(SERVICE, "save();", 1)), Some(save));
        // Untyped receivers are dynamic.
        assert_eq!(sym(&mut idx, at(SERVICE, "save();", 2)), None);
        // Property chain: `$user->address->city`.
        assert_eq!(
            sym(&mut idx, at(SERVICE, "city;", 0)).unwrap().display(),
            "Address::$city"
        );
    }

    #[test]
    fn finds_usages_with_kinds_and_dynamic_ones() {
        let (_d, mut idx) = setup();
        let o = at(USER, "User extends", 0);
        let u = find_usages(&mut idx, "app/Models/User.php", USER, o).unwrap();
        assert_eq!(u.title, "User");
        let decl = u.declaration.unwrap();
        assert_eq!(
            (decl.path.as_str(), decl.range.start.line),
            ("app/Models/User.php", 6)
        );
        let lines: Vec<(String, u32)> = u
            .usages
            .iter()
            .map(|l| (l.path.clone(), l.range.start.line))
            .collect();
        // Import, constructor type, @return, param type, return type, `new User()`; not the
        // comment and not the declaration.
        assert_eq!(
            lines,
            vec![
                ("app/Services/Billing.php".to_string(), 3),
                ("app/Services/Billing.php".to_string(), 8),
                ("app/Services/Billing.php".to_string(), 10),
                ("app/Services/Billing.php".to_string(), 11),
                ("app/Services/Billing.php".to_string(), 11),
                ("app/Services/Billing.php".to_string(), 13),
            ]
        );
        assert_eq!(u.usages[0].range.start.character, 15);

        let o = at(USER, "save()", 0);
        let u = find_usages(&mut idx, "app/Models/User.php", USER, o).unwrap();
        assert_eq!(u.title, "User::save()");
        let kinds: Vec<Option<UsageKind>> = u.usages.iter().map(|l| l.kind).collect();
        assert_eq!(kinds, vec![None, None, Some(UsageKind::Dynamic)]);
    }

    #[test]
    fn navigates_to_implementations_super_methods_and_types() {
        let (_d, mut idx) = setup();
        let names = |t: Vec<NavTarget>| t.into_iter().map(|t| t.name).collect::<Vec<_>>();

        let o = at(CONTACT, "Contact", 0);
        assert_eq!(
            names(implementations(
                &mut idx,
                "app/Models/Contact.php",
                CONTACT,
                o
            )),
            vec!["User"]
        );
        let o = at(MODEL, "save", 0);
        let impls = implementations(&mut idx, "app/Models/Model.php", MODEL, o);
        assert_eq!(names(impls.clone()), vec!["User::save()"]);
        assert_eq!(
            (impls[0].path.as_str(), impls[0].line),
            ("app/Models/User.php", 10)
        );

        // Ctrl+U inside User::save(): the parent's and the interface's declarations.
        let o = at(USER, "return true", 0);
        assert_eq!(
            names(super_targets(&mut idx, "app/Models/User.php", USER, o)),
            vec!["Model::save()", "Contact::save()"]
        );
        let o = at(USER, "User extends", 0);
        assert_eq!(
            names(super_targets(&mut idx, "app/Models/User.php", USER, o)),
            vec!["Model", "Contact"]
        );

        let o = at(SERVICE, "$user->save", 0);
        assert_eq!(
            names(type_targets(
                &mut idx,
                "app/Services/Billing.php",
                SERVICE,
                o
            )),
            vec!["User"]
        );
        let o = at(SERVICE, "address->", 0);
        let t = type_targets(&mut idx, "app/Services/Billing.php", SERVICE, o);
        assert_eq!(names(t.clone()), vec!["Address"]);
        assert_eq!(t[0].container.as_deref(), Some("App\\Models"));
        let o = at(SERVICE, "$copy;", 0);
        assert_eq!(
            names(type_targets(
                &mut idx,
                "app/Services/Billing.php",
                SERVICE,
                o
            )),
            vec!["User"]
        );
    }
}
