//! GraphQL: schemas from SDL files or introspection results, validation of queries
//! against them (`apollo-compiler`, MIT OR Apache-2.0), completion of fields, arguments
//! and types, and `graphql.config.*` / `.graphqlrc*` files (schema sources and
//! introspection endpoints).
//!
//! Offsets in and out are UTF-16 (CodeMirror positions).

use std::collections::BTreeMap;

use apollo_compiler::diagnostic::ToCliReport;
use apollo_compiler::schema::ExtendedType;
use apollo_compiler::validation::Valid;
use apollo_compiler::{ExecutableDocument, Schema};
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use forge_jsonschema::tree::Node;
use forge_jsonschema::DocKind;

/// The introspection query sent to an endpoint ("Get GraphQL Schema from Endpoint").
pub const INTROSPECTION_QUERY: &str = "query IntrospectionQuery {
  __schema {
    queryType { name }
    mutationType { name }
    subscriptionType { name }
    types { ...FullType }
    directives { name description locations args { ...InputValue } }
  }
}
fragment FullType on __Type {
  kind name description
  fields(includeDeprecated: true) { name description args { ...InputValue } type { ...TypeRef } isDeprecated deprecationReason }
  inputFields { ...InputValue }
  interfaces { ...TypeRef }
  enumValues(includeDeprecated: true) { name description isDeprecated deprecationReason }
  possibleTypes { ...TypeRef }
}
fragment InputValue on __InputValue { name description type { ...TypeRef } defaultValue }
fragment TypeRef on __Type { kind name ofType { kind name ofType { kind name ofType { kind name ofType { kind name ofType { kind name ofType { kind name } } } } } } }";

// ---- Offsets ----------------------------------------------------------------------------

fn byte_to_utf16(text: &str, byte: usize) -> usize {
    text[..byte.min(text.len())].encode_utf16().count()
}

fn utf16_to_byte(text: &str, utf16: usize) -> usize {
    let mut units = 0;
    for (b, c) in text.char_indices() {
        if units >= utf16 {
            return b;
        }
        units += c.len_utf16();
    }
    text.len()
}

// ---- Schema -----------------------------------------------------------------------------

/// A problem in a GraphQL file (UTF-16 offsets).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    pub from: usize,
    pub to: usize,
    pub message: String,
}

/// A schema built from SDL texts (`.graphql` schema files, or [`introspection_to_sdl`]).
pub struct SchemaSet {
    schema: Valid<Schema>,
    /// Problems of the schema files, by file path.
    pub problems: BTreeMap<String, Vec<Problem>>,
}

/// Whether a `.graphql` file defines schema types (rather than operations).
pub fn is_schema_file(text: &str) -> bool {
    text.lines().any(|l| {
        let t = l.trim_start();
        [
            "type ", "schema ", "schema{", "interface ", "union ", "enum ", "input ", "scalar ",
            "directive ", "extend type ", "extend schema",
        ]
        .iter()
        .any(|k| t.starts_with(k))
    })
}

impl SchemaSet {
    /// Build a schema from `(path, sdl)` files. Invalid schemas are still used as far as
    /// they parse, so queries are checked against what is there.
    pub fn new(files: &[(String, String)]) -> Self {
        let mut builder = Schema::builder();
        for (path, text) in files {
            builder = builder.parse(text.as_str(), path.as_str());
        }
        let mut problems: BTreeMap<String, Vec<Problem>> = BTreeMap::new();
        let texts: BTreeMap<&str, &str> = files.iter().map(|(p, t)| (p.as_str(), t.as_str())).collect();
        let schema = match builder.build().and_then(|s| s.validate()) {
            Ok(s) => s,
            Err(with) => {
                for d in with.errors.iter() {
                    let Some(span) = d.error.location() else { continue };
                    let Some(file) = d.sources.get(&span.file_id()) else { continue };
                    let path = file.path().to_string_lossy().to_string();
                    let text = texts.get(path.as_str()).copied().unwrap_or("");
                    problems.entry(path).or_default().push(Problem {
                        from: byte_to_utf16(text, span.offset()),
                        to: byte_to_utf16(text, span.end_offset()),
                        message: d.error.to_string(),
                    });
                }
                Valid::assume_valid(with.partial)
            }
        };
        SchemaSet { schema, problems }
    }

    /// Types defined by the schema files (built-ins left out), sorted.
    pub fn type_names(&self) -> Vec<String> {
        self.schema
            .types
            .iter()
            .filter(|(_, t)| !t.is_built_in())
            .map(|(n, _)| n.to_string())
            .collect()
    }

