//! JSON Schema support for JSON, JSON5/JSONC and YAML files, like PHPStorm's: validation
//! with PHPStorm's messages, completion of property names and values, documentation from
//! `description` / `markdownDescription`, Go to Declaration into the schema, and the schema
//! mappings (built-in ones for composer.json, package.json, Compose, CI files, ..., user
//! mappings, `$schema` in the file and the SchemaStore catalog).
//!
//! Everything here is pure (it also builds to WebAssembly for browser mode). Loading schema
//! files is up to the caller: [`Registry::missing`] lists the documents `$ref`s point to
//! that are not loaded yet. The `remote` feature adds [`remote::RemoteCache`] for schema
//! downloads into the user cache dir.
//!
//! A file mapped to the Kubernetes definitions ([`kubernetes`]) checks each document against
//! the schema of its own `apiVersion` / `kind`.
//!
//! Offsets in the answers of this module are UTF-16 (CodeMirror positions).

pub mod bundled;
pub mod complete;
pub mod format;
pub mod json;
pub mod kubernetes;
pub mod mapping;
pub mod navigate;
#[cfg(feature = "remote")]
pub mod remote;
pub mod schema;
pub mod text;
pub mod tree;
pub mod uri;
pub mod validate;
pub mod yaml;

use serde::{Deserialize, Serialize};

pub use mapping::{Catalog, Selection, Source, UserMapping};
pub use schema::{Draft, Registry, Sch, SchemaDoc};
use text::Utf16;
use tree::{Node, Seg};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DocKind {
    Json,
    Yaml,
}

impl DocKind {
    /// From the editor's language id (`json`, `yaml`, `docker-compose`, ...).
    pub fn from_language(id: &str) -> Option<DocKind> {
        match id {
            "json" | "jsonc" | "json5" => Some(DocKind::Json),
            "yaml" | "docker-compose" | "symfony-services" => Some(DocKind::Yaml),
            _ => None,
        }
    }

    /// From the file extension, for callers without a language id.
    pub fn from_path(path: &str) -> Option<DocKind> {
        let name = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
        let ext = name.rsplit('.').next().unwrap_or("");
        match ext {
            "json" | "jsonc" | "json5" => Some(DocKind::Json),
            "yml" | "yaml" => Some(DocKind::Yaml),
            _ if [".eslintrc", ".prettierrc", ".babelrc"].contains(&name.as_str()) => {
                Some(DocKind::Json)
            }
            _ => None,
        }
    }
}

/// The documents of a file (YAML may have several).
pub fn parse(kind: DocKind, text: &str) -> Vec<Node> {
    match kind {
        DocKind::Json => json::parse(text).into_iter().collect(),
        DocKind::Yaml => yaml::parse(text),
    }
}

/// The schema document `doc` is checked against: the file's schema, or for Kubernetes
/// manifests the schema of the document's kind.
pub fn document_root<'r>(reg: &'r Registry, root_uri: &str, doc: &Node) -> Option<Sch<'r>> {
    reg.get(&kubernetes::document_root(root_uri, doc)?)
}

/// Whether document `i` contains template expressions, between the end of the document
/// before it and the start of the next one (`{{- if … }}` lines around it count).
fn is_template(text: &str, docs: &[Node], i: usize) -> bool {
    let from = if i == 0 { 0 } else { docs[i - 1].end };
    let to = docs.get(i + 1).map_or(text.len(), |d| d.start);
    text.get(from.min(to)..to).is_some_and(|t| t.contains("{{"))
}

