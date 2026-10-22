//! Type and Call Hierarchy from the language servers (`typeHierarchy/*`, `callHierarchy/*`)
//! as [`forge_codeintel::hierarchy`]'s model, which computes the views. Items carry the
//! server's item and id as their `handle`, so the UI can expand them later; paths are
//! absolute (the app makes them workspace-relative).

use std::collections::{HashMap, VecDeque};
use std::path::Path;

use forge_codeintel::hierarchy::{
    CallDirection, CallNode, CallSite, HierarchyItem, HierarchyKind, TypeEntry, TypeGraph,
};
use forge_codeintel::{Position as CiPosition, Range as CiRange};
use forge_core::Result;
use lsp_types::{CallHierarchyItem, Position, Range, SymbolKind, TypeHierarchyItem, Url};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};

use crate::client::uri_to_path;
use crate::manager::LspManager;

/// At most this many types are asked for (supertypes and subtypes together).
const MAX_TYPES: usize = 300;

fn path_of(uri: &Url) -> String {
    uri_to_path(uri)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| uri.to_string())
}

fn kind_of(kind: SymbolKind) -> HierarchyKind {
    let n = serde_json::to_value(kind)
        .ok()
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    HierarchyKind::from_lsp(n as u32)
}

/// Converts an LSP range.
pub fn range(r: Range) -> CiRange {
    CiRange::new(
        CiPosition::new(r.start.line, r.start.character),
        CiPosition::new(r.end.line, r.end.character),
    )
}

fn item(
    server: &str,
    name: &str,
    kind: SymbolKind,
    detail: Option<&String>,
    uri: &Url,
    selection: Range,
    raw: Value,
) -> HierarchyItem {
    let path = path_of(uri);
    let start = selection.start;
    HierarchyItem {
        id: HierarchyItem::location_id(&path, start.line, start.character),
        name: name.to_string(),
        kind: kind_of(kind),
        container: detail.filter(|d| !d.is_empty()).cloned(),
        detail: None,
        path,
        line: start.line,
        column: start.character,
        is_abstract: false,
        handle: Some(json!({ "server": server, "item": raw })),
    }
}

/// A server's type item as a hierarchy item.
pub fn type_item(server: &str, t: &TypeHierarchyItem) -> HierarchyItem {
    let raw = serde_json::to_value(t).unwrap_or(Value::Null);
    item(
        server,
        &t.name,
        t.kind,
        t.detail.as_ref(),
        &t.uri,
        t.selection_range,
        raw,
    )
}

/// A server's call item as a hierarchy item; functions get PHPStorm's `name()`.
pub fn call_item(server: &str, c: &CallHierarchyItem) -> HierarchyItem {
    let raw = serde_json::to_value(c).unwrap_or(Value::Null);
    let mut out = item(
        server,
        &c.name,
        c.kind,
        c.detail.as_ref(),
        &c.uri,
        c.selection_range,
        raw,
    );
    if matches!(
        out.kind,
        HierarchyKind::Method | HierarchyKind::Function | HierarchyKind::Constructor
    ) && !out.name.ends_with(')')
    {
        out.name.push_str("()");
    }
    out
}

/// The server id and the server's own item behind a hierarchy item.
pub fn handle_of<T: DeserializeOwned>(item: &HierarchyItem) -> Option<(String, T)> {
    let h = item.handle.as_ref()?;
    let server = h.get("server")?.as_str()?.to_string();
    let raw = serde_json::from_value(h.get("item")?.clone()).ok()?;
    Some((server, raw))
}

/// Links `sub` below `sup`: interfaces of a class are `implements`, everything else
/// `extends`.
fn link(entries: &mut [TypeEntry], sub: usize, sup: usize) {
    if sub == sup {
        return;
    }
    let sup_id = entries[sup].item.id.clone();
    let as_interface = entries[sup].item.kind == HierarchyKind::Interface
        && entries[sub].item.kind != HierarchyKind::Interface;
    let e = &mut entries[sub];
    if e.extends.contains(&sup_id) || e.implements.contains(&sup_id) {
        return;
    }
    if as_interface {
        e.implements.push(sup_id);
    } else {
        e.extends.push(sup_id);
    }
}

