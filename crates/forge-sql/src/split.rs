//! Split a script into statements at `;`, respecting quotes, comments and PostgreSQL
//! dollar quoting.

use serde::Serialize;

use crate::dialect::Dialect;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Statement {
    pub text: String,
    /// Byte offsets of the statement in the script.
    pub start: usize,
    pub end: usize,
}

/// Statements of `sql`, trimmed; empty and comment-only pieces are dropped. MySQL and
/// ClickHouse also know `#` comments and backslash escapes in strings; SQL Server batches
/// end at `GO` lines; Oracle PL/SQL blocks end at a `/` line; Redis consoles have one
/// command per line and MongoDB consoles one call per line or `;`.
pub fn split_statements(sql: &str, dialect: Dialect) -> Vec<Statement> {
    match dialect {
        Dialect::Redis => return split_lines(sql),
        Dialect::Mongo => return split_mongo(sql),
        _ => {}
    }
    let mysql = matches!(dialect, Dialect::MySql | Dialect::ClickHouse);
    let b = sql.as_bytes();
    let mut out = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < b.len() {
        let line_start = i == 0 || b[i - 1] == b'\n';
        if line_start {
            if let Some(next) = separator_line(sql, i, dialect) {
                push_piece(sql, &mut out, start, i);
                i = next;
                start = i;
                continue;
            }
        }
        match b[i] {
            // PostgreSQL `E'…'` strings know backslash escapes.
            b'\'' if dialect == Dialect::Postgres && is_e_prefix(b, i) => {
                i = skip_quoted(b, i, true)
            }
            b'\'' | b'"' | b'`' => i = skip_quoted(b, i, mysql),
            // `]]` is an escaped `]`.
            b'[' if dialect == Dialect::SqlServer => i = skip_bracket(b, i),
            b'-' if b.get(i + 1) == Some(&b'-') => i = skip_line(b, i),
            b'#' if mysql => i = skip_line(b, i),
            b'/' if b.get(i + 1) == Some(&b'*') => {
                i = sql[i + 2..]
                    .find("*/")
                    .map(|p| i + 2 + p + 2)
                    .unwrap_or(b.len());
            }
            b'$' if dialect == Dialect::Postgres || mysql || dialect == Dialect::Sqlite => {
                i = skip_dollar(sql, i)
            }
            b';' if dialect == Dialect::Oracle && is_plsql(&sql[start..i]) => i += 1,
            b';' if dialect == Dialect::Sqlite && in_sqlite_trigger(&sql[start..i]) => i += 1,
            b';' => {
                push_piece(sql, &mut out, start, i);
                i += 1;
                start = i;
            }
            _ => i += 1,
        }
    }
    push_piece(sql, &mut out, start, b.len());
    out
}

fn push_piece(sql: &str, out: &mut Vec<Statement>, from: usize, to: usize) {
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
}

/// A line that ends a statement by itself (`GO` in T-SQL, `/` in Oracle): the offset after it.
fn separator_line(sql: &str, i: usize, dialect: Dialect) -> Option<usize> {
    let end = sql[i..].find('\n').map(|p| i + p).unwrap_or(sql.len());
    let line = sql[i..end].trim();
    let hit = match dialect {
        // `GO`, `GO 5`, `GO -- end of batch`.
        Dialect::SqlServer => {
            let code = line.split_once("--").map_or(line, |(code, _)| code);
            let mut words = code.split_whitespace();
            words.next().is_some_and(|w| w.eq_ignore_ascii_case("go"))
                && words
                    .next()
                    .map_or(true, |n| n.bytes().all(|c| c.is_ascii_digit()))
                && words.next().is_none()
        }
        Dialect::Oracle => line == "/",
        _ => false,
    };
    hit.then_some((end + 1).min(sql.len()))
}

/// Whether an Oracle statement is a PL/SQL block (its `;` do not end it).
fn is_plsql(piece: &str) -> bool {
    let words: Vec<String> = leading_words(piece, 5);
    let w: Vec<&str> = words.iter().map(String::as_str).collect();
    match w.as_slice() {
        ["BEGIN", ..] | ["DECLARE", ..] => true,
        ["CREATE", rest @ ..] => rest.iter().any(|x| {
            matches!(
                *x,
                "PROCEDURE" | "FUNCTION" | "PACKAGE" | "TRIGGER" | "TYPE" | "LIBRARY"
            )
        }),
        _ => false,
    }
}

