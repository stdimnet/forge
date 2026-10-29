//! Query history: every statement a console or Run SQL file executed, per data source,
//! newest first, with time, duration and row count. The app keeps one JSON file per data
//! source in the user data dir; this module is the model and the search.

use serde::{Deserialize, Serialize};

/// Entries kept per data source.
pub const DEFAULT_LIMIT: usize = 1000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub sql: String,
    /// Unix milliseconds when it ran.
    pub time: u64,
    pub duration_ms: u64,
    /// Rows returned (queries) or affected (changes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rows: Option<i64>,
    pub data_source: String,
    /// The console (tab name or file) it ran in.
    #[serde(default)]
    pub console: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct History {
    /// Newest first.
    pub entries: Vec<HistoryEntry>,
}

/// What the history popup asks for.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HistoryQuery {
    /// Words that must all occur in the statement (any case).
    pub text: String,
    /// Only this console.
    pub console: Option<String>,
    pub limit: Option<usize>,
}

impl History {
    /// Add an execution; the same statement run again in the same console moves to the top.
    pub fn add(&mut self, entry: HistoryEntry, limit: usize) {
        let same =
            |e: &HistoryEntry| e.sql.trim() == entry.sql.trim() && e.console == entry.console;
        if self.entries.first().is_some_and(same) {
            self.entries.remove(0);
        }
        self.entries.insert(0, entry);
        self.entries.truncate(limit.max(1));
    }

    pub fn search(&self, q: &HistoryQuery) -> Vec<HistoryEntry> {
        let words: Vec<String> = q.text.split_whitespace().map(str::to_lowercase).collect();
        self.entries
            .iter()
            .filter(|e| {
                q.console
                    .as_deref()
                    .is_none_or(|c| c.is_empty() || e.console == c)
            })
            .filter(|e| {
                let sql = e.sql.to_lowercase();
                words.iter().all(|w| sql.contains(w))
            })
            .take(q.limit.unwrap_or(500))
            .cloned()
            .collect()
    }

    /// A file's history (empty when it is missing or broken).
    pub fn parse(json: &str) -> Self {
        serde_json::from_str(json).unwrap_or_default()
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{\"entries\":[]}".into())
    }
}

/// File name of a data source's history (`<id>.json`, unsafe characters replaced).
pub fn file_name(data_source: &str) -> String {
    let safe: String = data_source
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("{}.json", if safe.is_empty() { "default" } else { &safe })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(sql: &str, console: &str, time: u64) -> HistoryEntry {
        HistoryEntry {
            sql: sql.into(),
            time,
            duration_ms: 3,
            rows: Some(1),
            data_source: "ds".into(),
            console: console.into(),
            error: None,
        }
    }

    #[test]
    fn newest_first_and_capped() {
        let mut h = History::default();
        h.add(e("SELECT 1", "console", 1), 3);
        h.add(e("SELECT 2", "console", 2), 3);
        h.add(e("SELECT 2", "console", 3), 3);
        h.add(e("SELECT 3", "console_1", 4), 3);
        h.add(e("SELECT 4", "console", 5), 3);
        let sqls: Vec<&str> = h.entries.iter().map(|x| x.sql.as_str()).collect();
        assert_eq!(sqls, ["SELECT 4", "SELECT 3", "SELECT 2"]);
        assert_eq!(h.entries[2].time, 3);
    }

    #[test]
    fn searches_words_and_console() {
        let mut h = History::default();
        h.add(e("SELECT * FROM users WHERE id = 1", "console", 1), 10);
        h.add(e("SELECT * FROM orders", "console_1", 2), 10);
        let q = |text: &str, console: Option<&str>| {
            h.search(&HistoryQuery {
                text: text.into(),
                console: console.map(str::to_string),
                limit: None,
            })
            .len()
        };
        assert_eq!(q("from USERS", None), 1);
        assert_eq!(q("select", None), 2);
        assert_eq!(q("select", Some("console_1")), 1);
        let round = History::parse(&h.to_json());
        assert_eq!(round, h);
        assert_eq!(History::parse("not json"), History::default());
        assert_eq!(file_name("pg/shop"), "pg_shop.json");
    }
}
