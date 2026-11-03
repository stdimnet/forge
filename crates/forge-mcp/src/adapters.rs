//! Turning the IDE services' answers into what MCP clients get: 1-based positions,
//! numbered line ranges, compact search and usage results, capped command output. The
//! services' types are passed in as JSON, so this crate stays independent of them.

use serde_json::{json, Value};

/// Longest output kept from a terminal command or a run.
pub const MAX_OUTPUT_CHARS: usize = 30_000;

/// Lines `start..=end` (1-based, clamped) prefixed with their numbers; the whole text when
/// neither is given.
pub fn line_range(text: &str, start: Option<u64>, end: Option<u64>) -> String {
    if start.is_none() && end.is_none() {
        return text.to_string();
    }
    let lines: Vec<&str> = text.lines().collect();
    let first = start.unwrap_or(1).max(1) as usize;
    let last = (end.unwrap_or(lines.len() as u64) as usize).min(lines.len());
    if first > last {
        return format!("(the file has {} lines)", lines.len());
    }
    let width = last.to_string().len();
    lines[first - 1..last]
        .iter()
        .enumerate()
        .map(|(i, l)| format!("{:>width$}  {l}", first + i))
        .collect::<Vec<_>>()
        .join("\n")
}

fn num(v: &Value, key: &str) -> u64 {
    v[key].as_u64().unwrap_or(0)
}

/// A Find in Files result (`forge_find::FindResult`, 0-based lines and columns) with 1-based
/// positions and without highlight ranges.
pub fn find_result(result: &Value) -> Value {
    let files: Vec<Value> = result["files"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|f| {
            let matches: Vec<Value> = f["matches"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|m| json!({ "line": num(m, "line") + 1, "column": num(m, "column") + 1, "text": m["text"] }))
                .collect();
            json!({ "path": f["path"], "matches": matches })
        })
        .collect();
    let mut out = json!({
        "files": files,
        "matchCount": result["matchCount"],
        "truncated": result["truncated"],
    });
    if let Some(e) = result["error"].as_str() {
        out["error"] = json!(e);
    }
    out
}

/// Symbol index hits (`forge_search_core::SymbolHit`, already 1-based).
pub fn symbol_hits(hits: &Value) -> Value {
    let symbols: Vec<Value> = hits
        .as_array()
        .into_iter()
        .flatten()
        .map(|h| {
            json!({
                "name": h["name"],
                "kind": h["kind"],
                "path": h["path"],
                "line": h["line"],
                "container": h["container"],
            })
        })
        .collect();
    json!({ "symbols": symbols })
}

/// A Go to Declaration answer (0-based targets).
pub fn declaration(answer: &Value) -> Value {
    let targets: Vec<Value> = answer["targets"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|t| {
            json!({
                "path": t["path"],
                "line": num(t, "line") + 1,
                "column": num(t, "column") + 1,
                "name": t["name"],
                "container": t["container"],
            })
        })
        .collect();
    let mut out = json!({
        "title": answer["title"],
        "onDeclaration": answer["onDeclaration"],
        "targets": targets,
        "source": answer["source"],
    });
    if let Some(m) = answer["message"].as_str() {
        out["message"] = json!(m);
    }
    out
}

/// A Find Usages answer: the declaration and the usages by file (0-based rows).
pub fn usages(answer: &Value) -> Value {
    let mut files: Vec<Value> = Vec::new();
    for group in answer["byFile"]["groups"].as_array().into_iter().flatten() {
        for f in group["files"].as_array().into_iter().flatten() {
            let rows: Vec<Value> = f["usages"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|u| {
                    json!({
                        "line": num(u, "line") + 1,
                        "column": num(u, "column") + 1,
                        "text": u["text"],
                        "kind": u["kind"],
                    })
                })
                .collect();
            files.push(json!({ "path": f["path"], "usages": rows }));
        }
    }
    let declaration = answer
        .get("declaration")
        .filter(|d| d.is_object())
        .map(|d| {
            json!({
                "path": d["path"],
                "line": num(&d["range"]["start"], "line") + 1,
                "column": num(&d["range"]["start"], "character") + 1,
            })
        });
    json!({
        "title": answer["title"],
        "source": answer["source"],
        "declaration": declaration,
        "total": answer["byFile"]["total"],
        "files": files,
    })
}

/// An inspection report (`forge_inspections::Report`, 0-based lines): its problems with
/// 1-based positions.
pub fn inspection_report(report: &Value) -> Value {
    let problems: Vec<Value> = report["problems"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|p| {
            json!({
                "path": p["path"],
                "line": num(p, "line") + 1,
                "column": num(p, "column") + 1,
                "severity": p["severityName"],
                "inspection": p["inspectionName"],
                "message": p["message"],
                "code": p["snippet"],
            })
        })
        .collect();
    json!({
        "count": problems.len(),
        "problems": problems,
        "files": report["files"],
        "profile": report["profile"],
        "errors": report["errors"],
    })
}

