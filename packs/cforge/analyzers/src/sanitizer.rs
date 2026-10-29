//! Sanitizer reports in the output of a run or a test: AddressSanitizer (and HWASan),
//! LeakSanitizer (each leak becomes its own report), ThreadSanitizer, MemorySanitizer and
//! UndefinedBehaviorSanitizer's `runtime error:` lines, with their stacks (access, allocation,
//! free, previous access, thread creation), extra lines and the SUMMARY.

use serde::{Deserialize, Serialize};

use crate::frames::{resolve_stacks, Stack, StackFrame};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Sanitizer {
    Address,
    Leak,
    Thread,
    Memory,
    Undefined,
}

impl Sanitizer {
    fn from_tool(tool: &str) -> Option<Self> {
        Some(match tool {
            "AddressSanitizer" | "HWAddressSanitizer" => Sanitizer::Address,
            "LeakSanitizer" => Sanitizer::Leak,
            "ThreadSanitizer" => Sanitizer::Thread,
            "MemorySanitizer" => Sanitizer::Memory,
            "UndefinedBehaviorSanitizer" => Sanitizer::Undefined,
            _ => return None,
        })
    }
}

/// One error a sanitizer reported.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SanitizerReport {
    pub sanitizer: Sanitizer,
    /// `AddressSanitizer`, `ThreadSanitizer`, …
    pub tool: String,
    /// `heap-buffer-overflow`, `data race`, `Direct leak`, `signed integer overflow`.
    pub kind: String,
    /// The headline without the `==pid==` prefix.
    pub message: String,
    /// Lines that are neither stacks nor the headline ("0x… is located 0 bytes after …").
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<String>,
    pub stacks: Vec<Stack>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// Where the error happened in the project (after [`resolve`]); UBSan's headline file
    /// before that.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<StackFrame>,
}

/// Drop the `==12345==` prefix of sanitizer lines.
fn strip_pid(line: &str) -> &str {
    if let Some(rest) = line.strip_prefix("==") {
        if let Some(end) = rest.find("==") {
            if end > 0 && rest[..end].bytes().all(|b| b.is_ascii_digit()) {
                return &rest[end + 2..];
            }
        }
    }
    line
}

/// `heap-buffer-overflow on address 0x…` → `heap-buffer-overflow`.
fn kind_of(rest: &str) -> String {
    let mut end = rest.len();
    for pat in [" on ", " (", ": "] {
        if let Some(i) = rest.find(pat) {
            end = end.min(i);
        }
    }
    rest[..end].trim().to_string()
}

/// `file:line[:col]`.
fn parse_location(s: &str) -> Option<(String, u32, u32)> {
    let (head, last) = s.rsplit_once(':')?;
    let last: u32 = last.parse().ok()?;
    if let Some((file, line)) = head.rsplit_once(':') {
        if let Ok(line) = line.parse::<u32>() {
            if !file.is_empty() {
                return Some((file.to_string(), line, last));
            }
        }
    }
    (!head.is_empty() && (head.contains('.') || head.contains('/')))
        .then(|| (head.to_string(), last, 0))
}

