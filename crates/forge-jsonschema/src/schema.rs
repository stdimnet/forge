//! Schema documents and the registry `$ref`s are resolved in. Schemas are parsed with the
//! position-keeping JSON parser, so Go to Declaration can point into them.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::tree::{Node, Seg, Value};
use crate::uri;

/// JSON Schema versions ("Schema version" of a mapping). Keywords of all versions are
/// understood; the version decides only where they disagree (`items` as an array,
/// boolean `exclusiveMinimum`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Draft {
    #[serde(rename = "4")]
    D4,
    #[serde(rename = "6")]
    D6,
    #[serde(rename = "7")]
    D7,
    #[serde(rename = "2019-09")]
    D2019,
    #[serde(rename = "2020-12")]
    D2020,
}

impl Draft {
    /// The version a `$schema` meta-schema URI names.
    pub fn from_meta(meta: &str) -> Option<Draft> {
        let m = meta.trim_end_matches('#');
        Some(if m.contains("draft-04") {
            Draft::D4
        } else if m.contains("draft-06") {
            Draft::D6
        } else if m.contains("draft-07") {
            Draft::D7
        } else if m.contains("2019-09") {
            Draft::D2019
        } else if m.contains("2020-12") {
            Draft::D2020
        } else {
            return None;
        })
    }

    /// A mapping's version setting: `4`, `6`, `7`, `2019-09`, `2020-12`; anything else is
    /// "auto" (the schema's own `$schema`).
    pub fn from_setting(s: &str) -> Option<Draft> {
        serde_json::from_value(serde_json::Value::String(s.trim().to_string())).ok()
    }
}

/// A loaded schema file.
#[derive(Debug)]
pub struct SchemaDoc {
    /// The URI it was loaded from (no fragment).
    pub uri: String,
    /// What the UI opens for Go to Declaration: a project path or a `json-schema://` URI.
    pub location: String,
    pub text: String,
    pub root: Node,
    pub draft: Option<Draft>,
    /// Absolute URIs (no fragment) of embedded resources (`$id`) → their path in the doc.
    ids: HashMap<String, Vec<Seg>>,
    /// `uri#name` of `$anchor`s (and draft-07 `"$id": "#name"`) → path.
    anchors: HashMap<String, Vec<Seg>>,
    /// Documents `$ref`s point to outside this one.
    external: Vec<String>,
}

impl SchemaDoc {
    pub fn parse(uri: &str, location: &str, text: String) -> Result<SchemaDoc, String> {
        let root = crate::json::parse(&text).ok_or_else(|| "empty schema".to_string())?;
        if !matches!(root.value, Value::Object(_) | Value::Bool(_)) {
            return Err("a schema must be an object".into());
        }
        let draft = root
            .get("$schema")
            .and_then(Node::as_str)
            .and_then(Draft::from_meta);
        let mut doc = SchemaDoc {
            uri: uri::document(uri).to_string(),
            location: location.to_string(),
            text: String::new(),
            root,
            draft,
            ids: HashMap::new(),
            anchors: HashMap::new(),
            external: Vec::new(),
        };
        let mut path = Vec::new();
        let mut refs = Vec::new();
        index(
            &doc.root,
            &doc.uri.clone(),
            draft,
            &mut path,
            &mut doc.ids,
            &mut doc.anchors,
            &mut refs,
        );
        let mut external: Vec<String> = Vec::new();
        for r in refs {
            let d = uri::document(&r).to_string();
            if d != doc.uri && !doc.ids.contains_key(&d) && !external.contains(&d) {
                external.push(d);
            }
        }
        doc.external = external;
        doc.text = text;
        Ok(doc)
    }

    /// The root `$id`, resolved (it may differ from the URI the file was loaded from).
    pub fn root_id(&self) -> Option<&str> {
        self.ids
            .iter()
            .find(|(_, p)| p.is_empty())
            .map(|(k, _)| k.as_str())
    }

    pub fn title(&self) -> Option<&str> {
        self.root.get("title").and_then(Node::as_str)
    }

    pub fn external_refs(&self) -> &[String] {
        &self.external
    }
}

fn id_keyword(draft: Option<Draft>) -> &'static str {
    if draft == Some(Draft::D4) {
        "id"
    } else {
        "$id"
    }
}

