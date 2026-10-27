//! DDL data sources: the model of SQL DDL files in the project (`CREATE TABLE …` scripts or
//! SQL migrations), parsed with `sqlparser` in the file's dialect. Statements are applied
//! in order (`CREATE`, `ALTER TABLE`, `CREATE INDEX`, `DROP`, `COMMENT ON`), so a folder of
//! migrations gives the final schema. Statements the parser does not know are skipped with
//! a warning; routines and triggers it cannot parse are still picked up by their header.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use forge_sql::{split_statements, Dialect};
use regex_lite::Regex;
use sqlparser::ast::{
    AlterTableOperation, ColumnDef, ColumnOption, CommentObject, CreateTableOptions, Expr,
    ObjectName, ObjectNamePart, ObjectType, RenameTableNameKind, SelectItem, SetExpr, SqlOption,
    Statement, TableConstraint, UserDefinedTypeRepresentation,
};
use sqlparser::dialect::{
    ClickHouseDialect, Dialect as SqlDialect, GenericDialect, MsSqlDialect, MySqlDialect,
    PostgreSqlDialect, SQLiteDialect,
};
use sqlparser::parser::Parser;

use crate::model::{
    CheckInfo, ColumnInfo, ForeignKey, IndexInfo, ModelWarning, RoutineArg, RoutineInfo,
    SchemaInfo, SequenceInfo, TableInfo, TableKind, TriggerInfo, TypeInfo,
};

/// One DDL file: its project-relative path and text.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DdlFile {
    pub path: String,
    pub text: String,
}

/// The parsed model of a DDL data source.
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedDdl {
    pub schemas: Vec<SchemaInfo>,
    pub warnings: Vec<ModelWarning>,
}

/// The schema unqualified names belong to.
pub fn default_schema(d: Dialect) -> &'static str {
    match d {
        Dialect::Sqlite => "main",
        Dialect::SqlServer => "dbo",
        Dialect::ClickHouse => "default",
        Dialect::Oracle => "PUBLIC",
        _ => "public",
    }
}

fn parser_dialect(d: Dialect) -> Box<dyn SqlDialect> {
    match d {
        Dialect::Postgres => Box::new(PostgreSqlDialect {}),
        Dialect::MySql => Box::new(MySqlDialect {}),
        Dialect::Sqlite => Box::new(SQLiteDialect {}),
        Dialect::SqlServer => Box::new(MsSqlDialect {}),
        Dialect::ClickHouse => Box::new(ClickHouseDialect {}),
        _ => Box::new(GenericDialect {}),
    }
}

/// A statement of a file with its 1-based line.
struct Piece {
    text: String,
    line: u32,
}

fn line_of(text: &str, offset: usize) -> u32 {
    text[..offset.min(text.len())]
        .bytes()
        .filter(|b| *b == b'\n')
        .count() as u32
        + 1
}

/// Statements of a file; MySQL scripts may switch the delimiter with `DELIMITER $$`.
fn pieces(text: &str, d: Dialect) -> Vec<Piece> {
    let mut out = Vec::new();
    let mut delimiter = ";".to_string();
    let mut chunk_start = 0;
    let mut offset = 0;
    let flush = |out: &mut Vec<Piece>, from: usize, to: usize, delim: &str| {
        let chunk = &text[from..to];
        if delim == ";" {
            for s in split_statements(chunk, d) {
                out.push(Piece {
                    line: line_of(text, from + s.start),
                    text: s.text,
                });
            }
        } else {
            let mut at = 0;
            for part in chunk.split(delim) {
                let trimmed = part.trim();
                if !trimmed.is_empty() {
                    let lead = part.len() - part.trim_start().len();
                    out.push(Piece {
                        line: line_of(text, from + at + lead),
                        text: trimmed.to_string(),
                    });
                }
                at += part.len() + delim.len();
            }
        }
    };
    for line in text.split_inclusive('\n') {
        let t = line.trim();
        if t.len() > 10 && t[..10].eq_ignore_ascii_case("delimiter ") {
            flush(&mut out, chunk_start, offset, &delimiter);
            delimiter = t[10..].trim().to_string();
            chunk_start = offset + line.len();
        }
        offset += line.len();
    }
    flush(&mut out, chunk_start, text.len(), &delimiter);
    out
}

fn ident_parts(name: &ObjectName) -> Vec<String> {
    name.0
        .iter()
        .map(|p| match p {
            ObjectNamePart::Identifier(i) => i.value.clone(),
            other => other.to_string(),
        })
        .collect()
}

fn unquote(s: &str) -> String {
    let t = s.trim();
    let b = t.as_bytes();
    if b.len() >= 2
        && matches!(
            (b[0], b[b.len() - 1]),
            (b'"', b'"') | (b'`', b'`') | (b'[', b']')
        )
    {
        t[1..t.len() - 1].to_string()
    } else {
        t.to_string()
    }
}

/// Builds the model statement by statement.
struct Builder {
    dialect: Dialect,
    default_schema: String,
    schemas: BTreeMap<String, SchemaInfo>,
    /// Schema names as first written (keys are lowercased).
    order: Vec<String>,
}

impl Builder {
    fn schema_key(&mut self, name: &str) -> String {
        let key = name.to_lowercase();
        if !self.schemas.contains_key(&key) {
            self.order.push(key.clone());
            self.schemas.insert(
                key.clone(),
                SchemaInfo {
                    name: name.to_string(),
                    ..Default::default()
                },
            );
        }
        key
    }

