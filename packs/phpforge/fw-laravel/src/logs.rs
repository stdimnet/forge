//! Laravel logs: `storage/logs/*.log` written by Monolog's line formatter
//! (`[2026-10-26 09:14:03] production.ERROR: Message {"exception":"[object] (…)\n[stacktrace]\n#0 …"}`)
//! parsed into entries with level, time, channel, message, the exception and its stack
//! frames. Frame paths are mapped into the project, also when the log was written inside a
//! container (`/var/www/html/app/…`).

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Largest part of a log file that is read (its end).
pub const DEFAULT_MAX_BYTES: u64 = 2 * 1024 * 1024;

/// Monolog levels, most severe last.
pub const LEVELS: &[&str] = &[
    "debug",
    "info",
    "notice",
    "warning",
    "error",
    "critical",
    "alert",
    "emergency",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogFile {
    /// Project-relative path.
    pub path: String,
    pub size: u64,
    /// Seconds since the Unix epoch.
    #[serde(default)]
    pub modified: u64,
}

/// One frame of a stack trace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StackFrame {
    /// The file as written in the log.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// The file in the project, when it exists there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// 1-based line, as written in the log.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    /// `App\Services\Payment->charge()`, `{main}`.
    pub call: String,
    /// Below `vendor/`.
    #[serde(default)]
    pub vendor: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogException {
    pub class: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// Where it was thrown.
    pub frame: StackFrame,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    /// `2026-10-26 09:14:03`
    pub time: String,
    /// `local`, `production`
    pub channel: String,
    /// Lowercase level (`error`).
    pub level: String,
    pub message: String,
    /// Context and extra data as written (without the exception).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub context: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exception: Option<LogException>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub frames: Vec<StackFrame>,
    /// 0-based line of the entry in the file.
    pub line: u32,
}

/// Reply of a log request.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogPage {
    /// The file that was read (empty when there are no logs).
    pub path: String,
    pub files: Vec<LogFile>,
    pub entries: Vec<LogEntry>,
    pub size: u64,
    /// Only the end of a large file was read.
    #[serde(default)]
    pub truncated: bool,
}

/// `storage/logs/*.log`, the most recently written first.
pub fn log_files(root: &Path) -> Vec<LogFile> {
    let mut out = Vec::new();
    let Ok(dir) = std::fs::read_dir(root.join("storage/logs")) else {
        return out;
    };
    for e in dir.flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("log") {
            continue;
        }
        let Ok(meta) = e.metadata() else { continue };
        out.push(LogFile {
            path: crate::project::rel_path(root, &p),
            size: meta.len(),
            modified: meta
                .modified()
                .ok()
                .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0),
        });
    }
    out.sort_by(|a, b| b.modified.cmp(&a.modified).then(a.path.cmp(&b.path)));
    out
}

/// Maps absolute paths from a log to project files (cached per directory prefix).
pub struct PathMapper<'a> {
    root_str: String,
    exists: &'a dyn Fn(&str) -> bool,
    prefixes: HashMap<String, Option<String>>,
}

impl<'a> PathMapper<'a> {
    pub fn new(root: &'a Path, exists: &'a dyn Fn(&str) -> bool) -> Self {
        let mut root_str = root.to_string_lossy().replace('\\', "/");
        if !root_str.ends_with('/') {
            root_str.push('/');
        }
        Self {
            root_str,
            exists,
            prefixes: HashMap::new(),
        }
    }

    /// `/var/www/html/app/Models/User.php` -> `app/Models/User.php`.
    pub fn map(&mut self, file: &str) -> Option<String> {
        let file = file.replace('\\', "/");
        if let Some(rel) = file.strip_prefix(&self.root_str) {
            return Some(rel.to_string());
        }
        // A known foreign prefix (the container's project root).
        for (prefix, mapped) in &self.prefixes {
            if let Some(rest) = file.strip_prefix(prefix.as_str()) {
                return mapped.as_ref().map(|m| format!("{m}{rest}"));
            }
        }
        let mut search = 0;
        while let Some(i) = file[search..].find('/') {
            let cut = search + i + 1;
            search = cut;
            let rest = &file[cut..];
            if rest.is_empty() {
                break;
            }
            if (self.exists)(rest) {
                self.prefixes
                    .insert(file[..cut].to_string(), Some(String::new()));
                return Some(rest.to_string());
            }
        }
        None
    }
}

