//! Logcat, like Android Studio's: `adb logcat -v threadtime` lines parsed into entries,
//! the package of each process (from `adb shell ps` and ActivityManager's `Start proc`
//! messages), links from stack trace frames to the project's sources, and Android Studio's
//! query language for filtering:
//!
//! - `level:WARN` (or `level:W`): that level and above;
//! - `tag:OkHttp`, `package:com.acme.notes`, `package:mine` (the project's apps),
//!   `message:timeout`, `process:12034`, `is:crash` (fatal exceptions), `is:stacktrace`;
//! - `-tag:Choreographer` excludes, `tag~:^Net.*` matches a regular expression;
//! - other words (or `"quoted text"`) must appear in the tag or the message.
//!
//! Terms of the same key are alternatives (`tag:A tag:B`), different keys must all match.

use std::collections::{HashMap, VecDeque};

use regex_lite::Regex;
use serde::{Deserialize, Serialize};

/// The log priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Level {
    #[serde(rename = "V")]
    Verbose,
    #[serde(rename = "D")]
    Debug,
    #[serde(rename = "I")]
    Info,
    #[serde(rename = "W")]
    Warn,
    #[serde(rename = "E")]
    Error,
    #[serde(rename = "A")]
    Assert,
}

impl Level {
    pub fn from_char(c: char) -> Option<Level> {
        Some(match c {
            'V' => Level::Verbose,
            'D' => Level::Debug,
            'I' => Level::Info,
            'W' => Level::Warn,
            'E' => Level::Error,
            'F' | 'A' => Level::Assert,
            _ => return None,
        })
    }

    /// `WARN`, `warning`, `W`, `e`, `fatal`, …
    pub fn parse(s: &str) -> Option<Level> {
        match s.trim().to_ascii_lowercase().as_str() {
            "v" | "verbose" => Some(Level::Verbose),
            "d" | "debug" => Some(Level::Debug),
            "i" | "info" => Some(Level::Info),
            "w" | "warn" | "warning" => Some(Level::Warn),
            "e" | "error" => Some(Level::Error),
            "a" | "f" | "assert" | "fatal" => Some(Level::Assert),
            _ => None,
        }
    }
}

/// A link from a stack trace frame to a project file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameLink {
    /// Project-relative path.
    pub path: String,
    /// 1-based line.
    pub line: u32,
    /// Byte range of `File.kt:42` in the message.
    pub start: usize,
    pub end: usize,
}

/// A logcat entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub id: u64,
    /// `10-27` and `10:14:03.512`.
    pub date: String,
    pub time: String,
    pub pid: u32,
    pub tid: u32,
    pub level: Level,
    pub tag: String,
    pub message: String,
    /// The process's package (`com.acme.notes`), when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
    /// A frame of a stack trace (`at …(File.kt:42)`) or an exception line.
    #[serde(default)]
    pub stack: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<FrameLink>,
}

/// Parse one `threadtime` line (`10-27 10:14:03.512  1432  1459 I ActivityManager: Start
/// proc …`). `--------- beginning of main` and other lines give `None`.
pub fn parse_line(line: &str) -> Option<LogEntry> {
    let line = line.trim_end_matches(['\r', '\n']);
    let mut rest = line.trim_start();
    let mut word = || -> Option<&str> {
        let r = rest.trim_start();
        let end = r.find(char::is_whitespace).unwrap_or(r.len());
        let (w, tail) = r.split_at(end);
        rest = tail;
        Some(w).filter(|w| !w.is_empty())
    };
    let mut date = word()?.to_string();
    // `-v threadtime,year`: `2026-10-27`.
    if date.len() == 10 && date.as_bytes()[4] == b'-' {
        date = date[5..].to_string();
    }
    if date.len() != 5 || date.as_bytes()[2] != b'-' {
        return None;
    }
    let time = word()?.to_string();
    if !time.contains(':') {
        return None;
    }
    let pid = word()?.parse().ok()?;
    let tid = word()?.parse().ok()?;
    let level_word = word()?;
    let level = Level::from_char(level_word.chars().next()?).filter(|_| level_word.len() == 1)?;
    // `Tag    : message`; the tag may contain spaces, the separator is the first ": ".
    let rest = rest.trim_start();
    let (tag, message) = match rest.find(": ") {
        Some(i) => (rest[..i].trim_end(), &rest[i + 2..]),
        None => (rest.trim_end_matches(':').trim_end(), ""),
    };
    Some(LogEntry {
        id: 0,
        date,
        time,
        pid,
        tid,
        level,
        tag: tag.to_string(),
        message: message.to_string(),
        package: None,
        stack: false,
        link: None,
    })
}