    /// Problems of an operation document (queries, mutations, fragments) against the schema.
    pub fn validate(&self, path: &str, text: &str) -> Vec<Problem> {
        validate_with(Some(&self.schema), path, text)
    }

    /// Completion at UTF-16 `offset` of an operation document.
    pub fn complete(&self, text: &str, offset: usize) -> Completion {
        complete_with(&self.schema, text, offset)
    }
}

/// Syntax problems of a GraphQL file when no schema is configured.
pub fn syntax_problems(path: &str, text: &str) -> Vec<Problem> {
    let parsed = apollo_compiler::ast::Document::parse(text, path);
    match parsed {
        Ok(_) => Vec::new(),
        Err(with) => with
            .errors
            .iter()
            .filter_map(|d| {
                let span = d.error.location()?;
                Some(Problem {
                    from: byte_to_utf16(text, span.offset()),
                    to: byte_to_utf16(text, span.end_offset()),
                    message: d.error.to_string(),
                })
            })
            .collect(),
    }
}

fn validate_with(schema: Option<&Valid<Schema>>, path: &str, text: &str) -> Vec<Problem> {
    let mut out = validate_all(schema, path, text);
    out.dedup();
    out
}

fn validate_all(schema: Option<&Valid<Schema>>, path: &str, text: &str) -> Vec<Problem> {
    let Some(schema) = schema else {
        return syntax_problems(path, text);
    };
    match ExecutableDocument::parse_and_validate(schema, text, path) {
        Ok(_) => Vec::new(),
        Err(with) => with
            .errors
            .iter()
            .filter_map(|d| {
                let span = d.error.location()?;
                let file = d.sources.get(&span.file_id())?;
                if file.path().to_string_lossy() != path {
                    return None;
                }
                Some(Problem {
                    from: byte_to_utf16(text, span.offset()),
                    to: byte_to_utf16(text, span.end_offset()),
                    message: d.error.to_string(),
                })
            })
            .collect(),
    }
}

// ---- Completion -------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub label: String,
    /// `field`, `argument`, `type`, `keyword`, `enum`.
    pub kind: String,
    /// The type: `[User!]!`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub documentation: Option<String>,
    #[serde(default)]
    pub deprecated: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Completion {
    /// UTF-16 offset of the word being completed.
    pub from: usize,
    pub items: Vec<Item>,
}

/// One significant token before the caret.
#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Name(String),
    Punct(char),
    Spread,
}

/// Tokens of `text` (comments and strings skipped).
fn tokens(text: &str) -> Vec<Tok> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == b'#' {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
        } else if c == b'"' {
            if text[i..].starts_with("\"\"\"") {
                i = text[i + 3..].find("\"\"\"").map_or(b.len(), |e| i + 3 + e + 3);
                continue;
            }
            i += 1;
            while i < b.len() && b[i] != b'"' && b[i] != b'\n' {
                if b[i] == b'\\' {
                    i += 1;
                }
                i += 1;
            }
            i += 1;
            continue;
        } else if text[i..].starts_with("...") {
            out.push(Tok::Spread);
            i += 3;
            continue;
        } else if c.is_ascii_alphabetic() || c == b'_' {
            let start = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            out.push(Tok::Name(text[start..i].to_string()));
            continue;
        } else if b"{}()[]:$@=!".contains(&c) {
            out.push(Tok::Punct(c as char));
        }
        i += 1;
    }
    out
}

/// Where the caret is, from the tokens before it.
#[derive(Debug, Clone, PartialEq)]
enum Context {
    /// Top level: `query`, `mutation`, `fragment`, ….
    TopLevel,
    /// Inside a selection set of this type.
    Selection(String),
    /// Inside the arguments of `field` of `parent`.
    Arguments { parent: String, field: String },
    /// After `on` / `:` in a variable definition: a type name.
    TypeName,
}

fn named_type_of(schema: &Schema, parent: &str, field: &str) -> Option<String> {
    if field == "__typename" {
        return None;
    }
    let ty = schema.types.get(parent)?;
    let fields = match ty {
        ExtendedType::Object(o) => &o.fields,
        ExtendedType::Interface(i) => &i.fields,
        _ => return None,
    };
    Some(fields.get(field)?.ty.inner_named_type().to_string())
}

