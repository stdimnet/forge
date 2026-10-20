//! Split a script into statements at `;`, respecting quotes, comments and PostgreSQL
//! dollar quoting.

use serde::Serialize;

use crate::config::Dialect;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Statement {
    pub text: String,
    /// Byte offsets of the statement in the script.
    pub start: usize,
    pub end: usize,
}

/// Statements of `sql`, trimmed; empty and comment-only pieces are dropped. MySQL also
/// knows `#` comments and backslash escapes in strings.
pub fn split_statements(sql: &str, dialect: Dialect) -> Vec<Statement> {
    let mysql = dialect == Dialect::MySql;
    let b = sql.as_bytes();
    let mut out = Vec::new();
    let mut start = 0;
    let mut i = 0;
    let push = |out: &mut Vec<Statement>, from: usize, to: usize| {
        let piece = &sql[from..to];
        if has_code(piece) {
            let lead = piece.len() - piece.trim_start().len();
            let text = piece.trim();
            out.push(Statement {
                text: text.to_string(),
                start: from + lead,
                end: from + lead + text.len(),
            });
        }
    };
    while i < b.len() {
        match b[i] {
            b'\'' | b'"' | b'`' => i = skip_quoted(b, i, mysql),
            b'-' if b.get(i + 1) == Some(&b'-') => i = skip_line(b, i),
            b'#' if mysql => i = skip_line(b, i),
            b'/' if b.get(i + 1) == Some(&b'*') => {
                i = sql[i + 2..]
                    .find("*/")
                    .map(|p| i + 2 + p + 2)
                    .unwrap_or(b.len());
            }
            b'$' => i = skip_dollar(sql, i),
            b';' => {
                push(&mut out, start, i);
                i += 1;
                start = i;
            }
            _ => i += 1,
        }
    }
    push(&mut out, start, b.len());
    out
}

fn skip_line(b: &[u8], i: usize) -> usize {
    b[i..]
        .iter()
        .position(|&c| c == b'\n')
        .map(|p| i + p + 1)
        .unwrap_or(b.len())
}

fn skip_quoted(b: &[u8], i: usize, backslash: bool) -> usize {
    let q = b[i];
    let mut j = i + 1;
    while j < b.len() {
        if backslash && b[j] == b'\\' && q != b'`' {
            j += 2;
            continue;
        }
        if b[j] == q {
            if b.get(j + 1) == Some(&q) {
                j += 2;
                continue;
            }
            return j + 1;
        }
        j += 1;
    }
    b.len()
}

fn skip_dollar(sql: &str, i: usize) -> usize {
    // `$tag$ ... $tag$` where tag is empty or an identifier; `$1` is a parameter.
    let rest = &sql[i + 1..];
    let tag_len = rest
        .bytes()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == b'_')
        .count();
    if rest.as_bytes().get(tag_len) != Some(&b'$')
        || rest.as_bytes().first().is_some_and(u8::is_ascii_digit)
    {
        return i + 1;
    }
    let delim = &sql[i..i + tag_len + 2];
    let body = i + delim.len();
    sql[body..]
        .find(delim)
        .map(|p| body + p + delim.len())
        .unwrap_or(sql.len())
}

/// Whether a piece contains anything besides whitespace and comments.
fn has_code(piece: &str) -> bool {
    let b = piece.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            c if c.is_ascii_whitespace() => i += 1,
            b'-' if b.get(i + 1) == Some(&b'-') => i = skip_line(b, i),
            b'#' => i = skip_line(b, i),
            b'/' if b.get(i + 1) == Some(&b'*') => {
                i = piece[i + 2..]
                    .find("*/")
                    .map(|p| i + 2 + p + 2)
                    .unwrap_or(b.len());
            }
            _ => return true,
        }
    }
    false
}

/// First keyword of a statement, upper-cased, skipping comments and parentheses.
pub fn leading_keyword(stmt: &str) -> String {
    let mut s = stmt.trim_start();
    loop {
        if let Some(r) = s.strip_prefix("--").or_else(|| s.strip_prefix('#')) {
            s = r.split_once('\n').map(|x| x.1).unwrap_or("").trim_start();
        } else if let Some(r) = s.strip_prefix("/*") {
            s = r.split_once("*/").map(|x| x.1).unwrap_or("").trim_start();
        } else if let Some(r) = s.strip_prefix('(') {
            s = r.trim_start();
        } else {
            break;
        }
    }
    s.chars()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect::<String>()
        .to_ascii_uppercase()
}

/// Statements that produce a result set (so an empty result still shows columns).
pub fn is_query(stmt: &str) -> bool {
    matches!(
        leading_keyword(stmt).as_str(),
        "SELECT"
            | "WITH"
            | "SHOW"
            | "PRAGMA"
            | "EXPLAIN"
            | "DESCRIBE"
            | "DESC"
            | "VALUES"
            | "TABLE"
    )
}

/// Transaction control typed by the user (so the console can track the state).
pub fn transaction_control(stmt: &str) -> Option<&'static str> {
    match leading_keyword(stmt).as_str() {
        "BEGIN" | "START" => Some("begin"),
        "COMMIT" | "END" => Some("commit"),
        "ROLLBACK" | "ABORT" => Some("rollback"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(sql: &str) -> Vec<String> {
        split_statements(sql, Dialect::MySql)
            .into_iter()
            .map(|s| s.text)
            .collect()
    }

    #[test]
    fn splits_on_semicolons_outside_strings_and_comments() {
        assert_eq!(
            texts(
                "select 1; select ';' as a -- ; no\n; /* ; */ select \"a;b\";\n\n-- only comment\n"
            ),
            vec![
                "select 1",
                "select ';' as a -- ; no",
                "/* ; */ select \"a;b\""
            ]
        );
    }

    #[test]
    fn handles_escapes_and_dollar_quotes() {
        assert_eq!(
            texts(r"select 'it''s; \' ;'; select 2"),
            vec![r"select 'it''s; \' ;'", "select 2"]
        );
        assert_eq!(
            texts("create function f() returns int as $$ begin; return 1; end $$ language plpgsql; select $1"),
            vec!["create function f() returns int as $$ begin; return 1; end $$ language plpgsql", "select $1"]
        );
        assert_eq!(texts("select $tag$a;b$tag$;"), vec!["select $tag$a;b$tag$"]);
        let pg: Vec<_> = split_statements(r"select 'C:\'; select 2", Dialect::Postgres)
            .into_iter()
            .map(|s| s.text)
            .collect();
        assert_eq!(pg, vec![r"select 'C:\'", "select 2"]);
    }

    #[test]
    fn reports_offsets() {
        let s = split_statements("  select 1 ;\nselect 2", Dialect::Sqlite);
        assert_eq!((s[0].start, s[0].end), (2, 10));
        assert_eq!(&"  select 1 ;\nselect 2"[s[1].start..s[1].end], "select 2");
    }

    #[test]
    fn classifies_statements() {
        assert!(is_query("-- x\n (SELECT 1)"));
        assert!(is_query("with x as (select 1) select * from x"));
        assert!(!is_query("update users set a = 1"));
        assert_eq!(transaction_control("start transaction"), Some("begin"));
        assert_eq!(transaction_control("commit"), Some("commit"));
        assert_eq!(transaction_control("select 1"), None);
    }
}
