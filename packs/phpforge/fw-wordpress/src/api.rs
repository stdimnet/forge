//! The WordPress requests of the IDE and their answers, shared by the desktop backend and
//! the browser mode (WebAssembly). Offsets in requests and answers are UTF-16 (JS strings).

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::editor::{self, CompletionList};
use crate::yoda;
use crate::{byte_to_utf16, utf16_to_byte, Snapshot};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Query {
    /// -> `Option<WordPressInfo>`
    Detect,
    /// Scan again -> `Option<WordPressInfo>`
    Refresh,
    /// -> `Vec<HookSummary>`
    Hooks,
    /// -> `Option<CompletionList>` (`from` / `to` UTF-16)
    Complete {
        path: String,
        content: String,
        offset: usize,
    },
    /// -> `Vec<Location>`
    Goto {
        path: String,
        content: String,
        offset: usize,
    },
    /// -> `Vec<GutterMark>`
    Gutter { path: String, content: String },
    /// -> `Option<UsagesAnswer>` (the shape of `code_find_usages`)
    Usages {
        path: String,
        content: String,
        offset: usize,
    },
    /// -> `Vec<Problem>`: the Yoda condition hints
    Inspect { path: String, content: String },
}

/// A weak warning with its quick-fix, UTF-16 offsets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    pub from: usize,
    pub to: usize,
    pub message: String,
    pub code: String,
    pub fix_title: String,
    pub replacement: String,
}

fn to_value<T: Serialize>(v: T) -> Value {
    serde_json::to_value(v).unwrap_or(Value::Null)
}

/// Answer `query` on `snap`.
pub fn answer(snap: &Snapshot, query: Query) -> Value {
    let index = &snap.index;
    match query {
        Query::Detect | Query::Refresh => to_value(&snap.info),
        Query::Hooks => to_value(index.hooks()),
        Query::Complete {
            path,
            content,
            offset,
        } => {
            let byte = utf16_to_byte(&content, offset);
            to_value(
                editor::complete(index, &path, &content, byte).map(|l| CompletionList {
                    from: byte_to_utf16(&content, l.from),
                    to: byte_to_utf16(&content, l.to),
                    items: l.items,
                }),
            )
        }
        Query::Goto {
            path,
            content,
            offset,
        } => to_value(editor::goto(
            index,
            &path,
            &content,
            utf16_to_byte(&content, offset),
        )),
        Query::Gutter { path, content } => to_value(editor::marks(index, &path, &content)),
        Query::Usages {
            path,
            content,
            offset,
        } => {
            let Some(u) = editor::usages(index, &path, &content, utf16_to_byte(&content, offset))
            else {
                return Value::Null;
            };
            json!({
                "title": format!("{} '{}'", u.kind.label(), u.hook),
                "source": "index",
                "declaration": null,
                "byType": editor::usage_tree(&u.refs, true),
                "byFile": editor::usage_tree(&u.refs, false),
                "element": "symbol",
                "scope": "Project Files",
            })
        }
        Query::Inspect { content, .. } => to_value(
            yoda::hints(&content)
                .into_iter()
                .map(|h| Problem {
                    from: byte_to_utf16(&content, h.start),
                    to: byte_to_utf16(&content, h.end),
                    message: h.message,
                    code: "wordpressYodaCondition".into(),
                    fix_title: "Flip to Yoda condition".into(),
                    replacement: h.replacement,
                })
                .collect::<Vec<_>>(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn snap() -> Snapshot {
        let files: HashMap<String, String> = [
            ("acme.php", "<?php\n/* Plugin Name: Acme */\n// é\nadd_action( 'acme_ready', 'acme_boot' );\ndo_action( 'acme_ready' );\nif ( $a == 'é' ) {}\n"),
        ]
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
        crate::scan_files(&files)
    }

    #[test]
    fn answers_with_utf16_offsets() {
        let s = snap();
        let info = answer(&s, Query::Detect);
        assert_eq!(info["kind"], "plugin");
        assert_eq!(info["hooks"], 2);
        let content = "<?php\n// é\nadd_action( 'acme_";
        let list = answer(
            &s,
            Query::Complete {
                path: "x.php".into(),
                content: content.into(),
                offset: content.encode_utf16().count(),
            },
        );
        assert_eq!(list["from"], content.encode_utf16().count() - 5);
        assert_eq!(list["items"][0]["label"], "acme_ready");
        let src = s.index.refs[0].path.clone();
        assert_eq!(src, "acme.php");
        let problems = answer(
            &s,
            Query::Inspect {
                path: "acme.php".into(),
                content: "<?php if ( $a == 'é' ) {}".into(),
            },
        );
        assert_eq!(problems[0]["replacement"], "'é' == $a");
        assert_eq!(problems[0]["to"], 20);
    }

    #[test]
    fn answers_usages_in_the_find_usages_shape() {
        let s = snap();
        let content = "<?php\n/* Plugin Name: Acme */\n// é\nadd_action( 'acme_ready', 'acme_boot' );\ndo_action( 'acme_ready' );\n";
        let at = content[..content.find("acme_ready").unwrap()]
            .encode_utf16()
            .count();
        let u = answer(
            &s,
            Query::Usages {
                path: "acme.php".into(),
                content: content.into(),
                offset: at,
            },
        );
        assert_eq!(u["title"], "action 'acme_ready'");
        assert_eq!(u["byType"]["total"], 2);
        assert_eq!(u["byType"]["groups"][0]["kind"], "hookRegistration");
        assert_eq!(u["byFile"]["groups"][0]["kind"], Value::Null);
        assert_eq!(u["element"], "symbol");
        let q: Query = serde_json::from_str(r#"{"gutter":{"path":"a.php","content":""}}"#).unwrap();
        assert_eq!(answer(&s, q), json!([]));
        assert_eq!(
            serde_json::from_str::<Query>(r#""hooks""#).unwrap(),
            Query::Hooks
        );
    }
}