/// A stack trace frame: (class, file, line, byte range of `file:line`).
pub fn frame(message: &str) -> Option<(String, String, u32, usize, usize)> {
    let t = message.trim_start();
    let offset = message.len() - t.len();
    let body = t.strip_prefix("at ")?;
    let open = body.rfind('(')?;
    let close = body[open..].find(')')? + open;
    let method = &body[..open];
    let class = method.rsplit_once('.').map(|(c, _)| c)?;
    let loc = &body[open + 1..close];
    let (file, line) = loc.rsplit_once(':')?;
    let line = line.parse().ok()?;
    if !(file.ends_with(".kt") || file.ends_with(".java")) {
        return None;
    }
    let start = offset + 3 + open + 1;
    Some((
        class.to_string(),
        file.to_string(),
        line,
        start,
        start + loc.len(),
    ))
}

/// The project file of a frame: a source file named like the frame's file in the folder of
/// the class's package (`com.acme.notes.NoteRepository$save$1` → `…/com/acme/notes/
/// NoteRepository.kt`).
pub fn resolve(class: &str, file: &str, sources: &[String]) -> Option<String> {
    let top = class.split('$').next().unwrap_or(class);
    let package = top.rsplit_once('.').map(|(p, _)| p).unwrap_or("");
    let suffix = if package.is_empty() {
        format!("/{file}")
    } else {
        format!("/{}/{file}", package.replace('.', "/"))
    };
    sources
        .iter()
        .find(|s| s.replace('\\', "/").ends_with(&suffix) || **s == suffix[1..])
        .cloned()
}

fn is_exception_line(message: &str) -> bool {
    let m = message.trim_start();
    m.starts_with("Caused by: ")
        || m.starts_with("FATAL EXCEPTION")
        || m.starts_with("... ")
        || Regex::new(r"^[\w$]+(\.[\w$]+)+(Exception|Error|Throwable)(: |$)")
            .is_ok_and(|re| re.is_match(m))
}

/// The text of `pending` (bytes read from `adb logcat`) that can be handed over: everything
/// except an incomplete UTF-8 sequence at the end, which waits for the next read. Invalid
/// bytes are replaced by U+FFFD instead of holding back everything after them.
pub fn take_text(pending: &mut Vec<u8>) -> String {
    let mut out = String::new();
    let mut rest: &[u8] = pending;
    loop {
        match std::str::from_utf8(rest) {
            Ok(s) => {
                out.push_str(s);
                rest = &[];
                break;
            }
            Err(e) => {
                let (valid, after) = rest.split_at(e.valid_up_to());
                out.push_str(std::str::from_utf8(valid).unwrap_or_default());
                match e.error_len() {
                    Some(bad) => {
                        out.push(char::REPLACEMENT_CHARACTER);
                        rest = &after[bad..];
                    }
                    // Incomplete at the end: keep it for the next read.
                    None => {
                        rest = after;
                        break;
                    }
                }
            }
        }
    }
    let keep = rest.len();
    pending.drain(..pending.len() - keep);
    out
}

/// The arguments of a stream: the recent `tail` lines, then new ones.
pub fn stream_args(serial: &str, tail: u32) -> Vec<String> {
    vec![
        "-s".into(),
        serial.into(),
        "logcat".into(),
        "-v".into(),
        "threadtime".into(),
        "-T".into(),
        tail.max(1).to_string(),
    ]
}

/// The arguments that list the processes of a device.
pub fn ps_args(serial: &str) -> Vec<String> {
    vec![
        "-s".into(),
        serial.into(),
        "shell".into(),
        "ps".into(),
        "-A".into(),
        "-o".into(),
        "PID,NAME".into(),
    ]
}

// ---- Query --------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Field {
    Tag,
    Package,
    Message,
    Level,
    Process,
    Is,
    Text,
}

#[derive(Debug, Clone)]
struct Term {
    field: Field,
    value: String,
    negate: bool,
    regex: Option<Regex>,
}

/// A parsed Logcat query.
#[derive(Debug, Clone, Default)]
pub struct Query {
    terms: Vec<Term>,
}