fn level_of(name: &str) -> Option<String> {
    let l = name.to_ascii_lowercase();
    LEVELS.contains(&l.as_str()).then_some(l)
}

/// `[2026-10-26 09:14:03] local.ERROR: message` -> (time, channel, level, message).
fn header(line: &str) -> Option<(String, String, String, String)> {
    let rest = line.strip_prefix('[')?;
    let close = rest.find(']')?;
    let time = &rest[..close];
    if time.len() < 10
        || !time.as_bytes()[..4].iter().all(|b| b.is_ascii_digit())
        || time.as_bytes()[4] != b'-'
    {
        return None;
    }
    let rest = rest[close + 1..].trim_start();
    let colon = rest
        .find(": ")
        .or_else(|| rest.strip_suffix(':').map(|r| r.len()))?;
    let head = &rest[..colon];
    let (channel, level) = head.rsplit_once('.')?;
    let level = level_of(level)?;
    let message = rest.get(colon + 2..).unwrap_or("").to_string();
    // `2026-10-26T09:14:03.123456+00:00` -> `2026-10-26 09:14:03`
    let mut shown = time.replace('T', " ");
    if let Some(dot) = shown.find('.') {
        shown.truncate(dot);
    } else if shown.len() > 19 {
        shown.truncate(19);
    }
    Some((shown, channel.to_string(), level, message))
}

/// `#3 /app/x.php(12): App\X->y()`, `#4 [internal function]: …`, `#5 {main}`.
fn frame(line: &str, map: &mut PathMapper) -> Option<StackFrame> {
    let rest = line.trim().strip_prefix('#')?;
    let sp = rest.find(' ')?;
    if !rest[..sp].chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let body = &rest[sp + 1..];
    if body == "{main}" {
        return Some(StackFrame {
            file: None,
            path: None,
            line: None,
            call: "{main}".into(),
            vendor: false,
        });
    }
    if let Some(call) = body.strip_prefix("[internal function]: ") {
        return Some(StackFrame {
            file: None,
            path: None,
            line: None,
            call: unescape(call),
            vendor: false,
        });
    }
    let close = body.find("): ")?;
    let open = body[..close].rfind('(')?;
    let file = &body[..open];
    let line_no: u32 = body[open + 1..close].parse().ok()?;
    let call = unescape(&body[close + 3..]);
    let path = map.map(file);
    let vendor = path.as_deref().unwrap_or(file).contains("vendor/");
    Some(StackFrame {
        file: Some(file.to_string()),
        path,
        line: Some(line_no),
        call,
        vendor,
    })
}

/// Undo the JSON escaping of exception text inside the context (`App\\X` -> `App\X`).
fn unescape(s: &str) -> String {
    s.replace("\\\\", "\\")
        .replace("\\\"", "\"")
        .replace("\\/", "/")
}

/// `[object] (RuntimeException(code: 0): Payment failed at /app/x.php:42)`
fn exception(text: &str, map: &mut PathMapper) -> Option<LogException> {
    let start = text.find("[object] (")? + 10;
    let rest = &text[start..];
    let paren = rest.find("(code: ")?;
    let class = rest[..paren].to_string();
    let after = &rest[paren + 7..];
    let code_end = after.find("): ")?;
    let code = after[..code_end].to_string();
    let msg_and_at = &after[code_end + 3..];
    let first_line = msg_and_at.lines().next().unwrap_or("");
    let at = first_line.rfind(" at ")?;
    let message = unescape(&first_line[..at]);
    let loc = first_line[at + 4..].trim_end_matches(')');
    let (file, line) = loc.rsplit_once(':')?;
    let line: u32 = line.trim().parse().ok()?;
    let path = map.map(file);
    let vendor = path.as_deref().unwrap_or(file).contains("vendor/");
    Some(LogException {
        class,
        message,
        code: (code != "0").then_some(code),
        frame: StackFrame {
            file: Some(file.to_string()),
            path,
            line: Some(line),
            call: String::new(),
            vendor,
        },
    })
}