/// Index of the `(` that opens the group closing at the end of `s`.
fn trailing_group(s: &str) -> Option<usize> {
    if !s.ends_with(')') {
        return None;
    }
    let mut depth = 0i32;
    for (i, c) in s.char_indices().rev() {
        match c {
            ')' => depth += 1,
            '(' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// The text of a frame line after `#n `.
pub(crate) fn parse_frame(index: u32, text: &str) -> StackFrame {
    let mut s = text.trim();
    if let Some(i) = s.find(" (BuildId:") {
        s = s[..i].trim_end();
    }
    let mut frame = StackFrame {
        index,
        ..StackFrame::default()
    };
    if s.starts_with("0x") {
        let (addr, rest) = s.split_once(' ').unwrap_or((s, ""));
        frame.address = Some(addr.to_string());
        s = rest.trim_start();
        s = s.strip_prefix("in ").unwrap_or(s);
    }
    // `(module+0x1234)` after the location (TSan) or instead of it.
    if let Some(open) = trailing_group(s) {
        let inner = &s[open + 1..s.len() - 1];
        if inner.contains("+0x") || inner.starts_with('/') {
            frame.module = Some(inner.to_string());
            s = s[..open].trim_end();
        }
    }
    match s.rsplit_once(' ') {
        Some((func, loc)) => match parse_location(loc) {
            Some((file, line, column)) => {
                frame.function = func.trim().to_string();
                frame.file = Some(file);
                frame.line = line;
                frame.column = column;
            }
            None if loc == "<null>" => frame.function = func.trim().to_string(),
            None => frame.function = s.to_string(),
        },
        None => frame.function = s.to_string(),
    }
    if frame.function.is_empty() {
        frame.function = "??".into();
    }
    frame
}

/// `#3 …` → (3, `…`).
fn frame_line(line: &str) -> Option<(u32, &str)> {
    let t = line.trim_start();
    let rest = t.strip_prefix('#')?;
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 || !rest[digits..].starts_with(' ') {
        return None;
    }
    Some((rest[..digits].parse().ok()?, rest[digits..].trim()))
}

enum Start {
    Report {
        tool: String,
        rest: String,
    },
    Ubsan {
        file: String,
        line: u32,
        column: u32,
        message: String,
    },
}

fn report_start(body: &str) -> Option<Start> {
    for prefix in ["ERROR: ", "WARNING: "] {
        if let Some(rest) = body.strip_prefix(prefix) {
            let (tool, rest) = rest.split_once(": ")?;
            Sanitizer::from_tool(tool)?;
            return Some(Start::Report {
                tool: tool.to_string(),
                rest: rest.trim().to_string(),
            });
        }
    }
    let (loc, message) = body.split_once(": runtime error: ")?;
    let (file, line, column) = parse_location(loc.trim())?;
    Some(Start::Ubsan {
        file,
        line,
        column,
        message: message.trim().to_string(),
    })
}

struct Building {
    report: SanitizerReport,
    /// A text line that becomes the title of the next stack.
    pending: Option<String>,
    /// Frames go into a new stack (after a blank or a title line).
    new_stack: bool,
}

impl Building {
    fn finish(mut self, out: &mut Vec<SanitizerReport>) {
        if let Some(p) = self.pending.take() {
            self.report.details.push(p);
        }
        let r = self.report;
        if r.sanitizer == Sanitizer::Leak || r.kind == "detected memory leaks" {
            // One report per leak, like the IDE's tree.
            let mut any = false;
            for s in r.stacks.iter().filter(|s| s.title.contains(" leak of ")) {
                any = true;
                let kind = s.title.split(" of ").next().unwrap_or("Leak").to_string();
                out.push(SanitizerReport {
                    sanitizer: Sanitizer::Leak,
                    tool: "LeakSanitizer".into(),
                    kind,
                    message: s.title.clone(),
                    details: Vec::new(),
                    stacks: vec![s.clone()],
                    summary: r.summary.clone(),
                    location: None,
                });
            }
            if any {
                return;
            }
        }
        out.push(r);
    }
}

/// Every sanitizer report in a run's output (stdout and stderr together).
pub fn parse(output: &str) -> Vec<SanitizerReport> {
    let mut out = Vec::new();
    let mut cur: Option<Building> = None;
    for raw in output.lines() {
        let line = raw.trim_end_matches('\r');
        let body = strip_pid(line.trim_start());
        if let Some(start) = report_start(body) {
            if let Some(b) = cur.take() {
                b.finish(&mut out);
            }
            let report = match start {
                Start::Report { tool, rest } => SanitizerReport {
                    sanitizer: Sanitizer::from_tool(&tool).unwrap_or(Sanitizer::Address),
                    kind: kind_of(&rest),
                    message: format!("{tool}: {rest}"),
                    tool,
                    details: Vec::new(),
                    stacks: Vec::new(),
                    summary: None,
                    location: None,
                },
                Start::Ubsan {
                    file,
                    line,
                    column,
                    message,
                } => SanitizerReport {
                    sanitizer: Sanitizer::Undefined,
                    tool: "UndefinedBehaviorSanitizer".into(),
                    kind: message
                        .split_once(':')
                        .map(|(k, _)| k.trim().to_string())
                        .unwrap_or_else(|| message.clone()),
                    message: format!("runtime error: {message}"),
                    details: Vec::new(),
                    stacks: Vec::new(),
                    summary: None,
                    location: Some(StackFrame {
                        function: String::new(),
                        file: Some(file),
                        line,
                        column,
                        ..StackFrame::default()
                    }),
                },
            };
            cur = Some(Building {
                report,
                pending: None,
                new_stack: true,
            });
            continue;
        }
        let Some(b) = cur.as_mut() else {
            continue;
        };
        let ubsan = b.report.sanitizer == Sanitizer::Undefined;
        if let Some((index, text)) = frame_line(line) {
            if b.new_stack || b.report.stacks.is_empty() {
                let title = b.pending.take().unwrap_or_else(|| {
                    if b.report.stacks.is_empty() {
                        "Stack".into()
                    } else {
                        format!("Stack {}", b.report.stacks.len() + 1)
                    }
                });
                b.report.stacks.push(Stack {
                    title,
                    frames: Vec::new(),
                });
                b.new_stack = false;
            }
            if let Some(s) = b.report.stacks.last_mut() {
                s.frames.push(parse_frame(index, text));
            }
            continue;
        }
        let text = body.trim();
        if let Some(summary) = text.strip_prefix("SUMMARY: ") {
            b.report.summary = Some(summary.to_string());
            if let Some(b) = cur.take() {
                b.finish(&mut out);
            }
            continue;
        }
        if text.is_empty() {
            b.new_stack = true;
            continue;
        }
        if text.bytes().all(|c| c == b'=') {
            continue;
        }
        if ubsan {
            // The program's own output follows UBSan's message and its stack.
            if let Some(b) = cur.take() {
                b.finish(&mut out);
            }
            continue;
        }
        if let Some(prev) = b.pending.replace(text.trim_end_matches(':').to_string()) {
            b.report.details.push(prev);
        }
        b.new_stack = true;
    }
    if let Some(b) = cur.take() {
        b.finish(&mut out);
    }
    out
}

/// Map the frames onto the project and set each report's location (its first project frame,
/// or UBSan's headline file).
pub fn resolve(reports: &mut [SanitizerReport], resolve: &dyn Fn(&str) -> Option<String>) {
    for r in reports.iter_mut() {
        let first = resolve_stacks(&mut r.stacks, resolve);
        let headline = r.location.take().map(|mut l| {
            l.path = l.file.as_deref().and_then(resolve);
            l
        });
        r.location = match (headline, first) {
            (Some(h), _) if h.path.is_some() => Some(h),
            (_, Some(f)) => Some(f),
            (h, None) => h,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEAP: &str = include_str!("../tests/fixtures/asan-heap.txt");
    const UAF: &str = include_str!("../tests/fixtures/asan-uaf.txt");
    const LEAK: &str = include_str!("../tests/fixtures/asan-leak.txt");
    const TSAN: &str = include_str!("../tests/fixtures/tsan.txt");
    const UBSAN: &str = include_str!("../tests/fixtures/ubsan-stack.txt");
    const MSAN: &str = include_str!("../tests/fixtures/msan.txt");
    const ROOT: &str = "/home/anna/Projects/shapes";

    fn resolved(text: &str) -> Vec<SanitizerReport> {
        let mut r = parse(text);
        let exists = |p: &str| p == "src/shapes.cpp";
        resolve(&mut r, &|f| {
            crate::frames::project_path(f, ROOT, "cmake-build-debug", &exists)
        });
        r
    }

    #[test]
    fn address_sanitizer_heap_buffer_overflow() {
        let r = resolved(HEAP);
        assert_eq!(r.len(), 1);
        let r = &r[0];
        assert_eq!(r.sanitizer, Sanitizer::Address);
        assert_eq!(r.kind, "heap-buffer-overflow");
        assert!(r
            .message
            .starts_with("AddressSanitizer: heap-buffer-overflow on address 0x502000000040"));
        assert_eq!(r.stacks.len(), 2);
        assert_eq!(
            r.stacks[0].title,
            "READ of size 4 at 0x502000000040 thread T0"
        );
        let top = &r.stacks[0].frames[0];
        assert_eq!(top.function, "area(Polygon*)");
        assert_eq!(top.address.as_deref(), Some("0x55dd6920a58c"));
        assert_eq!(
            (top.file.as_deref(), top.line),
            (Some("src/shapes.cpp"), 21)
        );
        assert_eq!(top.path.as_deref(), Some("src/shapes.cpp"));
        let start = r.stacks[0].frames.last().unwrap();
        assert_eq!(start.function, "_start");
        assert_eq!(
            start.module.as_deref(),
            Some("/home/anna/Projects/shapes/cmake-build-debug/asan+0x2384")
        );
        assert!(start.path.is_none());
        assert_eq!(r.stacks[1].title, "allocated by thread T0 here");
        assert_eq!(
            r.stacks[1].frames[0].function,
            "operator new[](unsigned long)"
        );
        assert!(
            r.stacks[1].frames[0].path.is_none(),
            "the runtime is no project file"
        );
        assert_eq!(r.stacks[1].frames[1].function, "make_polygon(int)");
        assert_eq!(r.details, ["0x502000000040 is located 0 bytes after 16-byte region [0x502000000030,0x502000000040)"]);
        assert_eq!(
            r.summary.as_deref(),
            Some("AddressSanitizer: heap-buffer-overflow src/shapes.cpp:21 in area(Polygon*)")
        );
        let at = r.location.as_ref().unwrap();
        assert_eq!((at.path.as_deref(), at.line), (Some("src/shapes.cpp"), 21));
    }

    #[test]
    fn use_after_free_has_access_free_and_allocation_stacks() {
        let r = resolved(UAF);
        assert_eq!(r.len(), 1);
        let titles: Vec<_> = r[0].stacks.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(
            titles,
            [
                "READ of size 4 at 0x502000000018 thread T0",
                "freed by thread T0 here",
                "previously allocated by thread T0 here"
            ]
        );
        assert_eq!(r[0].kind, "heap-use-after-free");
        let freed = &r[0].stacks[1].frames[1];
        assert_eq!(freed.function, "destroy(Polygon*)");
        assert_eq!(freed.path.as_deref(), Some("src/shapes.cpp"));
        assert_eq!(freed.line, 27);
        assert_eq!(r[0].location.as_ref().unwrap().line, 34);
    }

    #[test]
    fn leak_sanitizer_reports_each_leak() {
        let r = resolved(LEAK);
        assert_eq!(r.len(), 2);
        assert!(r.iter().all(|x| x.sanitizer == Sanitizer::Leak));
        assert_eq!(r[0].kind, "Direct leak");
        assert_eq!(
            r[0].message,
            "Direct leak of 16 byte(s) in 1 object(s) allocated from"
        );
        assert_eq!(r[1].kind, "Indirect leak");
        assert_eq!(
            r[0].summary.as_deref(),
            Some("AddressSanitizer: 48 byte(s) leaked in 2 allocation(s).")
        );
        assert_eq!(r[0].location.as_ref().unwrap().line, 12);
        assert_eq!(r[1].location.as_ref().unwrap().line, 13);
    }

    #[test]
    fn thread_sanitizer_data_race() {
        let r = resolved(TSAN);
        assert_eq!(r.len(), 1);
        let r = &r[0];
        assert_eq!(r.sanitizer, Sanitizer::Thread);
        assert_eq!(r.kind, "data race");
        let titles: Vec<_> = r.stacks.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(
            titles,
            [
                "Read of size 4 at 0x55fa526f201c by thread T2",
                "Previous write of size 4 at 0x55fa526f201c by thread T1",
                "Thread T2 (tid=18505, running) created by main thread at",
                "Thread T1 (tid=18502, finished) created by main thread at"
            ]
        );
        let f = &r.stacks[0].frames[0];
        assert_eq!(f.function, "bump()");
        assert_eq!((f.file.as_deref(), f.line), (Some("src/shapes.cpp"), 43));
        assert_eq!(f.module.as_deref(), Some("tsan+0x168a"));
        assert_eq!(
            r.stacks[0].frames[1].file.as_deref(),
            Some("/usr/include/c++/13/bits/invoke.h")
        );
        assert_eq!(r.stacks[0].frames[6].function, "<null>");
        assert_eq!(
            r.details,
            ["Location is global 'counter' of size 4 at 0x55fa526f201c (tsan+0x501c)"]
        );
        assert_eq!(
            r.summary.as_deref(),
            Some("ThreadSanitizer: data race src/shapes.cpp:43 in bump()")
        );
    }

    #[test]
    fn undefined_behavior_with_and_without_a_stack() {
        let r = resolved(UBSAN);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].sanitizer, Sanitizer::Undefined);
        assert_eq!(r[0].kind, "signed integer overflow");
        assert_eq!(r[0].stacks[0].frames[0].function, "overflow(int)");
        let at = r[0].location.as_ref().unwrap();
        assert_eq!(
            (at.path.as_deref(), at.line, at.column),
            (Some("src/shapes.cpp"), 39, 18)
        );
        // GCC without print_stacktrace: the headline only, then the program's output.
        let plain = "src/shapes.cpp:39:18: runtime error: signed integer overflow: 2147483647 + 2 cannot be represented in type 'int'\n-2147483647\n";
        let r = resolved(plain);
        assert_eq!(r.len(), 1);
        assert!(r[0].stacks.is_empty());
        assert_eq!(
            r[0].location.as_ref().unwrap().path.as_deref(),
            Some("src/shapes.cpp")
        );
    }

    #[test]
    fn memory_sanitizer_origin() {
        let r = resolved(MSAN);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].sanitizer, Sanitizer::Memory);
        assert_eq!(r[0].kind, "use-of-uninitialized-value");
        assert_eq!(
            r[0].stacks[1].title,
            "Uninitialized value was created by a heap allocation"
        );
        let f = &r[0].stacks[0].frames[0];
        assert_eq!((f.line, f.column), (70, 13));
        assert_eq!(f.path.as_deref(), Some("src/shapes.cpp"));
    }

    #[test]
    fn several_reports_and_noise() {
        let text = format!("hello\n{HEAP}\nmore output\n{UBSAN}");
        let r = parse(&text);
        assert_eq!(r.len(), 2);
        assert!(parse("no sanitizer here\n#1 not a frame either").is_empty());
        let json = serde_json::to_value(&r[0]).unwrap();
        assert_eq!(json["sanitizer"], "address");
    }
}
