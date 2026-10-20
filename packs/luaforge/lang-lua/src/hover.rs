//! Hover documentation from the index and EmmyLua/LuaLS annotations, as the Markdown AST of
//! `forge-markdown` (the same [`HoverDoc`] the UI renders for language server hovers).

use forge_markdown::{parse, parse_inline, Flavor, HoverDoc, HoverSection};
use serde::Serialize;

use crate::annotations::DocBlock;
use crate::model::*;
use crate::offsets::{byte_to_utf16, utf16_to_byte};
use crate::project::{word_at, DeclRef, ProjectIndex, Table};

/// A hover popup: the documentation and the UTF-16 range it is about.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hover {
    pub doc: HoverDoc,
    pub start: usize,
    pub end: usize,
    /// Language of `doc.signature`, for highlighting.
    pub language: &'static str,
}

fn inline(md: &str) -> Vec<forge_markdown::Inline> {
    parse_inline(md, Flavor::Docs)
}

fn entry(name: &str, ty: &str, description: &str) -> Vec<forge_markdown::Inline> {
    let mut md = format!("`{name}`");
    if !ty.is_empty() {
        md.push_str(&format!(" `{ty}`"));
    }
    if !description.is_empty() {
        md.push_str(&format!(" – {description}"));
    }
    inline(&md)
}

fn sections_of(doc: &DocBlock, out: &mut HoverDoc) {
    if !doc.params.is_empty() {
        out.sections.push(HoverSection {
            title: "Params:".into(),
            entries: doc
                .params
                .iter()
                .map(|p| {
                    let name = if p.optional {
                        format!("{}?", p.name)
                    } else {
                        p.name.clone()
                    };
                    entry(&name, &p.ty, &p.description)
                })
                .collect(),
        });
    }
    if !doc.returns.is_empty() {
        out.sections.push(HoverSection {
            title: "Returns:".into(),
            entries: doc
                .returns
                .iter()
                .map(|r| {
                    let mut md = format!("`{}`", r.ty);
                    if let Some(n) = &r.name {
                        md.push_str(&format!(" {n}"));
                    }
                    if !r.description.is_empty() {
                        md.push_str(&format!(" – {}", r.description));
                    }
                    inline(&md)
                })
                .collect(),
        });
    }
    if !doc.see.is_empty() {
        out.sections.push(HoverSection {
            title: "See also:".into(),
            entries: doc.see.iter().map(|s| inline(&format!("`{s}`"))).collect(),
        });
    }
    if let Some(d) = &doc.deprecated {
        out.deprecated = Some(inline(d));
    }
}

