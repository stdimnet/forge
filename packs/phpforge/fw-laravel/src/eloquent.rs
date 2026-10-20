//! Eloquent models: table columns from migrations, `$fillable`, casts, relations (with the
//! related model), local scopes, accessors and `@property` docblocks.

use std::collections::HashMap;
use std::path::Path;

use phpforge_lang_php::parser::index_source;
use phpforge_lang_php::{ClassLike, FileIndex, SymbolKind};
use serde::{Deserialize, Serialize};
use tree_sitter::Node;

use crate::php::{self, call_args, call_name, text, Value};
use crate::project::{files_under, read, Location};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Model {
    pub class: String,
    pub name: String,
    pub path: String,
    pub table: String,
    pub attributes: Vec<ModelAttribute>,
    pub relations: Vec<Relation>,
    pub scopes: Vec<Scope>,
    pub location: Location,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelAttribute {
    pub name: String,
    /// PHP type (`int`, `string`, `\Illuminate\Support\Carbon`, an enum class, ...).
    pub type_name: String,
    /// Column type from the migration (`string`, `decimal`, ...), if it is a column.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column: Option<String>,
    #[serde(default)]
    pub nullable: bool,
    #[serde(default)]
    pub fillable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cast: Option<String>,
    /// Defined by an accessor method rather than a column.
    #[serde(default)]
    pub accessor: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<Location>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Relation {
    pub name: String,
    /// `hasMany`, `belongsTo`, ...
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub related: Option<String>,
    pub location: Location,
}

impl Relation {
    pub fn is_many(&self) -> bool {
        self.kind.contains("Many")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scope {
    /// Name as called on a query (`active` for `scopeActive`).
    pub name: String,
    /// Parameters after the query builder, as PHP source.
    pub params: Vec<String>,
    pub location: Location,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Column {
    pub name: String,
    pub column_type: String,
    pub nullable: bool,
    pub location: Location,
}

const RELATIONS: &[&str] = &[
    "hasOne",
    "hasMany",
    "belongsTo",
    "belongsToMany",
    "hasOneThrough",
    "hasManyThrough",
    "morphOne",
    "morphMany",
    "morphTo",
    "morphToMany",
    "morphedByMany",
];

/// `OrderItem` → `order_item`.
pub fn snake(name: &str) -> String {
    crate::views::kebab(name).replace('-', "_")
}

/// `order_item` → `order_items`, `category` → `categories`.
pub fn plural(word: &str) -> String {
    if let Some(stem) = word.strip_suffix('y') {
        if !stem.ends_with(['a', 'e', 'i', 'o', 'u']) {
            return format!("{stem}ies");
        }
    }
    if word.ends_with('s') || word.ends_with('x') || word.ends_with("ch") || word.ends_with("sh") {
        return format!("{word}es");
    }
    format!("{word}s")
}

/// Columns per table from `database/migrations` (later migrations add and drop columns).
pub fn migration_columns(root: &Path) -> HashMap<String, Vec<Column>> {
    let mut tables: HashMap<String, Vec<Column>> = HashMap::new();
    for path in files_under(root, "database/migrations", ".php") {
        let Some(src) = read(root, &path) else {
            continue;
        };
        let Some(tree) = php::parse(&src) else {
            continue;
        };
        // Only `up()`: `down()` usually drops what `up()` created.
        let mut up_range: Option<(usize, usize)> = None;
        php::walk(tree.root_node(), &mut |n| {
            if n.kind() == "method_declaration"
                && n.child_by_field_name("name").map(|x| text(x, &src)) == Some("up")
            {
                up_range = Some((n.start_byte(), n.end_byte()));
                return false;
            }
            true
        });
        php::walk(tree.root_node(), &mut |n| {
            if let Some((s, e)) = up_range {
                if n.end_byte() < s || n.start_byte() > e {
                    return false;
                }
            }
            if n.kind() == "scoped_call_expression"
                && n.child_by_field_name("scope")
                    .map(|x| text(x, &src).trim_start_matches('\\').to_string())
                    .is_some_and(|s| s == "Schema" || s.ends_with("\\Schema"))
            {
                let method = call_name(n, &src).unwrap_or_default();
                if matches!(method, "create" | "table") {
                    let args = call_args(n);
                    if let (Some(name), Some(body)) = (
                        args.first().and_then(|a| php::string_value(*a, &src)),
                        args.get(1),
                    ) {
                        let cols = tables.entry(name).or_default();
                        if method == "create" {
                            cols.clear();
                        }
                        blueprint(*body, &src, &path, cols);
                    }
                }
                return false;
            }
            true
        });
    }
    tables
}

/// Apply the `$table->...` statements of a Blueprint closure to `cols`.
fn blueprint(closure: Node, src: &str, path: &str, cols: &mut Vec<Column>) {
    let Some(body) = closure.child_by_field_name("body") else {
        return;
    };
    let mut cursor = body.walk();
    for stmt in body.named_children(&mut cursor) {
        let Some(expr) = (if stmt.kind() == "expression_statement" {
            stmt.named_child(0)
        } else {
            None
        }) else {
            continue;
        };
        // Walk the chain down to `$table->type('name')`.
        let mut chain: Vec<Node> = Vec::new();
        let mut cur = expr;
        while matches!(
            cur.kind(),
            "member_call_expression" | "nullsafe_member_call_expression"
        ) {
            chain.push(cur);
            match cur.child_by_field_name("object") {
                Some(o) => cur = o,
                None => break,
            }
        }
        if cur.kind() != "variable_name" {
            continue;
        }
        let Some(base) = chain.pop() else { continue };
        let modifiers: Vec<&str> = chain.iter().filter_map(|c| call_name(*c, src)).collect();
        let nullable = modifiers.contains(&"nullable");
        let ty = call_name(base, src).unwrap_or_default().to_string();
        let args = call_args(base);
        let first = args.first().and_then(|a| php::string_value(*a, src));
        let loc = Location::at(path, src, base.start_byte());
        let mut add = |name: &str, ty: &str, nullable: bool| {
            cols.retain(|c| c.name != name);
            cols.push(Column {
                name: name.to_string(),
                column_type: ty.to_string(),
                nullable,
                location: loc.clone(),
            });
        };
        match ty.as_str() {
            "id" | "bigIncrements" | "increments" => {
                add(first.as_deref().unwrap_or("id"), "id", false)
            }
            "uuid" | "ulid" if first.is_none() => add("id", &ty, false),
            "timestamps" | "timestampsTz" | "nullableTimestamps" => {
                add("created_at", "timestamp", true);
                add("updated_at", "timestamp", true);
            }
            "softDeletes" | "softDeletesTz" => {
                add(first.as_deref().unwrap_or("deleted_at"), "timestamp", true)
            }
            "rememberToken" => add("remember_token", "string", true),
            "morphs" | "nullableMorphs" | "uuidMorphs" | "ulidMorphs" => {
                if let Some(n) = &first {
                    let null = ty.starts_with("nullable");
                    add(&format!("{n}_id"), "foreignId", null);
                    add(&format!("{n}_type"), "string", null);
                }
            }
            "foreignIdFor" => {
                let class = args.first().map(|a| php::value(*a, src));
                let col = args
                    .get(1)
                    .and_then(|a| php::string_value(*a, src))
                    .or_else(|| match class {
                        Some(Value::ClassRef(c)) => {
                            Some(format!("{}_id", snake(c.rsplit('\\').next().unwrap_or(&c))))
                        }
                        _ => None,
                    });
                if let Some(c) = col {
                    add(&c, "foreignId", nullable);
                }
            }
            "dropColumn" | "dropColumns" => {
                for a in &args {
                    for name in php::value(*a, src).strings() {
                        cols.retain(|c| c.name != name);
                    }
                }
            }
            "dropTimestamps" => cols.retain(|c| c.name != "created_at" && c.name != "updated_at"),
            "dropSoftDeletes" => cols.retain(|c| c.name != "deleted_at"),
            "renameColumn" => {
                if let (Some(from), Some(to)) =
                    (first, args.get(1).and_then(|a| php::string_value(*a, src)))
                {
                    if let Some(c) = cols.iter_mut().find(|c| c.name == from) {
                        c.name = to;
                    }
                }
            }
            "index" | "unique" | "primary" | "foreign" | "dropIndex" | "dropUnique"
            | "dropForeign" | "dropPrimary" | "fullText" | "spatialIndex" | "engine"
            | "charset" | "collation" | "comment" | "temporary" => {}
            _ => {
                if let Some(n) = first {
                    add(&n, &ty, nullable);
                }
            }
        }
    }
}

/// PHP type of a column type.
pub fn php_type(column_type: &str) -> &'static str {
    match column_type {
        "id"
        | "foreignId"
        | "integer"
        | "bigInteger"
        | "smallInteger"
        | "tinyInteger"
        | "mediumInteger"
        | "unsignedInteger"
        | "unsignedBigInteger"
        | "unsignedSmallInteger"
        | "unsignedTinyInteger"
        | "unsignedMediumInteger"
        | "increments"
        | "bigIncrements"
        | "year" => "int",
        "boolean" => "bool",
        "decimal" | "float" | "double" | "unsignedDecimal" => "float",
        "date" | "dateTime" | "dateTimeTz" | "timestamp" | "timestampTz" | "time" | "timeTz" => {
            "\\Illuminate\\Support\\Carbon"
        }
        "json" | "jsonb" => "array",
        _ => "string",
    }
}

/// PHP type of a cast (`datetime` → Carbon, `array` → array, `OrderStatus::class` → the enum).
fn cast_type(cast: &str) -> String {
    let base = cast.split(':').next().unwrap_or(cast);
    match base {
        "int" | "integer" | "timestamp" => "int".into(),
        "bool" | "boolean" => "bool".into(),
        "float" | "double" | "real" | "decimal" => "float".into(),
        "string" | "encrypted" | "hashed" => "string".into(),
        "array" | "json" | "encrypted:array" => "array".into(),
        "collection" | "encrypted:collection" => "\\Illuminate\\Support\\Collection".into(),
        "object" | "encrypted:object" => "object".into(),
        "date" | "datetime" | "custom_datetime" | "immutable_date" | "immutable_datetime" => {
            if base.starts_with("immutable") {
                "\\Carbon\\CarbonImmutable".into()
            } else {
                "\\Illuminate\\Support\\Carbon".into()
            }
        }
        other if other.contains('\\') || other.chars().next().is_some_and(char::is_uppercase) => {
            other.to_string()
        }
        _ => "mixed".into(),
    }
}

fn is_model(class: &ClassLike, known: &[String]) -> bool {
    class.kind == SymbolKind::Class
        && class.extends.iter().any(|e| {
            matches!(
                e.as_str(),
                "Illuminate\\Database\\Eloquent\\Model"
                    | "Illuminate\\Foundation\\Auth\\User"
                    | "Illuminate\\Database\\Eloquent\\Relations\\Pivot"
                    | "Illuminate\\Database\\Eloquent\\Relations\\MorphPivot"
            ) || known.contains(e)
                || e.ends_with("\\Model")
        })
}

/// All Eloquent models below `app/`.
pub fn list_models(root: &Path) -> Vec<Model> {
    let columns = migration_columns(root);
    let mut parsed: Vec<(String, String, FileIndex)> = files_under(root, "app", ".php")
        .into_iter()
        .filter_map(|p| {
            let src = read(root, &p)?;
            if !src.contains("extends") {
                return None;
            }
            let idx = index_source(&p, &src);
            Some((p, src, idx))
        })
        .collect();
    // Models extending other project models: resolve in a few passes.
    let mut known: Vec<String> = Vec::new();
    for _ in 0..3 {
        for (_, _, idx) in &parsed {
            for c in &idx.classes {
                if !known.contains(&c.fqn) && is_model(c, &known) {
                    known.push(c.fqn.clone());
                }
            }
        }
    }
    parsed.retain(|(_, _, idx)| idx.classes.iter().any(|c| known.contains(&c.fqn)));
    parsed
        .iter()
        .flat_map(|(path, src, idx)| {
            idx.classes
                .iter()
                .filter(|c| known.contains(&c.fqn))
                .map(|c| build_model(path, src, idx, c, &columns))
                .collect::<Vec<_>>()
        })
        .collect()
}

fn property_value(class: &ClassLike, name: &str) -> Option<Value> {
    let m = class.member(SymbolKind::Property, name)?;
    let raw = m.value.as_ref()?;
    let wrapped = format!("<?php return {raw};");
    let tree = php::parse(&wrapped)?;
    php::returned_value(&tree, &wrapped)
}

/// The array returned by a method (e.g. `casts()`).
fn method_return(src: &str, class: &ClassLike, name: &str) -> Option<Value> {
    let m = class.method(name)?;
    let body = src.get(m.range.start..m.range.end)?;
    let wrapped = format!("<?php class X {{ {body} }}");
    let tree = php::parse(&wrapped)?;
    let mut found = None;
    php::walk(tree.root_node(), &mut |n| {
        if found.is_none() && n.kind() == "return_statement" {
            if let Some(e) = n.named_child(0) {
                found = Some(php::value(e, &wrapped));
            }
        }
        found.is_none()
    });
    found
}

fn build_model(
    path: &str,
    src: &str,
    idx: &FileIndex,
    class: &ClassLike,
    columns: &HashMap<String, Vec<Column>>,
) -> Model {
    let table = property_value(class, "table")
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_else(|| plural(&snake(&class.name)));
    let fillable: Vec<String> = property_value(class, "fillable")
        .map(|v| v.strings())
        .unwrap_or_default();
    let mut casts: Vec<(String, String)> = Vec::new();
    for v in [
        property_value(class, "casts"),
        method_return(src, class, "casts"),
    ]
    .into_iter()
    .flatten()
    {
        for e in v.entries() {
            if let Some(k) = &e.key {
                let cast = match &e.value {
                    Value::Str(s) => s.clone(),
                    Value::ClassRef(c) => idx.resolve_class(c, class.range.start),
                    other => other.preview(),
                };
                casts.push((k.clone(), cast));
            }
        }
    }
    let mut attributes: Vec<ModelAttribute> = Vec::new();
    for c in columns.get(&table).into_iter().flatten() {
        attributes.push(ModelAttribute {
            name: c.name.clone(),
            type_name: php_type(&c.column_type).to_string(),
            column: Some(c.column_type.clone()),
            nullable: c.nullable,
            fillable: false,
            cast: None,
            accessor: false,
            location: Some(c.location.clone()),
        });
    }
    let loc_at = |byte: usize| Location::at(path, src, byte);
    fn ensure(attributes: &mut Vec<ModelAttribute>, name: &str, at: Option<Location>) -> usize {
        if let Some(i) = attributes.iter().position(|a| a.name == name) {
            return i;
        }
        attributes.push(ModelAttribute {
            name: name.to_string(),
            type_name: "mixed".into(),
            column: None,
            nullable: false,
            fillable: false,
            cast: None,
            accessor: false,
            location: at,
        });
        attributes.len() - 1
    }
    let fillable_at = class
        .member(SymbolKind::Property, "fillable")
        .map(|m| loc_at(m.name_range.start));
    for f in &fillable {
        let i = ensure(&mut attributes, f, fillable_at.clone());
        attributes[i].fillable = true;
    }
    for (k, cast) in &casts {
        let i = ensure(&mut attributes, k, None);
        attributes[i].type_name = cast_type(cast);
        attributes[i].cast = Some(cast.clone());
    }
    // `@property` docblocks.
    if let Some(doc) = &class.doc {
        for p in &doc.properties {
            let name = p.name.trim_start_matches('$');
            let i = ensure(&mut attributes, name, Some(loc_at(class.name_range.start)));
            if attributes[i].type_name == "mixed"
                || attributes[i].column.is_none() && attributes[i].cast.is_none()
            {
                attributes[i].type_name = p.type_name.clone();
            }
        }
    }
    let mut relations = Vec::new();
    let mut scopes = Vec::new();
    for m in class.methods() {
        // Local scopes: scopeActive($query, ...) or #[Scope] (Laravel 12).
        let is_scope_attr = m
            .attributes
            .iter()
            .any(|a| a.name == "Scope" || a.fqn.ends_with("\\Scope"));
        if let Some(rest) = m
            .name
            .strip_prefix("scope")
            .filter(|r| r.chars().next().is_some_and(char::is_uppercase))
        {
            scopes.push(Scope {
                name: lcfirst(rest),
                params: m.params.iter().skip(1).map(|p| p.render()).collect(),
                location: loc_at(m.name_range.start),
            });
            continue;
        }
        if is_scope_attr {
            scopes.push(Scope {
                name: m.name.clone(),
                params: m.params.iter().skip(1).map(|p| p.render()).collect(),
                location: loc_at(m.name_range.start),
            });
            continue;
        }
        // Accessors: getFullNameAttribute() or `fullName(): Attribute`.
        if let Some(rest) = m
            .name
            .strip_prefix("get")
            .and_then(|r| r.strip_suffix("Attribute"))
            .filter(|r| !r.is_empty())
        {
            let i = ensure(
                &mut attributes,
                &snake(rest),
                Some(loc_at(m.name_range.start)),
            );
            attributes[i].accessor = true;
            if let Some(t) = &m.type_hint {
                attributes[i].type_name = t.clone();
            }
            continue;
        }
        let ret = m.type_hint.as_deref().unwrap_or("");
        if ret.ends_with("Casts\\Attribute") || ret == "Attribute" {
            let i = ensure(
                &mut attributes,
                &snake(&m.name),
                Some(loc_at(m.name_range.start)),
            );
            attributes[i].accessor = true;
            continue;
        }
        if let Some((kind, related)) = relation_of(src, m.range.start, m.range.end) {
            relations.push(Relation {
                name: m.name.clone(),
                kind,
                related: related.map(|r| idx.resolve_class(&r, m.range.start)),
                location: loc_at(m.name_range.start),
            });
        }
    }
    Model {
        class: class.fqn.clone(),
        name: class.name.clone(),
        path: path.to_string(),
        table,
        attributes,
        relations,
        scopes,
        location: loc_at(class.name_range.start),
    }
}

fn lcfirst(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_lowercase().chain(c).collect(),
        None => String::new(),
    }
}

/// `$this->hasMany(Order::class)` inside a method body → (`hasMany`, `Order`).
fn relation_of(src: &str, start: usize, end: usize) -> Option<(String, Option<String>)> {
    let body = src.get(start..end)?;
    for kind in RELATIONS {
        let needle = format!("$this->{kind}(");
        if let Some(i) = body.find(&needle) {
            let rest = body[i + needle.len()..].trim_start();
            let related = rest
                .split_once("::class")
                .map(|(c, _)| c.trim().to_string())
                .filter(|c| {
                    c.chars()
                        .all(|ch| ch.is_alphanumeric() || ch == '_' || ch == '\\')
                })
                .or_else(|| {
                    let q = rest.chars().next().filter(|c| *c == '\'' || *c == '"')?;
                    let end = rest[1..].find(q)?;
                    Some(rest[1..1 + end].to_string())
                });
            return Some(((*kind).to_string(), related));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_names() {
        assert_eq!(plural(&snake("OrderItem")), "order_items");
        assert_eq!(plural(&snake("Category")), "categories");
        assert_eq!(plural(&snake("Address")), "addresses");
        assert_eq!(plural(&snake("Day")), "days");
    }

    #[test]
    fn cast_types() {
        assert_eq!(cast_type("datetime"), "\\Illuminate\\Support\\Carbon");
        assert_eq!(cast_type("decimal:2"), "float");
        assert_eq!(
            cast_type("App\\Models\\OrderStatus"),
            "App\\Models\\OrderStatus"
        );
    }
}
