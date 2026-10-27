//! The Coroutines view of the Kotlin debugger: the expressions the debugger evaluates in the
//! suspended program to dump its coroutines (with `kotlinx-coroutines-debug`'s probes
//! installed), and the parser of what they return.
//!
//! Two dumps are read: `DebugProbes.dumpCoroutines` (text with each coroutine's state and
//! stack, evaluated as Kotlin) and the `toString()` of
//! `DebugProbesImpl.dumpCoroutinesInfo()` (names, states, jobs and dispatchers; an
//! expression Java evaluators accept too).

use serde::{Deserialize, Serialize};

/// Kotlin: the full text dump through the public `DebugProbes` API.
pub const DUMP_EXPRESSION: &str = "java.io.ByteArrayOutputStream().also { kotlinx.coroutines.debug.DebugProbes.dumpCoroutines(java.io.PrintStream(it)) }.toString()";

/// Java syntax (java-debug, or when the Kotlin expression fails): the coroutine infos.
pub const INFO_EXPRESSION: &str =
    "kotlinx.coroutines.debug.internal.DebugProbesImpl.INSTANCE.dumpCoroutinesInfo().toString()";

/// How to install the probes, shown when the dump is not available.
pub const ENABLE_HINT: &str = "Add kotlinx-coroutines-debug to the run's dependencies and call DebugProbes.install() at startup (or start the JVM with -javaagent:<kotlinx-coroutines-debug jar>), then suspend the program at a breakpoint.";

/// A frame of a coroutine's stack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Frame {
    /// `com.acme.MainKt$main$1.invokeSuspend`
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// 1-based, as in stack traces.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
}

/// A coroutine of the dump.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Coroutine {
    /// `CoroutineName`, else `coroutine`.
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
    /// `RUNNING`, `SUSPENDED` or `CREATED`.
    pub state: String,
    /// `StandaloneCoroutine{Active}@5e9f23b4`
    pub job: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dispatcher: Option<String>,
    pub frames: Vec<Frame>,
    /// Where the coroutine was created (`(Coroutine creation stacktrace)`).
    pub creation: Vec<Frame>,
}

/// What the evaluation returned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CoroutineDump {
    Coroutines {
        coroutines: Vec<Coroutine>,
    },
    /// The probes are not installed or the library is missing: `message` says how to fix it.
    Unavailable {
        message: String,
    },
}

/// The text of a string value as a debugger shows it (`"a\nb"`, `"a" (id=12)`).
pub fn unquote(value: &str) -> String {
    let v = value.trim();
    let v = match v.rfind(" (id=") {
        Some(i) if v.ends_with(')') => v[..i].trim_end(),
        _ => v,
    };
    let Some(inner) = v.strip_prefix('"').and_then(|s| s.strip_suffix('"')) else {
        return v.to_string();
    };
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => {}
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

fn frame(line: &str) -> Option<Frame> {
    let rest = line.trim().strip_prefix("at ")?;
    let (method, location) = match rest.split_once('(') {
        Some((m, l)) => (m.trim(), l.trim_end_matches(')')),
        None => (rest.trim(), ""),
    };
    let (file, line) = match location.rsplit_once(':') {
        Some((f, l)) => (Some(f.to_string()), l.parse().ok()),
        None if !location.is_empty() => (Some(location.to_string()), None),
        None => (None, None),
    };
    Some(Frame {
        method: method.to_string(),
        file,
        line,
    })
}

/// `"producer#2":StandaloneCoroutine{Active}@5e9f23b4` → name, id, job.
fn coroutine_label(s: &str) -> (String, Option<u64>, String) {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix('"') {
        if let Some((quoted, job)) = rest.split_once("\":") {
            let (name, id) = match quoted.rsplit_once('#') {
                Some((n, i)) => (n.to_string(), i.parse().ok()),
                None => (quoted.to_string(), None),
            };
            return (name, id, job.to_string());
        }
    }
    ("coroutine".to_string(), None, s.to_string())
}

fn parse_text_dump(text: &str) -> Vec<Coroutine> {
    let mut out: Vec<Coroutine> = Vec::new();
    let mut creation = false;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("Coroutine ") {
            let (label, state) = match rest.rsplit_once(", state: ") {
                Some((l, s)) => (l, s.trim().to_string()),
                None => (rest, String::new()),
            };
            let (name, id, job) = coroutine_label(label);
            out.push(Coroutine {
                name,
                id,
                state,
                job,
                dispatcher: None,
                frames: Vec::new(),
                creation: Vec::new(),
            });
            creation = false;
        } else if line.trim() == "(Coroutine creation stacktrace)" {
            creation = true;
        } else if let (Some(c), Some(f)) = (out.last_mut(), frame(line)) {
            if creation {
                c.creation.push(f);
            } else {
                c.frames.push(f);
            }
        }
    }
    out
}

