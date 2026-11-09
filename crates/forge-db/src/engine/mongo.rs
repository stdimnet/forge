//! MongoDB with the official driver (Apache-2.0, rustls): databases and collections (with
//! fields sampled from their documents) in the tree, documents as JSON, and a console that
//! understands the shell calls people type: `db.users.find({ age: { $gt: 30 } }).limit(5)`,
//! `aggregate([...])`, `countDocuments`, `distinct`, `insertOne`, `updateMany`, `deleteMany`,
//! `getIndexes`, `show dbs`, `show collections`, `use shop`. Arguments are relaxed JSON (unquoted
//! keys, single quotes, `ObjectId("…")`, `ISODate("…")`).

use std::any::Any;

use futures_util::future::BoxFuture;
use futures_util::TryStreamExt;
use mongodb::bson::{doc, Bson, Document};
use mongodb::options::{ClientOptions, Credential, ServerAddress, Tls, TlsOptions};
use mongodb::{Client, IndexModel};
use serde_json::{json, Map, Value};

use super::{flat_cell, page_rows, ConnectSpec, EngineConn, EnginePool};
use crate::config::{Dialect, SslMode};
use crate::conn::{ColumnMeta, RawResult};
use crate::error::{DbError, DbResult};
use crate::introspect::{ColumnInfo, SchemaInfo, SchemaName, TableInfo, TableKind};

/// Documents read for a console call without `.limit()`.
pub const DEFAULT_LIMIT: i64 = 1000;
/// Documents sampled per collection for the fields in the tree.
const SAMPLE: i64 = 20;

pub struct MongoPool {
    client: Client,
    db: String,
    read_only: bool,
}

impl MongoPool {
    pub async fn connect(spec: &ConnectSpec<'_>) -> DbResult<Self> {
        let ds = spec.ds;
        let host = spec.host();
        let mut o = if host.starts_with("mongodb://") || host.starts_with("mongodb+srv://") {
            ClientOptions::parse(&host).await?
        } else {
            let mut o = ClientOptions::default();
            o.hosts = vec![ServerAddress::Tcp {
                host,
                port: Some(spec.port()),
            }];
            // One server (also through a tunnel): do not follow the replica set's host names.
            o.direct_connection = Some(true);
            o
        };
        o.app_name = Some("Forge".into());
        o.connect_timeout = Some(spec.timeout());
        o.server_selection_timeout = Some(spec.timeout());
        if !ds.user.trim().is_empty() {
            let mut c = Credential::default();
            c.username = Some(ds.user.trim().into());
            c.password = spec.password.map(str::to_string);
            o.credential = Some(c);
        }
        let mode = ds.options.ssl_mode;
        if mode == SslMode::Disable || mode == SslMode::Prefer {
            if o.tls.is_none() {
                o.tls = Some(Tls::Disabled);
            }
        } else {
            let mut t = TlsOptions::default();
            t.allow_invalid_certificates = Some(mode == SslMode::Require);
            if !ds.options.ssl.ca_file.is_empty() {
                t.ca_file_path = Some(crate::tls::resolve(&ds.options.ssl.ca_file, spec.root));
            }
            if !ds.options.ssl.key_file.is_empty() {
                return Err(DbError::Config(
                    "MongoDB reads the client certificate and key from one PEM file: set it as the client certificate and leave the key empty".into(),
                ));
            }
            if !ds.options.ssl.cert_file.is_empty() {
                t.cert_key_file_path =
                    Some(crate::tls::resolve(&ds.options.ssl.cert_file, spec.root));
            }
            o.tls = Some(Tls::Enabled(t));
        }
        let client = Client::with_options(o)?;
        let db = match ds.database.trim() {
            "" => "test".to_string(),
            d => d.to_string(),
        };
        client.database(&db).run_command(doc! { "ping": 1 }).await?;
        Ok(Self {
            client,
            db,
            read_only: ds.options.read_only,
        })
    }
}

impl EnginePool for MongoPool {
    fn dialect(&self) -> Dialect {
        Dialect::Mongo
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn acquire(&self) -> BoxFuture<'_, DbResult<Box<dyn EngineConn>>> {
        Box::pin(async move {
            Ok(Box::new(MongoConn {
                client: self.client.clone(),
                db: self.db.clone(),
                read_only: self.read_only,
            }) as Box<dyn EngineConn>)
        })
    }

