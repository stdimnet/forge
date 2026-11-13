//! EmmyLua / LuaLS annotations (`---@class`, `---@field`, `---@param`, `---@return`,
//! `---@type`, `---@alias`, `---@deprecated`, `---@see`) and the Markdown description of a
//! doc comment block.

use serde::Serialize;

/// `---@class Name : Parent, Other description`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassAnn {
    pub name: String,
    pub parents: Vec<String>,
    pub description: String,
    /// Byte offset of the name in the source.
    pub offset: usize,
    pub fields: Vec<FieldAnn>,
}

/// `---@field [visibility] name[?] type description`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldAnn {
    pub name: String,
    pub ty: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility: Option<String>,
    pub optional: bool,
    pub offset: usize,
}

/// `---@param name[?] type description`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParamAnn {
    pub name: String,
    pub ty: String,
    pub optional: bool,
    pub description: String,
}

/// `---@return type [name] [# description]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReturnAnn {
    pub ty: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub description: String,
}

/// `---@alias Name type`, with `---| value # description` lines for enums.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AliasAnn {
    pub name: String,
    pub ty: String,
    pub description: String,
    pub offset: usize,
}

/// One block of consecutive `---` comment lines.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocBlock {
    /// Markdown of the plain (non-tag) lines.
    pub description: String,
    pub classes: Vec<ClassAnn>,
    pub params: Vec<ParamAnn>,
    pub returns: Vec<ReturnAnn>,
    /// `---@type`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ty: Option<String>,
    pub aliases: Vec<AliasAnn>,
    /// Fields declared before any `@class` in the block.
    pub fields: Vec<FieldAnn>,
    /// `Some("")` for a bare `---@deprecated`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<String>,
    pub see: Vec<String>,
    /// Byte offset where the block starts.
    pub start: usize,
    /// Byte offset where the block ends.
    pub end: usize,
}

impl DocBlock {
    pub fn is_empty(&self) -> bool {
        self.description.trim().is_empty()
            && self.classes.is_empty()
            && self.params.is_empty()
            && self.returns.is_empty()
            && self.ty.is_none()
            && self.aliases.is_empty()
            && self.fields.is_empty()
            && self.deprecated.is_none()
            && self.see.is_empty()
    }

    pub fn param(&self, name: &str) -> Option<&ParamAnn> {
        self.params.iter().find(|p| p.name == name)
    }
}

fn is_name_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '.' | '-' | '*' | '`')
}

/// End index (exclusive) of the bracket group starting at `s[0]`.
fn balanced(s: &str, open: char, close: char) -> usize {
    let mut depth = 0;
    let mut quote: Option<char> = None;
    for (i, c) in s.char_indices() {
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '"' | '\'' => quote = Some(c),
            c if c == open => depth += 1,
            c if c == close => {
                depth -= 1;
                if depth == 0 {
                    return i + c.len_utf8();
                }
            }
            _ => {}
        }
    }
    s.len()
}

