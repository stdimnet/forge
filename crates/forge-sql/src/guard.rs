//! Safety checks before a console runs a script: read-only data sources refuse writes
//! (DML/DDL, Redis write commands, MongoDB write calls), and production data sources ask
//! before changes without a filter (`UPDATE`/`DELETE` without `WHERE`, `TRUNCATE`,
//! `FLUSHALL`, `deleteMany({})`).

use serde::{Deserialize, Serialize};

use crate::classify;
use crate::dialect::Dialect;
use crate::lex::{tokenize_default, Tok};
use crate::split::{leading_keyword, split_statements};

/// What the data source allows.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuardPolicy {
    #[serde(default)]
    pub read_only: bool,
    /// The data source is marked as production (red colour).
    #[serde(default)]
    pub production: bool,
    /// Data source name for the messages.
    #[serde(default)]
    pub name: String,
}

/// The verdict for a script.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Guard {
    /// Why the script is refused (a write on a read-only data source).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked: Option<String>,
    /// What the user must confirm before it runs (an unfiltered change on production).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirm: Option<String>,
}

/// Words and quoted identifiers (without their quotes) of a statement outside strings and
/// comments, upper-cased, with their parenthesis depth.
fn words(stmt: &str, d: Dialect) -> Vec<(String, i32)> {
    tokenize_default(stmt, d)
        .tokens
        .iter()
        .filter_map(|t| match t.tok {
            Tok::Word(w) => Some((w.to_ascii_uppercase(), t.depth)),
            Tok::Ident if t.end >= t.start + 2 => {
                Some((stmt[t.start + 1..t.end - 1].to_ascii_uppercase(), t.depth))
            }
            _ => None,
        })
        .collect()
}

/// `UPDATE`/`DELETE` without a top-level `WHERE`, or `TRUNCATE`: what it affects. A T-SQL
/// batch may hold several statements without `;` (`SELECT 1 DELETE FROM t`), so each
/// top-level `SELECT`, `INSERT`, `UPDATE`, `DELETE`, `TRUNCATE` or `MERGE` starts a part
/// that is checked on its own.
fn sql_unbounded(stmt: &str, d: Dialect) -> Option<String> {
    let ws = words(stmt, d);
    let ddl = matches!(
        leading_keyword(stmt).as_str(),
        "CREATE" | "ALTER" | "GRANT" | "REVOKE" | "DENY"
    );
    let mut starts = vec![0];
    if !ddl {
        for i in 1..ws.len() {
            let (w, depth) = &ws[i];
            let starter = matches!(
                w.as_str(),
                "SELECT" | "INSERT" | "UPDATE" | "DELETE" | "TRUNCATE" | "MERGE"
            );
            // `FOR UPDATE`, `ON DUPLICATE KEY UPDATE`, `DO UPDATE`, `THEN DELETE`, ….
            let clause = matches!(
                ws[i - 1].0.as_str(),
                "FOR" | "KEY" | "DO" | "THEN" | "ON" | "OF" | "OR" | "AFTER" | "BEFORE" | "INSTEAD"
            );
            if starter && *depth == 0 && !clause {
                starts.push(i);
            }
        }
    }
    starts.push(ws.len());
    let found: Vec<String> = starts
        .windows(2)
        .filter_map(|r| unbounded_part(&ws[r[0]..r[1]]))
        .collect();
    (!found.is_empty()).then(|| found.join("; "))
}

fn unbounded_part(ws: &[(String, i32)]) -> Option<String> {
    let kw = ws.first().map(|(w, _)| w.as_str()).unwrap_or("");
    let has_where = ws.iter().any(|(w, d)| w == "WHERE" && *d == 0);
    let after = |key: &str| {
        ws.iter()
            .position(|(w, d)| w == key && *d == 0)
            .and_then(|i| ws.get(i + 1))
            .map(|(w, _)| w.to_ascii_lowercase())
            .unwrap_or_default()
    };
    match kw {
        "DELETE" if !has_where => Some(format!(
            "DELETE without WHERE deletes every row of {}",
            or_table(after("FROM"))
        )),
        "UPDATE" if !has_where => Some(format!(
            "UPDATE without WHERE changes every row of {}",
            or_table(after("UPDATE"))
        )),
        "TRUNCATE" => Some(format!(
            "TRUNCATE deletes every row of {}",
            or_table(
                ws.iter()
                    .map(|(w, _)| w.as_str())
                    .find(|w| *w != "TRUNCATE" && *w != "TABLE")
                    .unwrap_or("")
                    .to_ascii_lowercase()
            )
        )),
        _ => None,
    }
}

fn or_table(name: String) -> String {
    if name.is_empty() {
        "the table".into()
    } else {
        name
    }
}

