//! The object tree of a data source, as DataGrip's Database Explorer shows it:
//! data source › database › schemas › tables, views, materialized views, sequences,
//! routines, types › columns, keys, foreign keys, indexes, checks and triggers, plus server
//! objects (roles and users).

use std::collections::BTreeMap;

use forge_sql::Dialect;
use serde::{Deserialize, Serialize};

fn is_false(b: &bool) -> bool {
    !*b
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TableKind {
    #[default]
    Table,
    View,
    /// A materialized view (PostgreSQL, Oracle, ClickHouse).
    #[serde(rename = "matview")]
    MaterializedView,
    /// A MongoDB collection.
    Collection,
}

impl TableKind {
    pub fn is_view(self) -> bool {
        matches!(self, TableKind::View | TableKind::MaterializedView)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ColumnInfo {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    #[serde(default)]
    pub auto_increment: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub comment: String,
    /// A virtual column: defined by the user in the project, computed by `expression`.
    #[serde(default, rename = "virtual", skip_serializing_if = "is_false")]
    pub is_virtual: bool,
    /// Expression of a virtual or generated column.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expression: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ForeignKey {
    pub name: String,
    pub columns: Vec<String>,
    pub ref_schema: String,
    pub ref_table: String,
    pub ref_columns: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub on_update: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub on_delete: String,
    /// A virtual foreign key: a relation the user defined in the project, not in the database.
    #[serde(default, rename = "virtual", skip_serializing_if = "is_false")]
    pub is_virtual: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct IndexInfo {
    pub name: String,
    pub columns: Vec<String>,
    pub unique: bool,
    pub primary: bool,
}

/// A `CHECK` constraint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CheckInfo {
    pub name: String,
    pub expression: String,
}

/// A trigger of a table or view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TriggerInfo {
    pub name: String,
    /// `BEFORE`, `AFTER` or `INSTEAD OF`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub timing: String,
    /// `INSERT`, `UPDATE OR DELETE`, …
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub events: String,
    /// The trigger's source (`CREATE TRIGGER …` or its body) when the catalog keeps it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub definition: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TableInfo {
    pub name: String,
    pub kind: TableKind,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub comment: String,
    pub columns: Vec<ColumnInfo>,
    #[serde(default)]
    pub primary_key: Vec<String>,
    #[serde(default)]
    pub foreign_keys: Vec<ForeignKey>,
    #[serde(default)]
    pub indexes: Vec<IndexInfo>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checks: Vec<CheckInfo>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub triggers: Vec<TriggerInfo>,
    /// Estimated row count when the database keeps one cheaply.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row_estimate: Option<i64>,
    /// The query of a view.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub definition: String,
}

impl TableInfo {
    pub fn column(&self, name: &str) -> Option<&ColumnInfo> {
        self.columns
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(name))
    }
}

/// An argument of a function or procedure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RoutineArg {
    #[serde(default)]
    pub name: String,
    pub data_type: String,
    /// `IN` (the default, left empty), `OUT`, `INOUT` or `VARIADIC`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub mode: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RoutineInfo {
    pub name: String,
    /// `function`, `procedure`, `aggregate` or `package`.
    pub kind: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub returns: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub arguments: Vec<RoutineArg>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub language: String,
    /// The routine's source when the catalog keeps it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub definition: String,
}

impl RoutineInfo {
    /// `(id bigint, OUT total numeric)`.
    pub fn signature(&self) -> String {
        let args: Vec<String> = self
            .arguments
            .iter()
            .map(|a| {
                [a.mode.as_str(), a.name.as_str(), a.data_type.as_str()]
                    .iter()
                    .filter(|s| !s.is_empty())
                    .copied()
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect();
        format!("({})", args.join(", "))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SequenceInfo {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub data_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub increment: Option<i64>,
}

/// A user-defined type: an enum, composite type, domain or other type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TypeInfo {
    pub name: String,
    /// `enum`, `composite`, `domain` or `type`.
    pub kind: String,
    /// Labels of an enum.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,
    /// Attributes of a composite type, the base type of a domain.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub definition: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SchemaInfo {
    pub name: String,
    pub tables: Vec<TableInfo>,
    #[serde(default)]
    pub routines: Vec<RoutineInfo>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sequences: Vec<SequenceInfo>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub types: Vec<TypeInfo>,
}

impl SchemaInfo {
    pub fn table(&self, name: &str) -> Option<&TableInfo> {
        self.tables.iter().find(|t| t.name == name).or_else(|| {
            self.tables
                .iter()
                .find(|t| t.name.eq_ignore_ascii_case(name))
        })
    }

    pub fn table_mut(&mut self, name: &str) -> Option<&mut TableInfo> {
        let i = self
            .tables
            .iter()
            .position(|t| t.name == name)
            .or_else(|| {
                self.tables
                    .iter()
                    .position(|t| t.name.eq_ignore_ascii_case(name))
            })?;
        self.tables.get_mut(i)
    }

    /// Replace (or add, keeping name order) one table: the incremental refresh of a node.
    pub fn merge_table(&mut self, table: TableInfo) {
        match self.tables.iter_mut().find(|t| t.name == table.name) {
            Some(t) => *t = table,
            None => {
                let at = self
                    .tables
                    .iter()
                    .position(|t| t.name > table.name)
                    .unwrap_or(self.tables.len());
                self.tables.insert(at, table);
            }
        }
    }
}

/// One schema of the data source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaName {
    pub name: String,
    /// The connection's current schema.
    pub current: bool,
    /// Passes the data source's schema filter: shown in the tree and introspected.
    #[serde(default = "yes")]
    pub visible: bool,
}

fn yes() -> bool {
    true
}

impl SchemaName {
    pub fn new(name: impl Into<String>, current: bool) -> Self {
        Self {
            name: name.into(),
            current,
            visible: true,
        }
    }
}

/// A server-level role or user.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RoleInfo {
    pub name: String,
    /// `role` or `user`.
    pub kind: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub superuser: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub can_login: bool,
}

/// A problem found while loading the model (a DDL statement that does not parse, a catalog
/// query the user may not run).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ModelWarning {
    /// Project-relative file of a DDL data source.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub file: String,
    /// 1-based line, 0 when unknown.
    #[serde(default)]
    pub line: u32,
    pub message: String,
}