/// Split at commas outside brackets and quotes.
fn split_top(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let (mut depth, mut quoted, mut start) = (0i32, false, 0);
    for (i, c) in s.char_indices() {
        match c {
            '"' => quoted = !quoted,
            '[' | '(' | '{' if !quoted => depth += 1,
            ']' | ')' | '}' if !quoted => depth -= 1,
            ',' if depth == 0 && !quoted => {
                out.push(s[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    if start < s.len() {
        out.push(s[start..].trim());
    }
    out.into_iter().filter(|p| !p.is_empty()).collect()
}

/// `[CoroutineInfo(state=SUSPENDED,context=[CoroutineName(producer), CoroutineId(2),
/// "producer#2":StandaloneCoroutine{Active}@5e9f23b4, Dispatchers.Default]), …]`
fn parse_info_list(text: &str) -> Option<Vec<Coroutine>> {
    let body = text.trim().strip_prefix('[')?.strip_suffix(']')?;
    let mut out = Vec::new();
    for item in split_top(body) {
        let open = item.find('(')?;
        let inner = item[open + 1..].strip_suffix(')')?;
        let mut state = String::new();
        let mut context = "";
        for field in split_top(inner) {
            if let Some(s) = field.strip_prefix("state=") {
                state = s.to_string();
            } else if let Some(c) = field.strip_prefix("context=") {
                context = c;
            }
        }
        let mut c = Coroutine {
            name: "coroutine".into(),
            id: None,
            state,
            job: String::new(),
            dispatcher: None,
            frames: Vec::new(),
            creation: Vec::new(),
        };
        let elements = context.trim().trim_start_matches('[').trim_end_matches(']');
        for e in split_top(elements) {
            if let Some(n) = e
                .strip_prefix("CoroutineName(")
                .and_then(|n| n.strip_suffix(')'))
            {
                c.name = n.to_string();
            } else if let Some(i) = e
                .strip_prefix("CoroutineId(")
                .and_then(|n| n.strip_suffix(')'))
            {
                c.id = i.parse().ok();
            } else if e.contains("Coroutine{") || e.contains("Coroutine@") || e.starts_with('"') {
                let (name, id, job) = coroutine_label(e);
                if c.name == "coroutine" && name != "coroutine" {
                    c.name = name;
                }
                c.id = c.id.or(id);
                c.job = job;
            } else if e.starts_with("Dispatchers.")
                || e.contains("Dispatcher")
                || e.contains("EventLoop")
            {
                c.dispatcher = Some(e.to_string());
            }
        }
        out.push(c);
    }
    Some(out)
}

/// Read what the evaluation of [`DUMP_EXPRESSION`] or [`INFO_EXPRESSION`] returned (or the
/// error it failed with).
pub fn parse_dump(value: &str) -> CoroutineDump {
    let text = unquote(value);
    let lower = text.to_ascii_lowercase();
    if lower.contains("not installed") || lower.contains("debug probes are not") {
        return CoroutineDump::Unavailable {
            message: format!("Coroutine debug probes are not installed. {ENABLE_HINT}"),
        };
    }
    if lower.contains("classnotfound")
        || lower.contains("cannot find")
        || lower.contains("unresolved reference")
        || lower.contains("cannot be resolved")
        || lower.contains("noclassdeffound")
    {
        return CoroutineDump::Unavailable {
            message: format!("kotlinx-coroutines-debug is not on the class path. {ENABLE_HINT}"),
        };
    }
    if text.contains("Coroutines dump") || text.lines().any(|l| l.starts_with("Coroutine ")) {
        return CoroutineDump::Coroutines {
            coroutines: parse_text_dump(&text),
        };
    }
    if let Some(list) = parse_info_list(&text) {
        return CoroutineDump::Coroutines { coroutines: list };
    }
    CoroutineDump::Unavailable {
        message: format!(
            "The debugger returned no coroutine dump ({}). {ENABLE_HINT}",
            text.lines().next().unwrap_or("").trim()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DUMP: &str = "Coroutines dump 2026/10/26 10:15:02\n\nCoroutine \"producer#2\":StandaloneCoroutine{Active}@5e9f23b4, state: SUSPENDED\n\tat kotlinx.coroutines.DelayKt.delay(Delay.kt:121)\n\tat com.acme.orders.MainKt$main$1$producer$1.invokeSuspend(Main.kt:18)\n\t(Coroutine creation stacktrace)\n\tat kotlin.coroutines.intrinsics.IntrinsicsKt__IntrinsicsJvmKt.createCoroutineUnintercepted(IntrinsicsJvm.kt:122)\n\nCoroutine \"consumer#3\":StandaloneCoroutine{Active}@1b6d3586, state: RUNNING\n\tat com.acme.orders.MainKt$main$1$consumer$1.invokeSuspend(Main.kt:27)\n\nCoroutine BlockingCoroutine{Active}@4554617c, state: RUNNING\n";

    #[test]
    fn reads_text_dumps() {
        let quoted = format!(
            "\"{}\"",
            DUMP.replace('\n', "\\n")
                .replace('\t', "\\t")
                .replace('"', "\\\"")
        );
        let CoroutineDump::Coroutines { coroutines } = parse_dump(&quoted) else {
            panic!("no coroutines");
        };
        assert_eq!(coroutines.len(), 3);
        let p = &coroutines[0];
        assert_eq!(
            (p.name.as_str(), p.id, p.state.as_str()),
            ("producer", Some(2), "SUSPENDED")
        );
        assert_eq!(p.job, "StandaloneCoroutine{Active}@5e9f23b4");
        assert_eq!(p.frames.len(), 2);
        assert_eq!(p.frames[1].file.as_deref(), Some("Main.kt"));
        assert_eq!(p.frames[1].line, Some(18));
        assert_eq!(p.creation.len(), 1);
        assert_eq!(coroutines[2].name, "coroutine");
        assert_eq!(coroutines[2].job, "BlockingCoroutine{Active}@4554617c");
    }

    #[test]
    fn reads_info_lists() {
        let v = "\"[CoroutineInfo(state=SUSPENDED,context=[CoroutineName(producer), CoroutineId(2), \\\"producer#2\\\":StandaloneCoroutine{Active}@5e9f23b4, Dispatchers.Default]), DebugCoroutineInfo(state=RUNNING,context=[CoroutineId(1), \\\"coroutine#1\\\":BlockingCoroutine{Active}@6d06d69c, BlockingEventLoop@7852e922])]\" (id=411)";
        let CoroutineDump::Coroutines { coroutines } = parse_dump(v) else {
            panic!("no coroutines");
        };
        assert_eq!(coroutines.len(), 2);
        assert_eq!(coroutines[0].name, "producer");
        assert_eq!(
            coroutines[0].dispatcher.as_deref(),
            Some("Dispatchers.Default")
        );
        assert_eq!(coroutines[1].id, Some(1));
        assert_eq!(coroutines[1].state, "RUNNING");
        assert_eq!(coroutines[1].job, "BlockingCoroutine{Active}@6d06d69c");
        assert_eq!(
            coroutines[1].dispatcher.as_deref(),
            Some("BlockingEventLoop@7852e922")
        );
        assert_eq!(
            parse_dump("\"[]\""),
            CoroutineDump::Coroutines { coroutines: vec![] }
        );
    }

    #[test]
    fn explains_missing_probes() {
        let e = parse_dump("java.lang.IllegalStateException: Debug probes are not installed");
        assert!(
            matches!(e, CoroutineDump::Unavailable { ref message } if message.contains("not installed"))
        );
        let e = parse_dump("Unresolved reference: DebugProbes");
        assert!(
            matches!(e, CoroutineDump::Unavailable { ref message } if message.contains("class path"))
        );
        assert!(matches!(
            parse_dump("42"),
            CoroutineDump::Unavailable { .. }
        ));
        assert_eq!(unquote("\"a\\\"b\""), "a\"b");
    }
}