/// `replace_text_in_file`: the new text and how many occurrences were replaced. `old` must
/// occur exactly once unless `all` is set.
pub fn replace_text(text: &str, old: &str, new: &str, all: bool) -> Result<(String, usize), String> {
    if old.is_empty() {
        return Err("oldText must not be empty".into());
    }
    let count = text.matches(old).count();
    match (count, all) {
        (0, _) => Err("oldText was not found in the file (it must match exactly, including whitespace)".into()),
        (1, _) | (_, true) => Ok((text.replace(old, new), count)),
        (n, false) => Err(format!(
            "oldText occurs {n} times; add surrounding lines to make it unique or set replaceAll"
        )),
    }
}

/// Output of a command or run, keeping the start and the end when it is too long.
#[derive(Debug, Default, Clone)]
pub struct OutputBuffer {
    head: String,
    tail: std::collections::VecDeque<char>,
    dropped: usize,
}

impl OutputBuffer {
    pub fn push(&mut self, text: &str) {
        let half = MAX_OUTPUT_CHARS / 2;
        for c in text.chars() {
            if self.head.chars().count() < half {
                self.head.push(c);
                continue;
            }
            self.tail.push_back(c);
            if self.tail.len() > half {
                self.tail.pop_front();
                self.dropped += 1;
            }
        }
    }

    pub fn text(&self) -> String {
        let tail: String = self.tail.iter().collect();
        if self.dropped == 0 {
            format!("{}{tail}", self.head)
        } else {
            format!(
                "{}\n… ({} characters left out) …\n{tail}",
                self.head, self.dropped
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_line_ranges() {
        let text = "a\nb\nc\nd";
        assert_eq!(line_range(text, None, None), text);
        assert_eq!(line_range(text, Some(2), Some(3)), "2  b\n3  c");
        assert_eq!(line_range(text, Some(3), None), "3  c\n4  d");
        assert_eq!(line_range(text, Some(9), None), "(the file has 4 lines)");
    }

    #[test]
    fn compacts_search_results() {
        let r = json!({
            "files": [{ "path": "a.php", "matches": [{ "line": 0, "column": 4, "length": 3, "text": "foo bar", "ranges": [[4, 7]] }] }],
            "matchCount": 1, "truncated": false
        });
        let c = find_result(&r);
        assert_eq!(
            c["files"][0]["matches"][0],
            json!({ "line": 1, "column": 5, "text": "foo bar" })
        );
        assert!(c.get("error").is_none());
    }

    #[test]
    fn compacts_navigation_results() {
        let d = declaration(&json!({
            "onDeclaration": false, "title": "User", "source": "index",
            "targets": [{ "path": "app/User.php", "line": 9, "column": 6, "name": "User" }]
        }));
        assert_eq!(d["targets"][0]["line"], 10);
        assert_eq!(d["targets"][0]["column"], 7);
        let u = usages(&json!({
            "title": "User", "source": "lsp",
            "declaration": { "path": "app/User.php", "range": { "start": { "line": 9, "character": 6 }, "end": { "line": 9, "character": 10 } } },
            "byFile": { "total": 1, "fileCount": 1, "groups": [{ "label": "", "count": 1, "files": [
                { "path": "app/Http/C.php", "usages": [{ "line": 2, "column": 4, "endLine": 2, "endColumn": 8, "text": "new User()", "ranges": [], "kind": "new" }] }
            ] }] }
        }));
        assert_eq!(u["declaration"]["line"], 10);
        assert_eq!(u["files"][0]["usages"][0]["line"], 3);
        assert_eq!(u["total"], 1);
    }

    #[test]
    fn compacts_inspection_reports() {
        let r = inspection_report(&json!({
            "problems": [{ "path": "a.php", "line": 4, "column": 0, "severityName": "Warning", "inspectionName": "Unused variable", "message": "Unused $x", "snippet": "$x = 1;" }],
            "files": 3, "profile": "Default", "errors": []
        }));
        assert_eq!(r["count"], 1);
        assert_eq!(r["problems"][0]["line"], 5);
        assert_eq!(r["problems"][0]["severity"], "Warning");
    }

    #[test]
    fn replaces_text() {
        assert_eq!(replace_text("a b a", "b", "c", false), Ok(("a c a".into(), 1)));
        assert!(replace_text("a b a", "a", "c", false).unwrap_err().contains("2 times"));
        assert_eq!(replace_text("a b a", "a", "c", true), Ok(("c b c".into(), 2)));
        assert!(replace_text("a", "x", "y", false).unwrap_err().contains("not found"));
        assert!(replace_text("a", "", "y", false).is_err());
    }

    #[test]
    fn caps_output() {
        let mut b = OutputBuffer::default();
        b.push("hello");
        assert_eq!(b.text(), "hello");
        let mut b = OutputBuffer::default();
        b.push(&"x".repeat(MAX_OUTPUT_CHARS * 2));
        let t = b.text();
        assert!(t.contains("characters left out"));
        assert!(t.chars().count() < MAX_OUTPUT_CHARS + 100);
    }
}
