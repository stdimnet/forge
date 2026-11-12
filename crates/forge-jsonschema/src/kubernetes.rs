//! Kubernetes manifests, recognized by their content: every YAML (or JSON) document with a
//! top-level `apiVersion` and `kind` of a built-in Kubernetes object gets the schema of that
//! object, so a file with several documents (`---`) validates each against its own kind.
//!
//! The schemas are the strict definitions of one Kubernetes version from
//! <https://github.com/kubenote/kubernetes-schema> (`standalone-strict/_definitions.json`,
//! bundled, see `schemas/NOTICE`). Every object definition names its group, version and
//! kind (`x-kubernetes-group-version-kind`), which is the index used here. Unknown kinds
//! (custom resources) get no schema.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::OnceLock;

use crate::bundled;
use crate::tree::Node;
use crate::uri;

/// The Kubernetes version the bundled schemas describe (a branch of kubernetes-schema).
pub const VERSION: &str = "v1.34.1";

/// The bundled definitions file (all objects of [`VERSION`]).
pub const DEFINITIONS: &str = bundled::KUBERNETES;

/// A Kubernetes object type: `apiVersion` (`apps/v1`, `v1`) and `kind` (`Deployment`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ObjectType {
    pub api_version: String,
    pub kind: String,
}

impl ObjectType {
    /// "Deployment (apps/v1)".
    pub fn label(&self) -> String {
        format!("{} ({})", self.kind, self.api_version)
    }
}

/// `(apiVersion, kind)` → definition name, from the bundled definitions.
fn index() -> &'static HashMap<(String, String), String> {
    static INDEX: OnceLock<HashMap<(String, String), String>> = OnceLock::new();
    INDEX.get_or_init(|| {
        let mut out = HashMap::new();
        let Some(b) = bundled::find(DEFINITIONS) else {
            return out;
        };
        let doc = b.doc();
        let defs = doc
            .root
            .get("definitions")
            .and_then(Node::as_object)
            .unwrap_or(&[]);
        for def in defs {
            let gvks = def
                .value
                .as_ref()
                .and_then(|v| v.get("x-kubernetes-group-version-kind"))
                .and_then(Node::as_array)
                .unwrap_or(&[]);
            for gvk in gvks {
                let field = |k: &str| gvk.get(k).and_then(Node::as_str).unwrap_or("");
                let (group, version, kind) = (field("group"), field("version"), field("kind"));
                if version.is_empty() || kind.is_empty() {
                    continue;
                }
                let api_version = if group.is_empty() {
                    version.to_string()
                } else {
                    format!("{group}/{version}")
                };
                out.entry((api_version, kind.to_string()))
                    .or_insert_with(|| def.key.clone());
            }
        }
        out
    })
}

/// The object type a document declares at its top level, whether Kubernetes knows it or
/// not (cheap: no schema is loaded).
pub fn declared(doc: &Node) -> Option<ObjectType> {
    let api_version = doc.get("apiVersion")?.as_str()?.trim();
    let kind = doc.get("kind")?.as_str()?.trim();
    if api_version.is_empty() || kind.is_empty() {
        return None;
    }
    Some(ObjectType {
        api_version: api_version.to_string(),
        kind: kind.to_string(),
    })
}

/// The schema URI of a built-in Kubernetes object type, `None` for custom resources.
pub fn schema_uri(t: &ObjectType) -> Option<String> {
    let def = index().get(&(t.api_version.clone(), t.kind.clone()))?;
    Some(format!("{DEFINITIONS}#/definitions/{def}"))
}

/// The built-in Kubernetes object type of a document, if it is one.
pub fn detect(doc: &Node) -> Option<ObjectType> {
    declared(doc).filter(|t| schema_uri(t).is_some())
}

/// The built-in Kubernetes objects of a file's documents, in order, without repeats.
pub fn detect_all(docs: &[Node]) -> Vec<ObjectType> {
    let mut out: Vec<ObjectType> = Vec::new();
    for t in docs.iter().filter_map(detect) {
        if !out.contains(&t) {
            out.push(t);
        }
    }
    out
}

/// The status bar name: "Kubernetes Deployment (apps/v1)", or the kinds of a file with
/// several ("Kubernetes: Deployment, Service").
pub fn name(types: &[ObjectType]) -> String {
    match types {
        [] => "Kubernetes".to_string(),
        [one] => format!("Kubernetes {}", one.label()),
        many => {
            let mut kinds: Vec<&str> = Vec::new();
            for t in many {
                if !kinds.contains(&t.kind.as_str()) {
                    kinds.push(&t.kind);
                }
            }
            let shown = kinds.len().min(4);
            let mut s = format!("Kubernetes: {}", kinds[..shown].join(", "));
            if kinds.len() > shown {
                s.push_str(", …");
            }
            s
        }
    }
}

/// Whether `root_uri` is the whole definitions file (no definition picked): its documents
/// then each use the schema of their own kind.
pub fn dispatches(root_uri: &str) -> bool {
    let (doc, frag) = uri::split_fragment(root_uri);
    matches!(frag, None | Some("")) && bundled::find(doc).is_some_and(|b| b.uri() == DEFINITIONS)
}

/// The schema a document of a file mapped to `root_uri` is checked against: `root_uri`
/// itself, or for the Kubernetes definitions the document's own kind (`None` for custom
/// resources and documents without `apiVersion` / `kind`).
pub fn document_root<'a>(root_uri: &'a str, doc: &Node) -> Option<Cow<'a, str>> {
    if !dispatches(root_uri) {
        return Some(Cow::Borrowed(root_uri));
    }
    declared(doc).and_then(|t| schema_uri(&t)).map(Cow::Owned)
}