/// Split a type expression off the start of `s`: `(type, rest)`. Understands unions
/// (`a | b`), optionals (`T?`), arrays (`T[]`), generics (`table<K, V>`), function types
/// (`fun(a: T): R`), table literals (`{ x: number }`) and string literals.
pub fn take_type(s: &str) -> (String, &str) {
    let mut rest = s.trim_start();
    let mut out = String::new();
    loop {
        let atom_len = if rest.starts_with('"') || rest.starts_with('\'') || rest.starts_with('`') {
            let q = rest.chars().next().unwrap_or('"');
            rest[1..].find(q).map(|i| i + 2).unwrap_or(rest.len())
        } else if rest.starts_with("fun(") {
            let mut n = 3 + balanced(&rest[3..], '(', ')');
            let after = &rest[n..];
            let trimmed = after.trim_start();
            if let Some(ret) = trimmed.strip_prefix(':') {
                let (_, tail) = take_type(ret);
                n = rest.len() - tail.len();
                // Do not swallow the whitespace before a description.
                n = rest[..n].trim_end().len();
            }
            n
        } else if rest.starts_with('{') {
            balanced(rest, '{', '}')
        } else if rest.starts_with('(') {
            balanced(rest, '(', ')')
        } else {
            rest.find(|c: char| !is_name_char(c)).unwrap_or(rest.len())
        };
        if atom_len == 0 {
            break;
        }
        let mut len = atom_len;
        // Suffixes: generics, arrays, optional marker.
        loop {
            let tail = &rest[len..];
            if tail.starts_with('<') {
                len += balanced(tail, '<', '>');
            } else if tail.starts_with("[]") {
                len += 2;
            } else if tail.starts_with('?') {
                len += 1;
            } else {
                break;
            }
        }
        out.push_str(&rest[..len]);
        rest = &rest[len..];
        let after = rest.trim_start();
        if let Some(next) = after.strip_prefix('|') {
            out.push_str(" | ");
            rest = next.trim_start();
            continue;
        }
        break;
    }
    (out.trim().to_string(), rest)
}

/// A description after a tag: `# text`, `@ text` or plain text.
fn description(s: &str) -> String {
    let t = s.trim();
    let t = t
        .strip_prefix('#')
        .or_else(|| t.strip_prefix('@'))
        .unwrap_or(t);
    t.trim().to_string()
}

/// `name` (or `...`) at the start of `s`: (name, offset in s, rest).
fn take_name(s: &str) -> Option<(&str, usize, &str)> {
    let lead = s.len() - s.trim_start().len();
    let t = &s[lead..];
    if let Some(rest) = t.strip_prefix("...") {
        return Some(("...", lead, rest));
    }
    let len = t
        .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.'))
        .unwrap_or(t.len());
    (len > 0).then(|| (&t[..len], lead, &t[len..]))
}