fn context(schema: &Schema, toks: &[Tok]) -> Context {
    // Stack of selection types; `None` for a set whose type is unknown.
    let mut stack: Vec<Option<String>> = Vec::new();
    // The type the next `{` opens.
    let mut pending: Option<Option<String>> = None;
    let mut last_field: Option<String> = None;
    let mut paren: Option<(Option<String>, String)> = None;
    let mut in_var_defs = false;
    let mut i = 0;
    while i < toks.len() {
        match &toks[i] {
            Tok::Name(n) if stack.is_empty() && paren.is_none() => match n.as_str() {
                "query" | "mutation" | "subscription" => {
                    let root = match n.as_str() {
                        "query" => apollo_compiler::ast::OperationType::Query,
                        "mutation" => apollo_compiler::ast::OperationType::Mutation,
                        _ => apollo_compiler::ast::OperationType::Subscription,
                    };
                    pending = Some(schema.root_operation(root).map(|t| t.to_string()));
                }
                "fragment" => {
                    // fragment Name on Type
                    if let (Some(Tok::Name(on)), Some(Tok::Name(t))) = (toks.get(i + 2), toks.get(i + 3)) {
                        if on == "on" {
                            pending = Some(Some(t.clone()));
                            i += 3;
                        }
                    }
                }
                _ => {}
            },
            Tok::Punct('(') if stack.is_empty() => in_var_defs = true,
            Tok::Punct(')') if stack.is_empty() => in_var_defs = false,
            Tok::Punct('{') => {
                let ty = pending.take().unwrap_or_else(|| {
                    if stack.is_empty() {
                        // Anonymous query `{ … }`.
                        schema
                            .root_operation(apollo_compiler::ast::OperationType::Query)
                            .map(|t| t.to_string())
                    } else {
                        let parent = stack.last().cloned().flatten();
                        match (parent, &last_field) {
                            (Some(p), Some(f)) => named_type_of(schema, &p, f),
                            _ => None,
                        }
                    }
                });
                stack.push(ty);
                last_field = None;
            }
            Tok::Punct('}') => {
                stack.pop();
                last_field = None;
                pending = None;
            }
            Tok::Punct('(') => {
                let parent = stack.last().cloned().flatten();
                if let Some(f) = &last_field {
                    paren = Some((parent, f.clone()));
                }
            }
            Tok::Punct(')') => paren = None,
            Tok::Spread => {
                if let (Some(Tok::Name(on)), Some(Tok::Name(t))) = (toks.get(i + 1), toks.get(i + 2)) {
                    if on == "on" {
                        pending = Some(Some(t.clone()));
                        i += 2;
                    }
                } else if let Some(Tok::Name(_)) = toks.get(i + 1) {
                    i += 1; // fragment spread
                }
            }
            Tok::Name(n) if paren.is_none() => {
                // `alias: field` keeps the field.
                last_field = Some(n.clone());
            }
            _ => {}
        }
        i += 1;
    }
    // A `:` right before the caret inside variable definitions, or `on`: a type.
    let last = toks.last();
    if matches!(last, Some(Tok::Name(n)) if n == "on")
        || (in_var_defs && stack.is_empty() && matches!(last, Some(Tok::Punct(':'))))
    {
        return Context::TypeName;
    }
    if let Some((Some(parent), field)) = paren {
        return Context::Arguments { parent, field };
    }
    match stack.last() {
        None => Context::TopLevel,
        Some(Some(t)) => Context::Selection(t.clone()),
        Some(None) => Context::Selection(String::new()),
    }
}