    /// (schema key, object name) of a possibly qualified name.
    fn resolve(&mut self, name: &ObjectName) -> (String, String) {
        let parts = ident_parts(name);
        let (schema, obj) = match parts.as_slice() {
            [] => (self.default_schema.clone(), String::new()),
            [one] => (self.default_schema.clone(), one.clone()),
            [.., s, o] => (s.clone(), o.clone()),
        };
        (self.schema_key(&schema), obj)
    }

    fn table_mut(&mut self, name: &ObjectName) -> Option<&mut TableInfo> {
        let (key, t) = self.resolve(name);
        self.schemas.get_mut(&key)?.table_mut(&t)
    }

    fn column(&self, table: &str, c: &ColumnDef, out: &mut TableInfo) {
        let data_type = c.data_type.to_string();
        let upper = data_type.to_ascii_uppercase();
        let mut col = ColumnInfo {
            name: c.name.value.clone(),
            nullable: true,
            auto_increment: upper.ends_with("SERIAL"),
            data_type,
            ..Default::default()
        };
        for o in &c.options {
            match &o.option {
                ColumnOption::NotNull => col.nullable = false,
                ColumnOption::Null => col.nullable = true,
                ColumnOption::Default(e) => col.default = Some(e.to_string()),
                ColumnOption::Comment(s) => col.comment = s.clone(),
                ColumnOption::Identity(_) => col.auto_increment = true,
                ColumnOption::Generated {
                    generation_expr, ..
                } => match generation_expr {
                    Some(e) => col.expression = Some(e.to_string()),
                    None => col.auto_increment = true,
                },
                ColumnOption::Materialized(e) | ColumnOption::Alias(e) => {
                    col.expression = Some(e.to_string())
                }
                ColumnOption::DialectSpecific(tokens) => {
                    let text: String = tokens.iter().map(|t| t.to_string()).collect();
                    let text = text.to_ascii_uppercase();
                    if text.contains("AUTO_INCREMENT") || text.contains("AUTOINCREMENT") {
                        col.auto_increment = true;
                    }
                }
                ColumnOption::Unique { is_primary, .. } => {
                    let name = o.name.as_ref().map(|n| n.value.clone());
                    if *is_primary {
                        col.nullable = false;
                        out.primary_key = vec![col.name.clone()];
                        out.indexes.push(IndexInfo {
                            name: name.unwrap_or_else(|| format!("{table}_pkey")),
                            columns: vec![col.name.clone()],
                            unique: true,
                            primary: true,
                        });
                    } else {
                        out.indexes.push(IndexInfo {
                            name: name.unwrap_or_else(|| format!("{table}_{}_key", col.name)),
                            columns: vec![col.name.clone()],
                            unique: true,
                            primary: false,
                        });
                    }
                }
                ColumnOption::ForeignKey {
                    foreign_table,
                    referred_columns,
                    on_delete,
                    on_update,
                    ..
                } => {
                    let parts = ident_parts(foreign_table);
                    let (rs, rt) = split_ref(&parts, &self.default_schema);
                    out.foreign_keys.push(ForeignKey {
                        name: o
                            .name
                            .as_ref()
                            .map(|n| n.value.clone())
                            .unwrap_or_else(|| format!("{table}_{}_fkey", col.name)),
                        columns: vec![col.name.clone()],
                        ref_schema: rs,
                        ref_table: rt,
                        ref_columns: if referred_columns.is_empty() {
                            vec!["id".into()]
                        } else {
                            referred_columns.iter().map(|i| i.value.clone()).collect()
                        },
                        on_update: on_update.map(|a| a.to_string()).unwrap_or_default(),
                        on_delete: on_delete.map(|a| a.to_string()).unwrap_or_default(),
                        is_virtual: false,
                    });
                }
                ColumnOption::Check(e) => out.checks.push(CheckInfo {
                    name: o
                        .name
                        .as_ref()
                        .map(|n| n.value.clone())
                        .unwrap_or_else(|| format!("{table}_{}_check", col.name)),
                    expression: e.to_string(),
                }),
                _ => {}
            }
        }
        if out.primary_key.contains(&col.name) {
            col.nullable = false;
        }
        out.columns
            .retain(|x| !x.name.eq_ignore_ascii_case(&col.name));
        out.columns.push(col);
    }