/// Whether a SQLite `CREATE TRIGGER … BEGIN … END` body is still open (its `;` end the
/// statements inside, not the trigger).
fn in_sqlite_trigger(piece: &str) -> bool {
    let words = leading_words(piece, 3);
    let trigger = matches!(
        words
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .as_slice(),
        ["CREATE", "TRIGGER", ..] | ["CREATE", "TEMP" | "TEMPORARY", "TRIGGER"]
    );
    if !trigger {
        return false;
    }
    let tail = piece.trim_end().to_ascii_uppercase();
    let ended = tail.ends_with("END")
        && !tail[..tail.len() - 3]
            .chars()
            .last()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_');
    !ended
}

/// The first `n` words of a statement, upper-cased, skipping comments.
fn leading_words(stmt: &str, n: usize) -> Vec<String> {
    let mut s = strip_leading_comments(stmt);
    let mut out = Vec::new();
    while out.len() < n {
        let word: String = s
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if word.is_empty() {
            break;
        }
        s = s[word.len()..].trim_start();
        out.push(word.to_ascii_uppercase());
    }
    out
}

fn strip_leading_comments(stmt: &str) -> &str {
    let mut s = stmt.trim_start();
    loop {
        if let Some(r) = s.strip_prefix("--").or_else(|| s.strip_prefix('#')) {
            s = r.split_once('\n').map(|x| x.1).unwrap_or("").trim_start();
        } else if let Some(r) = s.strip_prefix("//") {
            s = r.split_once('\n').map(|x| x.1).unwrap_or("").trim_start();
        } else if let Some(r) = s.strip_prefix("/*") {
            s = r.split_once("*/").map(|x| x.1).unwrap_or("").trim_start();
        } else if let Some(r) = s.strip_prefix('(') {
            s = r.trim_start();
        } else {
            return s;
        }
    }
}

/// Redis: every line that is not empty or a `#`/`//` comment is a command.
fn split_lines(text: &str) -> Vec<Statement> {
    let mut out = Vec::new();
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim();
        if !trimmed.is_empty() && !trimmed.starts_with('#') && !trimmed.starts_with("//") {
            let lead = line.len() - line.trim_start().len();
            out.push(Statement {
                text: trimmed.to_string(),
                start: offset + lead,
                end: offset + lead + trimmed.len(),
            });
        }
        offset += line.len();
    }
    out
}

