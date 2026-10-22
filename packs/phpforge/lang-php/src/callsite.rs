//! The call around a position: which function, method or constructor, which argument, and
//! whether the caret is inside a string literal. Text based, so it works while typing
//! (unclosed strings and parentheses). Used by metadata / attribute completion and Go to
//! Declaration on mapped `.phpstorm.meta.php` keys.

use crate::attributes::{array_shape, string_literal, ShapeKey};
use crate::hover::find_member;
use crate::index::ProjectIndex;
use crate::meta::MetaCallable;
use crate::model::{Attribute, ClassLike, FileIndex, FunctionDecl, Member, Param, SymbolKind};
use crate::navigate::{
    access_before, called_function, class_chain, enclosing_callable, find_class, ident_start,
    last_assignment_end, matching_open, receiver_types, type_classes,
};

/// What is called.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Callee {
    /// Function candidates (FQNs), the declared one first.
    Function(Vec<String>),
    /// A method of one of `classes` (the receiver's classes and their ancestors).
    Method { classes: Vec<String>, name: String },
    /// `new Class(...)`: the class and its ancestors.
    Constructor { classes: Vec<String> },
}

impl Callee {
    /// Whether a metadata target is this callee.
    pub fn matches(&self, t: &MetaCallable) -> bool {
        match self {
            Callee::Function(cands) => cands.iter().any(|c| t.is_function(c)),
            Callee::Method { classes, name } => t.is_method_of(classes, name),
            Callee::Constructor { classes } => t.is_method_of(classes, "__construct"),
        }
    }
}

/// The string literal the caret is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StringAt {
    /// Byte offset of the opening quote.
    pub quote: usize,
    pub quote_char: char,
}

/// A call around a position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallSite {
    pub callee: Callee,
    /// The argument the position is in (0-based).
    pub argument: usize,
    /// Byte offset of the opening parenthesis.
    pub open: usize,
    /// Byte range of the callee's name.
    pub name: (usize, usize),
    pub string: Option<StringAt>,
}

/// How far back the scanner starts (bytes).
const LOOKBACK: usize = 8000;

/// Open brackets and the string state at `offset`, scanning forward from a line start
/// before it. Returns the stack of `(bracket, position, commas)` and the string the offset
/// is in.
pub(crate) fn scan_to(src: &str, offset: usize) -> (Vec<(u8, usize, usize)>, Option<StringAt>) {
    let offset = offset.min(src.len());
    let mut start = offset.saturating_sub(LOOKBACK);
    while start > 0 && !src.is_char_boundary(start) {
        start -= 1;
    }
    if start > 0 {
        start = src[..start].rfind('\n').map_or(0, |i| i + 1);
    }
    let b = src.as_bytes();
    let mut stack: Vec<(u8, usize, usize)> = Vec::new();
    let mut i = start;
    while i < offset {
        let c = b[i];
        match c {
            b'\'' | b'"' => {
                let q = c;
                let open = i;
                i += 1;
                let mut closed = false;
                while i < offset {
                    if b[i] == b'\\' {
                        i += 2;
                        continue;
                    }
                    if b[i] == q {
                        closed = true;
                        break;
                    }
                    i += 1;
                }
                if !closed {
                    return (
                        stack,
                        Some(StringAt {
                            quote: open,
                            quote_char: q as char,
                        }),
                    );
                }
            }
            b'/' if b.get(i + 1) == Some(&b'/') => {
                while i < offset && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'#' if b.get(i + 1) != Some(&b'[') => {
                while i < offset && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i + 1 < offset && !(b[i] == b'*' && b[i + 1] == b'/') {
                    i += 1;
                }
                i += 1;
            }
            b'(' | b'[' | b'{' => stack.push((c, i, 0)),
            b')' | b']' | b'}' => {
                stack.pop();
            }
            b',' => {
                if let Some(top) = stack.last_mut() {
                    top.2 += 1;
                }
            }
            b';' => {
                // A statement ends: only braces stay open.
                stack.retain(|(c, _, _)| *c == b'{');
            }
            _ => {}
        }
        i += 1;
    }
    (stack, None)
}

/// The callee of the call whose `(` is at `open`.
pub(crate) fn callee_before(
    index: &mut ProjectIndex,
    file: &FileIndex,
    src: &str,
    open: usize,
) -> Option<(Callee, (usize, usize))> {
    let name_end = src[..open].trim_end().len();
    let name_start = ident_start(src, name_end);
    let name = &src[name_start..name_end];
    if name.is_empty() || name.starts_with('$') {
        return None;
    }
    let before = src[..name_start].trim_end();
    let prev_word = before
        .rsplit(|c: char| !(c.is_alphanumeric() || c == '_'))
        .next()
        .unwrap_or("");
    let callee = if prev_word.eq_ignore_ascii_case("new") {
        let fqn = file.resolve_class(name, name_start);
        Callee::Constructor {
            classes: class_chain(index, file, &fqn),
        }
    } else if let Some((recv_end, _)) = access_before(before) {
        let mut classes: Vec<String> = Vec::new();
        for owner in receiver_types(index, file, src, recv_end, 0) {
            for c in class_chain(index, file, &owner) {
                if !classes.iter().any(|x| x.eq_ignore_ascii_case(&c)) {
                    classes.push(c);
                }
            }
        }
        Callee::Method {
            classes,
            name: name.to_string(),
        }
    } else if prev_word.eq_ignore_ascii_case("function") || crate::navigate::is_keyword(name) {
        return None;
    } else {
        Callee::Function(called_function(index, file, name, name_start))
    };
    Some((callee, (name_start, name_end)))
}

/// The call whose argument list contains `offset`.
pub fn call_at(
    index: &mut ProjectIndex,
    file: &FileIndex,
    src: &str,
    offset: usize,
) -> Option<CallSite> {
    let (stack, string) = scan_to(src, offset);
    let &(bracket, open, commas) = stack.last()?;
    if bracket != b'(' {
        return None;
    }
    let (callee, name) = callee_before(index, file, src, open)?;
    Some(CallSite {
        callee,
        argument: commas,
        open,
        name,
        string,
    })
}

/// A function or method declaration.
#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum Decl {
    Function(FunctionDecl),
    Method { owner: ClassLike, member: Member },
}

