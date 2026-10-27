//! Valgrind Memcheck's XML output (`--xml=yes --xml-file=…`, protocol 4): the errors with
//! their stacks (each `auxwhat` titles the stack after it), the leak records and the leak
//! summary, error counts and the command that ran.

use serde::{Deserialize, Serialize};

use crate::frames::{resolve_stacks, Stack, StackFrame};
use crate::xml::{self, Element};

/// Bytes and blocks of one leak kind.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Leaked {
    pub bytes: u64,
    pub blocks: u64,
}

/// Memcheck's LEAK SUMMARY, added up from the leak records.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LeakSummary {
    pub definitely_lost: Leaked,
    pub indirectly_lost: Leaked,
    pub possibly_lost: Leaked,
    pub still_reachable: Leaked,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValgrindError {
    /// `0x1`.
    pub unique: String,
    pub thread: u32,
    /// `InvalidRead`, `UninitCondition`, `Leak_DefinitelyLost`, …
    pub kind: String,
    /// "Invalid read of size 4", "48 (16 direct, 32 indirect) bytes in 1 blocks are …".
    pub what: String,
    /// How often it happened (`errorcounts`).
    pub count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub leaked: Option<Leaked>,
    pub stacks: Vec<Stack>,
    /// The first project frame (after [`resolve`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<StackFrame>,
}