    fn constraint(&self, table: &str, c: &TableConstraint, out: &mut TableInfo) {
        let idx_cols = |cols: &[sqlparser::ast::IndexColumn]| -> Vec<String> {
            cols.iter()
                .map(|c| unquote(&c.column.expr.to_string()))
                .collect()
        };
        match c {
            TableConstraint::PrimaryKey { name, columns, .. } => {
                let cols = idx_cols(columns);
                for col in out.columns.iter_mut().filter(|x| cols.contains(&x.name)) {
                    col.nullable = false;
                }
                out.indexes.retain(|i| !i.primary);
                out.indexes.push(IndexInfo {
                    name: name
                        .as_ref()
                        .map(|n| n.value.clone())
                        .unwrap_or_else(|| format!("{table}_pkey")),
                    columns: cols.clone(),
                    unique: true,
                    primary: true,
                });
                out.primary_key = cols;
            }
            TableConstraint::Unique {
                name,
                index_name,
                columns,
                ..
            } => {
                let cols = idx_cols(columns);
                out.indexes.push(IndexInfo {
                    name: name
                        .as_ref()
                        .or(index_name.as_ref())
                        .map(|n| n.value.clone())
                        .unwrap_or_else(|| format!("{table}_{}_key", cols.join("_"))),
                    columns: cols,
                    unique: true,
                    primary: false,
                });
            }
            TableConstraint::ForeignKey {
                name,
                columns,
                foreign_table,
                referred_columns,
                on_delete,
                on_update,
                ..
            } => {
                let cols: Vec<String> = columns.iter().map(|i| i.value.clone()).collect();
                let parts = ident_parts(foreign_table);
                let (rs, rt) = split_ref(&parts, &self.default_schema);
                out.foreign_keys.push(ForeignKey {
                    name: name
                        .as_ref()
                        .map(|n| n.value.clone())
                        .unwrap_or_else(|| format!("{table}_{}_fkey", cols.join("_"))),
                    columns: cols,
                    ref_schema: rs,
                    ref_table: rt,
                    ref_columns: referred_columns.iter().map(|i| i.value.clone()).collect(),
                    on_update: on_update.map(|a| a.to_string()).unwrap_or_default(),
                    on_delete: on_delete.map(|a| a.to_string()).unwrap_or_default(),
                    is_virtual: false,
                });
            }
            TableConstraint::Check { name, expr, .. } => out.checks.push(CheckInfo {
                name: name
                    .as_ref()
                    .map(|n| n.value.clone())
                    .unwrap_or_else(|| format!("{table}_check{}", out.checks.len() + 1)),
                expression: expr.to_string(),
            }),
            TableConstraint::Index { name, columns, .. } => {
                let cols = idx_cols(columns);
                out.indexes.push(IndexInfo {
                    name: name
                        .as_ref()
                        .map(|n| n.value.clone())
                        .unwrap_or_else(|| format!("{table}_{}_index", cols.join("_"))),
                    columns: cols,
                    unique: false,
                    primary: false,
                });
            }
            _ => {}
        }
    }

    fn put_table(&mut self, key: &str, table: TableInfo) {
        if let Some(s) = self.schemas.get_mut(key) {
            s.tables
                .retain(|t| !t.name.eq_ignore_ascii_case(&table.name));
            s.tables.push(table);
        }
    }

    /// Column names (and types) of a view's query, best effort.
    fn view_columns(&mut self, query: &sqlparser::ast::Query) -> Vec<ColumnInfo> {
        let SetExpr::Select(select) = query.body.as_ref() else {
            return Vec::new();
        };
        let from: Vec<TableInfo> = select
            .from
            .iter()
            .filter_map(|f| match &f.relation {
                sqlparser::ast::TableFactor::Table { name, .. } => {
                    let (key, t) = self.resolve(name);
                    self.schemas.get(&key)?.table(&t).cloned()
                }
                _ => None,
            })
            .collect();
        let find = |col: &str| {
            from.iter()
                .find_map(|t| t.column(col))
                .map(|c| c.data_type.clone())
                .unwrap_or_default()
        };
        let mut out = Vec::new();
        for item in &select.projection {
            match item {
                SelectItem::UnnamedExpr(e) => {
                    let name = match e {
                        Expr::Identifier(i) => i.value.clone(),
                        Expr::CompoundIdentifier(parts) => {
                            parts.last().map(|i| i.value.clone()).unwrap_or_default()
                        }
                        other => other.to_string(),
                    };
                    out.push(ColumnInfo {
                        data_type: find(&name),
                        name,
                        nullable: true,
                        ..Default::default()
                    });
                }
                SelectItem::ExprWithAlias { expr, alias } => {
                    let source = match expr {
                        Expr::Identifier(i) => find(&i.value),
                        Expr::CompoundIdentifier(parts) => {
                            parts.last().map(|i| find(&i.value)).unwrap_or_default()
                        }
                        _ => String::new(),
                    };
                    out.push(ColumnInfo {
                        name: alias.value.clone(),
                        data_type: source,
                        nullable: true,
                        ..Default::default()
                    });
                }
                SelectItem::Wildcard(_) | SelectItem::QualifiedWildcard(..) => {
                    for t in &from {
                        out.extend(t.columns.iter().filter(|c| !c.is_virtual).cloned());
                    }
                }
            }
        }
        out
    }