impl Decl {
    pub fn params(&self) -> &[Param] {
        match self {
            Decl::Function(f) => &f.params,
            Decl::Method { member, .. } => &member.params,
        }
    }

    pub fn attributes(&self) -> &[Attribute] {
        match self {
            Decl::Function(f) => &f.attributes,
            Decl::Method { member, .. } => &member.attributes,
        }
    }

    /// The parameter receiving argument `n` (a variadic one takes the rest).
    pub fn param(&self, n: usize) -> Option<&Param> {
        let params = self.params();
        params
            .get(n)
            .or_else(|| params.last().filter(|p| p.variadic))
    }

    /// The class owning a method.
    pub fn owner(&self) -> Option<&ClassLike> {
        match self {
            Decl::Method { owner, .. } => Some(owner),
            Decl::Function(_) => None,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Decl::Function(f) => &f.name,
            Decl::Method { member, .. } => &member.name,
        }
    }

    pub fn doc_deprecated(&self) -> Option<String> {
        match self {
            Decl::Function(f) => f.doc.as_ref().and_then(|d| d.deprecated.clone()),
            Decl::Method { member, .. } => member.doc.as_ref().and_then(|d| d.deprecated.clone()),
        }
    }
}

/// The file declaring `fqn`'s class, for resolving names in its attributes.
pub fn declaring_file(index: &ProjectIndex, file: &FileIndex, decl: &Decl) -> FileIndex {
    let path = match decl {
        Decl::Method { owner, .. } => {
            if file
                .classes
                .iter()
                .any(|c| c.fqn.eq_ignore_ascii_case(&owner.fqn))
            {
                return file.clone();
            }
            index.class_path(&owner.fqn).map(str::to_string)
        }
        Decl::Function(f) => {
            if file
                .functions
                .iter()
                .any(|x| x.fqn.eq_ignore_ascii_case(&f.fqn))
            {
                return file.clone();
            }
            index
                .files()
                .find(|x| {
                    x.functions
                        .iter()
                        .any(|y| y.fqn.eq_ignore_ascii_case(&f.fqn))
                })
                .map(|x| x.path.clone())
        }
    };
    path.and_then(|p| index.file(&p).cloned())
        .unwrap_or_else(|| file.clone())
}

/// The declaration a callee resolves to.
pub fn callee_decl(index: &mut ProjectIndex, file: &FileIndex, callee: &Callee) -> Option<Decl> {
    match callee {
        Callee::Function(cands) => cands.iter().find_map(|c| {
            file.functions
                .iter()
                .chain(index.files().flat_map(|f| f.functions.iter()))
                .find(|f| f.fqn.eq_ignore_ascii_case(c))
                .cloned()
                .map(Decl::Function)
        }),
        Callee::Method { classes, name } => classes.iter().find_map(|c| {
            let class = find_class(index, file, c)?;
            let (owner, member) = find_member(index, &class, name, &[SymbolKind::Method])?;
            Some(Decl::Method { owner, member })
        }),
        Callee::Constructor { classes } => classes.iter().find_map(|c| {
            let class = find_class(index, file, c)?;
            let (owner, member) = find_member(index, &class, "__construct", &[SymbolKind::Method])?;
            Some(Decl::Method { owner, member })
        }),
    }
}

/// The keys of an `#[ArrayShape]` and where their types resolve.
#[derive(Debug, Clone)]
pub struct Shape {
    pub keys: Vec<ShapeKey>,
    /// The file declaring the shape and an offset in it (for class names in the types).
    pub file: FileIndex,
    pub offset: usize,
    pub owner: Option<String>,
}