/// Split `message {"a":1} []` into the message and its context.
fn split_context(message: &str) -> (String, String) {
    for marker in [" {\"", " {}", " [{", " []"] {
        if let Some(i) = message.find(marker) {
            let ctx = message[i + 1..].trim().to_string();
            let ctx = if ctx == "[] []" || ctx == "[]" || ctx == "{} []" {
                String::new()
            } else {
                ctx
            };
            return (message[..i].to_string(), ctx);
        }
    }
    (message.to_string(), String::new())
}

/// Parse log text. `first_line` is the file line of the text's first line.
pub fn parse(text: &str, first_line: u32, map: &mut PathMapper) -> Vec<LogEntry> {
    let mut out: Vec<LogEntry> = Vec::new();
    let mut body: Vec<&str> = Vec::new();
    let finish = |entry: &mut LogEntry, body: &[&str], map: &mut PathMapper| {
        let mut full = entry.message.clone();
        for l in body {
            full.push('\n');
            full.push_str(l);
        }
        if let Some(e) = exception(&full, map) {
            entry.exception = Some(e);
            let i = entry
                .message
                .find(" {\"exception\"")
                .unwrap_or(entry.message.len());
            entry.message.truncate(i);
            entry.context.clear();
        }
        for l in body {
            if let Some(f) = frame(l.trim_start_matches('"'), map) {
                entry.frames.push(f);
            }
        }
    };
    for (n, line) in text.lines().enumerate() {
        if let Some((time, channel, level, message)) = header(line) {
            if let Some(last) = out.last_mut() {
                finish(last, &body, map);
            }
            body.clear();
            let (message, context) = split_context(&message);
            out.push(LogEntry {
                time,
                channel,
                level,
                message: if context.starts_with("{\"exception\"") {
                    format!("{message} {context}")
                } else {
                    message
                },
                context,
                exception: None,
                frames: Vec::new(),
                line: first_line + n as u32,
            });
        } else if !out.is_empty() {
            body.push(line);
        }
    }
    if let Some(last) = out.last_mut() {
        finish(last, &body, map);
    }
    for e in &mut out {
        if e.exception.is_some() {
            e.context.clear();
        }
    }
    out
}