/// MongoDB shell: calls end at `;` or at a line break outside brackets, unless the next
/// line continues the call chain (`.sort(...)`).
fn split_mongo(text: &str) -> Vec<Statement> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'\'' | b'"' | b'`' => i = skip_quoted(b, i, true),
            // Up to the line break, which may end the call.
            b'/' if b.get(i + 1) == Some(&b'/') => {
                i = b[i..]
                    .iter()
                    .position(|&c| c == b'\n')
                    .map(|p| i + p)
                    .unwrap_or(b.len())
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                i = text[i + 2..]
                    .find("*/")
                    .map(|p| i + 2 + p + 2)
                    .unwrap_or(b.len());
            }
            b'{' | b'[' | b'(' => {
                depth += 1;
                i += 1;
            }
            b'}' | b']' | b')' => {
                depth -= 1;
                i += 1;
            }
            b';' if depth <= 0 => {
                push_piece(text, &mut out, start, i);
                i += 1;
                start = i;
                depth = 0;
            }
            b'\n' if depth <= 0 => {
                let continues = text[i + 1..].trim_start().starts_with('.');
                if !has_code(&text[start..i]) {
                    // Blank and comment lines are not part of the next call.
                    start = i + 1;
                } else if !continues {
                    push_piece(text, &mut out, start, i);
                    start = i + 1;
                    depth = 0;
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
    push_piece(text, &mut out, start, b.len());
    out
}

/// Whether the quote at `i` follows a lone `E`/`e` (not the end of a longer word).
fn is_e_prefix(b: &[u8], i: usize) -> bool {
    i >= 1
        && matches!(b[i - 1], b'e' | b'E')
        && (i < 2
            || !(b[i - 2].is_ascii_alphanumeric()
                || b[i - 2] == b'_'
                || b[i - 2] == b'$'
                || b[i - 2] >= 0x80))
}

fn skip_line(b: &[u8], i: usize) -> usize {
    b[i..]
        .iter()
        .position(|&c| c == b'\n')
        .map(|p| i + p + 1)
        .unwrap_or(b.len())
}

fn skip_bracket(b: &[u8], i: usize) -> usize {
    let mut j = i + 1;
    while j < b.len() {
        if b[j] == b']' {
            if b.get(j + 1) == Some(&b']') {
                j += 2;
                continue;
            }
            return j + 1;
        }
        j += 1;
    }
    b.len()
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
            b'/' if b.get(i + 1) == Some(&b'/') => i = skip_line(b, i),
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

fn utf16_to_byte(text: &str, units: usize) -> usize {
    let mut seen = 0;
    for (i, c) in text.char_indices() {
        if seen >= units {
            return i;
        }
        seen += c.len_utf16();
    }
    text.len()
}

fn byte_to_utf16(text: &str, byte: usize) -> usize {
    text[..byte.min(text.len())].encode_utf16().count()
}

/// What Ctrl+Enter runs in a console: the selection `from..to` if there is one, otherwise
/// the statement at the caret `from` (or the closest one before it, like DataGrip).
/// Offsets in and out are UTF-16 code units (editor positions).
pub fn statement_at(sql: &str, from: usize, to: usize, dialect: Dialect) -> Option<Statement> {
    let utf16 = |s: Statement| Statement {
        start: byte_to_utf16(sql, s.start),
        end: byte_to_utf16(sql, s.end),
        text: s.text,
    };
    let (from_b, to_b) = (utf16_to_byte(sql, from), utf16_to_byte(sql, to));
    if to_b > from_b {
        let text = &sql[from_b..to_b];
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return None;
        }
        let start = from_b + (text.len() - text.trim_start().len());
        return Some(utf16(Statement {
            text: trimmed.to_string(),
            start,
            end: start + trimmed.len(),
        }));
    }
    let stmts = split_statements(sql, dialect);
    let inside = stmts
        .iter()
        .position(|s| from_b >= s.start && from_b <= s.end + 1);
    let before = || stmts.iter().rposition(|s| s.end <= from_b);
    let idx = inside.or_else(before).unwrap_or(0);
    stmts.into_iter().nth(idx).map(utf16)
}

/// First keyword of a statement, upper-cased, skipping comments and parentheses.
pub fn leading_keyword(stmt: &str) -> String {
    strip_leading_comments(stmt)
        .chars()
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
            | "EXEC"
            | "EXECUTE"
            | "SP_HELP"
    )
}

