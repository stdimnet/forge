//! Read-only classification of SQL statements, on the tokens of [`crate::lex`].
//!
//! A statement counts as a read only when it starts with a known read (`SELECT`, `WITH`,
//! `SHOW`, `EXPLAIN`, …, a safe `SET` or `PRAGMA`, transaction control) and none of its
//! words starts a write anywhere inside it: data-modifying CTEs, `SELECT … INTO`, and T-SQL
//! batches whose statements need no `;` (`SELECT 1 DROP TABLE t`). Everything else, also
//! anything the tokenizer cannot read (an unclosed quote or comment), is a write. Each
//! plausible reading of the dialect ([`crate::lex::variants`]) must agree.

use crate::dialect::Dialect;
use crate::lex::{tokenize, variants, Tok, Token};

/// Why `stmt` is not a read (`DELETE`, `SET`, …), or `None` when it only reads.
pub fn write_kind(stmt: &str, d: Dialect) -> Option<String> {
    for &o in variants(d) {
        let lexed = tokenize(stmt, d, o);
        if lexed.unterminated {
            return Some("an unclosed quote or comment".into());
        }
        for part in lexed.tokens.split(|t| t.tok == Tok::Semi) {
            if let Some(kind) = statement_write(part, d) {
                return Some(kind);
            }
        }
    }
    None
}

fn upper(t: &Token<'_>) -> Option<String> {
    t.word().map(str::to_ascii_uppercase)
}