/// Parse the lines of one doc block. Each line is the comment body after `---` and the byte
/// offset where that body starts in the source.
pub fn parse_block(lines: &[(&str, usize)]) -> DocBlock {
    let mut doc = DocBlock::default();
    let mut desc: Vec<String> = Vec::new();
    // Where enum lines (`---| …`) go.
    let mut last_alias: Option<usize> = None;
    for &(line, base) in lines {
        // `---|` alias continuation lines.
        if let Some(value) = line.strip_prefix('|') {
            if let Some(a) = last_alias.and_then(|i| doc.aliases.get_mut(i)) {
                let (ty, rest) = take_type(value.trim_start_matches(['>', '+']));
                if !ty.is_empty() {
                    if !a.ty.is_empty() {
                        a.ty.push_str(" | ");
                    }
                    a.ty.push_str(&ty);
                }
                let d = description(rest);
                if !d.is_empty() {
                    a.description.push_str(&format!("\n- `{ty}`: {d}"));
                }
            }
            continue;
        }
        let trimmed = line.trim_start();
        let Some(tagged) = trimmed.strip_prefix('@') else {
            last_alias = None;
            desc.push(
                line.strip_prefix(' ')
                    .unwrap_or(line)
                    .trim_end()
                    .to_string(),
            );
            continue;
        };
        let tag_len = tagged
            .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .unwrap_or(tagged.len());
        let tag = &tagged[..tag_len];
        let body = &tagged[tag_len..];
        let body_base = base + (line.len() - body.len());
        match tag {
            "class" => {
                let mut body = body.trim_start();
                if body.starts_with('(') {
                    body = &body[balanced(body, '(', ')')..];
                }
                if let Some((name, at, rest)) = take_name(body) {
                    let offset = body_base + (tagged.len() - tag_len - body.len()) + at;
                    let mut rest = rest.trim_start();
                    let mut parents = Vec::new();
                    if let Some(p) = rest.strip_prefix(':') {
                        rest = p;
                        loop {
                            let (ty, tail) = take_type(rest);
                            if ty.is_empty() {
                                break;
                            }
                            parents.push(ty);
                            match tail.trim_start().strip_prefix(',') {
                                Some(t) => rest = t,
                                None => {
                                    rest = tail;
                                    break;
                                }
                            }
                        }
                    }
                    doc.classes.push(ClassAnn {
                        name: name.to_string(),
                        parents,
                        description: description(rest),
                        offset,
                        fields: Vec::new(),
                    });
                }
            }
            "field" => {
                let mut b = body.trim_start();
                let mut visibility = None;
                for v in ["public", "protected", "private", "package"] {
                    if let Some(r) = b.strip_prefix(v).filter(|r| r.starts_with(' ')) {
                        visibility = Some(v.to_string());
                        b = r.trim_start();
                    }
                }
                let lead = body.len() - b.len();
                let (name, at, rest) = if b.starts_with('[') {
                    let n = balanced(b, '[', ']');
                    (&b[..n], 0, &b[n..])
                } else {
                    match take_name(b) {
                        Some(n) => n,
                        None => continue,
                    }
                };
                let (optional, rest) = match rest.strip_prefix('?') {
                    Some(r) => (true, r),
                    None => (false, rest),
                };
                let (ty, rest) = take_type(rest);
                let field = FieldAnn {
                    name: name.to_string(),
                    ty,
                    description: description(rest),
                    visibility,
                    optional,
                    offset: body_base + lead + at,
                };
                match doc.classes.last_mut() {
                    Some(c) => c.fields.push(field),
                    None => doc.fields.push(field),
                }
            }
            "param" => {
                if let Some((name, _, rest)) = take_name(body) {
                    let (optional, rest) = match rest.strip_prefix('?') {
                        Some(r) => (true, r),
                        None => (false, rest),
                    };
                    let (ty, rest) = take_type(rest);
                    doc.params.push(ParamAnn {
                        name: name.to_string(),
                        ty,
                        optional,
                        description: description(rest),
                    });
                }
            }
            "return" => {
                let (ty, rest) = take_type(body);
                let mut rest = rest;
                let mut name = None;
                let t = rest.trim_start();
                if !t.starts_with('#') {
                    if let Some((n, _, r)) = take_name(t) {
                        // A single word followed by `#` or nothing is the value's name.
                        let after = r.trim_start();
                        if after.is_empty() || after.starts_with('#') {
                            name = Some(n.to_string());
                            rest = r;
                        }
                    }
                }
                doc.returns.push(ReturnAnn {
                    ty,
                    name,
                    description: description(rest),
                });
            }
            "type" => {
                let (ty, _) = take_type(body);
                doc.ty = Some(ty);
            }
            "alias" => {
                if let Some((name, at, rest)) = take_name(body) {
                    let (ty, rest) = take_type(rest);
                    doc.aliases.push(AliasAnn {
                        name: name.to_string(),
                        ty,
                        description: description(rest),
                        offset: body_base + at,
                    });
                    last_alias = Some(doc.aliases.len() - 1);
                    continue;
                }
            }
            "deprecated" => doc.deprecated = Some(description(body)),
            "see" => doc.see.push(body.trim().to_string()),
            _ => {}
        }
        last_alias = None;
    }
    while desc.last().is_some_and(|l| l.trim().is_empty()) {
        desc.pop();
    }
    while desc.first().is_some_and(|l| l.trim().is_empty()) {
        desc.remove(0);
    }
    doc.description = desc.join("\n");
    doc
}