/// Redis commands that only read.
const REDIS_READS: &[&str] = &[
    "GET",
    "MGET",
    "STRLEN",
    "GETRANGE",
    "SUBSTR",
    "EXISTS",
    "TYPE",
    "TTL",
    "PTTL",
    "EXPIRETIME",
    "PEXPIRETIME",
    "KEYS",
    "SCAN",
    "HGET",
    "HMGET",
    "HGETALL",
    "HKEYS",
    "HVALS",
    "HLEN",
    "HEXISTS",
    "HSCAN",
    "HSTRLEN",
    "HRANDFIELD",
    "LRANGE",
    "LLEN",
    "LINDEX",
    "LPOS",
    "SMEMBERS",
    "SISMEMBER",
    "SMISMEMBER",
    "SCARD",
    "SSCAN",
    "SRANDMEMBER",
    "SINTER",
    "SINTERCARD",
    "SUNION",
    "SDIFF",
    "ZRANGE",
    "ZRANGEBYSCORE",
    "ZREVRANGE",
    "ZREVRANGEBYSCORE",
    "ZRANGEBYLEX",
    "ZREVRANGEBYLEX",
    "ZCARD",
    "ZSCORE",
    "ZMSCORE",
    "ZRANK",
    "ZREVRANK",
    "ZCOUNT",
    "ZLEXCOUNT",
    "ZSCAN",
    "ZRANDMEMBER",
    "XRANGE",
    "XREVRANGE",
    "XLEN",
    "XINFO",
    "XREAD",
    "XPENDING",
    "INFO",
    "DBSIZE",
    "PING",
    "ECHO",
    "TIME",
    "SELECT",
    "OBJECT",
    "DUMP",
    "RANDOMKEY",
    "BITCOUNT",
    "BITPOS",
    "GETBIT",
    "PFCOUNT",
    "GEOPOS",
    "GEODIST",
    "GEOHASH",
    "GEOSEARCH",
    "GEORADIUS_RO",
    "GEORADIUSBYMEMBER_RO",
    "LASTSAVE",
    "ROLE",
    "COMMAND",
    "TOUCH",
    "LCS",
    "SORT_RO",
    "EVAL_RO",
    "EVALSHA_RO",
    "FCALL_RO",
    "HELLO",
    "AUTH",
    "QUIT",
    "RESET",
    "READONLY",
    "WAIT",
];

/// `CONFIG GET`, `CLIENT LIST`, …: read-only subcommands of admin commands.
const REDIS_READ_SUBCOMMANDS: &[(&str, &[&str])] = &[
    ("CONFIG", &["GET"]),
    ("CLIENT", &["LIST", "INFO", "GETNAME", "ID"]),
    ("SLOWLOG", &["GET", "LEN"]),
    ("MEMORY", &["USAGE", "STATS", "DOCTOR"]),
    ("CLUSTER", &["INFO", "NODES", "SLOTS", "SHARDS"]),
    ("ACL", &["WHOAMI", "LIST", "USERS", "CAT"]),
];

/// The name of a Redis command line (`set a 1` → `SET`) and its first argument.
fn redis_words(line: &str) -> (String, String) {
    let mut it = line.split_whitespace();
    let cmd = it.next().unwrap_or("").to_ascii_uppercase();
    let sub = it.next().unwrap_or("").to_ascii_uppercase();
    (cmd, sub)
}

/// Whether a Redis command may change data.
pub fn redis_is_write(line: &str) -> bool {
    let (cmd, sub) = redis_words(line);
    if cmd.is_empty() || REDIS_READS.contains(&cmd.as_str()) {
        return false;
    }
    !REDIS_READ_SUBCOMMANDS
        .iter()
        .any(|(c, subs)| *c == cmd && subs.contains(&sub.as_str()))
}

/// A parsed MongoDB shell call: `db.users.find({...})` → collection `users`, method `find`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MongoCall<'a> {
    /// Empty for database methods (`db.getCollectionNames()`).
    pub collection: &'a str,
    pub method: &'a str,
    /// The argument text between the outer parentheses.
    pub args: &'a str,
}

/// Parse `db.<collection>.<method>(<args>)` (also `db.getCollection("x").find()` and
/// `db["x"].find()`); chained calls after the first are left in `args`' tail.
pub fn mongo_call(text: &str) -> Option<MongoCall<'_>> {
    let rest = text.trim().strip_prefix("db")?;
    let (collection, rest) = if let Some(r) = rest.strip_prefix(".getCollection(") {
        let (name, r) = r.split_once(')')?;
        (name.trim().trim_matches(['"', '\'']), r.strip_prefix('.')?)
    } else if let Some(r) = rest.strip_prefix('[') {
        let (name, r) = r.split_once(']')?;
        (name.trim().trim_matches(['"', '\'']), r.strip_prefix('.')?)
    } else {
        let r = rest.strip_prefix('.')?;
        let open = r.find('(')?;
        match r[..open].rsplit_once('.') {
            Some((coll, _)) => (coll, &r[coll.len() + 1..]),
            None => ("", r),
        }
    };
    let open = rest.find('(')?;
    let method = rest[..open].trim();
    let close = matching_paren(rest, open)?;
    Some(MongoCall {
        collection,
        method,
        args: &rest[open + 1..close],
    })
}