fn complete_with(schema: &Schema, text: &str, offset: usize) -> Completion {
    let byte = utf16_to_byte(text, offset);
    let before = &text[..byte];
    let word_start = before
        .rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .map_or(0, |i| i + 1);
    let toks = tokens(&before[..word_start]);
    let mut items = Vec::new();
    match context(schema, &toks) {
        Context::TopLevel => {
            for k in ["query", "mutation", "subscription", "fragment"] {
                items.push(Item {
                    label: k.into(),
                    kind: "keyword".into(),
                    detail: None,
                    documentation: None,
                    deprecated: false,
                });
            }
        }
        Context::TypeName => {
            for (name, t) in &schema.types {
                if t.is_built_in() && !matches!(t, ExtendedType::Scalar(_)) {
                    continue;
                }
                items.push(Item {
                    label: name.to_string(),
                    kind: "type".into(),
                    detail: None,
                    documentation: t.description().map(|d| d.to_string()),
                    deprecated: false,
                });
            }
        }
        Context::Selection(ty) => {
            let fields = match schema.types.get(ty.as_str()) {
                Some(ExtendedType::Object(o)) => Some(&o.fields),
                Some(ExtendedType::Interface(i)) => Some(&i.fields),
                _ => None,
            };
            for (name, f) in fields.into_iter().flatten() {
                items.push(Item {
                    label: name.to_string(),
                    kind: "field".into(),
                    detail: Some(f.ty.to_string()),
                    documentation: f.description.as_ref().map(|d| d.to_string()),
                    deprecated: f.directives.get("deprecated").is_some(),
                });
            }
            items.push(Item {
                label: "__typename".into(),
                kind: "field".into(),
                detail: Some("String!".into()),
                documentation: None,
                deprecated: false,
            });
            if let Some(ExtendedType::Union(u)) = schema.types.get(ty.as_str()) {
                for m in &u.members {
                    items.push(Item {
                        label: format!("... on {}", m.name),
                        kind: "type".into(),
                        detail: None,
                        documentation: None,
                        deprecated: false,
                    });
                }
            }
        }
        Context::Arguments { parent, field } => {
            let f = match schema.types.get(parent.as_str()) {
                Some(ExtendedType::Object(o)) => o.fields.get(field.as_str()),
                Some(ExtendedType::Interface(i)) => i.fields.get(field.as_str()),
                _ => None,
            };
            for a in f.map(|f| f.arguments.iter()).into_iter().flatten() {
                items.push(Item {
                    label: a.name.to_string(),
                    kind: "argument".into(),
                    detail: Some(a.ty.to_string()),
                    documentation: a.description.as_ref().map(|d| d.to_string()),
                    deprecated: false,
                });
            }
        }
    }
    Completion {
        from: byte_to_utf16(text, word_start),
        items,
    }
}

// ---- Introspection ----------------------------------------------------------------------

fn type_ref(t: &Json) -> Option<String> {
    match t.get("kind")?.as_str()? {
        "NON_NULL" => Some(format!("{}!", type_ref(t.get("ofType")?)?)),
        "LIST" => Some(format!("[{}]", type_ref(t.get("ofType")?)?)),
        _ => Some(t.get("name")?.as_str()?.to_string()),
    }
}

fn description(d: Option<&Json>, indent: &str) -> String {
    match d.and_then(Json::as_str).filter(|s| !s.trim().is_empty()) {
        Some(s) => format!("{indent}\"\"\"{}\"\"\"\n", s.replace("\"\"\"", "\\\"\"\"")),
        None => String::new(),
    }
}

fn args(list: Option<&Json>) -> String {
    let Some(list) = list.and_then(Json::as_array).filter(|l| !l.is_empty()) else {
        return String::new();
    };
    let parts: Vec<String> = list
        .iter()
        .filter_map(|a| {
            let mut s = format!("{}: {}", a.get("name")?.as_str()?, type_ref(a.get("type")?)?);
            if let Some(d) = a.get("defaultValue").and_then(Json::as_str) {
                s.push_str(&format!(" = {d}"));
            }
            Some(s)
        })
        .collect();
    format!("({})", parts.join(", "))
}

fn deprecated(v: &Json) -> String {
    if v.get("isDeprecated").and_then(Json::as_bool) != Some(true) {
        return String::new();
    }
    match v.get("deprecationReason").and_then(Json::as_str) {
        Some(r) => format!(" @deprecated(reason: {})", Json::String(r.to_string())),
        None => " @deprecated".into(),
    }
}