/// Walk every object of the schema: record `$id`s, anchors and `$ref`s with the base URI
/// in effect.
fn index(
    node: &Node,
    base: &str,
    draft: Option<Draft>,
    path: &mut Vec<Seg>,
    ids: &mut HashMap<String, Vec<Seg>>,
    anchors: &mut HashMap<String, Vec<Seg>>,
    refs: &mut Vec<String>,
) {
    if path.len() > 64 {
        return;
    }
    match &node.value {
        Value::Object(props) => {
            let mut base = base.to_string();
            if let Some(id) = node.get(id_keyword(draft)).and_then(Node::as_str) {
                if let Some(name) = id.strip_prefix('#') {
                    if !name.is_empty() {
                        anchors.insert(format!("{base}#{name}"), path.clone());
                    }
                } else {
                    let abs = uri::resolve(&base, id);
                    let (d, frag) = uri::split_fragment(&abs);
                    base = d.to_string();
                    ids.insert(base.clone(), path.clone());
                    if let Some(f) = frag.filter(|f| !f.is_empty() && !f.starts_with('/')) {
                        anchors.insert(format!("{base}#{f}"), path.clone());
                    }
                }
            }
            for kw in ["$anchor", "$dynamicAnchor"] {
                if let Some(a) = node.get(kw).and_then(Node::as_str) {
                    anchors.insert(format!("{base}#{a}"), path.clone());
                }
            }
            for kw in ["$ref", "$dynamicRef"] {
                if let Some(r) = node.get(kw).and_then(Node::as_str) {
                    if !r.starts_with('#') {
                        refs.push(uri::resolve(&base, r));
                    }
                }
            }
            for p in props {
                if let Some(v) = &p.value {
                    if matches!(v.value, Value::Object(_) | Value::Array(_)) {
                        path.push(Seg::Key(p.key.clone()));
                        index(v, &base, draft, path, ids, anchors, refs);
                        path.pop();
                    }
                }
            }
        }
        Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                if matches!(item.value, Value::Object(_) | Value::Array(_)) {
                    path.push(Seg::Index(i));
                    index(item, base, draft, path, ids, anchors, refs);
                    path.pop();
                }
            }
        }
        _ => {}
    }
}

/// Follow `path` from `node`.
pub fn walk<'a>(node: &'a Node, path: &[Seg]) -> Option<&'a Node> {
    let mut n = node;
    for seg in path {
        n = match (seg, &n.value) {
            (Seg::Key(k), Value::Object(_)) => n.get(k)?,
            (Seg::Index(i), Value::Array(items)) => items.get(*i)?,
            _ => return None,
        };
    }
    Some(n)
}

/// A JSON pointer (`/definitions/a~1b`) as path segments.
pub fn pointer(ptr: &str) -> Vec<String> {
    uri::percent_decode(ptr)
        .split('/')
        .skip(1)
        .map(|s| s.replace("~1", "/").replace("~0", "~"))
        .collect()
}

/// All loaded schema documents.
#[derive(Debug, Default, Clone)]
pub struct Registry {
    docs: HashMap<String, Arc<SchemaDoc>>,
    /// Embedded resources (`$id` inside a document, or a root `$id` differing from the
    /// URI the document was loaded from) → (document URI, path).
    embedded: HashMap<String, (String, Vec<Seg>)>,
    /// URIs that could not be loaded; refs to them accept anything.
    failed: HashMap<String, String>,
    /// The version from the mapping, overriding `$schema` of the documents.
    pub draft: Option<Draft>,
}

/// A schema in a document, with the base URI its `$ref`s resolve against.
#[derive(Debug, Clone, Copy)]
pub struct Sch<'r> {
    pub doc: &'r SchemaDoc,
    pub node: &'r Node,
    pub base: &'r str,
}

impl Registry {
    pub fn new() -> Registry {
        Registry::default()
    }

    pub fn insert(&mut self, doc: Arc<SchemaDoc>) {
        for (id, path) in &doc.ids {
            if id != &doc.uri {
                self.embedded
                    .insert(id.clone(), (doc.uri.clone(), path.clone()));
            }
        }
        self.failed.remove(&doc.uri);
        self.docs.insert(doc.uri.clone(), doc);
    }

    /// Make `alias` refer to the loaded document `uri` (the same schema under another URL).
    pub fn alias(&mut self, alias: &str, uri: &str) {
        if alias != uri && self.docs.contains_key(uri) {
            self.embedded
                .insert(alias.to_string(), (uri.to_string(), Vec::new()));
        }
    }