/// Hover at a UTF-16 offset of an indexed file.
pub fn hover(index: &ProjectIndex, path: &str, offset: usize) -> Option<Hover> {
    let file = index.file(path)?;
    let text = index.text(path)?;
    let byte = utf16_to_byte(text, offset);
    let range = |s: usize, e: usize| (byte_to_utf16(text, s), byte_to_utf16(text, e));
    let make = |doc: HoverDoc, (start, end): (usize, usize)| Hover {
        doc,
        start,
        end,
        language: "lua",
    };

    if let Some(req) = file.require_at(byte) {
        return Some(make(
            module_doc(index, &req.module),
            range(req.start, req.end),
        ));
    }
    if file.in_doc_comment(byte) {
        let word = word_at(text, byte);
        let doc = type_doc(index, word)?;
        let start = text[..byte]
            .rfind(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.'))
            .map(|i| i + 1)
            .unwrap_or(0);
        return Some(make(doc, range(start, start + word.len())));
    }
    if let Some(r) = file.ref_at(byte) {
        let doc = match &r.target {
            Target::Local(i) => {
                let d = index.decl(path, *i)?;
                decl_doc(index, d)
            }
            Target::Global(name) => {
                let globals: Vec<DeclRef<'_>> = index.search_globals(name).into_iter().collect();
                match globals
                    .iter()
                    .find(|d| d.decl.doc.is_some())
                    .or(globals.first())
                {
                    Some(d) => decl_doc(index, *d),
                    None => type_doc(index, name)?,
                }
            }
            Target::Field(base, name) => {
                let table = index.resolve_value(path, base, 0)?;
                let members = index.members(&table, Some(name), 0);
                match members
                    .iter()
                    .find(|d| d.decl.doc.is_some())
                    .or(members.first())
                {
                    Some(d) => decl_doc(index, *d),
                    None => class_field_doc(index, &table, name)?,
                }
            }
        };
        return Some(make(doc, range(r.start, r.end)));
    }
    let (i, _) = file.decl_at(byte)?;
    let d = index.decl(path, i)?;
    Some(make(decl_doc(index, d), range(d.decl.start, d.decl.end)))
}

/// The type text of a declaration for its signature line.
fn type_text(index: &ProjectIndex, d: DeclRef<'_>) -> Option<String> {
    if let Some(t) = d.decl.annotated_type() {
        return Some(t.to_string());
    }
    match &d.decl.value {
        Value::Literal(l) => Some(l.to_string()),
        Value::Typed(t) => Some(t.clone()),
        Value::Function => Some("function".into()),
        Value::Require(_) => None,
        v => match index.resolve_value(d.path, v, 0)? {
            Table::Class(c) => Some(c),
            _ => Some("table".into()),
        },
    }
}

fn params_text(d: &Decl) -> String {
    let doc = d.doc.as_ref();
    let params: Vec<String> = d
        .params
        .iter()
        .map(|p| match doc.and_then(|doc| doc.param(p)) {
            Some(a) if !a.ty.is_empty() => {
                format!("{p}{}: {}", if a.optional { "?" } else { "" }, a.ty)
            }
            _ => p.clone(),
        })
        .collect();
    let mut s = format!("({})", params.join(", "));
    if let Some(doc) = doc.filter(|doc| !doc.returns.is_empty()) {
        let rets: Vec<&str> = doc.returns.iter().map(|r| r.ty.as_str()).collect();
        s.push_str(&format!(": {}", rets.join(", ")));
    }
    s
}

/// Documentation of a declaration.
pub fn decl_doc(index: &ProjectIndex, d: DeclRef<'_>) -> HoverDoc {
    let decl = d.decl;
    if let Some(class) = &decl.class {
        if let Some(doc) = type_doc(index, class) {
            return doc;
        }
    }
    // `local Player = require("player")` where the module returns a class: the class.
    if let (DeclKind::Local, Value::Require(m)) = (decl.kind, &decl.value) {
        if let Some(Table::Class(c)) = index.resolve_value(d.path, &decl.value, 0) {
            if let Some(mut doc) = type_doc(index, &c) {
                doc.definition = index.resolve_require(m);
                return doc;
            }
        }
    }
    let mut out = HoverDoc::default();
    // The owner: a class name, else the table expression.
    if decl.kind.is_field() {
        let owner = decl
            .owner
            .as_ref()
            .and_then(|o| index.resolve_value(d.path, o, 0));
        out.definition = Some(match owner {
            Some(Table::Class(c)) => c,
            _ => decl.container.clone(),
        })
        .filter(|s| !s.is_empty());
    }
    let container = if decl.container.is_empty() {
        String::new()
    } else {
        decl.container.clone()
    };
    let attrib = decl
        .attrib
        .as_ref()
        .map(|a| format!(" <{a}>"))
        .unwrap_or_default();
    let typed = |prefix: String| match type_text(index, d) {
        Some(t) => format!("{prefix}: {t}"),
        None => prefix,
    };
    let signature = match decl.kind {
        DeclKind::LocalFunction => format!("local function {}{}", decl.name, params_text(decl)),
        DeclKind::GlobalFunction => format!("function {}{}", decl.name, params_text(decl)),
        DeclKind::FieldFunction => {
            format!("function {container}.{}{}", decl.name, params_text(decl))
        }
        DeclKind::Method => format!("function {container}:{}{}", decl.name, params_text(decl)),
        DeclKind::Local => match &decl.value {
            Value::Require(m) if decl.annotated_type().is_none() => {
                format!("local {}{attrib} = require(\"{m}\")", decl.name)
            }
            _ => typed(format!("local {}{attrib}", decl.name)),
        },
        DeclKind::Param if decl.name == "self" => typed("(self) self".to_string()),
        DeclKind::Param => typed(format!("(parameter) {}", decl.name)),
        DeclKind::ForVar => typed(format!("(loop variable) {}", decl.name)),
        DeclKind::Global => typed(decl.name.clone()),
        DeclKind::Field => typed(format!("{container}.{}", decl.name)),
    };
    out.signature = Some(signature);
    let mut description = decl
        .doc
        .as_ref()
        .map(|d| d.description.clone())
        .unwrap_or_default();
    // Parameters document themselves through the function's `---@param`.
    if decl.kind == DeclKind::Param && description.is_empty() {
        if let Some(desc) = param_description(index, d) {
            description = desc;
        }
    }
    if let Value::Require(m) = &decl.value {
        if let Some(target) = index.resolve_require(m) {
            description = format!("{description}\n\nModule `{m}` from `{target}`")
                .trim()
                .to_string();
        }
    }
    out.body = parse(&description, Flavor::Docs);
    if let Some(doc) = &decl.doc {
        sections_of(doc, &mut out);
    }
    out
}

/// `---@param` description of a parameter, from the function that declares it.
fn param_description(index: &ProjectIndex, d: DeclRef<'_>) -> Option<String> {
    let file = index.file(d.path)?;
    file.decls[..d.index]
        .iter()
        .rev()
        .find(|f| f.kind.is_function() && f.params.contains(&d.decl.name))
        .and_then(|f| f.doc.as_ref())
        .and_then(|doc| doc.param(&d.decl.name))
        .map(|p| p.description.clone())
        .filter(|s| !s.is_empty())
}

/// Documentation of a `---@class` or `---@alias`.
pub fn type_doc(index: &ProjectIndex, name: &str) -> Option<HoverDoc> {
    let mut out = HoverDoc::default();
    if let Some((_, class)) = index.class(name) {
        let mut sig = format!("class {}", class.ann.name);
        if !class.ann.parents.is_empty() {
            sig.push_str(&format!(" : {}", class.ann.parents.join(", ")));
        }
        out.signature = Some(sig);
        let desc = [class.doc.as_str(), class.ann.description.as_str()]
            .iter()
            .filter(|s| !s.is_empty())
            .copied()
            .collect::<Vec<_>>()
            .join("\n\n");
        out.body = parse(&desc, Flavor::Docs);
        if !class.ann.fields.is_empty() {
            out.sections.push(HoverSection {
                title: "Fields:".into(),
                entries: class
                    .ann
                    .fields
                    .iter()
                    .map(|f| entry(&f.name, &f.ty, &f.description))
                    .collect(),
            });
        }
        return Some(out);
    }
    let (_, alias) = index.alias(name)?;
    out.signature = Some(format!("alias {} = {}", alias.ann.name, alias.ann.ty));
    out.body = parse(alias.ann.description.trim(), Flavor::Docs);
    Some(out)
}

fn class_field_doc(index: &ProjectIndex, table: &Table, name: &str) -> Option<HoverDoc> {
    let Table::Class(c) = table else { return None };
    let (_, class) = index.class(c)?;
    let field = class.ann.fields.iter().find(|f| f.name == name)?;
    Some(HoverDoc {
        definition: Some(c.clone()),
        signature: Some(format!("{c}.{}: {}", field.name, field.ty)),
        body: parse(&field.description, Flavor::Docs),
        ..HoverDoc::default()
    })
}

/// A hover for a `require`d module: its name, file and the returned table's documentation.
fn module_doc(index: &ProjectIndex, module: &str) -> HoverDoc {
    let mut out = HoverDoc {
        signature: Some(format!("require(\"{module}\")")),
        ..HoverDoc::default()
    };
    let Some(target) = index.resolve_require(module) else {
        out.body = parse(
            &format!("Module `{module}` not found in the project."),
            Flavor::Docs,
        );
        return out;
    };
    let mut desc = String::new();
    if let Some(file) = index.file(&target) {
        if let Some(Value::Local(i)) = &file.returns {
            if let Some(doc) = file.decls.get(*i).and_then(|d| d.doc.as_ref()) {
                desc = doc.description.clone();
            }
            if let Some(class) = file.decls.get(*i).and_then(|d| d.class.as_ref()) {
                if let Some((_, c)) = index.class(class) {
                    if desc.is_empty() {
                        desc = c.doc.clone();
                    }
                }
            }
        }
    }
    out.definition = Some(target.clone());
    out.body = parse(desc.trim(), Flavor::Docs);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_markdown::Inline;

    fn project(files: &[(&str, &str)]) -> ProjectIndex {
        let mut p = ProjectIndex::new(&[]);
        for (path, text) in files {
            p.set_file(path, text);
        }
        p
    }

    fn text_of(inlines: &[Inline]) -> String {
        Inline::text_of(inlines)
    }

    const PLAYER: &str = r#"---@class Player : Entity
---@field name string Display name
local Player = {}

--- Moves the player by a delta.
---
--- Collisions are **not** checked.
---@param dx number Horizontal speed
---@param dy? number
---@return boolean moved # whether it moved
---@deprecated Use `walk`
function Player:move(dx, dy) return true end

return Player
"#;

    #[test]
    fn function_hover_with_annotations() {
        let main = "local Player = require(\"player\")\nlocal p = Player\np:move(1, 2)\n";
        let p = project(&[("player.lua", PLAYER), ("main.lua", main)]);
        let at = main.find("move").unwrap() + 1;
        let h = hover(&p, "main.lua", at).unwrap();
        assert_eq!(
            h.doc.signature.as_deref(),
            Some("function Player:move(dx: number, dy?: number): boolean")
        );
        assert_eq!(h.doc.definition.as_deref(), Some("Player"));
        assert_eq!(h.doc.sections[0].title, "Params:");
        assert_eq!(
            text_of(&h.doc.sections[0].entries[0]),
            "dx number – Horizontal speed"
        );
        assert_eq!(h.doc.sections[1].title, "Returns:");
        assert_eq!(
            text_of(&h.doc.sections[1].entries[0]),
            "boolean moved – whether it moved"
        );
        assert!(h.doc.deprecated.is_some());
        assert_eq!(
            h.body_text(),
            "Moves the player by a delta.Collisions are not checked."
        );
        assert_eq!(
            (h.start, h.end),
            (main.find("move").unwrap(), main.find("move").unwrap() + 4)
        );
    }

    #[test]
    fn class_require_and_local_hovers() {
        let main = "local Player = require(\"player\")\n---@type Player\nlocal hero = nil\nprint(hero.name)\n";
        let p = project(&[("player.lua", PLAYER), ("main.lua", main)]);
        let h = hover(&p, "main.lua", main.find("\"player\"").unwrap() + 2).unwrap();
        assert_eq!(h.doc.signature.as_deref(), Some("require(\"player\")"));
        assert_eq!(h.doc.definition.as_deref(), Some("player.lua"));
        let h = hover(&p, "main.lua", main.find("hero.name").unwrap() + 1).unwrap();
        assert_eq!(h.doc.signature.as_deref(), Some("local hero: Player"));
        let h = hover(&p, "main.lua", main.find(".name").unwrap() + 2).unwrap();
        assert_eq!(h.doc.signature.as_deref(), Some("Player.name: string"));
        let h = hover(&p, "main.lua", main.find("@type Player").unwrap() + 8).unwrap();
        assert_eq!(h.doc.signature.as_deref(), Some("class Player : Entity"));
        assert_eq!(h.doc.sections[0].title, "Fields:");
        let h = hover(&p, "main.lua", 7).unwrap();
        assert_eq!(h.doc.signature.as_deref(), Some("class Player : Entity"));
    }

    #[test]
    fn parameter_hover_uses_param_docs() {
        let p = project(&[("player.lua", PLAYER)]);
        let at = PLAYER.find("(dx").unwrap() + 2;
        let h = hover(&p, "player.lua", at).unwrap();
        assert_eq!(h.doc.signature.as_deref(), Some("(parameter) dx: number"));
        assert_eq!(h.body_text(), "Horizontal speed");
    }

    impl Hover {
        fn body_text(&self) -> String {
            self.doc
                .body
                .iter()
                .map(|b| match b {
                    forge_markdown::Block::Paragraph { inline } => Inline::text_of(inline),
                    _ => String::new(),
                })
                .collect()
        }
    }
}