/// The document containing byte `offset` (the last one starting before it).
pub fn document_at(docs: &[Node], offset: usize) -> Option<&Node> {
    docs.iter()
        .rev()
        .find(|d| d.start <= offset)
        .or_else(|| docs.first())
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProblemOut {
    pub from: usize,
    pub to: usize,
    pub message: String,
    pub severity: validate::Severity,
}

/// Problems of every document of the file against the schema at `root_uri`.
pub fn validate(reg: &Registry, root_uri: &str, kind: DocKind, text: &str) -> Vec<ProblemOut> {
    let u = Utf16::new(text);
    let docs = parse(kind, text);
    let dispatch = kubernetes::dispatches(root_uri);
    docs.iter()
        .enumerate()
        .flat_map(|(i, d)| {
            // A Helm chart's template (`{{ … }}`) is no manifest until it is rendered.
            if dispatch && is_template(text, &docs, i) {
                return Vec::new();
            }
            match document_root(reg, root_uri, d) {
                Some(root) => validate::validate(reg, root, d),
                None => Vec::new(),
            }
        })
        .map(|p| ProblemOut {
            from: u.of(p.start),
            to: u.of(p.end.max(p.start)),
            message: p.message,
            severity: p.severity,
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionOut {
    pub from: usize,
    pub replace_from: usize,
    pub replace_to: usize,
    pub items: Vec<complete::Item>,
}

/// Completion at UTF-16 `offset`.
pub fn complete(
    reg: &Registry,
    root_uri: &str,
    kind: DocKind,
    text: &str,
    offset: usize,
) -> Option<CompletionOut> {
    let byte = text::byte_offset(text, offset);
    let docs = parse(kind, text);
    let root = match document_at(&docs, byte) {
        Some(doc) => document_root(reg, root_uri, doc)?,
        None => reg.get(root_uri)?,
    };
    let c = complete::complete(reg, root, kind, text, &docs, byte)?;
    let u = Utf16::new(text);
    Some(CompletionOut {
        from: u.of(c.from),
        replace_from: u.of(c.replace.0),
        replace_to: u.of(c.replace.1),
        items: c.items,
    })
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HoverOut {
    pub from: usize,
    pub to: usize,
    /// `name: type`, or the value's type.
    pub title: String,
    /// Markdown.
    pub markdown: String,
}

fn hit_schemas<'r>(
    reg: &'r Registry,
    root: Sch<'r>,
    doc: &Node,
    hit: &navigate::Hit<'_>,
) -> (Vec<navigate::At<'r>>, Vec<Sch<'r>>) {
    let (path, node): (Vec<Seg>, Option<&Node>) = match hit {
        navigate::Hit::Key(p, k, _) => {
            let mut p = p.clone();
            p.push(Seg::Key(k.to_string()));
            (p, None)
        }
        navigate::Hit::Value(p, n) => (p.clone(), Some(*n)),
    };
    let (ats, at_node) = navigate::at_path(reg, root, Some(doc), &path);
    let node = node.or(at_node);
    let mut schemas: Vec<Sch<'r>> = Vec::new();
    for at in &ats {
        for s in navigate::expand(reg, at.sch, node) {
            if !schemas.iter().any(|x| x.same(&s)) {
                schemas.push(s);
            }
        }
    }
    (ats, schemas)
}

/// Documentation of the key or value at UTF-16 `offset`.
pub fn hover(
    reg: &Registry,
    root_uri: &str,
    kind: DocKind,
    text: &str,
    offset: usize,
) -> Option<HoverOut> {
    let byte = text::byte_offset(text, offset);
    let docs = parse(kind, text);
    let doc = document_at(&docs, byte)?;
    let root = document_root(reg, root_uri, doc)?;
    let hit = navigate::locate(doc, byte)?;
    let (_, schemas) = hit_schemas(reg, root, doc, &hit);
    let u = Utf16::new(text);
    let ty = navigate::type_label(&schemas);
    match &hit {
        navigate::Hit::Key(_, key, (s, e)) => {
            let mut md = String::new();
            if let Some(t) = schemas.iter().find_map(|s| s.str("title")) {
                md.push_str(&format!("**{t}**\n\n"));
            }
            md.push_str(navigate::description(&schemas)?);
            Some(HoverOut {
                from: u.of(*s),
                to: u.of(*e),
                title: match ty {
                    Some(t) => format!("{key}: {t}"),
                    None => key.to_string(),
                },
                markdown: md,
            })
        }
        navigate::Hit::Value(_, n) => {
            // An enum value with its own description.
            let doc = schemas.iter().find_map(|s| {
                let values = s.get("enum")?.as_array()?;
                let i = values.iter().position(|v| v.same_value(n))?;
                s.get("markdownEnumDescriptions")
                    .or_else(|| s.get("enumDescriptions"))?
                    .as_array()?
                    .get(i)?
                    .as_str()
            })?;
            Some(HoverOut {
                from: u.of(n.start),
                to: u.of(n.end),
                title: n.to_json(),
                markdown: doc.to_string(),
            })
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    /// A project path or a `json-schema://` location.
    pub path: String,
    /// 0-based line and UTF-16 column.
    pub line: usize,
    pub column: usize,
    /// A line of the schema for the chooser.
    pub preview: String,
}

/// Go to Declaration: where the property at UTF-16 `offset` is declared in the schema.
pub fn goto(
    reg: &Registry,
    root_uri: &str,
    kind: DocKind,
    text: &str,
    offset: usize,
) -> Vec<Target> {
    let byte = text::byte_offset(text, offset);
    let docs = parse(kind, text);
    let Some(doc) = document_at(&docs, byte) else {
        return Vec::new();
    };
    let Some(root) = document_root(reg, root_uri, doc) else {
        return Vec::new();
    };
    let Some(hit @ navigate::Hit::Key(..)) = navigate::locate(doc, byte) else {
        return Vec::new();
    };
    let (ats, _) = hit_schemas(reg, root, doc, &hit);
    let mut out: Vec<Target> = Vec::new();
    for at in ats {
        let Some((sdoc, s, _)) = at.decl else {
            continue;
        };
        let (line, column) = text::line_col(&sdoc.text, s);
        let preview = sdoc.text.lines().nth(line).unwrap_or("").trim().to_string();
        let t = Target {
            path: sdoc.location.clone(),
            line,
            column,
            preview,
        };
        if !out.contains(&t) {
            out.push(t);
        }
    }
    out
}

/// A registry with every bundled schema the file's schema needs that Forge ships.
pub fn load_bundled(reg: &mut Registry, uri: &str) -> bool {
    match bundled::find(uri) {
        Some(b) => {
            bundled::load_into(reg, b);
            true
        }
        None => false,
    }
}

#[cfg(test)]
mod tests;
