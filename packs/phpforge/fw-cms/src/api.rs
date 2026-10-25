//! The CMS requests of the IDE and their answers, shared by the desktop backend and the
//! browser mode (WebAssembly). Offsets are UTF-16 (JS strings).

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{byte_to_utf16, drupal, utf16_to_byte, Snapshot};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Query {
    /// -> `Vec<CmsInfo>`
    Detect,
    /// Scan again -> `Vec<CmsInfo>`
    Refresh,
    /// Drupal hook implementations after `function` -> `Option<drupal::CompletionList>`
    Complete {
        path: String,
        content: String,
        offset: usize,
    },
    /// The docs of the hook the function at `offset` implements -> `Option<HookDocs>`
    Goto {
        path: String,
        content: String,
        offset: usize,
    },
    /// -> `Vec<drupal::GutterMark>`
    Gutter { path: String, content: String },
}

fn to_value<T: Serialize>(v: T) -> Value {
    serde_json::to_value(v).unwrap_or(Value::Null)
}

pub fn answer(snap: &Snapshot, query: Query) -> Value {
    match query {
        Query::Detect | Query::Refresh => to_value(&snap.cms),
        Query::Complete {
            path,
            content,
            offset,
        } => to_value(
            drupal::complete(snap, &path, &content, utf16_to_byte(&content, offset)).map(
                |mut l| {
                    l.from = byte_to_utf16(&content, l.from);
                    l
                },
            ),
        ),
        Query::Goto {
            path,
            content,
            offset,
        } => to_value(drupal::goto(
            snap,
            &path,
            &content,
            utf16_to_byte(&content, offset),
        )),
        Query::Gutter { path, content } => to_value(drupal::marks(snap, &path, &content)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn answers_drupal_queries() {
        let files: HashMap<String, String> = [
            ("composer.json", r#"{"require": {"drupal/core": "^11"}}"#),
            (
                "web/modules/custom/acme/acme.info.yml",
                "name: Acme\ntype: module\n",
            ),
        ]
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
        let snap = crate::scan_files(&files);
        assert_eq!(answer(&snap, Query::Detect)[0]["id"], "drupal");
        let content = "<?php // é\nfunction acme_";
        let list = answer(
            &snap,
            Query::Complete {
                path: "web/modules/custom/acme/acme.module".into(),
                content: content.into(),
                offset: content.encode_utf16().count(),
            },
        );
        assert_eq!(list["from"], content.encode_utf16().count() - 5);
        assert_eq!(list["items"][0]["label"], "acme_help");
        let q: Query = serde_json::from_str(
            r#"{"gutter":{"path":"a.module","content":"<?php\nfunction a_cron() {}"}}"#,
        )
        .unwrap();
        assert_eq!(answer(&snap, q)[0]["docs"]["hook"], "hook_cron");
    }
}