/// SDL text of an introspection result (`{ "data": { "__schema": … } }` or
/// `{ "__schema": … }`).
pub fn introspection_to_sdl(json: &str) -> Result<String, String> {
    let v: Json = serde_json::from_str(json).map_err(|e| format!("Invalid introspection result: {e}"))?;
    let schema = v
        .pointer("/data/__schema")
        .or_else(|| v.get("__schema"))
        .ok_or("The response has no __schema (is introspection disabled on the server?)")?;
    let mut out = String::new();
    let root = |k: &str| schema.pointer(&format!("/{k}/name")).and_then(Json::as_str);
    let (q, m, s) = (root("queryType"), root("mutationType"), root("subscriptionType"));
    if q.is_some_and(|q| q != "Query") || m.is_some_and(|m| m != "Mutation") || s.is_some_and(|s| s != "Subscription") {
        out.push_str("schema {\n");
        for (k, n) in [("query", q), ("mutation", m), ("subscription", s)] {
            if let Some(n) = n {
                out.push_str(&format!("  {k}: {n}\n"));
            }
        }
        out.push_str("}\n\n");
    }
    const BUILT_IN: &[&str] = &["String", "Int", "Float", "Boolean", "ID"];
    for t in schema.get("types").and_then(Json::as_array).into_iter().flatten() {
        let Some(name) = t.get("name").and_then(Json::as_str) else { continue };
        if name.starts_with("__") || BUILT_IN.contains(&name) {
            continue;
        }
        out.push_str(&description(t.get("description"), ""));
        let fields = |key: &str, with_args: bool| -> String {
            let mut s = String::new();
            for f in t.get(key).and_then(Json::as_array).into_iter().flatten() {
                let (Some(n), Some(ty)) = (f.get("name").and_then(Json::as_str), f.get("type").and_then(type_ref)) else {
                    continue;
                };
                s.push_str(&description(f.get("description"), "  "));
                let a = if with_args { args(f.get("args")) } else { String::new() };
                let default = f
                    .get("defaultValue")
                    .and_then(Json::as_str)
                    .map(|d| format!(" = {d}"))
                    .unwrap_or_default();
                s.push_str(&format!("  {n}{a}: {ty}{default}{}\n", deprecated(f)));
            }
            s
        };
        let interfaces = t
            .get("interfaces")
            .and_then(Json::as_array)
            .filter(|l| !l.is_empty())
            .map(|l| {
                let names: Vec<&str> = l.iter().filter_map(|i| i.get("name")?.as_str()).collect();
                format!(" implements {}", names.join(" & "))
            })
            .unwrap_or_default();
        match t.get("kind").and_then(Json::as_str) {
            Some("OBJECT") => out.push_str(&format!("type {name}{interfaces} {{\n{}}}\n\n", fields("fields", true))),
            Some("INTERFACE") => out.push_str(&format!("interface {name}{interfaces} {{\n{}}}\n\n", fields("fields", true))),
            Some("INPUT_OBJECT") => out.push_str(&format!("input {name} {{\n{}}}\n\n", fields("inputFields", false))),
            Some("SCALAR") => out.push_str(&format!("scalar {name}\n\n")),
            Some("UNION") => {
                let members: Vec<&str> = t
                    .get("possibleTypes")
                    .and_then(Json::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|p| p.get("name")?.as_str())
                    .collect();
                out.push_str(&format!("union {name} = {}\n\n", members.join(" | ")));
            }
            Some("ENUM") => {
                let mut s = String::new();
                for v in t.get("enumValues").and_then(Json::as_array).into_iter().flatten() {
                    if let Some(n) = v.get("name").and_then(Json::as_str) {
                        s.push_str(&description(v.get("description"), "  "));
                        s.push_str(&format!("  {n}{}\n", deprecated(v)));
                    }
                }
                out.push_str(&format!("enum {name} {{\n{s}}}\n\n"));
            }
            _ => {}
        }
    }
    const BUILT_IN_DIRECTIVES: &[&str] = &["skip", "include", "deprecated", "specifiedBy", "oneOf"];
    for d in schema.get("directives").and_then(Json::as_array).into_iter().flatten() {
        let Some(name) = d.get("name").and_then(Json::as_str) else { continue };
        if BUILT_IN_DIRECTIVES.contains(&name) {
            continue;
        }
        let locations: Vec<&str> = d
            .get("locations")
            .and_then(Json::as_array)
            .into_iter()
            .flatten()
            .filter_map(Json::as_str)
            .collect();
        out.push_str(&description(d.get("description"), ""));
        out.push_str(&format!("directive @{name}{} on {}\n\n", args(d.get("args")), locations.join(" | ")));
    }
    Ok(out.trim_end().to_string() + "\n")
}

// ---- graphql.config ---------------------------------------------------------------------

/// Names of GraphQL config files, in the order they are looked up.
pub const CONFIG_FILES: &[&str] = &[
    "graphql.config.json",
    "graphql.config.yml",
    "graphql.config.yaml",
    ".graphqlrc",
    ".graphqlrc.json",
    ".graphqlrc.yml",
    ".graphqlrc.yaml",
    "graphql.config.js",
    "graphql.config.ts",
    "graphql.config.cjs",
    "graphql.config.mjs",
    ".graphqlconfig",
];

/// An endpoint to introspect and run queries against.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Endpoint {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    /// Where the introspected schema is saved (`schema.graphql` by default).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema_file: Option<String>,
}