    fn close(&self) -> BoxFuture<'_, ()> {
        Box::pin(async move { self.client.clone().shutdown().await })
    }
}

pub struct MongoConn {
    client: Client,
    db: String,
    read_only: bool,
}

// ---- relaxed JSON ------------------------------------------------------------------------

fn bad(msg: impl Into<String>) -> DbError {
    DbError::Sql(msg.into())
}

/// Shell syntax to JSON: unquoted keys, single-quoted strings, trailing commas, comments,
/// `ObjectId("…")`, `ISODate("…")`/`new Date("…")`, `NumberLong(…)`, `NumberInt(…)`,
/// `NumberDecimal("…")` and `/regex/flags` become (extended) JSON.
pub fn js_to_json(src: &str) -> DbResult<String> {
    let chars: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len() + 16);
    let mut i = 0;
    let last_sig = |out: &str| out.trim_end().chars().last();
    while i < chars.len() {
        let c = chars[i];
        match c {
            '"' | '\'' => {
                let (s, next) = read_string(&chars, i)?;
                out.push_str(&serde_json::to_string(&s).unwrap_or_default());
                i = next;
            }
            '/' if chars.get(i + 1) == Some(&'/') => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            '/' if chars.get(i + 1) == Some(&'*') => {
                i += 2;
                while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                    i += 1;
                }
                i += 2;
            }
            '/' if matches!(last_sig(&out), None | Some(':' | ',' | '[' | '(')) => {
                let mut j = i + 1;
                let mut pattern = String::new();
                while j < chars.len() && chars[j] != '/' {
                    if chars[j] == '\\' && j + 1 < chars.len() {
                        pattern.push(chars[j]);
                        j += 1;
                    }
                    pattern.push(chars[j]);
                    j += 1;
                }
                j += 1;
                let mut flags = String::new();
                while j < chars.len() && chars[j].is_ascii_alphabetic() {
                    flags.push(chars[j]);
                    j += 1;
                }
                out.push_str(
                    &json!({ "$regularExpression": { "pattern": pattern, "options": flags } })
                        .to_string(),
                );
                i = j;
            }
            '}' | ']' => {
                let trimmed = out.trim_end().len();
                if out[..trimmed].ends_with(',') {
                    out.truncate(trimmed - 1);
                }
                out.push(c);
                i += 1;
            }
            c if c.is_alphabetic() || c == '_' || c == '$' => {
                let start = i;
                while i < chars.len()
                    && (chars[i].is_alphanumeric()
                        || chars[i] == '_'
                        || chars[i] == '$'
                        || chars[i] == '.')
                {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect();
                let mut j = i;
                while j < chars.len() && chars[j].is_whitespace() {
                    j += 1;
                }
                if chars.get(j) == Some(&':') {
                    out.push_str(&serde_json::to_string(&word).unwrap_or_default());
                    continue;
                }
                match word.as_str() {
                    "true" | "false" | "null" => out.push_str(&word),
                    "undefined" => out.push_str("null"),
                    "new" => {}
                    _ if chars.get(j) == Some(&'(') => {
                        let close = matching(&chars, j)?;
                        let inner: String = chars[j + 1..close].iter().collect();
                        out.push_str(&constructor(&word, inner.trim())?);
                        i = close + 1;
                    }
                    _ => return Err(bad(format!("unexpected `{word}` in arguments"))),
                }
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    Ok(out)
}

fn read_string(chars: &[char], start: usize) -> DbResult<(String, usize)> {
    let q = chars[start];
    let mut s = String::new();
    let mut i = start + 1;
    while i < chars.len() {
        match chars[i] {
            '\\' if i + 1 < chars.len() => {
                let e = chars[i + 1];
                s.push(match e {
                    'n' => '\n',
                    't' => '\t',
                    'r' => '\r',
                    other => other,
                });
                i += 2;
            }
            c if c == q => return Ok((s, i + 1)),
            c => {
                s.push(c);
                i += 1;
            }
        }
    }
    Err(bad("unterminated string"))
}

fn matching(chars: &[char], open: usize) -> DbResult<usize> {
    let mut depth = 0;
    let mut i = open;
    while i < chars.len() {
        match chars[i] {
            '"' | '\'' => i = read_string(chars, i)?.1 - 1,
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Ok(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    Err(bad("unbalanced parentheses"))
}

fn constructor(name: &str, arg: &str) -> DbResult<String> {
    let text = || -> String {
        let a = arg.trim();
        let chars: Vec<char> = a.chars().collect();
        if a.starts_with('"') || a.starts_with('\'') {
            read_string(&chars, 0).map(|s| s.0).unwrap_or_default()
        } else {
            a.to_string()
        }
    };
    Ok(match name {
        "ObjectId" => json!({ "$oid": text() }),
        "ISODate" | "Date" => {
            if arg.is_empty() {
                json!({ "$date": chrono::Utc::now().to_rfc3339() })
            } else {
                json!({ "$date": text() })
            }
        }
        "NumberLong" => json!({ "$numberLong": text() }),
        "NumberInt" => json!({ "$numberInt": text() }),
        "NumberDecimal" => json!({ "$numberDecimal": text() }),
        "UUID" => json!({ "$uuid": text() }),
        other => return Err(bad(format!("{other}() is not supported in arguments"))),
    }
    .to_string())
}

/// The comma-separated arguments of a call as JSON values.
pub fn parse_args(args: &str) -> DbResult<Vec<Value>> {
    if args.trim().is_empty() {
        return Ok(Vec::new());
    }
    let text = js_to_json(&format!("[{args}]"))?;
    match serde_json::from_str(&text) {
        Ok(Value::Array(a)) => Ok(a),
        Ok(_) => Err(bad("expected arguments")),
        Err(e) => Err(bad(format!("cannot read the arguments: {e}"))),
    }
}

fn to_bson(v: Value) -> DbResult<Bson> {
    Bson::try_from(v).map_err(|e| bad(format!("invalid extended JSON: {e}")))
}

fn to_doc(v: Option<Value>) -> DbResult<Document> {
    match v.map(to_bson).transpose()? {
        None | Some(Bson::Null) => Ok(Document::new()),
        Some(Bson::Document(d)) => Ok(d),
        Some(other) => Err(bad(format!("expected a document, got {other}"))),
    }
}

fn to_docs(v: Option<Value>) -> DbResult<Vec<Document>> {
    match v {
        Some(Value::Array(items)) => items.into_iter().map(|x| to_doc(Some(x))).collect(),
        Some(other) => Ok(vec![to_doc(Some(other))?]),
        None => Ok(Vec::new()),
    }
}

/// A parsed console line.
#[derive(Debug, Clone, PartialEq)]
pub struct Call {
    pub collection: String,
    pub method: String,
    pub args: Vec<Value>,
    /// Chained cursor calls: `sort`, `limit`, `skip`, `projection`.
    pub chain: Vec<(String, Vec<Value>)>,
}

/// Parse `db.users.find({...}).sort({...}).limit(5)`.
pub fn parse_call(line: &str) -> DbResult<Call> {
    let text = line.trim().trim_end_matches(';');
    let call = forge_sql::guard::mongo_call(text).ok_or_else(|| {
        bad("expected db.<collection>.<method>(…), show dbs, show collections or use <db>")
    })?;
    let args = parse_args(call.args)?;
    // Everything after the first call's closing parenthesis.
    let first_open = call.args.as_ptr() as usize - text.as_ptr() as usize - 1;
    let close = forge_sql::guard::matching_paren(text, first_open).unwrap_or(text.len() - 1);
    let mut rest = text[close + 1..].trim();
    let mut chain = Vec::new();
    while let Some(r) = rest.strip_prefix('.') {
        let open = r
            .find('(')
            .ok_or_else(|| bad(format!("expected a call: {r}")))?;
        let name = r[..open].trim().to_string();
        let close = forge_sql::guard::matching_paren(r, open)
            .ok_or_else(|| bad("unbalanced parentheses"))?;
        chain.push((name, parse_args(&r[open + 1..close])?));
        rest = r[close + 1..].trim();
    }
    if !rest.is_empty() {
        return Err(bad(format!("unexpected text after the call: {rest}")));
    }
    Ok(Call {
        collection: call.collection.to_string(),
        method: call.method.to_string(),
        args,
        chain,
    })
}

// ---- results ------------------------------------------------------------------------------

/// A document field as a grid cell: ids and dates as text, nested values as JSON.
fn cell(v: &Value) -> Value {
    if let Value::Object(o) = v {
        if o.len() == 1 {
            if let Some(Value::String(s)) = o.get("$oid") {
                return Value::String(format!("ObjectId(\"{s}\")"));
            }
            if let Some(d) = o.get("$date") {
                return match d {
                    Value::String(s) => Value::String(s.clone()),
                    other => Value::String(other.to_string()),
                };
            }
            if let Some(Value::String(s)) = o.get("$numberDecimal").or(o.get("$numberLong")) {
                return Value::String(s.clone());
            }
        }
    }
    flat_cell(v.clone())
}

/// Documents as a result: the union of their top-level fields (`_id` first) as columns and
/// the documents themselves as relaxed extended JSON for the JSON view.
pub fn documents_result(docs: Vec<Document>) -> RawResult {
    let json: Vec<Value> = docs
        .into_iter()
        .map(|d| Bson::Document(d).into_relaxed_extjson())
        .collect();
    let mut columns: Vec<String> = Vec::new();
    for d in &json {
        if let Value::Object(o) = d {
            for k in o.keys() {
                if !columns.contains(k) {
                    columns.push(k.clone());
                }
            }
        }
    }
    if let Some(i) = columns.iter().position(|c| c == "_id") {
        let id = columns.remove(i);
        columns.insert(0, id);
    }
    let rows = json
        .iter()
        .map(|d| {
            columns
                .iter()
                .map(|c| d.get(c).map(cell).unwrap_or(Value::Null))
                .collect()
        })
        .collect();
    RawResult {
        columns: columns
            .into_iter()
            .map(|name| ColumnMeta {
                name,
                type_name: "bson".into(),
            })
            .collect(),
        rows,
        rows_affected: 0,
        has_more: false,
        documents: json,
    }
}

fn single(name: &str, v: Value) -> RawResult {
    RawResult {
        columns: vec![ColumnMeta {
            name: name.into(),
            type_name: "bson".into(),
        }],
        rows: vec![vec![flat_cell(v)]],
        ..Default::default()
    }
}

fn names_result(name: &str, names: Vec<String>) -> RawResult {
    RawResult {
        columns: vec![ColumnMeta {
            name: name.into(),
            type_name: "string".into(),
        }],
        rows: names.into_iter().map(|n| vec![Value::String(n)]).collect(),
        ..Default::default()
    }
}

/// BSON type name of a value, for the sampled fields.
fn type_name(b: &Bson) -> &'static str {
    match b {
        Bson::Double(_) => "double",
        Bson::String(_) => "string",
        Bson::Array(_) => "array",
        Bson::Document(_) => "object",
        Bson::Boolean(_) => "bool",
        Bson::Null => "null",
        Bson::Int32(_) => "int",
        Bson::Int64(_) => "long",
        Bson::Decimal128(_) => "decimal",
        Bson::ObjectId(_) => "objectId",
        Bson::DateTime(_) => "date",
        Bson::Binary(_) => "binData",
        Bson::RegularExpression(_) => "regex",
        Bson::Timestamp(_) => "timestamp",
        _ => "mixed",
    }
}

/// Fields of sampled documents with their types (`string|null` when they differ).
pub fn sample_fields(docs: &[Document]) -> Vec<ColumnInfo> {
    let mut out: Vec<ColumnInfo> = Vec::new();
    for d in docs {
        for (k, v) in d {
            let t = type_name(v);
            match out.iter_mut().find(|c| &c.name == k) {
                Some(c) => {
                    if !c.data_type.split('|').any(|x| x == t) {
                        c.data_type = format!("{}|{t}", c.data_type);
                    }
                }
                None => out.push(ColumnInfo {
                    name: k.clone(),
                    data_type: t.into(),
                    nullable: true,
                    ..Default::default()
                }),
            }
        }
    }
    for c in &mut out {
        c.nullable = c.name != "_id";
    }
    out
}

impl MongoConn {
    async fn console(&mut self, line: &str, skip: usize, max: usize) -> DbResult<RawResult> {
        let text = line.trim().trim_end_matches(';').trim();
        if self.read_only && forge_sql::guard::mongo_is_write(text) {
            return Err(DbError::Refused(
                "the data source is read-only: this call changes data".into(),
            ));
        }
        let mut out = self.dispatch(text).await?;
        if out.documents.is_empty() {
            let (rows, more) = page_rows(std::mem::take(&mut out.rows), skip, max);
            out.rows = rows;
            out.has_more = more;
        } else {
            let docs = std::mem::take(&mut out.documents);
            let rows = std::mem::take(&mut out.rows);
            let more = rows.len() > skip + max;
            out.rows = rows.into_iter().skip(skip).take(max).collect();
            out.documents = docs.into_iter().skip(skip).take(max).collect();
            out.has_more = more;
        }
        Ok(out)
    }

    async fn dispatch(&mut self, text: &str) -> DbResult<RawResult> {
        let words: Vec<&str> = text.split_whitespace().collect();
        match words.as_slice() {
            ["show", "dbs" | "databases"] => {
                return Ok(names_result(
                    "database",
                    self.client.list_database_names().await?,
                ))
            }
            ["show", "collections"] => {
                let mut names = self
                    .client
                    .database(&self.db)
                    .list_collection_names()
                    .await?;
                names.sort();
                return Ok(names_result("collection", names));
            }
            ["use", db] => {
                self.db = db.to_string();
                return Ok(single("result", json!(format!("switched to db {db}"))));
            }
            ["db"] => return Ok(single("database", json!(self.db))),
            _ => {}
        }
        let call = parse_call(text)?;
        let db = self.client.database(&self.db);
        let mut args = call.args.into_iter();
        let mut next = || args.next();
        if call.collection.is_empty() {
            return Ok(match call.method.as_str() {
                "getCollectionNames" => {
                    let mut names = db.list_collection_names().await?;
                    names.sort();
                    names_result("collection", names)
                }
                "runCommand" => {
                    let command = to_doc(next())?;
                    // The console line was checked as text; the document MongoDB gets
                    // decides (its first key is the command).
                    if self.read_only
                        && !command
                            .keys()
                            .next()
                            .is_some_and(|k| forge_sql::guard::is_mongo_read_command(k))
                    {
                        return Err(DbError::Refused(
                            "the data source is read-only: this command changes data".into(),
                        ));
                    }
                    let r = db.run_command(command).await?;
                    documents_result(vec![r])
                }
                "stats" => documents_result(vec![db.run_command(doc! { "dbStats": 1 }).await?]),
                "createCollection" => {
                    let name = next()
                        .and_then(|v| v.as_str().map(str::to_string))
                        .ok_or_else(|| bad("createCollection(name)"))?;
                    db.create_collection(&name).await?;
                    single("result", json!(format!("created {name}")))
                }
                "dropDatabase" => {
                    db.drop().await?;
                    single("result", json!(format!("dropped {}", self.db)))
                }
                "getName" => single("database", json!(self.db)),
                other => return Err(bad(format!("db.{other}() is not supported"))),
            });
        }
        let coll = db.collection::<Document>(&call.collection);
        let affected = |n: u64, what: &str| RawResult {
            rows_affected: n,
            ..single("result", json!(format!("{n} document(s) {what}")))
        };
        Ok(match call.method.as_str() {
            "find" | "findOne" => {
                let filter = to_doc(next())?;
                let projection = next().map(|p| to_doc(Some(p))).transpose()?;
                let mut find = coll.find(filter);
                if let Some(p) = projection {
                    find = find.projection(p);
                }
                let mut limit = if call.method == "findOne" {
                    1
                } else {
                    DEFAULT_LIMIT
                };
                for (name, a) in call.chain {
                    let first = a.into_iter().next();
                    match name.as_str() {
                        "sort" => find = find.sort(to_doc(first)?),
                        "projection" => find = find.projection(to_doc(first)?),
                        "limit" => limit = first.and_then(|v| v.as_i64()).unwrap_or(limit),
                        "skip" => find = find.skip(first.and_then(|v| v.as_u64()).unwrap_or(0)),
                        "pretty" | "toArray" => {}
                        other => {
                            return Err(bad(format!(".{other}() is not supported after find()")))
                        }
                    }
                }
                let docs: Vec<Document> = find.limit(limit).await?.try_collect().await?;
                documents_result(docs)
            }
            "aggregate" => {
                let pipeline = to_docs(next())?;
                let docs: Vec<Document> = coll.aggregate(pipeline).await?.try_collect().await?;
                documents_result(docs)
            }
            "countDocuments" | "count" => {
                single("count", json!(coll.count_documents(to_doc(next())?).await?))
            }
            "estimatedDocumentCount" => {
                single("count", json!(coll.estimated_document_count().await?))
            }
            "distinct" => {
                let field = next()
                    .and_then(|v| v.as_str().map(str::to_string))
                    .ok_or_else(|| bad("distinct(field, filter)"))?;
                let values = coll.distinct(&field, to_doc(next())?).await?;
                RawResult {
                    columns: vec![ColumnMeta {
                        name: field,
                        type_name: "bson".into(),
                    }],
                    rows: values
                        .into_iter()
                        .map(|b| vec![cell(&b.into_relaxed_extjson())])
                        .collect(),
                    ..Default::default()
                }
            }
            "getIndexes" => {
                let idx: Vec<IndexModel> = coll.list_indexes().await?.try_collect().await?;
                let docs = idx
                    .into_iter()
                    .map(|m| {
                        let mut d = doc! { "key": m.keys };
                        if let Some(o) = m.options {
                            if let Some(n) = o.name {
                                d.insert("name", n);
                            }
                            if let Some(u) = o.unique {
                                d.insert("unique", u);
                            }
                        }
                        d
                    })
                    .collect();
                documents_result(docs)
            }
            "insertOne" => {
                coll.insert_one(to_doc(next())?).await?;
                affected(1, "inserted")
            }
            "insertMany" => {
                let r = coll.insert_many(to_docs(next())?).await?;
                affected(r.inserted_ids.len() as u64, "inserted")
            }
            "updateOne" | "updateMany" | "replaceOne" => {
                let filter = to_doc(next())?;
                let update = to_doc(next())?;
                let r = match call.method.as_str() {
                    "updateOne" => coll.update_one(filter, update).await?,
                    "updateMany" => coll.update_many(filter, update).await?,
                    _ => coll.replace_one(filter, update).await?,
                };
                affected(r.modified_count, "modified")
            }
            "deleteOne" => affected(
                coll.delete_one(to_doc(next())?).await?.deleted_count,
                "deleted",
            ),
            "deleteMany" | "remove" => affected(
                coll.delete_many(to_doc(next())?).await?.deleted_count,
                "deleted",
            ),
            "drop" => {
                coll.drop().await?;
                single("result", json!(format!("dropped {}", call.collection)))
            }
            "createIndex" => {
                let name = coll
                    .create_index(IndexModel::builder().keys(to_doc(next())?).build())
                    .await?
                    .index_name;
                single("index", json!(name))
            }
            "stats" => documents_result(vec![
                db.run_command(doc! { "collStats": &call.collection })
                    .await?,
            ]),
            other => return Err(bad(format!("{other}() is not supported"))),
        })
    }

    async fn collections(&self, db_name: &str, only: Option<&str>) -> DbResult<SchemaInfo> {
        let db = self.client.database(db_name);
        let mut specs: Vec<_> = db.list_collections().await?.try_collect().await?;
        specs.sort_by(|a, b| a.name.cmp(&b.name));
        let mut tables = Vec::new();
        for spec in specs.into_iter().take(500) {
            if only.is_some_and(|o| o != spec.name) || spec.name.starts_with("system.") {
                continue;
            }
            let view = matches!(spec.collection_type, mongodb::results::CollectionType::View);
            let coll = db.collection::<Document>(&spec.name);
            let sample: Vec<Document> = coll
                .find(doc! {})
                .limit(SAMPLE)
                .await?
                .try_collect()
                .await?;
            let count = if view {
                None
            } else {
                coll.estimated_document_count().await.ok()
            };
            tables.push(TableInfo {
                name: spec.name,
                kind: if view {
                    TableKind::View
                } else {
                    TableKind::Collection
                },
                columns: sample_fields(&sample),
                primary_key: vec!["_id".into()],
                row_estimate: count.map(|c| c as i64),
                ..Default::default()
            });
        }
        Ok(SchemaInfo {
            name: db_name.into(),
            tables,
            ..Default::default()
        })
    }
}

impl EngineConn for MongoConn {
    fn dialect(&self) -> Dialect {
        Dialect::Mongo
    }

    fn run<'a>(
        &'a mut self,
        sql: &'a str,
        skip: usize,
        max: usize,
    ) -> BoxFuture<'a, DbResult<RawResult>> {
        Box::pin(self.console(sql, skip, max))
    }

    fn server_version(&mut self) -> BoxFuture<'_, DbResult<String>> {
        Box::pin(async move {
            let info = self
                .client
                .database("admin")
                .run_command(doc! { "buildInfo": 1 })
                .await?;
            Ok(format!(
                "MongoDB {}",
                info.get_str("version").unwrap_or("?")
            ))
        })
    }

    fn schemas(&mut self) -> BoxFuture<'_, DbResult<Vec<SchemaName>>> {
        Box::pin(async move {
            let mut names = match self.client.list_database_names().await {
                Ok(n) => n,
                // Users without listDatabases see their own database.
                Err(_) => vec![self.db.clone()],
            };
            if !names.contains(&self.db) {
                names.push(self.db.clone());
            }
            names.sort();
            Ok(names
                .into_iter()
                .map(|name| SchemaName::new(name.clone(), name == self.db))
                .collect())
        })
    }

    fn schema<'a>(
        &'a mut self,
        schema: &'a str,
        only: Option<&'a str>,
    ) -> BoxFuture<'a, DbResult<SchemaInfo>> {
        Box::pin(self.collections(schema, only))
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// `{ "a": 1 }` as a pretty JSON object for a document viewer.
pub fn pretty(doc: &Map<String, Value>) -> String {
    serde_json::to_string_pretty(doc).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_shell_syntax_to_json() {
        let j = js_to_json("{ age: { $gt: 30 }, name: 'O\\'Brien', tags: ['a',], }").unwrap();
        let v: Value = serde_json::from_str(&j).unwrap();
        assert_eq!(
            v,
            json!({"age": {"$gt": 30}, "name": "O'Brien", "tags": ["a"]})
        );
        let j = js_to_json("{_id: ObjectId(\"65a0f0c2e4b0a1b2c3d4e5f6\"), at: ISODate('2025-01-06T09:30:00Z'), n: NumberLong(5), re: /^ad/i} // comment").unwrap();
        let v: Value = serde_json::from_str(&j).unwrap();
        assert_eq!(v["_id"], json!({"$oid": "65a0f0c2e4b0a1b2c3d4e5f6"}));
        assert_eq!(v["at"], json!({"$date": "2025-01-06T09:30:00Z"}));
        assert_eq!(v["n"], json!({"$numberLong": "5"}));
        assert_eq!(v["re"]["$regularExpression"]["pattern"], json!("^ad"));
        assert!(js_to_json("{a: foo}").is_err());
    }

    #[test]
    fn parses_calls_with_chains() {
        let c =
            parse_call("db.users.find({ active: true }, { name: 1 }).sort({ name: -1 }).limit(5);")
                .unwrap();
        assert_eq!(c.collection, "users");
        assert_eq!(c.method, "find");
        assert_eq!(c.args, vec![json!({"active": true}), json!({"name": 1})]);
        assert_eq!(
            c.chain,
            vec![
                ("sort".into(), vec![json!({"name": -1})]),
                ("limit".into(), vec![json!(5)])
            ]
        );
        let c = parse_call("db.orders.aggregate([{ $group: { _id: '$status', n: { $sum: 1 } } }])")
            .unwrap();
        assert_eq!(c.args[0][0]["$group"]["_id"], json!("$status"));
        assert!(parse_call("users.find()").is_err());
        assert!(parse_call("db.users.find() garbage").is_err());
    }

    #[test]
    fn documents_become_rows_and_json() {
        let id = mongodb::bson::oid::ObjectId::parse_str("65a0f0c2e4b0a1b2c3d4e5f6").unwrap();
        let docs = vec![
            doc! { "name": "Ada", "_id": id, "tags": ["math"] },
            doc! { "_id": 2, "name": "Alan", "age": 41 },
        ];
        let fields = sample_fields(&docs);
        assert_eq!(fields[0].name, "name");
        assert_eq!(
            fields.iter().find(|c| c.name == "_id").unwrap().data_type,
            "objectId|int"
        );
        let r = documents_result(docs);
        let names: Vec<_> = r.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["_id", "name", "tags", "age"]);
        assert_eq!(
            r.rows[0][0],
            json!("ObjectId(\"65a0f0c2e4b0a1b2c3d4e5f6\")")
        );
        assert_eq!(r.rows[0][2], json!("[\"math\"]"));
        assert_eq!(r.rows[1][3], json!(41));
        assert_eq!(
            r.documents[0]["_id"],
            json!({"$oid": "65a0f0c2e4b0a1b2c3d4e5f6"})
        );
    }
}