/// Index of the `)` matching the `(` at `open`, skipping strings.
pub fn matching_paren(text: &str, open: usize) -> Option<usize> {
    let b = text.as_bytes();
    let mut depth = 0;
    let mut i = open;
    while i < b.len() {
        match b[i] {
            q @ (b'\'' | b'"') => {
                i += 1;
                while i < b.len() && b[i] != q {
                    if b[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
            }
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

const MONGO_READS: &[&str] = &[
    "find",
    "findOne",
    "countDocuments",
    "estimatedDocumentCount",
    "count",
    "distinct",
    "getIndexes",
    "stats",
    "explain",
    "getCollectionNames",
    "getCollectionInfos",
    "getName",
    "listCollections",
    "runCommand",
    "serverStatus",
    "version",
];

/// Whether a MongoDB console line may change data.
pub fn mongo_is_write(text: &str) -> bool {
    let t = text.trim();
    if t.starts_with("show ") || t.starts_with("use ") || t == "db" {
        return false;
    }
    match mongo_call(t) {
        Some(call) if call.method == "aggregate" => {
            call.args.contains("$out") || call.args.contains("$merge")
        }
        Some(call) if call.method == "runCommand" => !call.args.contains("ping"),
        Some(call) => !MONGO_READS.contains(&call.method),
        None => true,
    }
}

/// Empty filter (`{}` or no argument) of a MongoDB call.
fn mongo_empty_filter(args: &str) -> bool {
    let first = args.trim();
    first.is_empty() || {
        let compact: String = first.chars().filter(|c| !c.is_whitespace()).collect();
        compact == "{}" || compact.starts_with("{},")
    }
}

/// A statement that changes data without a filter, described for the confirmation.
pub fn unbounded_change(stmt: &str, dialect: Dialect) -> Option<String> {
    match dialect {
        Dialect::Redis => match redis_words(stmt).0.as_str() {
            "FLUSHALL" => Some("FLUSHALL deletes every key of every database".into()),
            "FLUSHDB" => Some("FLUSHDB deletes every key of the database".into()),
            _ => None,
        },
        Dialect::Mongo => {
            let call = mongo_call(stmt)?;
            match call.method {
                "deleteMany" | "remove" if mongo_empty_filter(call.args) => Some(format!(
                    "{}() without a filter deletes every document of {}",
                    call.method, call.collection
                )),
                "updateMany" if mongo_empty_filter(call.args) => Some(format!(
                    "updateMany() without a filter changes every document of {}",
                    call.collection
                )),
                "drop" => Some(format!("drop() removes the collection {}", call.collection)),
                "dropDatabase" => Some("dropDatabase() removes the database".into()),
                _ => None,
            }
        }
        _ => sql_unbounded(stmt, dialect),
    }
}

/// Whether one statement changes data or schema; returns its kind (`DELETE`, `SET`, …).
pub fn write_kind(stmt: &str, dialect: Dialect) -> Option<String> {
    match dialect {
        Dialect::Redis => redis_is_write(stmt).then(|| redis_words(stmt).0),
        Dialect::Mongo => mongo_is_write(stmt).then(|| {
            mongo_call(stmt)
                .map(|c| format!("{}()", c.method))
                .unwrap_or_else(|| "this command".into())
        }),
        _ => classify::write_kind(stmt, dialect),
    }
}

/// Check a script before it runs.
pub fn guard(script: &str, dialect: Dialect, policy: &GuardPolicy) -> Guard {
    let name = if policy.name.is_empty() {
        "This data source".to_string()
    } else {
        policy.name.clone()
    };
    let mut out = Guard::default();
    let mut confirms = Vec::new();
    for st in split_statements(script, dialect) {
        if policy.read_only {
            if let Some(kind) = write_kind(&st.text, dialect) {
                out.blocked = Some(format!(
                    "{name} is read-only: {kind} is not allowed. Turn off Read-only in the data source properties to change data."
                ));
                return out;
            }
        }
        if policy.production {
            if let Some(what) = unbounded_change(&st.text, dialect) {
                confirms.push(what);
            }
        }
    }
    if !confirms.is_empty() {
        out.confirm = Some(format!(
            "{name} is a production data source. {}. Run it anyway?",
            confirms.join("; ")
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ro() -> GuardPolicy {
        GuardPolicy {
            read_only: true,
            production: false,
            name: "shop".into(),
        }
    }

    fn prod() -> GuardPolicy {
        GuardPolicy {
            read_only: false,
            production: true,
            name: "shop-prod".into(),
        }
    }

    #[test]
    fn read_only_blocks_writes_but_not_reads() {
        assert_eq!(
            guard("select * from users; show tables", Dialect::MySql, &ro()),
            Guard::default()
        );
        let g = guard(
            "select 1; delete from users where id = 1",
            Dialect::MySql,
            &ro(),
        );
        assert!(g.blocked.unwrap().contains("shop is read-only: DELETE"));
        assert!(guard("create table x (id int)", Dialect::Postgres, &ro())
            .blocked
            .is_some());
        assert!(guard(
            "with d as (delete from t returning *) select * from d",
            Dialect::Postgres,
            &ro()
        )
        .blocked
        .is_some());
        assert!(
            guard("select * into backup from users", Dialect::SqlServer, &ro())
                .blocked
                .is_some()
        );
        assert!(guard("select 'delete' as x", Dialect::MySql, &ro())
            .blocked
            .is_none());
        assert!(guard("PRAGMA table_info(users)", Dialect::Sqlite, &ro())
            .blocked
            .is_none());
        assert!(guard("begin\n null;\nend;\n/", Dialect::Oracle, &ro())
            .blocked
            .is_some());
        assert!(guard("BEGIN TRANSACTION", Dialect::SqlServer, &ro())
            .blocked
            .is_none());
    }

    #[test]
    fn production_confirms_unfiltered_changes() {
        let g = guard("UPDATE users SET active = 0", Dialect::MySql, &prod());
        let msg = g.confirm.unwrap();
        assert!(
            msg.contains("shop-prod is a production data source"),
            "{msg}"
        );
        assert!(
            msg.contains("UPDATE without WHERE changes every row of users"),
            "{msg}"
        );
        assert!(guard(
            "delete from orders where id in (select id from x)",
            Dialect::MySql,
            &prod()
        )
        .confirm
        .is_none());
        assert!(guard(
            "update t set a = (select b from u where u.id = 1)",
            Dialect::MySql,
            &prod()
        )
        .confirm
        .is_some());
        assert!(
            guard("DELETE FROM [dbo].[logs]", Dialect::SqlServer, &prod())
                .confirm
                .unwrap()
                .contains("dbo")
        );
        assert!(guard("truncate table sessions", Dialect::Postgres, &prod())
            .confirm
            .unwrap()
            .contains("sessions"));
        assert!(
            guard("delete from users", Dialect::MySql, &GuardPolicy::default())
                .confirm
                .is_none()
        );
    }

    #[test]
    fn redis_commands() {
        assert!(!redis_is_write("GET user:1"));
        assert!(!redis_is_write("config get maxmemory"));
        assert!(redis_is_write("CONFIG SET maxmemory 1"));
        assert!(redis_is_write("set a 1"));
        assert!(guard("HGETALL user:1\nDEL user:1", Dialect::Redis, &ro())
            .blocked
            .unwrap()
            .contains("DEL"));
        assert!(guard("FLUSHDB", Dialect::Redis, &prod()).confirm.is_some());
    }

    #[test]
    fn mongo_calls() {
        let c = mongo_call("db.users.find({ age: { $gt: 30 } }).limit(5)").unwrap();
        assert_eq!((c.collection, c.method), ("users", "find"));
        assert_eq!(c.args, "{ age: { $gt: 30 } }");
        let c = mongo_call("db.getCollection('order items').aggregate([])").unwrap();
        assert_eq!((c.collection, c.method), ("order items", "aggregate"));
        let c = mongo_call("db.getCollectionNames()").unwrap();
        assert_eq!((c.collection, c.method), ("", "getCollectionNames"));
        assert!(!mongo_is_write("db.users.aggregate([{ $match: {} }])"));
        assert!(mongo_is_write("db.users.aggregate([{ $out: 'x' }])"));
        assert!(mongo_is_write("db.users.insertOne({a: 1})"));
        assert!(!mongo_is_write("show collections"));
        assert!(guard("db.users.deleteMany({})", Dialect::Mongo, &prod())
            .confirm
            .unwrap()
            .contains("users"));
        assert!(
            guard("db.users.deleteMany({ a: 1 })", Dialect::Mongo, &prod())
                .confirm
                .is_none()
        );
        assert!(guard(
            "db.users.updateMany({}, {$set: {a: 1}})",
            Dialect::Mongo,
            &prod()
        )
        .confirm
        .is_some());
    }
}