impl Shape {
    /// The classes of a key's type.
    pub fn key_classes(&self, key: &str) -> Vec<String> {
        self.keys
            .iter()
            .find(|k| k.key == key)
            .map(|k| {
                type_classes(
                    k.type_text(),
                    &self.file,
                    self.offset,
                    self.owner.as_deref(),
                )
            })
            .unwrap_or_default()
    }

    /// A key's type for display (`int`, `Mailer`).
    pub fn key_type(&self, key: &ShapeKey) -> String {
        crate::model::short_name(key.type_text()).to_string()
    }
}

/// Byte index of the `[` matching the `]` at `close`.
fn matching_bracket(src: &str, close: usize) -> Option<usize> {
    let b = src.as_bytes();
    let mut depth = 0i32;
    let mut i = close + 1;
    while i > 0 {
        i -= 1;
        match b[i] {
            b']' => depth += 1,
            b'[' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            b'\n' if close - i > 4000 => return None,
            _ => {}
        }
    }
    None
}

/// The `#[ArrayShape]` of the expression ending at `end`: a call to a function or method
/// declaring one, a parameter with one, a variable assigned from such a call, or a
/// property.
pub fn shape_of(
    index: &mut ProjectIndex,
    file: &FileIndex,
    src: &str,
    end: usize,
    depth: u8,
) -> Option<Shape> {
    if depth > 4 {
        return None;
    }
    let head = src[..end].trim_end();
    if head.ends_with(')') {
        let open = matching_open(src, head.len() - 1)?;
        let (callee, _) = callee_before(index, file, src, open)?;
        let decl = callee_decl(index, file, &callee)?;
        let keys = array_shape(decl.attributes())?;
        let offset = match &decl {
            Decl::Method { member, .. } => member.range.start,
            Decl::Function(f) => f.range.start,
        };
        return Some(Shape {
            keys,
            file: declaring_file(index, file, &decl),
            offset,
            owner: decl.owner().map(|o| o.fqn.clone()),
        });
    }
    let start = ident_start(src, head.len());
    let word = &src[start..head.len()];
    if let Some(var) = word.strip_prefix('$') {
        if let Some((params, _, _)) = enclosing_callable(file, start) {
            if let Some(keys) = params
                .iter()
                .find(|p| p.name == var)
                .and_then(|p| array_shape(&p.attributes))
            {
                return Some(Shape {
                    keys,
                    file: file.clone(),
                    offset: start,
                    owner: file.class_at(start).map(|c| c.fqn.clone()),
                });
            }
        }
        let j = last_assignment_end(file, src, head.len(), word)?;
        return shape_of(index, file, src, j, depth + 1);
    }
    let before = src[..start].trim_end();
    if let Some((recv_end, "->")) = access_before(before) {
        for owner in receiver_types(index, file, src, recv_end, depth + 1) {
            let Some(class) = find_class(index, file, &owner) else {
                continue;
            };
            if let Some((o, m)) = find_member(index, &class, word, &[SymbolKind::Property]) {
                if let Some(keys) = array_shape(&m.attributes) {
                    let decl = Decl::Method {
                        owner: o.clone(),
                        member: m.clone(),
                    };
                    return Some(Shape {
                        keys,
                        file: declaring_file(index, file, &decl),
                        offset: m.range.start,
                        owner: Some(o.fqn),
                    });
                }
            }
        }
    }
    None
}

/// Classes of `$data['key']` (the `]` just before `end`) from an `#[ArrayShape]`.
pub(crate) fn subscript_classes(
    index: &mut ProjectIndex,
    file: &FileIndex,
    src: &str,
    end: usize,
    depth: u8,
) -> Vec<String> {
    let Some(open) = matching_bracket(src, end - 1) else {
        return vec![];
    };
    let Some(key) = string_literal(&src[open + 1..end - 1]) else {
        return vec![];
    };
    shape_of(index, file, src, open, depth + 1)
        .map(|s| s.key_classes(&key))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_brackets_strings_and_comments() {
        let src = "<?php\nfoo($a, bar('x, y', [1, 2]), '";
        let (stack, string) = scan_to(src, src.len());
        assert_eq!(stack.len(), 1);
        assert_eq!(stack[0].2, 2);
        assert_eq!(string.map(|s| s.quote), Some(src.len() - 1));
        let src = "<?php\n// foo(\n# bar(\n/* baz( */ qux(1, ";
        let (stack, string) = scan_to(src, src.len());
        assert_eq!(stack.len(), 1);
        assert_eq!(stack[0].2, 1);
        assert!(string.is_none());
        let src = "<?php\n#[Attr(1)]\nf(); g(";
        let (stack, _) = scan_to(src, src.len());
        assert_eq!(stack.len(), 1);
        let src = "<?php\nf('it\\'s', ";
        let (stack, string) = scan_to(src, src.len());
        assert_eq!((stack[0].2, string), (1, None));
    }
}