/// Read and parse a log file (only its last `max_bytes`).
pub fn read(root: &Path, rel: &str, max_bytes: u64) -> LogPage {
    let files = log_files(root);
    let rel = if rel.is_empty() {
        match files.first() {
            Some(f) => f.path.clone(),
            None => {
                return LogPage {
                    files,
                    ..Default::default()
                }
            }
        }
    } else {
        rel.to_string()
    };
    let path = root.join(&rel);
    // Only files of the logs directory.
    if !rel.starts_with("storage/logs/") || rel.contains("..") {
        return LogPage {
            files,
            ..Default::default()
        };
    }
    let Ok(mut f) = std::fs::File::open(&path) else {
        return LogPage {
            path: rel,
            files,
            ..Default::default()
        };
    };
    let size = f.metadata().map(|m| m.len()).unwrap_or(0);
    let start = size.saturating_sub(max_bytes);
    let mut buf = Vec::new();
    if f.seek(SeekFrom::Start(start)).is_err() || f.read_to_end(&mut buf).is_err() {
        return LogPage {
            path: rel,
            files,
            size,
            ..Default::default()
        };
    }
    let mut text = String::from_utf8_lossy(&buf).into_owned();
    let mut first_line = 0;
    if start > 0 {
        // Drop the partial line; line numbers are unknown, count from the cut.
        if let Some(i) = text.find('\n') {
            text.drain(..=i);
        }
        first_line = 0;
    }
    let exists = |rel: &str| root.join(rel).is_file();
    let mut map = PathMapper::new(root, &exists);
    LogPage {
        entries: parse(&text, first_line, &mut map),
        path: rel,
        files,
        size,
        truncated: start > 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOG: &str = r#"[2026-10-26 09:14:03] local.INFO: Order placed {"order":1042} []
[2026-10-26 09:15:11] production.ERROR: Payment failed {"exception":"[object] (RuntimeException(code: 0): Payment failed at /var/www/html/app/Services/Payment.php:42)
[stacktrace]
#0 /var/www/html/app/Http/Controllers/OrderController.php(31): App\\Services\\Payment->charge()
#1 /var/www/html/vendor/laravel/framework/src/Illuminate/Routing/Controller.php(54): App\\Http\\Controllers\\OrderController->store()
#2 [internal function]: Illuminate\\Routing\\Controller->callAction()
#3 {main}
"}
[2026-10-26T09:16:00.123456+00:00] local.WARNING: Slow query: select * from orders
"#;

    fn mapper_test(f: impl FnOnce(&mut PathMapper)) {
        let exists = |p: &str| {
            matches!(
                p,
                "app/Services/Payment.php"
                    | "app/Http/Controllers/OrderController.php"
                    | "vendor/laravel/framework/src/Illuminate/Routing/Controller.php"
            )
        };
        let root = Path::new("/home/me/shop");
        let mut m = PathMapper::new(root, &exists);
        f(&mut m);
    }

    #[test]
    fn parses_entries_exceptions_and_frames() {
        mapper_test(|m| {
            let e = parse(LOG, 0, m);
            assert_eq!(e.len(), 3);
            assert_eq!(
                (e[0].level.as_str(), e[0].channel.as_str()),
                ("info", "local")
            );
            assert_eq!(e[0].message, "Order placed");
            assert_eq!(e[0].context, "{\"order\":1042} []");
            let err = &e[1];
            assert_eq!(err.level, "error");
            assert_eq!(err.message, "Payment failed");
            assert_eq!(err.line, 1);
            let ex = err.exception.as_ref().unwrap();
            assert_eq!(ex.class, "RuntimeException");
            assert_eq!(ex.frame.path.as_deref(), Some("app/Services/Payment.php"));
            assert_eq!(ex.frame.line, Some(42));
            assert_eq!(err.frames.len(), 4);
            assert_eq!(
                err.frames[0].path.as_deref(),
                Some("app/Http/Controllers/OrderController.php")
            );
            assert_eq!(err.frames[0].line, Some(31));
            assert!(err.frames[1].vendor);
            assert_eq!(
                err.frames[2].call,
                "Illuminate\\Routing\\Controller->callAction()"
            );
            assert_eq!(err.frames[3].call, "{main}");
            assert_eq!(e[2].time, "2026-10-26 09:16:00");
            assert_eq!(e[2].message, "Slow query: select * from orders");
        });
    }

    #[test]
    fn maps_project_paths() {
        mapper_test(|m| {
            assert_eq!(
                m.map("/home/me/shop/app/X.php").as_deref(),
                Some("app/X.php")
            );
            assert_eq!(m.map("/srv/other/Thing.php"), None);
            assert_eq!(
                m.map("/var/www/html/app/Services/Payment.php").as_deref(),
                Some("app/Services/Payment.php")
            );
            // The container prefix is remembered.
            assert_eq!(
                m.map("/var/www/html/routes/web.php").as_deref(),
                Some("routes/web.php")
            );
        });
    }

    #[test]
    fn reads_the_end_of_large_files() {
        let dir = tempfile::tempdir().unwrap();
        let logs = dir.path().join("storage/logs");
        std::fs::create_dir_all(&logs).unwrap();
        let mut text = String::new();
        for i in 0..200 {
            text.push_str(&format!("[2026-10-26 10:00:00] local.DEBUG: line {i}\n"));
        }
        std::fs::write(logs.join("laravel.log"), &text).unwrap();
        std::fs::write(logs.join("notes.txt"), "x").unwrap();
        let page = read(dir.path(), "", 1000);
        assert_eq!(page.path, "storage/logs/laravel.log");
        assert_eq!(page.files.len(), 1);
        assert!(page.truncated);
        assert_eq!(page.entries.last().unwrap().message, "line 199");
        assert!(page.entries.len() < 30);
        assert!(read(dir.path(), "../.env", 1000).entries.is_empty());
    }
}