/// Transaction control typed by the user (so the console can track the state).
pub fn transaction_control(stmt: &str) -> Option<&'static str> {
    match leading_keyword(stmt).as_str() {
        // `BEGIN TRAN`, not a T-SQL `BEGIN … END` block.
        "BEGIN"
            if leading_words(stmt, 2)
                .get(1)
                .is_some_and(|w| w.starts_with("TRAN")) =>
        {
            Some("begin")
        }
        "BEGIN" if leading_words(stmt, 2).len() == 1 => Some("begin"),
        "START" => Some("begin"),
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
    fn keeps_sqlite_trigger_bodies_together() {
        let sql = "CREATE TRIGGER t AFTER INSERT ON a BEGIN UPDATE a SET n = 1; DELETE FROM b; END;\nSELECT 1;";
        let parts: Vec<_> = split_statements(sql, Dialect::Sqlite)
            .into_iter()
            .map(|s| s.text)
            .collect();
        assert_eq!(
            parts,
            vec![
                "CREATE TRIGGER t AFTER INSERT ON a BEGIN UPDATE a SET n = 1; DELETE FROM b; END",
                "SELECT 1"
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
        let pg: Vec<_> = split_statements(r"select E'a\';b'; select 'c\'", Dialect::Postgres)
            .into_iter()
            .map(|s| s.text)
            .collect();
        assert_eq!(pg, vec![r"select E'a\';b'", r"select 'c\'"]);
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
    fn finds_the_statement_at_the_caret() {
        let sql = "select 1;\n\nselect 'é' from t;\n-- end";
        let at = |from: usize, to: usize| {
            statement_at(sql, from, to, Dialect::MySql).map(|s| (s.text, s.start, s.end))
        };
        assert_eq!(at(3, 3), Some(("select 1".into(), 0, 8)));
        assert_eq!(at(9, 9), Some(("select 1".into(), 0, 8)));
        assert_eq!(at(15, 15), Some(("select 'é' from t".into(), 11, 28)));
        assert_eq!(at(40, 40), Some(("select 'é' from t".into(), 11, 28)));
        assert_eq!(at(10, 22), Some(("select 'é'".into(), 11, 21)));
        assert_eq!(at(9, 11), None);
        assert_eq!(statement_at("-- x", 0, 0, Dialect::MySql), None);
        let sql = "SELECT 1;\n\nSELECT *\nFROM users;\n";
        let text =
            |from: usize, to: usize| statement_at(sql, from, to, Dialect::MySql).map(|s| s.text);
        assert_eq!(text(15, 15).as_deref(), Some("SELECT *\nFROM users"));
        assert_eq!(text(3, 3).as_deref(), Some("SELECT 1"));
        // After the last statement: the one before the caret.
        assert_eq!(
            text(sql.len(), sql.len()).as_deref(),
            Some("SELECT *\nFROM users")
        );
        assert_eq!(text(0, 6).as_deref(), Some("SELECT"));
    }

    fn texts_in(sql: &str, d: Dialect) -> Vec<String> {
        split_statements(sql, d)
            .into_iter()
            .map(|s| s.text)
            .collect()
    }

    #[test]
    fn splits_sql_server_batches_and_brackets() {
        assert_eq!(
            texts_in(
                "select [a;b], [c]];d] from t\nGO\nselect 2;select 3\ngo 2 -- twice\nselect 4",
                Dialect::SqlServer
            ),
            vec![
                "select [a;b], [c]];d] from t",
                "select 2",
                "select 3",
                "select 4"
            ]
        );
    }

    #[test]
    fn keeps_oracle_plsql_blocks_together() {
        let sql = "select 1 from dual;\nBEGIN\n  x := 1;\n  y := 2;\nEND;\n/\nselect 2 from dual;";
        assert_eq!(
            texts_in(sql, Dialect::Oracle),
            vec![
                "select 1 from dual",
                "BEGIN\n  x := 1;\n  y := 2;\nEND;",
                "select 2 from dual"
            ]
        );
    }

    #[test]
    fn splits_redis_lines_and_mongo_calls() {
        let r = split_statements("# keys\nSET a 1\n\n  GET a  \n", Dialect::Redis);
        assert_eq!(r.len(), 2);
        assert_eq!(r[1].text, "GET a");
        assert_eq!(
            &"# keys\nSET a 1\n\n  GET a  \n"[r[1].start..r[1].end],
            "GET a"
        );
        assert_eq!(
            texts_in(
                "// users\ndb.users.find({\n  age: {$gt: 30}\n})\n  .limit(5)\ndb.orders.countDocuments({}); db.x.find()",
                Dialect::Mongo
            ),
            vec![
                "db.users.find({\n  age: {$gt: 30}\n})\n  .limit(5)",
                "db.orders.countDocuments({})",
                "db.x.find()"
            ]
        );
    }

    #[test]
    fn classifies_statements() {
        assert!(is_query("-- x\n (SELECT 1)"));
        assert!(is_query("with x as (select 1) select * from x"));
        assert!(!is_query("update users set a = 1"));
        assert_eq!(transaction_control("start transaction"), Some("begin"));
        assert_eq!(transaction_control("commit"), Some("commit"));
        assert_eq!(transaction_control("select 1"), None);
        assert_eq!(transaction_control("BEGIN TRANSACTION"), Some("begin"));
        assert_eq!(transaction_control("begin x := 1; end;"), None);
    }
}