/// The first type of a union without `nil`/optional markers: `Player|nil` -> `Player`.
pub fn primary_type(ty: &str) -> &str {
    ty.split('|')
        .map(|t| t.trim().trim_end_matches('?'))
        .find(|t| !t.is_empty() && *t != "nil")
        .unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(src: &str) -> DocBlock {
        let mut offset = 0;
        let lines: Vec<(&str, usize)> = src
            .lines()
            .map(|l| {
                let body = l.trim_start().strip_prefix("---").unwrap_or(l);
                let at = offset + (l.len() - body.len());
                offset += l.len() + 1;
                (body, at)
            })
            .collect();
        parse_block(&lines)
    }

    #[test]
    fn types_with_unions_generics_and_functions() {
        assert_eq!(
            take_type("string|nil rest"),
            ("string | nil".into(), " rest")
        );
        assert_eq!(
            take_type("table<string, number> the map"),
            ("table<string, number>".into(), " the map")
        );
        assert_eq!(
            take_type("fun(a: number, b: string): boolean callback"),
            ("fun(a: number, b: string): boolean".into(), " callback")
        );
        assert_eq!(take_type("Vec2[]? x"), ("Vec2[]?".into(), " x"));
        assert_eq!(
            take_type("\"left\" | \"right\""),
            ("\"left\" | \"right\"".into(), "")
        );
        assert_eq!(primary_type("Player|nil"), "Player");
    }

    #[test]
    fn class_with_parents_and_fields() {
        let src = "--- A thing that moves.\n---@class Player : Entity, Drawable the hero\n---@field name string Display name\n---@field private hp? integer\n---@field [string] any";
        let doc = block(src);
        assert_eq!(doc.description, "A thing that moves.");
        let c = &doc.classes[0];
        assert_eq!(c.name, "Player");
        assert_eq!(c.parents, vec!["Entity", "Drawable"]);
        assert_eq!(c.description, "the hero");
        assert_eq!(&src[c.offset..c.offset + 6], "Player");
        assert_eq!(c.fields.len(), 3);
        assert_eq!(c.fields[0].name, "name");
        assert_eq!(c.fields[0].ty, "string");
        assert_eq!(c.fields[0].description, "Display name");
        assert_eq!(&src[c.fields[0].offset..c.fields[0].offset + 4], "name");
        assert_eq!(c.fields[1].visibility.as_deref(), Some("private"));
        assert!(c.fields[1].optional);
        assert_eq!(c.fields[2].name, "[string]");
    }

    #[test]
    fn params_returns_type_deprecated() {
        let doc = block(
            "--- Moves the player.\n---\n--- Uses **delta** time.\n---@param dx number Horizontal speed\n---@param opts? {fast: boolean}\n---@param ... any\n---@return boolean ok # whether it moved\n---@return string|nil err\n---@deprecated use `walk`\n---@see Player.walk",
        );
        assert_eq!(doc.description, "Moves the player.\n\nUses **delta** time.");
        assert_eq!(doc.params[0].name, "dx");
        assert_eq!(doc.params[0].ty, "number");
        assert_eq!(doc.params[0].description, "Horizontal speed");
        assert!(doc.params[1].optional);
        assert_eq!(doc.params[1].ty, "{fast: boolean}");
        assert_eq!(doc.params[2].name, "...");
        assert_eq!(doc.returns[0].ty, "boolean");
        assert_eq!(doc.returns[0].name.as_deref(), Some("ok"));
        assert_eq!(doc.returns[0].description, "whether it moved");
        assert_eq!(doc.returns[1].ty, "string | nil");
        assert_eq!(doc.returns[1].name.as_deref(), Some("err"));
        assert_eq!(doc.deprecated.as_deref(), Some("use `walk`"));
        assert_eq!(doc.see, vec!["Player.walk"]);
        assert_eq!(
            block("---@type table<string, Item>").ty.as_deref(),
            Some("table<string, Item>")
        );
    }

    #[test]
    fn alias_with_enum_lines() {
        let doc = block(
            "---@alias Direction\n---| \"up\" # Move up\n---| \"down\"\n---@alias Id integer",
        );
        assert_eq!(doc.aliases[0].name, "Direction");
        assert_eq!(doc.aliases[0].ty, "\"up\" | \"down\"");
        assert!(doc.aliases[0].description.contains("Move up"));
        assert_eq!(doc.aliases[1].ty, "integer");
    }
}