/// A GraphQL config (the root project; `projects` are merged into it).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    /// The config file (workspace relative).
    pub path: String,
    /// Local schema files or globs, relative to the config's folder; URLs become
    /// [`Endpoint`]s.
    pub schema: Vec<String>,
    pub documents: Vec<String>,
    pub endpoints: Vec<Endpoint>,
}

fn strings(n: Option<&Node>) -> Vec<String> {
    match n {
        Some(n) => match n.as_array() {
            Some(a) => a.iter().flat_map(|x| strings(Some(x))).collect(),
            None => match n.as_str() {
                Some(s) => vec![s.to_string()],
                // `schema: { "https://…": { headers } }`
                None => n
                    .as_object()
                    .map(|o| o.iter().map(|p| p.key.clone()).collect())
                    .unwrap_or_default(),
            },
        },
        None => Vec::new(),
    }
}

fn headers(n: Option<&Node>) -> BTreeMap<String, String> {
    n.and_then(Node::as_object)
        .unwrap_or(&[])
        .iter()
        .filter_map(|p| Some((p.key.clone(), p.value.as_ref()?.as_str()?.to_string())))
        .collect()
}

fn read_project(path: &str, root: &Node, cfg: &mut Config) {
    let schema_node = root.get("schema").or_else(|| root.get("schemaPath"));
    for s in strings(schema_node) {
        if s.starts_with("http://") || s.starts_with("https://") {
            let h = schema_node
                .and_then(|n| n.get(&s))
                .and_then(|o| o.get("headers"));
            cfg.endpoints.push(Endpoint {
                name: s.clone(),
                url: s,
                headers: headers(h),
                schema_file: None,
            });
        } else {
            cfg.schema.push(s);
        }
    }
    cfg.documents.extend(strings(root.get("documents")));
    // `extensions.endpoints` (graphql-config v2 / JetBrains GraphQL plugin).
    let eps = root
        .get("extensions")
        .and_then(|e| e.get("endpoints"))
        .and_then(Node::as_object)
        .unwrap_or(&[]);
    for e in eps {
        let Some(v) = e.value.as_ref() else { continue };
        let (url, h) = match v.as_str() {
            Some(u) => (u.to_string(), BTreeMap::new()),
            None => match v.get("url").and_then(Node::as_str) {
                Some(u) => (u.to_string(), headers(v.get("headers"))),
                None => continue,
            },
        };
        cfg.endpoints.push(Endpoint {
            name: e.key.clone(),
            url,
            headers: h,
            schema_file: cfg.schema.first().cloned(),
        });
    }
    let _ = path;
}

/// Read a GraphQL config file. JavaScript configs are read best effort (`schema: '…'`,
/// `documents: '…'`).
pub fn parse_config(path: &str, text: &str) -> Config {
    let mut cfg = Config {
        path: path.to_string(),
        ..Config::default()
    };
    let name = path.rsplit('/').next().unwrap_or(path);
    if name.ends_with(".js") || name.ends_with(".ts") || name.ends_with(".cjs") || name.ends_with(".mjs") {
        for (key, list) in [("schema", 0), ("documents", 1)] {
            let Some(i) = text.find(&format!("{key}:")) else { continue };
            let rest = &text[i + key.len() + 1..];
            let end = rest.find(['\n', '}']).unwrap_or(rest.len());
            for part in rest[..end].split(',') {
                let p = part.trim().trim_matches(['[', ']', ' ']);
                let s = p.trim_matches(['"', '\'', '`']);
                if s.len() + 2 == p.len() && !s.is_empty() {
                    if list == 0 {
                        if s.starts_with("http") {
                            cfg.endpoints.push(Endpoint {
                                name: s.to_string(),
                                url: s.to_string(),
                                ..Endpoint::default()
                            });
                        } else {
                            cfg.schema.push(s.to_string());
                        }
                    } else {
                        cfg.documents.push(s.to_string());
                    }
                }
            }
        }
        return cfg;
    }
    let kind = if text.trim_start().starts_with('{') {
        DocKind::Json
    } else {
        DocKind::Yaml
    };
    let Some(root) = forge_jsonschema::parse(kind, text).into_iter().next() else {
        return cfg;
    };
    read_project(path, &root, &mut cfg);
    if let Some(projects) = root.get("projects").and_then(Node::as_object) {
        for p in projects {
            if let Some(v) = &p.value {
                read_project(path, v, &mut cfg);
            }
        }
    }
    cfg
}

// ---- Requests ---------------------------------------------------------------------------