    pub fn mark_failed(&mut self, uri: &str, error: String) {
        self.failed.insert(uri::document(uri).to_string(), error);
    }

    pub fn failure(&self, uri: &str) -> Option<&str> {
        self.failed.get(uri::document(uri)).map(String::as_str)
    }

    pub fn contains(&self, uri: &str) -> bool {
        let d = uri::document(uri);
        self.docs.contains_key(d) || self.embedded.contains_key(d)
    }

    pub fn doc(&self, uri: &str) -> Option<&Arc<SchemaDoc>> {
        let d = uri::document(uri);
        self.docs
            .get(d)
            .or_else(|| self.embedded.get(d).and_then(|(u, _)| self.docs.get(u)))
    }

    pub fn docs(&self) -> impl Iterator<Item = &Arc<SchemaDoc>> {
        self.docs.values()
    }

    /// Documents referenced by the loaded ones that are neither loaded nor failed.
    pub fn missing(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for doc in self.docs.values() {
            for r in doc.external_refs() {
                if !self.contains(r) && !self.failed.contains_key(r) && !out.contains(r) {
                    out.push(r.clone());
                }
            }
        }
        out.sort();
        out
    }

    /// The schema at `uri` (with an optional fragment).
    pub fn get(&self, uri: &str) -> Option<Sch<'_>> {
        let (d, frag) = uri::split_fragment(uri);
        let (doc, base, start): (&SchemaDoc, &str, &Node) = if let Some(doc) = self.docs.get(d) {
            (doc, self.base_of(doc, &[]).unwrap_or(&doc.uri), &doc.root)
        } else if let Some((key, (u, path))) = self.embedded.get_key_value(d) {
            let doc = self.docs.get(u)?;
            (doc, key.as_str(), walk(&doc.root, path)?)
        } else {
            return None;
        };
        let at = Sch {
            doc,
            node: start,
            base,
        };
        match frag {
            None | Some("") => Some(at),
            Some(f) if f.starts_with('/') => {
                let mut cur = at;
                for seg in pointer(f) {
                    let next = match &cur.node.value {
                        Value::Object(_) => cur.node.get(&seg)?,
                        Value::Array(items) => items.get(seg.parse::<usize>().ok()?)?,
                        _ => return None,
                    };
                    cur = self.enter(cur, next);
                }
                Some(cur)
            }
            Some(name) => {
                let key = format!("{base}#{name}");
                let path = doc.anchors.get(&key)?;
                let node = walk(&doc.root, path)?;
                Some(Sch {
                    doc,
                    node,
                    base: self.base_of(doc, path).unwrap_or(base),
                })
            }
        }
    }

    /// The base URI in effect at `path` of `doc`: the deepest `$id` on the way.
    fn base_of<'r>(&'r self, doc: &'r SchemaDoc, path: &[Seg]) -> Option<&'r str> {
        let mut best: Option<(&'r str, usize)> = None;
        for (id, p) in &doc.ids {
            if p.len() <= path.len()
                && path[..p.len()] == p[..]
                && best.is_none_or(|b| p.len() >= b.1)
            {
                best = Some((id.as_str(), p.len()));
            }
        }
        best.map(|b| self.interned(doc, b.0))
            .or(Some(doc.uri.as_str()))
    }

    /// A string equal to `id` that lives as long as the registry.
    fn interned<'r>(&'r self, doc: &'r SchemaDoc, id: &str) -> &'r str {
        if id == doc.uri {
            return doc.uri.as_str();
        }
        self.embedded
            .get_key_value(id)
            .map(|(k, _)| k.as_str())
            .unwrap_or(doc.uri.as_str())
    }

    /// The subschema `node` (a child of `parent`), with its own base when it has an `$id`.
    pub fn enter<'r>(&'r self, parent: Sch<'r>, node: &'r Node) -> Sch<'r> {
        let mut base = parent.base;
        if let Some(id) = node
            .get(id_keyword(parent.doc.draft))
            .and_then(Node::as_str)
        {
            if !id.starts_with('#') {
                let abs = uri::resolve(parent.base, id);
                base = self.interned(parent.doc, uri::document(&abs));
            }
        }
        Sch {
            doc: parent.doc,
            node,
            base,
        }
    }

    /// The target of a `$ref` written in `at`.
    pub fn resolve(&self, at: Sch<'_>, reference: &str) -> Option<Sch<'_>> {
        let abs = uri::resolve(at.base, reference);
        self.get(&abs)
    }

    /// The version that decides ambiguous keywords for `s`.
    pub fn draft_of(&self, s: Sch<'_>) -> Draft {
        self.draft.or(s.doc.draft).unwrap_or(Draft::D7)
    }
}

