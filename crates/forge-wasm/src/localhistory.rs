//! Local History (`forge-localhistory`) over an in-memory store, mirroring the
//! `local_history_*` IPC commands for browser mode. The mock reads and writes the demo
//! project's files itself; this module records and answers the queries.

use std::cell::RefCell;
use std::collections::HashMap;

use forge_localhistory::{
    ChangeKind, Content, Limits, LocalHistory, MemStorage, NewChange, NewEntry, RevertAction,
    RevertPlan,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use wasm_bindgen::prelude::*;

use crate::to_json;

thread_local! {
    static HISTORY: RefCell<LocalHistory<MemStorage>> = RefCell::new(fresh());
}

fn fresh() -> LocalHistory<MemStorage> {
    LocalHistory::open(MemStorage::default(), Limits::default()).expect("in-memory store")
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct File {
    path: String,
    before: Option<String>,
    after: Option<String>,
    /// Renames and moves: where the file came from.
    #[serde(default)]
    from: Option<String>,
}

#[derive(Deserialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "kind"
)]
enum Target {
    File {
        path: String,
        rev_path: String,
        rev: u64,
    },
    Folder {
        path: String,
        rev: u64,
    },
    Change {
        from: u64,
        to: u64,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", tag = "op")]
enum Call {
    Reset,
    Record {
        kind: ChangeKind,
        name: String,
        time: i64,
        #[serde(default)]
        group: Option<u64>,
        files: Vec<File>,
    },
    CloseGroup {
        group: u64,
    },
    Baseline {
        path: String,
        text: String,
        time: i64,
    },
    Label {
        name: String,
        time: i64,
    },
    List {
        path: String,
        dir: bool,
        #[serde(default)]
        filter: String,
    },
    Content {
        path: String,
        rev: u64,
    },
    FolderChanges {
        path: String,
        rev: u64,
    },
    Selection {
        path: String,
        text: String,
        start: usize,
        end: usize,
    },
    Recent {
        limit: usize,
    },
    ChangeFiles {
        from: u64,
        to: u64,
    },
    #[serde(rename_all = "camelCase")]
    Patch {
        path: String,
        dir: bool,
        rev_path: String,
        rev: u64,
        /// Files as they are now (`null` = missing).
        current: HashMap<String, Option<String>>,
    },
    Revert {
        target: Target,
    },
}

/// A revert step for the mock to run on its demo files.
#[derive(Serialize)]
struct Op {
    path: String,
    /// `write`, `delete`, `mkdir` or `rmdir`.
    kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
}

fn plan_json(plan: RevertPlan) -> Value {
    let ops: Vec<Op> = plan
        .ops
        .into_iter()
        .map(|o| {
            let (kind, text) = match o.action {
                RevertAction::Write(b) => ("write", Some(String::from_utf8_lossy(&b).into_owned())),
                RevertAction::Delete => ("delete", None),
                RevertAction::CreateDir => ("mkdir", None),
                RevertAction::DeleteDir => ("rmdir", None),
            };
            Op {
                path: o.path,
                kind,
                text,
            }
        })
        .collect();
    json!({ "ops": ops, "skipped": plan.skipped })
}

fn call(h: &mut LocalHistory<MemStorage>, c: Call) -> std::io::Result<Value> {
    Ok(match c {
        Call::Reset => {
            *h = fresh();
            Value::Null
        }
        Call::Record {
            kind,
            name,
            time,
            group,
            files,
        } => {
            let entries = files
                .into_iter()
                .map(|f| NewEntry {
                    path: f.path,
                    before: Content::text(f.before),
                    after: Content::text(f.after),
                    from: f.from,
                })
                .collect();
            json!(h.record(NewChange {
                kind,
                name,
                time,
                group,
                entries,
            })?)
        }
        Call::CloseGroup { group } => {
            h.close_group(group);
            Value::Null
        }
        Call::Baseline { path, text, time } => json!(h.baseline(&path, text.as_bytes(), time)?),
        Call::Label { name, time } => json!(h.put_label(&name, time)?),
        Call::List { path, dir, filter } => v(&h.history(&path, dir, &filter)?),
        Call::Content { path, rev } => v(&h.content(&path, rev)?),
        Call::FolderChanges { path, rev } => v(&h.folder_changes(&path, rev)),
        Call::Selection {
            path,
            text,
            start,
            end,
        } => v(&h.selection_history(&path, &text, start, end)?),
        Call::Recent { limit } => v(&h.recent_changes(limit)),
        Call::ChangeFiles { from, to } => v(&h.change_files(from, to)),
        Call::Patch {
            path,
            dir,
            rev_path,
            rev,
            current,
        } => json!(h.patch(&path, dir, &rev_path, rev, |p| current
            .get(p)
            .cloned()
            .flatten())?),
        Call::Revert { target } => plan_json(match target {
            Target::File {
                path,
                rev_path,
                rev,
            } => h.revert_file(&path, &rev_path, rev)?,
            Target::Folder { path, rev } => h.revert_folder(&path, rev)?,
            Target::Change { from, to } => h.revert_change(from, to)?,
        }),
    })
}

fn v<T: Serialize>(x: &T) -> Value {
    serde_json::to_value(x).unwrap_or(Value::Null)
}

/// `{ op, … }` → the result of the operation (see [`Call`]).
#[wasm_bindgen]
pub fn local_history_call(request_json: &str) -> String {
    match serde_json::from_str::<Call>(request_json) {
        Ok(c) => HISTORY.with(|h| match call(&mut h.borrow_mut(), c) {
            Ok(v) => to_json(&v),
            Err(e) => to_json(&json!({ "error": e.to_string() })),
        }),
        Err(e) => to_json(&json!({ "error": e.to_string() })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_and_answers_like_the_ipc_commands() {
        local_history_call(r#"{"op":"reset"}"#);
        let id = local_history_call(
            r#"{"op":"record","kind":"save","name":"Saved","time":1,"files":[{"path":"a.php","before":"one","after":"two"}]}"#,
        );
        assert_eq!(id, "1");
        let list = local_history_call(r#"{"op":"list","path":"a.php","dir":false}"#);
        assert!(list.contains(r#""name":"Saved""#), "{list}");
        let content = local_history_call(r#"{"op":"content","path":"a.php","rev":1}"#);
        assert_eq!(content, r#"{"state":"file","text":"one"}"#);
        let plan = local_history_call(
            r#"{"op":"revert","target":{"kind":"file","path":"a.php","revPath":"a.php","rev":1}}"#,
        );
        assert!(
            plan.contains(r#""text":"one""#) && plan.contains(r#""kind":"write""#),
            "{plan}"
        );
        let patch = local_history_call(
            r#"{"op":"patch","path":"a.php","dir":false,"revPath":"a.php","rev":1,"current":{"a.php":"two"}}"#,
        );
        assert!(patch.contains("+two"), "{patch}");
        assert!(local_history_call(r#"{"op":"nope"}"#).contains("error"));
    }
}