impl LspManager {
    /// The type at `position` with its supertypes and subtypes, transitively, from the first
    /// server that knows it. `None` when no server supports type hierarchies or none found a
    /// type there.
    pub async fn type_graph(&self, path: &Path, position: Position) -> Result<Option<TypeGraph>> {
        let prepared = self.prepare_type_hierarchy(path, position).await?;
        let Some((server, root)) = prepared.into_iter().next() else {
            return Ok(None);
        };
        let mut entries: Vec<TypeEntry> = vec![];
        let mut raw: Vec<TypeHierarchyItem> = vec![];
        let mut ids: HashMap<String, usize> = HashMap::new();
        let mut add = |t: TypeHierarchyItem,
                       entries: &mut Vec<TypeEntry>,
                       raw: &mut Vec<TypeHierarchyItem>| {
            let item = type_item(&server, &t);
            if let Some(i) = ids.get(&item.id) {
                return (*i, false);
            }
            ids.insert(item.id.clone(), entries.len());
            entries.push(TypeEntry {
                item,
                ..Default::default()
            });
            raw.push(t);
            (entries.len() - 1, true)
        };
        let (root_index, _) = add(root, &mut entries, &mut raw);
        for subtypes in [false, true] {
            let mut queue = VecDeque::from([root_index]);
            while let Some(i) = queue.pop_front() {
                if entries.len() >= MAX_TYPES {
                    break;
                }
                let found = self
                    .type_hierarchy(&server, raw[i].clone(), subtypes)
                    .await
                    .unwrap_or_default();
                for t in found {
                    let (j, new) = add(t, &mut entries, &mut raw);
                    if subtypes {
                        link(&mut entries, j, i);
                    } else {
                        link(&mut entries, i, j);
                    }
                    if new {
                        queue.push_back(j);
                    }
                }
            }
        }
        Ok(Some(TypeGraph {
            root: entries[root_index].item.id.clone(),
            types: entries,
            method: None,
        }))
    }

    /// The function at `position` a Call Hierarchy starts at, from the first server that
    /// knows it.
    pub async fn call_root(
        &self,
        path: &Path,
        position: Position,
    ) -> Result<Option<HierarchyItem>> {
        let prepared = self.prepare_call_hierarchy(path, position).await?;
        Ok(prepared
            .into_iter()
            .next()
            .map(|(server, c)| call_item(&server, &c)))
    }

    /// Callers or callees of an item from [`Self::call_root`] or an earlier answer; empty for
    /// items without a server handle.
    pub async fn call_nodes(
        &self,
        of: &HierarchyItem,
        direction: CallDirection,
    ) -> Result<Vec<CallNode>> {
        let Some((server, raw)) = handle_of::<CallHierarchyItem>(of) else {
            return Ok(vec![]);
        };
        let found = match direction {
            CallDirection::Callers => self.incoming_calls(&server, raw).await?,
            CallDirection::Callees => self.outgoing_calls(&server, raw).await?,
        };
        Ok(found
            .into_iter()
            .map(|(c, ranges)| {
                let item = call_item(&server, &c);
                // Incoming calls are made in the caller, outgoing ones in `of`.
                let site_path = match direction {
                    CallDirection::Callers => item.path.clone(),
                    CallDirection::Callees => of.path.clone(),
                };
                CallNode {
                    calls: ranges
                        .into_iter()
                        .map(|r| CallSite {
                            path: site_path.clone(),
                            range: range(r),
                            preview: String::new(),
                        })
                        .collect(),
                    item,
                }
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn items_keep_the_server_handle() {
        let c: CallHierarchyItem = serde_json::from_value(json!({
            "name": "save", "kind": 6, "detail": "User", "uri": "file:///p/User.php",
            "range": {"start": {"line": 1, "character": 0}, "end": {"line": 3, "character": 1}},
            "selectionRange": {"start": {"line": 1, "character": 20}, "end": {"line": 1, "character": 24}}
        }))
        .unwrap();
        let item = call_item("intelephense", &c);
        assert_eq!(item.name, "save()");
        assert_eq!(item.kind, HierarchyKind::Method);
        assert_eq!(item.container.as_deref(), Some("User"));
        assert_eq!((item.line, item.column), (1, 20));
        let (server, back) = handle_of::<CallHierarchyItem>(&item).unwrap();
        assert_eq!(server, "intelephense");
        assert_eq!(back, c);
    }
}