/// Split a query into words, keeping `"quoted text"` together.
fn words(q: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for c in q.chars() {
        match c {
            '"' => quoted = !quoted,
            c if c.is_whitespace() && !quoted => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

impl Query {
    pub fn parse(q: &str) -> Query {
        let mut terms = Vec::new();
        for w in words(q) {
            let (negate, w) = match w.strip_prefix('-') {
                Some(r) if r.contains(':') => (true, r.to_string()),
                _ => (false, w),
            };
            let (field, value, regex) = match w.split_once(':') {
                Some((k, v)) if !v.is_empty() => {
                    let (k, re) = match k.strip_suffix('~') {
                        Some(k) => (k, true),
                        None => (k, false),
                    };
                    let field = match k {
                        "tag" => Field::Tag,
                        "package" => Field::Package,
                        "message" | "msg" => Field::Message,
                        "level" => Field::Level,
                        "process" | "pid" => Field::Process,
                        "is" => Field::Is,
                        _ => {
                            terms.push(Term {
                                field: Field::Text,
                                value: w.to_ascii_lowercase(),
                                negate,
                                regex: None,
                            });
                            continue;
                        }
                    };
                    (field, v.to_string(), re)
                }
                _ => (Field::Text, w.clone(), false),
            };
            let compiled = if regex {
                Regex::new(&format!("(?i){value}")).ok()
            } else {
                None
            };
            terms.push(Term {
                field,
                value: if regex {
                    value
                } else {
                    value.to_ascii_lowercase()
                },
                negate,
                regex: compiled,
            });
        }
        Query { terms }
    }

    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }

    fn term_matches(t: &Term, e: &LogEntry, mine: &[String]) -> bool {
        let text = |s: &str| match &t.regex {
            Some(re) => re.is_match(s),
            None => s.to_ascii_lowercase().contains(&t.value),
        };
        match t.field {
            Field::Tag => match &t.regex {
                Some(re) => re.is_match(&e.tag),
                None => {
                    e.tag.to_ascii_lowercase() == t.value
                        || (t.value.ends_with('*')
                            && e.tag
                                .to_ascii_lowercase()
                                .starts_with(t.value.trim_end_matches('*')))
                }
            },
            Field::Package => {
                let pkg = e.package.as_deref().unwrap_or_default();
                if t.value == "mine" {
                    // `:sync` processes of the app belong to it too.
                    mine.iter()
                        .any(|m| pkg == m || pkg.starts_with(&format!("{m}:")))
                } else {
                    text(pkg)
                }
            }
            Field::Message => text(&e.message),
            Field::Level => Level::parse(&t.value).is_some_and(|l| e.level >= l),
            Field::Process => t.value.parse::<u32>().is_ok_and(|p| p == e.pid),
            Field::Is => match t.value.as_str() {
                "crash" => e.level >= Level::Error && (e.stack || e.tag == "AndroidRuntime"),
                "stacktrace" => e.stack,
                "firebase" => e.tag.starts_with("Firebase") || e.tag.starts_with("FA"),
                _ => false,
            },
            Field::Text => text(&e.message) || text(&e.tag),
        }
    }

    /// Whether an entry matches; `mine` are the project's application ids.
    pub fn matches(&self, e: &LogEntry, mine: &[String]) -> bool {
        let mut groups: HashMap<Field, bool> = HashMap::new();
        for t in &self.terms {
            let hit = Self::term_matches(t, e, mine);
            if t.negate {
                if hit {
                    return false;
                }
                continue;
            }
            if t.field == Field::Text {
                // Every free word must match.
                if !hit {
                    return false;
                }
                continue;
            }
            let g = groups.entry(t.field).or_insert(false);
            *g |= hit;
        }
        groups.values().all(|g| *g)
    }
}

// ---- Buffer -------------------------------------------------------------------------------

/// What the Logcat view shows: the last matching entries.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogcatView {
    pub entries: Vec<LogEntry>,
    /// Entries in the buffer.
    pub total: usize,
    /// Entries that match the query (`entries` holds at most the limit).
    pub matched: usize,
    /// Packages seen, for the package filter's completion.
    pub packages: Vec<String>,
}

/// The entries of a stream (a ring buffer) with the processes and sources it resolves.
#[derive(Debug)]
pub struct Logcat {
    entries: VecDeque<LogEntry>,
    capacity: usize,
    next_id: u64,
    partial: String,
    processes: HashMap<u32, String>,
    sources: Vec<String>,
    start_proc: Regex,
}

impl Logcat {
    pub fn new(capacity: usize) -> Logcat {
        Logcat {
            entries: VecDeque::new(),
            capacity: capacity.max(100),
            next_id: 1,
            partial: String::new(),
            processes: HashMap::new(),
            sources: Vec::new(),
            start_proc: Regex::new(r"Start proc (\d+):([^/\s]+)/").expect("valid pattern"),
        }
    }

    /// Project source files (`.kt`, `.java`) that stack trace frames link to.
    pub fn set_sources(&mut self, sources: Vec<String>) {
        self.sources = sources;
    }

    /// Processes from `adb shell ps`.
    pub fn set_processes(&mut self, ps: &[(u32, String)]) {
        for (pid, name) in ps {
            self.processes.insert(*pid, name.clone());
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.partial.clear();
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Add output of `adb logcat` (any chunk; an incomplete last line waits for the rest).
    /// Returns the new entries.
    pub fn push(&mut self, chunk: &str) -> Vec<LogEntry> {
        self.partial.push_str(chunk);
        let Some(last) = self.partial.rfind('\n') else {
            return Vec::new();
        };
        let complete: String = self.partial.drain(..=last).collect();
        let mut out = Vec::new();
        for line in complete.lines() {
            let Some(mut e) = parse_line(line) else {
                continue;
            };
            if let Some(c) = self.start_proc.captures(&e.message) {
                if let Ok(pid) = c[1].parse() {
                    self.processes.insert(pid, c[2].to_string());
                }
            }
            e.id = self.next_id;
            self.next_id += 1;
            e.package = self.processes.get(&e.pid).cloned();
            if let Some((class, file, line, start, end)) = frame(&e.message) {
                e.stack = true;
                e.link = resolve(&class, &file, &self.sources).map(|path| FrameLink {
                    path,
                    line,
                    start,
                    end,
                });
            } else if e.level >= Level::Warn && is_exception_line(&e.message) {
                e.stack = true;
            }
            if self.entries.len() == self.capacity {
                self.entries.pop_front();
            }
            self.entries.push_back(e.clone());
            out.push(e);
        }
        out
    }

    /// Packages seen in the buffer (sorted).
    pub fn packages(&self) -> Vec<String> {
        let mut p: Vec<String> = self.processes.values().cloned().collect();
        p.sort();
        p.dedup();
        p
    }

    /// The last `limit` entries that match `query`.
    pub fn view(&self, query: &Query, mine: &[String], limit: usize) -> LogcatView {
        let matching: Vec<&LogEntry> = self
            .entries
            .iter()
            .filter(|e| query.matches(e, mine))
            .collect();
        let matched = matching.len();
        let skip = matched.saturating_sub(limit);
        LogcatView {
            entries: matching.into_iter().skip(skip).cloned().collect(),
            total: self.entries.len(),
            matched,
            packages: self.packages(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_bytes_do_not_stall_the_stream() {
        let mut pending = b"I/app: \xff ok\n\xe2\x82".to_vec();
        assert_eq!(take_text(&mut pending), "I/app: \u{fffd} ok\n");
        // The incomplete euro sign waits for its last byte.
        assert_eq!(pending, b"\xe2\x82");
        pending.extend_from_slice(b"\xac\n");
        assert_eq!(take_text(&mut pending), "\u{20ac}\n");
        assert!(pending.is_empty());
        let mut bad_first = b"\xffrest".to_vec();
        assert_eq!(take_text(&mut bad_first), "\u{fffd}rest");
    }

    const LOG: &str = include_str!("../tests/fixtures/logcat-threadtime.txt");
    const PS: &str = include_str!("../tests/fixtures/adb-ps.txt");

    fn buffer() -> Logcat {
        let mut l = Logcat::new(1000);
        l.set_sources(vec![
            "app/src/main/java/com/acme/notes/NoteRepository.kt".into(),
            "app/src/main/java/com/acme/notes/MainActivity.kt".into(),
        ]);
        l.set_processes(&crate::adb::parse_ps(PS));
        l.push(LOG);
        l
    }

    #[test]
    fn parses_threadtime_lines() {
        let e = parse_line("10-27 10:14:03.512  1432  1459 I ActivityManager: Start proc 12034:com.acme.notes/u0a190 for top-activity").unwrap();
        assert_eq!(
            (e.date.as_str(), e.time.as_str(), e.pid, e.tid),
            ("10-27", "10:14:03.512", 1432, 1459)
        );
        assert_eq!(e.level, Level::Info);
        assert_eq!(e.tag, "ActivityManager");
        assert!(e.message.starts_with("Start proc 12034"));
        let spaced = parse_line("10-27 10:14:05.001 12034 12034 D Notes Sync : started").unwrap();
        assert_eq!(
            (spaced.tag.as_str(), spaced.message.as_str()),
            ("Notes Sync", "started")
        );
        let year = parse_line("2026-10-27 10:14:05.001 12034 12034 W Tag: m").unwrap();
        assert_eq!(year.date, "10-27");
        assert!(parse_line("--------- beginning of main").is_none());
        assert!(parse_line("").is_none());
        assert_eq!(Level::parse("WARN"), Some(Level::Warn));
        assert_eq!(Level::from_char('F'), Some(Level::Assert));
    }

    #[test]
    fn packages_and_stack_trace_links() {
        let l = buffer();
        assert_eq!(l.len(), 16);
        let crash: Vec<&LogEntry> = l
            .entries
            .iter()
            .filter(|e| e.tag == "AndroidRuntime")
            .collect();
        assert!(crash
            .iter()
            .all(|e| e.package.as_deref() == Some("com.acme.notes")));
        assert_eq!(crash.iter().filter(|e| e.stack).count(), 6);
        let linked: Vec<(&str, u32)> = crash
            .iter()
            .filter_map(|e| e.link.as_ref().map(|k| (k.path.as_str(), k.line)))
            .collect();
        assert_eq!(
            linked,
            [
                ("app/src/main/java/com/acme/notes/NoteRepository.kt", 42),
                ("app/src/main/java/com/acme/notes/MainActivity.kt", 31)
            ]
        );
        let first = crash.iter().find_map(|e| e.link.clone()).unwrap();
        let msg = &crash.iter().find(|e| e.link.is_some()).unwrap().message;
        assert_eq!(&msg[first.start..first.end], "NoteRepository.kt:42");
        // Frames outside the project have no link.
        assert!(crash
            .iter()
            .any(|e| e.message.contains("android.app.Activity") && e.link.is_none()));
        // A process started during the stream gets its package from ActivityManager.
        let sync = l.entries.iter().find(|e| e.pid == 13001).unwrap();
        assert_eq!(sync.package.as_deref(), Some("com.acme.notes:sync"));
        assert!(l.packages().contains(&"com.acme.notes".to_string()));
    }

    #[test]
    fn queries() {
        let l = buffer();
        let mine = vec!["com.acme.notes".to_string()];
        let count = |q: &str| l.view(&Query::parse(q), &mine, 1000).matched;
        assert_eq!(count(""), 16);
        assert_eq!(count("package:mine"), 13);
        assert_eq!(count("package:mine level:error"), 7);
        assert_eq!(count("level:W"), 9);
        assert_eq!(count("tag:NoteRepository"), 2);
        assert_eq!(count("tag:NoteRepository tag:OkHttp"), 4);
        assert_eq!(count("-tag:AndroidRuntime package:mine"), 6);
        assert_eq!(count("tag~:^Note"), 3);
        assert_eq!(count("is:crash"), 7);
        assert_eq!(count("\"saving note\""), 1);
        assert_eq!(count("timeout okhttp"), 1);
        assert_eq!(count("process:1432"), 2);
        let v = l.view(&Query::parse("package:mine"), &mine, 3);
        assert_eq!((v.entries.len(), v.matched, v.total), (3, 13, 16));
        assert!(v.entries[0].id < v.entries[2].id);
    }

    #[test]
    fn partial_chunks_and_capacity() {
        let mut l = Logcat::new(100);
        assert_eq!(
            l.push("10-27 10:14:03.512  1432  1459 I Tag: one\n10-27 10:14:03.6")
                .len(),
            1
        );
        assert_eq!(l.len(), 1);
        let more = l.push("00  1432  1459 I Tag: two\n");
        assert_eq!(more[0].message, "two");
        for i in 0..150 {
            l.push(&format!("10-27 10:14:03.512  1  1 I T: {i}\n"));
        }
        assert_eq!(l.len(), 100);
        l.clear();
        assert!(l.is_empty());
        assert_eq!(stream_args("emulator-5554", 500)[5..], ["-T", "500"]);
        assert_eq!(ps_args("x")[2], "shell");
        assert_eq!(frame("\tat a.B.c(Unknown Source:4)"), None);
    }
}