/// The JSON body of a GraphQL-over-HTTP request: `{ "query", "variables"?,
/// "operationName"? }`.
pub fn request_body(query: &str, variables: Option<&str>, operation: Option<&str>) -> Result<String, String> {
    let mut m = serde_json::Map::new();
    m.insert("query".into(), Json::String(query.trim().to_string()));
    if let Some(v) = variables.map(str::trim).filter(|v| !v.is_empty()) {
        let parsed: Json = serde_json::from_str(v).map_err(|e| format!("Variables are not valid JSON: {e}"))?;
        m.insert("variables".into(), parsed);
    }
    if let Some(op) = operation {
        m.insert("operationName".into(), Json::String(op.to_string()));
    }
    Ok(Json::Object(m).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SDL: &str = r#"
"""A user of the shop"""
type User {
  id: ID!
  name: String
  "Orders, newest first"
  orders(first: Int = 10, status: Status): [Order!]!
  legacy: String @deprecated(reason: "use name")
}
type Order { id: ID! total: Float! items: [Item!]! }
type Item { sku: String! }
enum Status { OPEN SHIPPED }
union SearchResult = User | Order
type Query {
  me: User
  user(id: ID!): User
  search(text: String!): [SearchResult!]!
}
type Mutation { rename(name: String!): User }
"#;

    fn schema() -> SchemaSet {
        SchemaSet::new(&[("schema.graphql".to_string(), SDL.to_string())])
    }

    fn labels(c: &Completion) -> Vec<&str> {
        c.items.iter().map(|i| i.label.as_str()).collect()
    }

    #[test]
    fn validates_queries_against_the_schema() {
        let s = schema();
        assert!(s.problems.is_empty(), "{:?}", s.problems);
        assert!(s.validate("q.graphql", "query { me { id name orders(first: 2) { total } } }").is_empty());
        let q = "query Q {\n  me { id email }\n  user { name }\n}";
        let problems = s.validate("q.graphql", q);
        let messages: Vec<&str> = problems.iter().map(|p| p.message.as_str()).collect();
        assert_eq!(problems.len(), 2, "{messages:?}");
        assert!(messages[0].contains("email"), "{messages:?}");
        let email = q.find("email").unwrap();
        assert_eq!(problems[0].from, email);
        assert!(messages[1].contains("id"), "{messages:?}");
        // Syntax errors.
        let p = s.validate("q.graphql", "query { me { id ");
        assert_eq!(p.len(), 1, "{p:?}");
        assert!(syntax_problems("q.graphql", "{ a }").is_empty());
        assert!(!syntax_problems("q.graphql", "{ a ").is_empty());
    }

    #[test]
    fn reports_schema_problems() {
        let s = SchemaSet::new(&[("s.graphql".to_string(), "type Query { a: Missing }".to_string())]);
        let p = &s.problems["s.graphql"];
        assert!(p[0].message.contains("Missing"), "{p:?}");
        assert!(s.type_names().contains(&"Query".to_string()));
        assert!(is_schema_file(SDL));
        assert!(!is_schema_file("query { me { id } }"));
    }

    #[test]
    fn completes_fields_arguments_and_types() {
        let s = schema();
        let q = "query { me { na";
        let c = s.complete(q, q.len());
        assert_eq!(c.from, q.len() - 2);
        assert_eq!(labels(&c), vec!["id", "name", "orders", "legacy", "__typename"]);
        assert_eq!(c.items[2].detail.as_deref(), Some("[Order!]!"));
        assert_eq!(c.items[2].documentation.as_deref(), Some("Orders, newest first"));
        assert!(c.items[3].deprecated);

        let q = "{ me { orders { items { ";
        assert_eq!(labels(&s.complete(q, q.len())), vec!["sku", "__typename"]);
        let q = "query { me { orders(";
        assert_eq!(labels(&s.complete(q, q.len())), vec!["first", "status"]);
        let q = "mutation { ";
        assert_eq!(labels(&s.complete(q, q.len())), vec!["rename", "__typename"]);
        let q = "query { search(text: \"x\") { ... on ";
        assert!(labels(&s.complete(q, q.len())).contains(&"Order"));
        let q = "query { search(text: \"x\") { ";
        assert!(labels(&s.complete(q, q.len())).contains(&"... on User"));
        let q = "fragment F on Order { ";
        assert_eq!(labels(&s.complete(q, q.len()))[0], "id");
        assert_eq!(labels(&s.complete("", 0)), vec!["query", "mutation", "subscription", "fragment"]);
        let q = "query { me { id } }\nquery Two { me { orders { } } }";
        let at = q.rfind("{ }").unwrap() + 2;
        assert_eq!(labels(&s.complete(q, at))[0], "id");
    }

    #[test]
    fn converts_introspection_results_to_sdl() {
        let json = r#"{"data":{"__schema":{
          "queryType":{"name":"Query"},"mutationType":null,"subscriptionType":null,
          "types":[
            {"kind":"OBJECT","name":"Query","description":null,"fields":[
              {"name":"user","description":"Find a user","args":[{"name":"id","description":null,"type":{"kind":"NON_NULL","name":null,"ofType":{"kind":"SCALAR","name":"ID","ofType":null}},"defaultValue":null}],
               "type":{"kind":"OBJECT","name":"User","ofType":null},"isDeprecated":false,"deprecationReason":null}],
             "inputFields":null,"interfaces":[],"enumValues":null,"possibleTypes":null},
            {"kind":"OBJECT","name":"User","description":null,"fields":[
              {"name":"tags","description":null,"args":[],"type":{"kind":"NON_NULL","name":null,"ofType":{"kind":"LIST","name":null,"ofType":{"kind":"SCALAR","name":"String","ofType":null}}},"isDeprecated":true,"deprecationReason":"gone"}],
             "inputFields":null,"interfaces":[],"enumValues":null,"possibleTypes":null},
            {"kind":"ENUM","name":"Role","description":null,"fields":null,"inputFields":null,"interfaces":null,"enumValues":[{"name":"ADMIN","description":null,"isDeprecated":false,"deprecationReason":null}],"possibleTypes":null},
            {"kind":"SCALAR","name":"String","description":null,"fields":null,"inputFields":null,"interfaces":null,"enumValues":null,"possibleTypes":null},
            {"kind":"OBJECT","name":"__Type","description":null,"fields":[],"inputFields":null,"interfaces":[],"enumValues":null,"possibleTypes":null}
          ],
          "directives":[{"name":"skip","description":null,"locations":["FIELD"],"args":[]}]}}}"#;
        let sdl = introspection_to_sdl(json).unwrap();
        assert_eq!(
            sdl,
            "type Query {\n  \"\"\"Find a user\"\"\"\n  user(id: ID!): User\n}\n\ntype User {\n  tags: [String]! @deprecated(reason: \"gone\")\n}\n\nenum Role {\n  ADMIN\n}\n"
        );
        // The SDL is a valid schema.
        let s = SchemaSet::new(&[("introspected.graphql".to_string(), sdl)]);
        assert!(s.problems.is_empty(), "{:?}", s.problems);
        assert!(introspection_to_sdl(r#"{"errors": [{"message": "no"}]}"#).is_err());
    }

    #[test]
    fn reads_graphql_configs() {
        let yaml = "schema: schema.graphql\ndocuments: 'src/**/*.graphql'\nextensions:\n  endpoints:\n    dev:\n      url: http://localhost:4000/graphql\n      headers:\n        Authorization: Bearer x\n";
        let c = parse_config(".graphqlrc.yml", yaml);
        assert_eq!(c.schema, vec!["schema.graphql"]);
        assert_eq!(c.documents, vec!["src/**/*.graphql"]);
        assert_eq!(c.endpoints[0].url, "http://localhost:4000/graphql");
        assert_eq!(c.endpoints[0].headers["Authorization"], "Bearer x");
        let json = r#"{"schema": {"https://api.example.com/graphql": {"headers": {"X-Key": "1"}}}, "documents": ["a.graphql", "b.graphql"]}"#;
        let c = parse_config("graphql.config.json", json);
        assert_eq!(c.endpoints[0].url, "https://api.example.com/graphql");
        assert_eq!(c.endpoints[0].headers["X-Key"], "1");
        assert_eq!(c.documents.len(), 2);
        let js = "export default {\n  schema: 'schema/*.graphql',\n  documents: ['src/**/*.gql'],\n}";
        let c = parse_config("graphql.config.ts", js);
        assert_eq!((c.schema.as_slice(), c.documents.as_slice()), (&["schema/*.graphql".to_string()][..], &["src/**/*.gql".to_string()][..]));
    }

    #[test]
    fn builds_request_bodies() {
        assert_eq!(
            request_body("query { me { id } }", Some("{\"a\": 1}"), None).unwrap(),
            r#"{"query":"query { me { id } }","variables":{"a":1}}"#
        );
        assert!(request_body("{ a }", Some("{ nope"), None).is_err());
    }
}