/// The write of one statement's tokens (no `;` in them).
fn statement_write(toks: &[Token<'_>], d: Dialect) -> Option<String> {
    let first = toks.iter().position(|t| t.tok != Tok::Punct('('))?;
    let toks = &toks[first..];
    let Some(lead) = upper(&toks[0]) else {
        return Some("this statement".into());
    };
    let rest = &toks[1..];
    let next = rest.first().and_then(upper);
    let ok = match lead.as_str() {
        "SELECT" | "WITH" | "VALUES" | "SHOW" | "DESCRIBE" | "DESC" => true,
        "TABLE" => d == Dialect::Postgres,
        // Oracle's `EXPLAIN PLAN` writes into PLAN_TABLE.
        "EXPLAIN" => d != Dialect::Oracle,
        "USE" => matches!(d, Dialect::MySql | Dialect::SqlServer | Dialect::ClickHouse),
        "EXISTS" => d == Dialect::ClickHouse,
        "HELP" => d == Dialect::MySql,
        "PRINT" => d == Dialect::SqlServer,
        "PRAGMA" => d == Dialect::Sqlite && pragma_reads(rest),
        "SET" => {
            if let Some(kind) = set_write(rest, d) {
                return Some(kind);
            }
            true
        }
        // T-SQL variables; Oracle and PostgreSQL `DECLARE` start blocks and cursors.
        "DECLARE" => d == Dialect::SqlServer && next.as_deref().is_some_and(is_variable),
        "BEGIN" => matches!(
            next.as_deref(),
            None | Some(
                "TRAN"
                    | "TRANSACTION"
                    | "WORK"
                    | "ISOLATION"
                    | "READ"
                    | "NOT"
                    | "DEFERRABLE"
                    | "DEFERRED"
                    | "IMMEDIATE"
                    | "EXCLUSIVE"
                    | "DISTRIBUTED"
            )
        ),
        "START" => next.as_deref() == Some("TRANSACTION"),
        "COMMIT" | "ROLLBACK" | "SAVEPOINT" | "RELEASE" => true,
        "END" | "ABORT" => d == Dialect::Postgres,
        "SAVE" => d == Dialect::SqlServer,
        "EXEC" | "EXECUTE" => d == Dialect::SqlServer && safe_procedure(rest),
        w => d == Dialect::SqlServer && SAFE_PROCEDURES.contains(&w),
    };
    if !ok {
        return Some(lead);
    }
    if matches!(lead.as_str(), "BEGIN" | "START" | "SET") && rest.iter().any(|t| t.is("WRITE")) {
        return Some("READ WRITE".into());
    }
    embedded_write(toks, d)
}

fn is_variable(w: &str) -> bool {
    w.starts_with('@') && !w.starts_with("@@")
}

/// Words that start a write wherever they appear in a statement (sub-statements,
/// data-modifying CTEs, T-SQL batches without `;`). Followed by `(` most of them are
/// functions (`REPLACE(s, 'a', 'b')`).
const EMBEDDED_WRITES: &[&str] = &[
    "INSERT",
    "UPDATE",
    "DELETE",
    "MERGE",
    "UPSERT",
    "REPLACE",
    "CREATE",
    "ALTER",
    "DROP",
    "TRUNCATE",
    "RENAME",
    "GRANT",
    "REVOKE",
    "DENY",
    "EXEC",
    "EXECUTE",
    "CALL",
    "INTO",
    "BACKUP",
    "RESTORE",
    "RECONFIGURE",
    "SHUTDOWN",
    "CHECKPOINT",
    "DBCC",
    "BULK",
    "KILL",
    "WRITETEXT",
    "UPDATETEXT",
    "SETUSER",
    "DECLARE",
    "SET",
];

/// Functions with side effects.
const WRITE_FUNCTIONS: &[&str] = &[
    // PostgreSQL
    "SET_CONFIG",
    "SETVAL",
    "NEXTVAL",
    "PG_TERMINATE_BACKEND",
    "PG_CANCEL_BACKEND",
    "PG_RELOAD_CONF",
    "PG_ROTATE_LOGFILE",
    "LO_IMPORT",
    "LO_EXPORT",
    "LO_UNLINK",
    "LO_CREATE",
    "DBLINK_EXEC",
    // SQL Server: queries on linked servers and external data
    "OPENQUERY",
    "OPENROWSET",
    "OPENDATASOURCE",
    // SQLite
    "LOAD_EXTENSION",
];

/// A write word anywhere in a statement that starts with a read.
fn embedded_write(toks: &[Token<'_>], d: Dialect) -> Option<String> {
    let lead = upper(&toks[0]).unwrap_or_default();
    for (i, t) in toks.iter().enumerate().skip(1) {
        let Some(w) = upper(t) else { continue };
        let call = toks.get(i + 1).is_some_and(|n| n.tok == Tok::Punct('('));
        let prev = toks[i - 1].word().map(str::to_ascii_uppercase);
        let prev = prev.as_deref();
        if call && WRITE_FUNCTIONS.contains(&w.as_str()) {
            return Some(format!("{}()", w.to_ascii_lowercase()));
        }
        if !EMBEDDED_WRITES.contains(&w.as_str()) {
            continue;
        }
        let write = match w.as_str() {
            // `EXEC('DROP …')` runs dynamic SQL.
            "EXEC" | "EXECUTE" => !safe_procedure(&toks[i + 1..]),
            _ if call => false,
            // `SHOW CREATE TABLE`, `SHOW GRANTS`.
            _ if lead == "SHOW" && i == 1 => false,
            // `SELECT … FOR UPDATE`, `FOR NO KEY UPDATE`: row locks.
            "UPDATE" => !matches!(prev, Some("FOR" | "KEY")),
            "SET" if prev == Some("CHARACTER") => false,
            "SET" => {
                if let Some(kind) = set_write(&toks[i + 1..], d) {
                    return Some(kind);
                }
                false
            }
            "DECLARE" => {
                d != Dialect::SqlServer
                    || !toks
                        .get(i + 1)
                        .and_then(Token::word)
                        .is_some_and(is_variable)
            }
            _ => true,
        };
        if write {
            return Some(if w == "INTO" { "SELECT INTO".into() } else { w });
        }
    }
    None
}

/// Read-only system procedures of SQL Server, also run without `EXEC` at the start of a
/// batch.
const SAFE_PROCEDURES: &[&str] = &[
    "SP_HELP",
    "SP_HELPTEXT",
    "SP_HELPINDEX",
    "SP_HELPCONSTRAINT",
    "SP_HELPDB",
    "SP_HELPFILE",
    "SP_HELPROLE",
    "SP_HELPUSER",
    "SP_COLUMNS",
    "SP_TABLES",
    "SP_STORED_PROCEDURES",
    "SP_PKEYS",
    "SP_FKEYS",
    "SP_DATABASES",
    "SP_WHO",
    "SP_WHO2",
    "SP_LOCK",
    "SP_DEPENDS",
    "SP_DESCRIBE_FIRST_RESULT_SET",
];

/// `sp_help …` or `sys.sp_help …` after `EXEC`.
fn safe_procedure(after: &[Token<'_>]) -> bool {
    let name = match after {
        [schema, dot, name, ..] if schema.is("sys") && dot.tok == Tok::Punct('.') => name,
        [name, ..] => name,
        [] => return false,
    };
    upper(name).is_some_and(|w| SAFE_PROCEDURES.contains(&w.as_str()))
}

/// Session options a read-only console may change. Everything else, `SET ROLE`,
/// `SET GLOBAL`, `SET … READ WRITE` and `@@` system variables included, is a write.
const SAFE_SET: &[&str] = &[
    // SQL Server
    "NOCOUNT",
    "ANSI_NULLS",
    "ANSI_PADDING",
    "ANSI_WARNINGS",
    "ANSI_NULL_DFLT_ON",
    "ANSI_NULL_DFLT_OFF",
    "ANSI_DEFAULTS",
    "ARITHABORT",
    "ARITHIGNORE",
    "CONCAT_NULL_YIELDS_NULL",
    "NUMERIC_ROUNDABORT",
    "QUOTED_IDENTIFIER",
    "XACT_ABORT",
    "DATEFIRST",
    "DATEFORMAT",
    "LANGUAGE",
    "LOCK_TIMEOUT",
    "DEADLOCK_PRIORITY",
    "ROWCOUNT",
    "TEXTSIZE",
    "STATISTICS",
    "SHOWPLAN_ALL",
    "SHOWPLAN_TEXT",
    "SHOWPLAN_XML",
    "NOEXEC",
    "PARSEONLY",
    "FMTONLY",
    "FORCEPLAN",
    // MySQL
    "NAMES",
    "CHARSET",
    "CHARACTER",
    "CHARACTER_SET_CLIENT",
    "CHARACTER_SET_RESULTS",
    "CHARACTER_SET_CONNECTION",
    "COLLATION_CONNECTION",
    "SQL_MODE",
    "TIME_ZONE",
    "AUTOCOMMIT",
    "MAX_EXECUTION_TIME",
    "SQL_SELECT_LIMIT",
    "GROUP_CONCAT_MAX_LEN",
    "NET_READ_TIMEOUT",
    "NET_WRITE_TIMEOUT",
    "WAIT_TIMEOUT",
    "INTERACTIVE_TIMEOUT",
    "LOCK_WAIT_TIMEOUT",
    "INNODB_LOCK_WAIT_TIMEOUT",
    "LC_TIME_NAMES",
    "LC_MESSAGES",
    "PROFILING",
    "OPTIMIZER_SWITCH",
    "SQL_SAFE_UPDATES",
    "SQL_BIG_SELECTS",
    "SQL_NOTES",
    "SQL_WARNINGS",
    "DIV_PRECISION_INCREMENT",
    "CTE_MAX_RECURSION_DEPTH",
    "MAX_JOIN_SIZE",
    "SORT_BUFFER_SIZE",
    "JOIN_BUFFER_SIZE",
    "TMP_TABLE_SIZE",
    "MAX_HEAP_TABLE_SIZE",
    "TRANSACTION_ISOLATION",
    "TX_ISOLATION",
    "EXPLAIN_FORMAT",
    // PostgreSQL
    "SEARCH_PATH",
    "SCHEMA",
    "STATEMENT_TIMEOUT",
    "IDLE_IN_TRANSACTION_SESSION_TIMEOUT",
    "TIMEZONE",
    "TIME",
    "DATESTYLE",
    "INTERVALSTYLE",
    "CLIENT_ENCODING",
    "CLIENT_MIN_MESSAGES",
    "APPLICATION_NAME",
    "EXTRA_FLOAT_DIGITS",
    "BYTEA_OUTPUT",
    "WORK_MEM",
    "TEMP_BUFFERS",
    "RANDOM_PAGE_COST",
    "SEQ_PAGE_COST",
    "EFFECTIVE_CACHE_SIZE",
    "JIT",
    "ENABLE_SEQSCAN",
    "ENABLE_INDEXSCAN",
    "ENABLE_BITMAPSCAN",
    "ENABLE_HASHJOIN",
    "ENABLE_MERGEJOIN",
    "ENABLE_NESTLOOP",
    "ENABLE_SORT",
    "ENABLE_HASHAGG",
    "MAX_PARALLEL_WORKERS_PER_GATHER",
    "PLAN_CACHE_MODE",
    "CONSTRAINTS",
    // SQL Server, MySQL, PostgreSQL, Oracle: `SET TRANSACTION ISOLATION LEVEL …`,
    // `SET TRANSACTION READ ONLY` (`READ WRITE` is refused separately).
    "TRANSACTION",
    // ClickHouse
    "MAX_THREADS",
    "MAX_MEMORY_USAGE",
    "MAX_RESULT_ROWS",
    "OUTPUT_FORMAT_JSON_QUOTE_64BIT_INTEGERS",
];

/// The write of `SET <after>`, or `None` for a safe session option.
fn set_write(after: &[Token<'_>], d: Dialect) -> Option<String> {
    let blocked = |what: &str| Some(format!("SET {what}"));
    if after
        .iter()
        .any(|t| t.word().is_some_and(|w| w.starts_with("@@")))
    {
        return blocked("@@");
    }
    if after.iter().any(|t| t.is("WRITE")) {
        return Some("READ WRITE".into());
    }
    // Targets: the first one and, in MySQL and ClickHouse, every one after a top-level `,`
    // (`SET a = 1, b = 2`).
    let mut targets = vec![0];
    if matches!(d, Dialect::MySql | Dialect::ClickHouse) {
        let depth = after.first().map_or(0, |t| t.depth);
        targets.extend(
            after
                .iter()
                .enumerate()
                .filter(|(_, t)| t.tok == Tok::Punct(',') && t.depth == depth)
                .map(|(i, _)| i + 1),
        );
    } else if d == Dialect::SqlServer {
        // `SET ANSI_NULLS, QUOTED_IDENTIFIER ON`.
        let mut i = 1;
        while after.get(i).is_some_and(|t| t.tok == Tok::Punct(','))
            && after.get(i + 1).is_some_and(|t| t.word().is_some())
        {
            targets.push(i + 1);
            i += 2;
        }
    }
    for at in targets {
        let mut j = at;
        // `SET SESSION x`, `SET LOCAL x`; `SET SESSION CHARACTERISTICS AS TRANSACTION …`.
        if after
            .get(j)
            .is_some_and(|t| t.is("SESSION") || t.is("LOCAL"))
        {
            j += 1;
            if after.get(j).is_some_and(|t| t.is("CHARACTERISTICS")) {
                continue;
            }
        }
        let Some(target) = after.get(j) else {
            return blocked("");
        };
        let Some(name) = upper(target) else {
            return blocked("");
        };
        if is_variable(&name) && matches!(d, Dialect::MySql | Dialect::SqlServer) {
            continue;
        }
        if !SAFE_SET.contains(&name.as_str()) {
            return blocked(&name);
        }
    }
    None
}

/// PRAGMAs that only read: queried without a value, or the introspection ones with a
/// table or index name.
fn pragma_reads(after: &[Token<'_>]) -> bool {
    // `PRAGMA main.table_info(t)`.
    let after = match after {
        [_, dot, rest @ ..] if dot.tok == Tok::Punct('.') => rest,
        _ => after,
    };
    let Some(name) = after.first().and_then(upper) else {
        return false;
    };
    match after.get(1).map(|t| t.tok) {
        None => PRAGMA_QUERIES.contains(&name.as_str()),
        Some(Tok::Punct('(')) => PRAGMA_INTROSPECTION.contains(&name.as_str()),
        Some(_) => false,
    }
}

const PRAGMA_INTROSPECTION: &[&str] = &[
    "TABLE_INFO",
    "TABLE_XINFO",
    "TABLE_LIST",
    "INDEX_LIST",
    "INDEX_INFO",
    "INDEX_XINFO",
    "FOREIGN_KEY_LIST",
    "FOREIGN_KEY_CHECK",
    "INTEGRITY_CHECK",
    "QUICK_CHECK",
];

const PRAGMA_QUERIES: &[&str] = &[
    "TABLE_INFO",
    "TABLE_LIST",
    "INDEX_LIST",
    "FOREIGN_KEY_LIST",
    "FOREIGN_KEY_CHECK",
    "INTEGRITY_CHECK",
    "QUICK_CHECK",
    "DATABASE_LIST",
    "COLLATION_LIST",
    "COMPILE_OPTIONS",
    "FUNCTION_LIST",
    "MODULE_LIST",
    "PRAGMA_LIST",
    "USER_VERSION",
    "SCHEMA_VERSION",
    "APPLICATION_ID",
    "JOURNAL_MODE",
    "PAGE_SIZE",
    "PAGE_COUNT",
    "FREELIST_COUNT",
    "ENCODING",
    "FOREIGN_KEYS",
    "AUTO_VACUUM",
    "CACHE_SIZE",
    "SYNCHRONOUS",
    "DATA_VERSION",
    "MAX_PAGE_COUNT",
    "BUSY_TIMEOUT",
    "RECURSIVE_TRIGGERS",
    "TEMP_STORE",
    "SECURE_DELETE",
    "QUERY_ONLY",
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guard::{guard, GuardPolicy};

    const SQL_DIALECTS: [Dialect; 6] = [
        Dialect::MySql,
        Dialect::Postgres,
        Dialect::Sqlite,
        Dialect::SqlServer,
        Dialect::ClickHouse,
        Dialect::Oracle,
    ];

    fn ro() -> GuardPolicy {
        GuardPolicy {
            read_only: true,
            production: false,
            name: "shop".into(),
        }
    }

    fn blocked(sql: &str, d: Dialect) -> bool {
        guard(sql, d, &ro()).blocked.is_some()
    }

    #[track_caller]
    fn assert_blocked(sql: &str, d: Dialect) {
        assert!(
            blocked(sql, d),
            "{d:?} let a write pass as read-only: {sql}"
        );
    }

    #[track_caller]
    fn assert_read(sql: &str, d: Dialect) {
        let g = guard(sql, d, &ro());
        assert!(g.blocked.is_none(), "{d:?} refused a read: {sql}: {g:?}");
    }

    #[test]
    fn sql_server_statements_without_semicolons() {
        let d = Dialect::SqlServer;
        assert_blocked("SELECT 1\nDROP TABLE users", d);
        assert_blocked("SELECT 1 DROP TABLE users", d);
        assert_blocked("select * from t\ndelete from users", d);
        assert_blocked("SELECT 1\nEXEC('DROP TABLE users')", d);
        assert_blocked("SELECT 1\nEXEC sp_rename 'a', 'b'", d);
        assert_blocked("SET NOCOUNT ON\nUPDATE users SET a = 1", d);
        assert_blocked("DECLARE @x int\nTRUNCATE TABLE users", d);
        assert_blocked("SELECT 1 IF 1 = 1 BEGIN DROP TABLE t END", d);
        assert_blocked("SELECT * FROM OPENQUERY(srv, 'SELECT 1')", d);
        assert_blocked("sp_rename 'a', 'b'", d);
        assert_blocked("SELECT 1\nSET ROLE admin", d);
        // A bracket identifier with an escaped `]` hides nothing.
        assert_blocked("SELECT [a]];] FROM t\nDROP TABLE t", d);
        assert_blocked("SELECT 1\nGO\nDROP TABLE t", d);
        assert_read("SET NOCOUNT ON\nSELECT * FROM [delete]", d);
        assert_read("DECLARE @n int = 5\nSELECT TOP (@n) * FROM t", d);
        assert_read("SELECT 1\nGO\nSELECT 2", d);
        assert_read("EXEC sp_help 'users'", d);
        assert_read("sp_columns users", d);
        assert_read("SELECT * FROM t WITH (NOLOCK) WHERE a = N'it''s'", d);
        assert_read("BEGIN TRAN\nSELECT 1\nCOMMIT", d);
        // The production check sees the DELETE without WHERE after the SELECT.
        let prod = GuardPolicy {
            production: true,
            ..GuardPolicy::default()
        };
        let g = guard("SELECT 1\nDELETE FROM users", d, &prod);
        assert!(g
            .confirm
            .is_some_and(|c| c.contains("DELETE without WHERE deletes every row of users")),);
        assert!(
            guard("SELECT * FROM t FOR UPDATE", Dialect::Postgres, &prod)
                .confirm
                .is_none()
        );
    }

    #[test]
    fn set_and_admin_statements_are_writes() {
        use Dialect::*;
        for (sql, d) in [
            (
                "SET SESSION CHARACTERISTICS AS TRANSACTION READ WRITE",
                Postgres,
            ),
            ("SET transaction_read_only = off", Postgres),
            ("SET default_transaction_read_only = off", Postgres),
            ("SET ROLE admin", Postgres),
            ("SET SESSION AUTHORIZATION admin", Postgres),
            ("RESET ALL", Postgres),
            ("DISCARD ALL", Postgres),
            ("REFRESH MATERIALIZED VIEW v", Postgres),
            ("ANALYZE users", Postgres),
            ("BEGIN READ WRITE", Postgres),
            (
                "SELECT set_config('default_transaction_read_only', 'off', false)",
                Postgres,
            ),
            ("SELECT nextval('s')", Postgres),
            ("SET GLOBAL max_connections = 1", MySql),
            ("SET @@global.read_only = 0", MySql),
            ("SET SESSION TRANSACTION READ WRITE", MySql),
            ("START TRANSACTION READ WRITE", MySql),
            ("SET names utf8, @@session.transaction_read_only = 0", MySql),
            ("SET sql_mode = '', transaction_read_only = 0", MySql),
            ("FLUSH TABLES", MySql),
            ("RESET MASTER", MySql),
            ("SHUTDOWN", MySql),
            ("SET PASSWORD = 'x'", MySql),
            ("BACKUP DATABASE shop TO DISK = 'x'", SqlServer),
            ("RESTORE DATABASE shop FROM DISK = 'x'", SqlServer),
            ("DENY SELECT ON users TO bob", SqlServer),
            ("RECONFIGURE", SqlServer),
            ("SET readonly = 0", ClickHouse),
            ("PRAGMA journal_mode(WAL)", Sqlite),
            ("PRAGMA user_version = 3", Sqlite),
            ("PRAGMA optimize", Sqlite),
            ("SET ROLE dba", Oracle),
            ("EXPLAIN PLAN FOR SELECT 1 FROM dual", Oracle),
        ] {
            assert_blocked(sql, d);
        }
        for (sql, d) in [
            ("SET search_path TO app, public", Postgres),
            ("SET LOCAL statement_timeout = '5s'", Postgres),
            ("SET TIME ZONE 'UTC'", Postgres),
            (
                "SET TRANSACTION ISOLATION LEVEL SERIALIZABLE READ ONLY",
                Postgres,
            ),
            ("SHOW search_path", Postgres),
            ("SET NAMES utf8mb4", MySql),
            ("SET @a = 1, @b = 2", MySql),
            ("SET SESSION sql_mode = 'ANSI'", MySql),
            ("SHOW CREATE TABLE users", MySql),
            ("SET STATISTICS IO ON", SqlServer),
            ("SET ANSI_NULLS, QUOTED_IDENTIFIER ON", SqlServer),
            (
                "SET TRANSACTION ISOLATION LEVEL READ UNCOMMITTED",
                SqlServer,
            ),
            ("PRAGMA table_info(users)", Sqlite),
            ("PRAGMA main.index_list(users)", Sqlite),
            ("PRAGMA user_version", Sqlite),
            ("SET TRANSACTION READ ONLY", Oracle),
        ] {
            assert_read(sql, d);
        }
    }

    #[test]
    fn allowlist_refuses_unknown_statements() {
        for d in SQL_DIALECTS {
            for sql in [
                "COPY users TO '/tmp/x'",
                "LISTEN x",
                "frobnicate the database",
                "'just a string'",
                "SELECT 'unclosed",
                "SELECT 1 /* unclosed",
            ] {
                assert_blocked(sql, d);
            }
            assert_read("select a, b from t where c = 'delete' -- drop\n", d);
            assert_read("(select 1)", d);
            assert_read("WITH x AS (SELECT 1) SELECT * FROM x", d);
        }
        assert_read("select replace(name, 'a', 'b') from t", Dialect::MySql);
        assert_read("explain analyze select * from t", Dialect::Postgres);
        assert_blocked("explain analyze delete from t", Dialect::Postgres);
        assert_blocked("select * into new_t from t", Dialect::Postgres);
        assert_blocked("select * from t into outfile '/tmp/x'", Dialect::MySql);
    }

    #[test]
    fn dialect_literals_cannot_hide_writes() {
        use Dialect::*;
        for (sql, d) in [
            // PostgreSQL `E''` strings: `\'` is an escaped quote.
            (r"SELECT E'\'' ; DELETE FROM users; --'", Postgres),
            (r"SELECT e'\\\'' ; DROP TABLE users --'", Postgres),
            // Nested comments in PostgreSQL and SQL Server.
            ("SELECT 1 /* /* */ ' */ ; DROP TABLE users; -- '", Postgres),
            ("SELECT 1 /* /* */ ' */ DROP TABLE users -- '", SqlServer),
            // Dollar quotes.
            ("SELECT $a$ ' $a$; DROP TABLE users; -- '", Postgres),
            // `standard_conforming_strings = off`.
            (r"SELECT 'a\'; DROP TABLE users; --'", Postgres),
            // MySQL: backslash escapes, `NO_BACKSLASH_ESCAPES`, `ANSI_QUOTES`.
            (r"SELECT 'a\'; DROP TABLE users; -- '", MySql),
            (r"SELECT 'a\\'; DROP TABLE users; -- '", MySql),
            (r#"SELECT "a\"; DROP TABLE users; -- ""#, MySql),
            // MySQL: `--` needs a space; `/*! */` is code.
            ("SELECT 1 --1; DROP TABLE users", MySql),
            ("SELECT 1 /*!50000 ; DROP TABLE users */", MySql),
            ("SELECT 1 /*M!100100 ; DROP TABLE users */", MySql),
            // Oracle `q''` strings.
            ("SELECT q'[ ' ]' FROM dual; DROP TABLE users", Oracle),
        ] {
            assert_blocked(sql, d);
        }
        for (sql, d) in [
            (r"SELECT E'it\'s; DROP TABLE users'", Postgres),
            ("SELECT $x$; DROP TABLE users $x$", Postgres),
            ("SELECT 1 /* /* ; DROP TABLE users */ */", Postgres),
            ("SELECT N'; DROP TABLE users', [x; drop]", SqlServer),
            ("SELECT q'{ ; DROP TABLE users }' FROM dual", Oracle),
            ("SELECT 1 /* ; DROP TABLE users */", MySql),
        ] {
            assert_read(sql, d);
        }
    }

    /// A tiny deterministic pseudo-random generator (xorshift), so the test needs no crate.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> usize {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0 as usize
        }

        fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
            items[self.next() % items.len()]
        }
    }

    const WRITES: &[&str] = &[
        "DELETE FROM users",
        "delete from users where id = 1",
        "UPDATE users SET admin = 1",
        "INSERT INTO users (id) VALUES (1)",
        "DROP TABLE users",
        "TRUNCATE TABLE users",
        "ALTER TABLE users ADD x INT",
        "CREATE TABLE x (id INT)",
        "GRANT ALL ON users TO bob",
        "MERGE INTO users u USING t ON (u.id = t.id) WHEN MATCHED THEN DELETE",
    ];

    /// Prefixes that end on something a naive scanner reads as an open string or comment,
    /// while the dialect has already closed it.
    fn hiding_prefixes(d: Dialect) -> Vec<&'static str> {
        let mut out = vec!["SELECT 1;", "SELECT 'x'' ;' AS a;", "SELECT 1 /* x */;"];
        match d {
            Dialect::Postgres => out.extend([
                r"SELECT E'\'';",
                r"SELECT E'\\';",
                "SELECT $q$ ' $q$;",
                "SELECT 1 /* /* */ ' */;",
                r"SELECT 'a\';",
            ]),
            Dialect::MySql => out.extend([
                r"SELECT 'a\';",
                r"SELECT 'a\\';",
                r#"SELECT "a\";"#,
                "SELECT 1 --1;",
                "SELECT 1 /*!",
            ]),
            Dialect::SqlServer => out.extend([
                "SELECT 1\n",
                "SELECT [a]];]\n",
                "SELECT 1 /* /* */ ' */\n",
                "SET NOCOUNT ON\n",
                "DECLARE @x int\n",
            ]),
            Dialect::Oracle => out.extend(["SELECT q'[ ' ]' FROM dual;"]),
            Dialect::Sqlite => out.extend(["SELECT [a;b];", "SELECT `a'`;"]),
            _ => {}
        }
        out
    }

    /// Random case, comments and whitespace between the words of a statement.
    fn noisy(rng: &mut Rng, sql: &str, d: Dialect) -> String {
        let gaps: &[&str] = if d == Dialect::MySql {
            &[" ", "\n", "\t", " /* x */ ", " -- note\n", " # x\n", "/**/"]
        } else {
            &[" ", "\n", "\t", " /* x */ ", " -- note\n", "/**/"]
        };
        sql.split(' ')
            .map(|w| {
                if rng.next().is_multiple_of(2) {
                    w.to_ascii_lowercase()
                } else {
                    w.to_ascii_uppercase()
                }
            })
            .collect::<Vec<_>>()
            .join(rng.pick(gaps))
    }

    #[test]
    fn random_writes_never_pass_as_read_only() {
        let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
        for d in SQL_DIALECTS {
            let prefixes = hiding_prefixes(d);
            for _ in 0..400 {
                let plain = rng.pick(WRITES);
                let write = noisy(&mut rng, plain, d);
                let prefix = rng.pick(&prefixes);
                let suffix = rng.pick(&["", ";", " -- '", " /* ' */", "\n-- \"", " ;"]);
                let sql = if prefix.ends_with("/*!") {
                    format!("{prefix} ; {write} */")
                } else {
                    format!("{prefix} {write}{suffix}")
                };
                assert_blocked(&sql, d);
            }
        }
    }

    #[test]
    fn random_writes_inside_literals_and_comments_stay_reads() {
        let mut rng = Rng(0x1234_5678_9abc_def1);
        for d in SQL_DIALECTS {
            for _ in 0..200 {
                let write = rng.pick(WRITES).replace('\'', "''");
                let sql = match rng.next() % 3 {
                    0 => format!("SELECT '{write}; ' AS a FROM t"),
                    1 => format!("SELECT a FROM t /* {write}; */ WHERE b = 1"),
                    _ => format!("SELECT a -- {write};\nFROM t"),
                };
                assert_read(&sql, d);
            }
        }
    }
}
