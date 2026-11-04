//! The log of one language server: how Forge started it, what the server wrote to stderr or
//! sent as `window/logMessage`, and how it ended. Kept in memory (the last [`MAX_LINES`]
//! lines) and shown in Settings | Languages & Frameworks | Language Servers, so a server that
//! fails to start says why, also where Forge has no console (Windows).

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// Lines kept per server.
pub const MAX_LINES: usize = 500;

/// Where a log line came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogSource {
    /// Forge itself: start, start errors, exit.
    Forge,
    /// The server's stderr.
    Stderr,
    /// `window/logMessage` and `window/showMessage`.
    Server,
}

impl LogSource {
    fn tag(self) -> &'static str {
        match self {
            LogSource::Forge => "forge",
            LogSource::Stderr => "stderr",
            LogSource::Server => "server",
        }
    }
}

#[derive(Debug, Clone)]
struct Line {
    source: LogSource,
    text: String,
}

/// A shared, bounded server log. Cheap to clone; clones write to the same log.
#[derive(Debug, Clone, Default)]
pub struct ServerLog(Arc<Mutex<VecDeque<Line>>>);

impl ServerLog {
    pub fn new() -> Self {
        Self::default()
    }

    /// Append `text` (one entry per line).
    pub fn push(&self, source: LogSource, text: &str) {
        let mut lines = self.0.lock().expect("server log lock");
        for line in text.lines() {
            let line = line.trim_end();
            if line.is_empty() {
                continue;
            }
            if lines.len() == MAX_LINES {
                lines.pop_front();
            }
            lines.push_back(Line {
                source,
                text: line.to_string(),
            });
        }
    }

    /// Every line, oldest first, as `[source] text`.
    pub fn lines(&self) -> Vec<String> {
        self.0
            .lock()
            .expect("server log lock")
            .iter()
            .map(|l| format!("[{}] {}", l.source.tag(), l.text))
            .collect()
    }

    /// The last `n` lines of the server's stderr, oldest first.
    pub fn stderr_tail(&self, n: usize) -> Vec<String> {
        let lines = self.0.lock().expect("server log lock");
        let mut tail: Vec<String> = lines
            .iter()
            .rev()
            .filter(|l| l.source == LogSource::Stderr)
            .take(n)
            .map(|l| l.text.clone())
            .collect();
        tail.reverse();
        tail
    }

    pub fn is_empty(&self) -> bool {
        self.0.lock().expect("server log lock").is_empty()
    }
}

/// `error`, completed with what the server said on stderr and how it ended, for the status
/// bar and the Language Servers page: "server exited during startup (exit code: 1):
/// php-lsp: cannot read …".
pub fn explain(error: &str, status: Option<&str>, stderr_tail: &[String]) -> String {
    let mut out = error.trim().to_string();
    if let Some(status) = status.filter(|s| !out.contains(*s)) {
        out.push_str(&format!(" ({status})"));
    }
    if !stderr_tail.is_empty() {
        out.push_str(": ");
        out.push_str(&stderr_tail.join(" | "));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_last_lines_with_their_source() {
        let log = ServerLog::new();
        log.push(LogSource::Forge, "Starting forge --php-lsp --stdio");
        log.push(LogSource::Stderr, "line 1\r\n\r\nline 2\n");
        log.push(LogSource::Server, "indexing");
        assert_eq!(
            log.lines(),
            vec![
                "[forge] Starting forge --php-lsp --stdio",
                "[stderr] line 1",
                "[stderr] line 2",
                "[server] indexing",
            ]
        );
        assert_eq!(log.stderr_tail(1), vec!["line 2"]);
        assert_eq!(log.stderr_tail(5), vec!["line 1", "line 2"]);

        for i in 0..MAX_LINES + 10 {
            log.push(LogSource::Stderr, &format!("x{i}"));
        }
        let lines = log.lines();
        assert_eq!(lines.len(), MAX_LINES);
        assert_eq!(
            lines.last().unwrap(),
            &format!("[stderr] x{}", MAX_LINES + 9)
        );
    }

    #[test]
    fn explains_failures() {
        assert_eq!(
            explain("server exited during startup", None, &[]),
            "server exited during startup"
        );
        assert_eq!(
            explain(
                "server exited during startup",
                Some("exit code: 1"),
                &["php-lsp: bad".into(), "more".into()]
            ),
            "server exited during startup (exit code: 1): php-lsp: bad | more"
        );
    }
}