impl ValgrindError {
    pub fn is_leak(&self) -> bool {
        self.kind.starts_with("Leak_")
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValgrindReport {
    /// `memcheck`.
    pub tool: String,
    /// The program and its arguments.
    pub command: String,
    pub pid: u32,
    /// Errors first (in order), then the leak records.
    pub errors: Vec<ValgrindError>,
    pub leak_summary: LeakSummary,
    /// The number of errors (with repeats), without leaks.
    pub error_count: u32,
    /// `FINISHED` when Valgrind ended normally.
    pub state: String,
}

fn number(el: &Element, name: &str) -> u64 {
    el.child_text(name)
        .and_then(|t| {
            t.strip_prefix("0x")
                .map(|h| u64::from_str_radix(h, 16).ok())
                .unwrap_or_else(|| t.parse().ok())
        })
        .unwrap_or(0)
}

fn frame(index: u32, f: &Element) -> StackFrame {
    let file = match (f.child_text("dir"), f.child_text("file")) {
        (Some(dir), Some(file)) => Some(format!("{}/{}", dir.trim_end_matches('/'), file)),
        (None, Some(file)) => Some(file.to_string()),
        _ => None,
    };
    StackFrame {
        index,
        address: f.child_text("ip").map(str::to_string),
        function: f.child_text("fn").unwrap_or("???").to_string(),
        line: number(f, "line") as u32,
        column: 0,
        module: f.child_text("obj").map(str::to_string),
        file,
        path: None,
    }
}

fn stack(el: &Element, title: String) -> Stack {
    Stack {
        title,
        frames: el
            .children_named("frame")
            .enumerate()
            .map(|(i, f)| frame(i as u32, f))
            .collect(),
    }
}

fn error(el: &Element) -> ValgrindError {
    let kind = el.child_text("kind").unwrap_or("").to_string();
    let xwhat = el.child("xwhat");
    let what = el
        .child_text("what")
        .or_else(|| xwhat.and_then(|x| x.child_text("text")))
        .unwrap_or("")
        .to_string();
    let leaked = xwhat
        .filter(|x| x.child("leakedbytes").is_some())
        .map(|x| Leaked {
            bytes: number(x, "leakedbytes"),
            blocks: number(x, "leakedblocks"),
        });
    // The first stack belongs to the error; each auxwhat titles the stack after it.
    let mut stacks = Vec::new();
    let mut title: Option<String> = None;
    for c in &el.children {
        match c.name.as_str() {
            "auxwhat" => title = Some(c.text.trim().to_string()),
            "xauxwhat" => title = c.child_text("text").map(str::to_string),
            "stack" => {
                let t = title.take().unwrap_or_else(|| {
                    if stacks.is_empty() {
                        what.clone()
                    } else {
                        "Stack".into()
                    }
                });
                stacks.push(stack(c, t));
            }
            _ => {}
        }
    }
    ValgrindError {
        unique: el.child_text("unique").unwrap_or("").to_string(),
        thread: number(el, "tid") as u32,
        kind,
        what,
        count: 1,
        leaked,
        stacks,
        location: None,
    }
}

/// Parse Memcheck's XML output.
pub fn parse(text: &str) -> Result<ValgrindReport, String> {
    let root = xml::parse(text)?;
    if root.name != "valgrindoutput" {
        return Err("not a Valgrind XML file".into());
    }
    let mut report = ValgrindReport {
        tool: root.child_text("tool").unwrap_or("memcheck").to_string(),
        pid: number(&root, "pid") as u32,
        ..ValgrindReport::default()
    };
    if let Some(argv) = root.child("args").and_then(|a| a.child("argv")) {
        let mut parts: Vec<&str> = argv.child_text("exe").into_iter().collect();
        parts.extend(argv.children_named("arg").map(|a| a.text.trim()));
        report.command = parts.join(" ");
    }
    for c in &root.children {
        match c.name.as_str() {
            "error" => report.errors.push(error(c)),
            "status" => {
                if let Some(s) = c.child_text("state") {
                    report.state = s.to_string();
                }
            }
            "errorcounts" => {
                for pair in c.children_named("pair") {
                    let unique = pair.child_text("unique").unwrap_or("");
                    let count = number(pair, "count") as u32;
                    if let Some(e) = report.errors.iter_mut().find(|e| e.unique == unique) {
                        e.count = count.max(1);
                    }
                }
            }
            _ => {}
        }
    }
    // Errors in order, then leaks by size.
    let (mut leaks, errors): (Vec<_>, Vec<_>) =
        report.errors.drain(..).partition(ValgrindError::is_leak);
    leaks.sort_by(|a, b| {
        let size = |e: &ValgrindError| e.leaked.map(|l| l.bytes).unwrap_or(0);
        size(b).cmp(&size(a))
    });
    report.error_count = errors.iter().map(|e| e.count).sum();
    for l in &leaks {
        let Some(n) = l.leaked else { continue };
        let slot = match l.kind.as_str() {
            "Leak_DefinitelyLost" => &mut report.leak_summary.definitely_lost,
            "Leak_IndirectlyLost" => &mut report.leak_summary.indirectly_lost,
            "Leak_PossiblyLost" => &mut report.leak_summary.possibly_lost,
            _ => &mut report.leak_summary.still_reachable,
        };
        slot.bytes += n.bytes;
        slot.blocks += n.blocks;
    }
    report.errors = errors.into_iter().chain(leaks).collect();
    Ok(report)
}

/// Map the frames onto the project and set each error's location.
pub fn resolve(report: &mut ValgrindReport, resolve: &dyn Fn(&str) -> Option<String>) {
    for e in report.errors.iter_mut() {
        e.location = resolve_stacks(&mut e.stacks, resolve);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const XML: &str = include_str!("../tests/fixtures/memcheck.xml");

    #[test]
    fn reads_memcheck_errors_leaks_and_summary() {
        let mut r = parse(XML).unwrap();
        let exists = |_: &str| false;
        resolve(&mut r, &|f| {
            crate::frames::project_path(f, "/home/anna/Projects/shapes", "", &exists)
        });
        assert_eq!(r.tool, "memcheck");
        assert_eq!(r.state, "FINISHED");
        assert_eq!(
            r.command,
            "/home/anna/Projects/shapes/cmake-build-debug/shapes_app all"
        );
        let kinds: Vec<_> = r.errors.iter().map(|e| e.kind.as_str()).collect();
        assert_eq!(
            kinds,
            [
                "InvalidRead",
                "UninitCondition",
                "Leak_DefinitelyLost",
                "Leak_DefinitelyLost"
            ]
        );
        assert_eq!(r.error_count, 2);
        let read = &r.errors[0];
        assert_eq!(read.what, "Invalid read of size 4");
        assert_eq!(read.stacks.len(), 2);
        assert_eq!(read.stacks[0].title, "Invalid read of size 4");
        assert_eq!(
            read.stacks[1].title,
            "Address 0x4e260e0 is 0 bytes after a block of size 16 alloc'd"
        );
        let top = &read.stacks[0].frames[0];
        assert_eq!(top.function, "area(Polygon*)");
        assert_eq!(
            top.file.as_deref(),
            Some("/home/anna/Projects/shapes/src/shapes.cpp")
        );
        assert_eq!(top.path.as_deref(), Some("src/shapes.cpp"));
        assert_eq!(top.line, 21);
        assert_eq!(top.address.as_deref(), Some("0x109377"));
        // operator new in the preload library has no file.
        assert!(read.stacks[1].frames[0].path.is_none());
        assert_eq!(read.location.as_ref().unwrap().line, 21);
        let uninit = &r.errors[1];
        assert_eq!(
            uninit.stacks[1].title,
            "Uninitialised value was created by a heap allocation"
        );
        let leak = &r.errors[2];
        assert_eq!(
            leak.leaked,
            Some(Leaked {
                bytes: 48,
                blocks: 1
            })
        );
        assert!(leak.what.starts_with("48 (16 direct, 32 indirect) bytes"));
        assert_eq!(
            r.leak_summary.definitely_lost,
            Leaked {
                bytes: 80,
                blocks: 2
            }
        );
        assert_eq!(r.leak_summary.possibly_lost, Leaked::default());
        let json = serde_json::to_value(&r).unwrap();
        assert_eq!(json["leakSummary"]["definitelyLost"]["bytes"], 80);
    }

    #[test]
    fn truncated_and_foreign_documents() {
        let cut = &XML[..XML.find("<status>\n  <state>FINISHED").unwrap()];
        let r = parse(cut).unwrap();
        assert_eq!(r.errors.len(), 2);
        assert_eq!(r.state, "RUNNING");
        assert!(parse("<html/>").is_err());
        assert!(parse("garbage").is_err());
    }
}