    fn apply(&mut self, stmt: &Statement, source: &str) -> Result<(), String> {
        match stmt {
            Statement::CreateTable(ct) => {
                let (key, name) = self.resolve(&ct.name);
                let mut t = TableInfo {
                    name: name.clone(),
                    kind: TableKind::Table,
                    ..Default::default()
                };
                for c in &ct.columns {
                    self.column(&name, c, &mut t);
                }
                for c in &ct.constraints {
                    self.constraint(&name, c, &mut t);
                }
                if let Some(c) = &ct.comment {
                    t.comment = c.to_string();
                }
                if let CreateTableOptions::Plain(opts)
                | CreateTableOptions::Options(opts)
                | CreateTableOptions::With(opts) = &ct.table_options
                {
                    for o in opts {
                        if let SqlOption::Comment(c) = o {
                            t.comment = c.to_string();
                        }
                    }
                }
                if t.columns.is_empty() {
                    if let Some(q) = &ct.query {
                        t.columns = self.view_columns(q);
                    }
                }
                self.put_table(&key, t);
            }
            Statement::CreateView {
                materialized,
                name,
                columns,
                query,
                ..
            } => {
                let (key, view) = self.resolve(name);
                let mut cols = self.view_columns(query);
                if !columns.is_empty() {
                    cols = columns
                        .iter()
                        .enumerate()
                        .map(|(i, c)| ColumnInfo {
                            name: c.name.value.clone(),
                            data_type: c
                                .data_type
                                .as_ref()
                                .map(|t| t.to_string())
                                .or_else(|| cols.get(i).map(|x| x.data_type.clone()))
                                .unwrap_or_default(),
                            nullable: true,
                            ..Default::default()
                        })
                        .collect();
                }
                self.put_table(
                    &key,
                    TableInfo {
                        name: view,
                        kind: if *materialized {
                            TableKind::MaterializedView
                        } else {
                            TableKind::View
                        },
                        columns: cols,
                        definition: query.to_string(),
                        ..Default::default()
                    },
                );
            }
            Statement::CreateIndex(ci) => {
                let cols: Vec<String> = ci
                    .columns
                    .iter()
                    .map(|c| unquote(&c.column.expr.to_string()))
                    .collect();
                let iname = ci
                    .name
                    .as_ref()
                    .and_then(|n| ident_parts(n).last().cloned());
                let Some(t) = self.table_mut(&ci.table_name) else {
                    return Err(format!("CREATE INDEX on unknown table {}", ci.table_name));
                };
                let iname = iname.unwrap_or_else(|| format!("{}_{}_index", t.name, cols.join("_")));
                t.indexes.retain(|i| i.name != iname);
                t.indexes.push(IndexInfo {
                    name: iname,
                    columns: cols,
                    unique: ci.unique,
                    primary: false,
                });
            }
            Statement::AlterTable {
                name, operations, ..
            } => {
                let (key, tname) = self.resolve(name);
                let Some(mut t) = self
                    .schemas
                    .get(&key)
                    .and_then(|s| s.table(&tname))
                    .cloned()
                else {
                    return Err(format!("ALTER TABLE of unknown table {name}"));
                };
                let mut renamed: Option<String> = None;
                for op in operations {
                    match op {
                        AlterTableOperation::AddColumn { column_def, .. } => {
                            let tn = t.name.clone();
                            self.column(&tn, column_def, &mut t);
                        }
                        AlterTableOperation::AddConstraint { constraint, .. } => {
                            let tn = t.name.clone();
                            self.constraint(&tn, constraint, &mut t);
                        }
                        AlterTableOperation::DropColumn { column_names, .. } => {
                            for c in column_names {
                                t.columns.retain(|x| !x.name.eq_ignore_ascii_case(&c.value));
                            }
                        }
                        AlterTableOperation::DropConstraint { name, .. }
                        | AlterTableOperation::DropForeignKey { name, .. } => {
                            let n = &name.value;
                            t.foreign_keys.retain(|f| &f.name != n);
                            t.checks.retain(|c| &c.name != n);
                            if t.indexes.iter().any(|i| &i.name == n && i.primary) {
                                t.primary_key.clear();
                            }
                            t.indexes.retain(|i| &i.name != n);
                        }
                        AlterTableOperation::DropIndex { name } => {
                            t.indexes.retain(|i| i.name != name.value);
                        }
                        AlterTableOperation::DropPrimaryKey { .. } => {
                            t.primary_key.clear();
                            t.indexes.retain(|i| !i.primary);
                        }
                        AlterTableOperation::RenameColumn {
                            old_column_name,
                            new_column_name,
                        } => {
                            let (o, n) = (&old_column_name.value, &new_column_name.value);
                            for c in t.columns.iter_mut().filter(|c| &c.name == o) {
                                c.name = n.clone();
                            }
                            for list in t
                                .indexes
                                .iter_mut()
                                .map(|i| &mut i.columns)
                                .chain(t.foreign_keys.iter_mut().map(|f| &mut f.columns))
                                .chain(std::iter::once(&mut t.primary_key))
                            {
                                for c in list.iter_mut().filter(|c| *c == o) {
                                    *c = n.clone();
                                }
                            }
                        }
                        AlterTableOperation::RenameTable { table_name } => {
                            let n = match table_name {
                                RenameTableNameKind::As(n) | RenameTableNameKind::To(n) => n,
                            };
                            renamed = ident_parts(n).last().cloned();
                        }
                        _ => {}
                    }
                }
                if let Some(s) = self.schemas.get_mut(&key) {
                    s.tables.retain(|x| x.name != t.name);
                }
                if let Some(n) = renamed {
                    t.name = n;
                }
                self.put_table(&key, t);
            }
            Statement::Drop {
                object_type, names, ..
            } => {
                for n in names {
                    let (key, obj) = self.resolve(n);
                    let Some(s) = self.schemas.get_mut(&key) else {
                        continue;
                    };
                    match object_type {
                        ObjectType::Table | ObjectType::View | ObjectType::MaterializedView => {
                            s.tables.retain(|t| !t.name.eq_ignore_ascii_case(&obj))
                        }
                        ObjectType::Index => {
                            for t in &mut s.tables {
                                t.indexes.retain(|i| i.name != obj);
                            }
                        }
                        ObjectType::Sequence => s.sequences.retain(|q| q.name != obj),
                        ObjectType::Type => s.types.retain(|t| t.name != obj),
                        _ => {}
                    }
                }
            }
            Statement::CreateSequence {
                name,
                data_type,
                sequence_options,
                ..
            } => {
                let (key, seq) = self.resolve(name);
                let mut q = SequenceInfo {
                    name: seq,
                    data_type: data_type
                        .as_ref()
                        .map(|t| t.to_string())
                        .unwrap_or_default(),
                    ..Default::default()
                };
                for o in sequence_options {
                    match o {
                        sqlparser::ast::SequenceOptions::StartWith(e, _) => {
                            q.start = e.to_string().parse().ok()
                        }
                        sqlparser::ast::SequenceOptions::IncrementBy(e, _) => {
                            q.increment = e.to_string().parse().ok()
                        }
                        _ => {}
                    }
                }
                if let Some(s) = self.schemas.get_mut(&key) {
                    s.sequences.retain(|x| x.name != q.name);
                    s.sequences.push(q);
                }
            }
            Statement::CreateType {
                name,
                representation,
            } => {
                let (key, ty) = self.resolve(name);
                let t = match representation {
                    UserDefinedTypeRepresentation::Enum { labels } => TypeInfo {
                        name: ty,
                        kind: "enum".into(),
                        labels: labels.iter().map(|l| l.value.clone()).collect(),
                        definition: String::new(),
                    },
                    UserDefinedTypeRepresentation::Composite { attributes } => TypeInfo {
                        name: ty,
                        kind: "composite".into(),
                        labels: Vec::new(),
                        definition: attributes
                            .iter()
                            .map(|a| format!("{} {}", a.name.value, a.data_type))
                            .collect::<Vec<_>>()
                            .join(", "),
                    },
                };
                if let Some(s) = self.schemas.get_mut(&key) {
                    s.types.retain(|x| x.name != t.name);
                    s.types.push(t);
                }
            }
            Statement::CreateDomain(cd) => {
                let (key, ty) = self.resolve(&cd.name);
                if let Some(s) = self.schemas.get_mut(&key) {
                    s.types.retain(|x| x.name != ty);
                    s.types.push(TypeInfo {
                        name: ty,
                        kind: "domain".into(),
                        labels: Vec::new(),
                        definition: cd.data_type.to_string(),
                    });
                }
            }
            Statement::CreateFunction(f) => {
                let (key, name) = self.resolve(&f.name);
                let r = RoutineInfo {
                    name,
                    kind: "function".into(),
                    returns: f
                        .return_type
                        .as_ref()
                        .map(|t| t.to_string())
                        .unwrap_or_default(),
                    arguments: f
                        .args
                        .iter()
                        .flatten()
                        .map(|a| RoutineArg {
                            name: a.name.as_ref().map(|n| n.value.clone()).unwrap_or_default(),
                            data_type: a.data_type.to_string(),
                            mode: a.mode.as_ref().map(|m| m.to_string()).unwrap_or_default(),
                        })
                        .collect(),
                    language: f
                        .language
                        .as_ref()
                        .map(|l| l.value.clone())
                        .unwrap_or_default(),
                    definition: source.trim().to_string(),
                };
                self.put_routine(&key, r);
            }
            Statement::CreateProcedure { name, params, .. } => {
                let (key, name) = self.resolve(name);
                let r = RoutineInfo {
                    name,
                    kind: "procedure".into(),
                    arguments: params
                        .iter()
                        .flatten()
                        .map(|p| RoutineArg {
                            name: p.name.value.clone(),
                            data_type: p.data_type.to_string(),
                            mode: p.mode.as_ref().map(|m| m.to_string()).unwrap_or_default(),
                        })
                        .collect(),
                    definition: source.trim().to_string(),
                    ..Default::default()
                };
                self.put_routine(&key, r);
            }
            Statement::CreateTrigger(tr) => {
                let name = ident_parts(&tr.name).last().cloned().unwrap_or_default();
                let trigger = TriggerInfo {
                    name,
                    timing: tr.period.to_string(),
                    events: tr
                        .events
                        .iter()
                        .map(|e| e.to_string())
                        .collect::<Vec<_>>()
                        .join(" OR "),
                    definition: source.trim().to_string(),
                };
                let Some(t) = self.table_mut(&tr.table_name) else {
                    return Err(format!("trigger on unknown table {}", tr.table_name));
                };
                t.triggers.retain(|x| x.name != trigger.name);
                t.triggers.push(trigger);
            }
            Statement::Comment {
                object_type,
                object_name,
                comment,
                ..
            } => {
                let text = comment.clone().unwrap_or_default();
                match object_type {
                    CommentObject::Table => {
                        if let Some(t) = self.table_mut(object_name) {
                            t.comment = text;
                        }
                    }
                    CommentObject::Column => {
                        let mut parts = ident_parts(object_name);
                        let col = parts.pop().unwrap_or_default();
                        let table: ObjectName = ObjectName(
                            parts
                                .into_iter()
                                .map(|p| ObjectNamePart::Identifier(sqlparser::ast::Ident::new(p)))
                                .collect(),
                        );
                        if let Some(t) = self.table_mut(&table) {
                            for c in t.columns.iter_mut().filter(|c| c.name == col) {
                                c.comment = text.clone();
                            }
                        }
                    }
                    _ => {}
                }
            }
            Statement::CreateSchema { schema_name, .. } => {
                if let sqlparser::ast::SchemaName::Simple(n)
                | sqlparser::ast::SchemaName::NamedAuthorization(n, _) = schema_name
                {
                    if let Some(s) = ident_parts(n).last() {
                        self.schema_key(s);
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn put_routine(&mut self, key: &str, r: RoutineInfo) {
        if let Some(s) = self.schemas.get_mut(key) {
            s.routines
                .retain(|x| !(x.name == r.name && x.arguments == r.arguments));
            s.routines.push(r);
        }
    }

    /// Sequences whose options come in an order the parser does not take.
    fn fallback_sequence(&mut self, text: &str) -> bool {
        static SEQ: OnceLock<Regex> = OnceLock::new();
        let re = SEQ.get_or_init(|| {
            Regex::new(r#"(?is)^\s*create\s+(?:temp(?:orary)?\s+)?sequence\s+(?:if\s+not\s+exists\s+)?([\w$."`\[\]]+)"#)
                .expect("valid regex")
        });
        let Some(m) = re.captures(text) else {
            return false;
        };
        let parts: Vec<String> = m[1].split('.').map(unquote).collect();
        let (schema, name) = split_ref(&parts, &self.default_schema);
        let key = self.schema_key(&schema);
        let num = |pattern: &str| {
            Regex::new(pattern)
                .ok()
                .and_then(|re| re.captures(text).and_then(|c| c[1].parse().ok()))
        };
        let q = SequenceInfo {
            name,
            data_type: Regex::new(r"(?i)\bas\s+(\w+)")
                .ok()
                .and_then(|re| re.captures(text).map(|c| c[1].to_string()))
                .unwrap_or_default(),
            start: num(r"(?i)\bstart\s+(?:with\s+)?(-?\d+)"),
            increment: num(r"(?i)\bincrement\s+(?:by\s+)?(-?\d+)"),
        };
        if let Some(s) = self.schemas.get_mut(&key) {
            s.sequences.retain(|x| x.name != q.name);
            s.sequences.push(q);
        }
        true
    }

    /// Routines and triggers the parser does not understand, by their header.
    fn fallback(&mut self, text: &str) -> bool {
        if self.fallback_sequence(text) {
            return true;
        }
        static HEAD: OnceLock<Regex> = OnceLock::new();
        let head = HEAD.get_or_init(|| {
            Regex::new(r#"(?is)^\s*create\s+(?:or\s+(?:replace|alter)\s+)?(?:definer\s*=\s*\S+\s+)?(function|procedure|trigger)\s+(?:if\s+not\s+exists\s+)?([\w$."`\[\]]+)"#)
                .expect("valid regex")
        });
        let Some(m) = head.captures(text) else {
            return false;
        };
        let kind = m[1].to_lowercase();
        let parts: Vec<String> = m[2].split('.').map(unquote).collect();
        let (schema, name) = split_ref(&parts, &self.default_schema);
        let key = self.schema_key(&schema);
        if kind == "trigger" {
            let Some(t) = trigger_regex().captures(text) else {
                return false;
            };
            let tparts: Vec<String> = t[3].split('.').map(unquote).collect();
            let (ts, tn) = split_ref(&tparts, &schema);
            let tkey = self.schema_key(&ts);
            let trigger = TriggerInfo {
                name,
                timing: t[1].to_uppercase(),
                events: t[2]
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .to_uppercase(),
                definition: text.trim().to_string(),
            };
            let Some(table) = self.schemas.get_mut(&tkey).and_then(|s| s.table_mut(&tn)) else {
                return false;
            };
            table.triggers.retain(|x| x.name != trigger.name);
            table.triggers.push(trigger);
            return true;
        }
        let returns = Regex::new(r"(?is)\breturns\s+([\w\s(),\[\]]+?)(?:\s+(?:as|language|begin|deterministic|immutable|stable|volatile|reads|modifies|no|contains|security|with)\b|\s*\$|$)")
            .ok()
            .and_then(|re| re.captures(text).map(|c| c[1].trim().to_string()))
            .unwrap_or_default();
        self.put_routine(
            &key,
            RoutineInfo {
                name,
                kind,
                returns,
                definition: text.trim().to_string(),
                ..Default::default()
            },
        );
        true
    }

    fn finish(mut self) -> Vec<SchemaInfo> {
        self.order
            .iter()
            .filter_map(|k| self.schemas.remove(k))
            .map(|mut s| {
                s.tables.sort_by(|a, b| a.name.cmp(&b.name));
                s.routines.sort_by(|a, b| a.name.cmp(&b.name));
                s.sequences.sort_by(|a, b| a.name.cmp(&b.name));
                s.types.sort_by(|a, b| a.name.cmp(&b.name));
                s
            })
            .collect()
    }
}

fn trigger_regex() -> &'static Regex {
    static TRIGGER: OnceLock<Regex> = OnceLock::new();
    TRIGGER.get_or_init(|| {
        Regex::new(r#"(?is)\b(before|after|instead\s+of)\s+((?:insert|update|delete|truncate)(?:\s+of\s+[\w\s,]+?)?(?:\s+or\s+(?:insert|update|delete|truncate))*)\s+on\s+([\w$."`\[\]]+)"#)
            .expect("valid regex")
    })
}

/// Timing (`BEFORE`) and events (`INSERT OR UPDATE`) of a `CREATE TRIGGER` statement.
pub fn trigger_head(sql: &str) -> (String, String) {
    match trigger_regex().captures(sql) {
        Some(t) => (
            t[1].split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .to_uppercase(),
            t[2].split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .to_uppercase(),
        ),
        None => (String::new(), String::new()),
    }
}

/// (schema, name) of `schema.name` or `name` (in `default`).
fn split_ref(parts: &[String], default: &str) -> (String, String) {
    match parts {
        [] => (default.into(), String::new()),
        [one] => (default.into(), one.clone()),
        [.., s, o] => (s.clone(), o.clone()),
    }
}

fn is_ddl(text: &str) -> bool {
    matches!(
        forge_sql::leading_keyword(text).as_str(),
        "CREATE" | "ALTER" | "DROP" | "COMMENT"
    )
}

/// Parse DDL files (in order) into schemas; unqualified names go to `default_schema`
/// (`""`: the dialect's default).
pub fn parse(d: Dialect, default_schema: &str, files: &[DdlFile]) -> ParsedDdl {
    let default_schema = if default_schema.is_empty() {
        self::default_schema(d).to_string()
    } else {
        default_schema.to_string()
    };
    let mut b = Builder {
        dialect: d,
        default_schema: default_schema.clone(),
        schemas: BTreeMap::new(),
        order: Vec::new(),
    };
    b.schema_key(&default_schema);
    let mut warnings = Vec::new();
    let pd = parser_dialect(b.dialect);
    for f in files {
        for p in pieces(&f.text, d) {
            if !is_ddl(&p.text) {
                continue;
            }
            let parsed = Parser::parse_sql(pd.as_ref(), &p.text)
                .or_else(|_| Parser::parse_sql(&GenericDialect {}, &p.text));
            match parsed {
                Ok(stmts) => {
                    for s in &stmts {
                        if let Err(message) = b.apply(s, &p.text) {
                            warnings.push(ModelWarning {
                                file: f.path.clone(),
                                line: p.line,
                                message,
                            });
                        }
                    }
                }
                Err(e) => {
                    if !b.fallback(&p.text) {
                        warnings.push(ModelWarning {
                            file: f.path.clone(),
                            line: p.line,
                            message: format!("statement skipped: {e}"),
                        });
                    }
                }
            }
        }
    }
    ParsedDdl {
        schemas: b.finish(),
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, text: &str) -> DdlFile {
        DdlFile {
            path: path.into(),
            text: text.into(),
        }
    }

    #[test]
    fn parses_postgres_schema_script() {
        let sql = r#"
-- Shop schema
CREATE SCHEMA billing;
CREATE TYPE order_status AS ENUM ('new', 'paid', 'shipped');
CREATE SEQUENCE invoice_no START WITH 1000 INCREMENT BY 1;
CREATE TABLE users (
    id bigserial PRIMARY KEY,
    email varchar(255) NOT NULL UNIQUE,
    name text,
    created_at timestamptz DEFAULT now()
);
CREATE TABLE orders (
    id bigint GENERATED ALWAYS AS IDENTITY,
    user_id bigint NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    status order_status NOT NULL DEFAULT 'new',
    total numeric(10, 2) CHECK (total >= 0),
    CONSTRAINT orders_pkey PRIMARY KEY (id)
);
CREATE INDEX orders_user_idx ON orders (user_id);
COMMENT ON TABLE orders IS 'Customer orders';
COMMENT ON COLUMN orders.total IS 'Gross amount';
CREATE TABLE billing.invoices (
    id bigint PRIMARY KEY,
    order_id bigint,
    CONSTRAINT invoices_order_fk FOREIGN KEY (order_id) REFERENCES public.orders (id)
);
CREATE VIEW order_totals AS SELECT o.user_id, sum(o.total) AS spent FROM orders o GROUP BY o.user_id;
CREATE MATERIALIZED VIEW big_orders AS SELECT * FROM orders WHERE total > 100;
CREATE FUNCTION order_total(o_id bigint) RETURNS numeric LANGUAGE sql AS $$ SELECT total FROM orders WHERE id = o_id $$;
CREATE TRIGGER orders_touch BEFORE UPDATE ON orders FOR EACH ROW EXECUTE FUNCTION touch();
INSERT INTO users (email) VALUES ('a@b.c');
"#;
        let p = parse(Dialect::Postgres, "", &[file("db/schema.sql", sql)]);
        assert!(p.warnings.is_empty(), "{:?}", p.warnings);
        let names: Vec<_> = p.schemas.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["public", "billing"]);
        let public = &p.schemas[0];
        let users = public.table("users").unwrap();
        assert_eq!(users.primary_key, ["id"]);
        assert!(users.columns[0].auto_increment);
        assert!(!users.columns[1].nullable);
        assert!(users
            .indexes
            .iter()
            .any(|i| i.unique && i.columns == ["email"]));
        let orders = public.table("orders").unwrap();
        assert_eq!(orders.primary_key, ["id"]);
        assert!(orders.columns[0].auto_increment);
        assert_eq!(orders.comment, "Customer orders");
        assert_eq!(orders.column("total").unwrap().comment, "Gross amount");
        assert_eq!(
            orders.column("status").unwrap().default.as_deref(),
            Some("'new'")
        );
        let fk = &orders.foreign_keys[0];
        assert_eq!(
            (fk.ref_table.as_str(), fk.on_delete.as_str()),
            ("users", "CASCADE")
        );
        assert_eq!(orders.checks.len(), 1);
        assert!(orders.indexes.iter().any(|i| i.name == "orders_user_idx"));
        assert_eq!(orders.triggers[0].timing, "BEFORE");
        assert_eq!(orders.triggers[0].events, "UPDATE");
        let view = public.table("order_totals").unwrap();
        assert_eq!(view.kind, TableKind::View);
        let cols: Vec<_> = view.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(cols, ["user_id", "spent"]);
        assert_eq!(view.columns[0].data_type, "BIGINT");
        let mv = public.table("big_orders").unwrap();
        assert_eq!(mv.kind, TableKind::MaterializedView);
        assert_eq!(mv.columns.len(), orders.columns.len());
        assert_eq!(public.types[0].labels, ["new", "paid", "shipped"]);
        assert_eq!(public.sequences[0].start, Some(1000));
        let f = &public.routines[0];
        assert_eq!(f.name, "order_total");
        assert_eq!(f.arguments[0].name, "o_id");
        assert_eq!(f.returns, "NUMERIC");
        let inv = p.schemas[1].table("invoices").unwrap();
        assert_eq!(inv.foreign_keys[0].ref_schema, "public");
    }

    #[test]
    fn applies_migrations_in_order() {
        let files = [
            file(
                "migrations/V1__init.sql",
                "CREATE TABLE `posts` (`id` int NOT NULL AUTO_INCREMENT, `title` varchar(200), PRIMARY KEY (`id`), KEY `posts_title` (`title`)) COMMENT='Blog posts';",
            ),
            file(
                "migrations/V2__authors.sql",
                "CREATE TABLE authors (id int PRIMARY KEY);\nALTER TABLE posts ADD COLUMN author_id int, ADD CONSTRAINT posts_author_fk FOREIGN KEY (author_id) REFERENCES authors (id);\nALTER TABLE posts RENAME COLUMN title TO headline;",
            ),
            file(
                "migrations/V3__cleanup.sql",
                "DROP TABLE IF EXISTS legacy;\nCREATE TABLE tmp (x int);\nDROP TABLE tmp;\nTHIS IS NOT SQL;\nCREATE TABLE broken (;",
            ),
        ];
        let p = parse(Dialect::MySql, "shop", &files);
        assert_eq!(p.schemas.len(), 1);
        let s = &p.schemas[0];
        assert_eq!(s.name, "shop");
        let names: Vec<_> = s.tables.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["authors", "posts"]);
        let posts = s.table("posts").unwrap();
        assert_eq!(posts.comment, "Blog posts");
        assert!(posts.columns[0].auto_increment);
        assert!(posts.column("headline").is_some());
        assert_eq!(
            posts
                .indexes
                .iter()
                .find(|i| i.name == "posts_title")
                .unwrap()
                .columns,
            ["headline"]
        );
        assert_eq!(posts.foreign_keys[0].name, "posts_author_fk");
        assert_eq!(p.warnings.len(), 1, "{:?}", p.warnings);
        assert_eq!(p.warnings[0].file, "migrations/V3__cleanup.sql");
        assert_eq!(p.warnings[0].line, 5);
    }

    #[test]
    fn mysql_delimiters_and_fallback_triggers() {
        let sql = "CREATE TABLE t (id int PRIMARY KEY, n int);\nDELIMITER $$\nCREATE TRIGGER t_bump BEFORE INSERT ON t FOR EACH ROW BEGIN SET NEW.n = NEW.n + 1; END$$\nCREATE PROCEDURE p_reset(IN x int) BEGIN UPDATE t SET n = x; END$$\nDELIMITER ;\nCREATE INDEX t_n ON t (n);";
        let p = parse(Dialect::MySql, "app", &[file("schema.sql", sql)]);
        assert!(p.warnings.is_empty(), "{:?}", p.warnings);
        let t = p.schemas[0].table("t").unwrap();
        assert_eq!(t.triggers.len(), 1);
        assert_eq!(t.triggers[0].name, "t_bump");
        assert_eq!(t.triggers[0].events, "INSERT");
        assert_eq!(t.indexes.len(), 2);
        let r = &p.schemas[0].routines[0];
        assert_eq!((r.name.as_str(), r.kind.as_str()), ("p_reset", "procedure"));
    }

    #[test]
    fn sqlite_and_sqlserver() {
        let p = parse(
            Dialect::Sqlite,
            "",
            &[file(
                "app.sql",
                "CREATE TABLE notes (id INTEGER PRIMARY KEY AUTOINCREMENT, body TEXT NOT NULL, tag_id INTEGER REFERENCES tags(id));\nCREATE UNIQUE INDEX notes_body ON notes(body);",
            )],
        );
        let n = p.schemas[0].table("notes").unwrap();
        assert_eq!(p.schemas[0].name, "main");
        assert!(n.columns[0].auto_increment);
        assert_eq!(n.foreign_keys[0].ref_table, "tags");
        assert!(n.indexes.iter().any(|i| i.unique && i.name == "notes_body"));
        let p = parse(
            Dialect::SqlServer,
            "",
            &[file(
                "db.sql",
                "CREATE TABLE [sales].[customers] ([id] int IDENTITY(1,1) NOT NULL, [name] nvarchar(100), CONSTRAINT [pk_customers] PRIMARY KEY ([id]));\nGO\nCREATE VIEW sales.v_names AS SELECT name FROM sales.customers;\nGO",
            )],
        );
        assert!(p.warnings.is_empty(), "{:?}", p.warnings);
        let s = p.schemas.iter().find(|s| s.name == "sales").unwrap();
        let c = s.table("customers").unwrap();
        assert!(c.columns[0].auto_increment);
        assert_eq!(c.primary_key, ["id"]);
        assert_eq!(
            s.table("v_names").unwrap().columns[0].data_type,
            "NVARCHAR(100)"
        );
    }
}