/// Everything the explorer knows about a data source; cached per data source in the user
/// cache dir, so the tree shows without connecting.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DbModel {
    pub dialect: Dialect,
    /// The connected database (catalog) for engines that have one above their schemas
    /// (PostgreSQL, SQL Server); empty otherwise.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub catalog: String,
    /// Other databases on the server (not introspected; shown for orientation).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub catalogs: Vec<String>,
    /// Every schema with its filter result.
    pub schemas: Vec<SchemaName>,
    /// Introspected schemas by name (only visible ones).
    #[serde(default)]
    pub loaded: BTreeMap<String, SchemaInfo>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub roles: Vec<RoleInfo>,
    /// Unix milliseconds of the last refresh of the schema list.
    #[serde(default)]
    pub refreshed_at: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<ModelWarning>,
}

impl DbModel {
    pub fn new(dialect: Dialect) -> Self {
        Self {
            dialect,
            catalog: String::new(),
            catalogs: Vec::new(),
            schemas: Vec::new(),
            loaded: BTreeMap::new(),
            roles: Vec::new(),
            refreshed_at: 0,
            warnings: Vec::new(),
        }
    }

    pub fn current_schema(&self) -> Option<&str> {
        self.schemas
            .iter()
            .find(|s| s.current)
            .map(|s| s.name.as_str())
    }

    pub fn is_visible(&self, schema: &str) -> bool {
        self.schemas.iter().any(|s| s.name == schema && s.visible)
    }

    /// Drop introspected schemas the filter hides now.
    pub fn retain_visible(&mut self) {
        let visible: Vec<String> = self
            .schemas
            .iter()
            .filter(|s| s.visible)
            .map(|s| s.name.clone())
            .collect();
        self.loaded.retain(|k, _| visible.contains(k));
    }

    /// A table of a loaded schema.
    pub fn table(&self, schema: &str, table: &str) -> Option<&TableInfo> {
        self.loaded.get(schema)?.table(table)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_for_the_ui() {
        let t = TableInfo {
            name: "orders".into(),
            kind: TableKind::MaterializedView,
            columns: vec![ColumnInfo {
                name: "total".into(),
                data_type: "numeric".into(),
                is_virtual: true,
                expression: Some("price * qty".into()),
                ..Default::default()
            }],
            ..Default::default()
        };
        let json = serde_json::to_value(&t).unwrap();
        assert_eq!(json["kind"], "matview");
        assert_eq!(json["columns"][0]["virtual"], true);
        assert!(json.get("checks").is_none());
        let back: TableInfo = serde_json::from_value(json).unwrap();
        assert_eq!(back, t);
    }

    #[test]
    fn merges_tables_in_name_order() {
        let mut s = SchemaInfo {
            name: "public".into(),
            tables: vec![
                TableInfo {
                    name: "a".into(),
                    ..Default::default()
                },
                TableInfo {
                    name: "c".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        s.merge_table(TableInfo {
            name: "b".into(),
            ..Default::default()
        });
        s.merge_table(TableInfo {
            name: "c".into(),
            comment: "new".into(),
            ..Default::default()
        });
        let names: Vec<_> = s.tables.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["a", "b", "c"]);
        assert_eq!(s.table("C").unwrap().comment, "new");
    }

    #[test]
    fn routine_signature() {
        let r = RoutineInfo {
            name: "f".into(),
            kind: "function".into(),
            arguments: vec![
                RoutineArg {
                    name: "id".into(),
                    data_type: "bigint".into(),
                    mode: String::new(),
                },
                RoutineArg {
                    name: "total".into(),
                    data_type: "numeric".into(),
                    mode: "OUT".into(),
                },
            ],
            ..Default::default()
        };
        assert_eq!(r.signature(), "(id bigint, OUT total numeric)");
    }
}