impl<'r> Sch<'r> {
    pub fn get(&self, kw: &str) -> Option<&'r Node> {
        self.node.get(kw)
    }

    pub fn str(&self, kw: &str) -> Option<&'r str> {
        self.node.get(kw).and_then(Node::as_str)
    }

    pub fn num(&self, kw: &str) -> Option<f64> {
        self.node.get(kw).and_then(Node::as_f64)
    }

    /// `true` / `false` schemas.
    pub fn boolean(&self) -> Option<bool> {
        self.node.as_bool()
    }

    /// The documentation: `markdownDescription` (VS Code's extension) before
    /// `description`.
    pub fn description(&self) -> Option<&'r str> {
        self.str("markdownDescription")
            .or_else(|| self.str("description"))
            .filter(|s| !s.trim().is_empty())
    }

    /// The `type` keyword as a list.
    pub fn types(&self) -> Vec<&'r str> {
        match self.get("type").map(|t| &t.value) {
            Some(Value::String(s)) => vec![s.as_str()],
            Some(Value::Array(items)) => items.iter().filter_map(Node::as_str).collect(),
            _ => Vec::new(),
        }
    }

    pub fn same(&self, other: &Sch<'_>) -> bool {
        std::ptr::eq(self.node, other.node)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(uri: &str, text: &str) -> Arc<SchemaDoc> {
        Arc::new(SchemaDoc::parse(uri, uri, text.to_string()).unwrap())
    }

    #[test]
    fn resolves_pointers_ids_and_anchors() {
        let mut reg = Registry::new();
        reg.insert(doc(
            "https://example.com/root.json",
            r##"{
              "$schema": "https://json-schema.org/draft/2020-12/schema",
              "$defs": {
                "a": {"type": "string"},
                "b~/c": {"type": "integer"},
                "inner": {"$id": "inner.json", "$defs": {"x": {"$anchor": "ex", "type": "boolean"}}, "$ref": "#/$defs/x"}
              },
              "properties": {"o": {"$ref": "other.json#/$defs/z"}}
            }"##,
        ));
        assert_eq!(
            reg.missing(),
            vec!["https://example.com/other.json".to_string()]
        );
        let root = reg.get("https://example.com/root.json").unwrap();
        assert_eq!(reg.draft_of(root), Draft::D2020);
        let a = reg.resolve(root, "#/$defs/a").unwrap();
        assert_eq!(a.types(), vec!["string"]);
        assert_eq!(
            reg.resolve(root, "#/$defs/b~0~1c").unwrap().types(),
            vec!["integer"]
        );
        let inner = reg.get("https://example.com/inner.json").unwrap();
        assert_eq!(inner.base, "https://example.com/inner.json");
        // `#/$defs/x` inside `inner.json` resolves against inner's base.
        let x = reg.resolve(inner, "#/$defs/x").unwrap();
        assert_eq!(x.types(), vec!["boolean"]);
        assert_eq!(
            reg.get("https://example.com/inner.json#ex")
                .unwrap()
                .types(),
            vec!["boolean"]
        );
        reg.mark_failed("https://example.com/other.json", "offline".into());
        assert!(reg.missing().is_empty());
        assert_eq!(
            reg.failure("https://example.com/other.json#/x"),
            Some("offline")
        );
    }

    #[test]
    fn draft4_ids_and_aliases() {
        let mut reg = Registry::new();
        reg.insert(doc(
            "https://a.org/s.json",
            r##"{"$schema": "http://json-schema.org/draft-04/schema#", "id": "https://a.org/canonical", "definitions": {"d": {"id": "#dd", "type": "null"}}}"##,
        ));
        assert!(reg.contains("https://a.org/canonical"));
        assert_eq!(
            reg.get("https://a.org/canonical#dd").unwrap().types(),
            vec!["null"]
        );
        reg.alias("https://b.org/s.json", "https://a.org/s.json");
        assert!(reg.get("https://b.org/s.json#/definitions/d").is_some());
        assert_eq!(Draft::from_setting("2019-09"), Some(Draft::D2019));
        assert_eq!(Draft::from_setting("auto"), None);
    }
}
