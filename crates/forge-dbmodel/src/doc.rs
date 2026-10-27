//! Quick Documentation (Ctrl+Q) of a node of the Database tool window: its DDL and a list
//! of properties (type, nullability, default, comment, references, …).

use forge_sql::Dialect;
use serde::{Deserialize, Serialize};

use crate::generate;
use crate::model::{ColumnInfo, RoleInfo, SchemaInfo, TableInfo, TableKind};

/// A node of the tree: `kind` is `schema`, `table`, `view`, `matview`, `column`, `key`,
/// `fk`, `index`, `check`, `trigger`, `routine`, `sequence` or `type`; `table` names the
/// table of table children; `name` the object itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ObjectRef {
    pub kind: String,
    #[serde(default)]
    pub table: String,
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocProperty {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ObjectDoc {
    /// What the object is: `table`, `column`, `virtual foreign key`, …
    pub kind: String,
    pub name: String,
    /// `schema.table` the object belongs to.
    pub location: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub comment: String,
    pub ddl: String,
    pub properties: Vec<DocProperty>,
}

fn prop(out: &mut Vec<DocProperty>, name: &str, value: impl Into<String>) {
    let value = value.into();
    if !value.is_empty() {
        out.push(DocProperty {
            name: name.into(),
            value,
        });
    }
}

fn column_line(d: Dialect, c: &ColumnInfo) -> String {
    let mut l = format!("{} {}", d.quote_ident(&c.name), c.data_type);
    if c.is_virtual {
        return format!(
            "-- virtual column (project)\n{} AS ({})",
            l,
            c.expression.as_deref().unwrap_or("")
        );
    }
    if !c.nullable {
        l.push_str(" NOT NULL");
    }
    if let Some(def) = &c.default {
        l.push_str(&format!(" DEFAULT {def}"));
    }
    if let Some(expr) = &c.expression {
        l.push_str(&format!(" GENERATED ALWAYS AS ({expr})"));
    }
    l
}

fn kind_label(t: &TableInfo) -> &'static str {
    match t.kind {
        TableKind::Table => "table",
        TableKind::View => "view",
        TableKind::MaterializedView => "materialized view",
        TableKind::Collection => "collection",
    }
}

/// Documentation of one object of `schema`, `None` when it is not in the model.
pub fn document(d: Dialect, schema: &SchemaInfo, obj: &ObjectRef) -> Option<ObjectDoc> {
    let s = schema.name.as_str();
    let table = || schema.table(&obj.table).or_else(|| schema.table(&obj.name));
    let mut props = Vec::new();
    let doc = match obj.kind.as_str() {
        "schema" => {
            let count = |k: TableKind| schema.tables.iter().filter(|t| t.kind == k).count();
            prop(&mut props, "Tables", count(TableKind::Table).to_string());
            prop(&mut props, "Views", count(TableKind::View).to_string());
            if count(TableKind::MaterializedView) > 0 {
                prop(
                    &mut props,
                    "Materialized views",
                    count(TableKind::MaterializedView).to_string(),
                );
            }
            prop(&mut props, "Routines", schema.routines.len().to_string());
            if !schema.sequences.is_empty() {
                prop(&mut props, "Sequences", schema.sequences.len().to_string());
            }
            ObjectDoc {
                kind: "schema".into(),
                name: s.into(),
                location: String::new(),
                comment: String::new(),
                ddl: format!("CREATE SCHEMA {};\n", d.quote_ident(s)),
                properties: props,
            }
        }
        "table" | "view" | "matview" | "collection" => {
            let t = table()?;
            if let Some(n) = t.row_estimate {
                prop(&mut props, "Rows (estimate)", n.to_string());
            }
            prop(&mut props, "Columns", t.columns.len().to_string());
            if !t.primary_key.is_empty() {
                prop(&mut props, "Primary key", t.primary_key.join(", "));
            }
            let refs: Vec<String> = t
                .foreign_keys
                .iter()
                .map(|f| {
                    format!(
                        "{}{}",
                        f.ref_table,
                        if f.is_virtual { " (virtual)" } else { "" }
                    )
                })
                .collect();
            prop(&mut props, "References", refs.join(", "));
            let referenced_by: Vec<String> = schema
                .tables
                .iter()
                .filter(|o| {
                    o.foreign_keys.iter().any(|f| {
                        f.ref_table == t.name && (f.ref_schema.is_empty() || f.ref_schema == s)
                    })
                })
                .map(|o| o.name.clone())
                .collect();
            prop(&mut props, "Referenced by", referenced_by.join(", "));
            if !t.triggers.is_empty() {
                let names: Vec<_> = t.triggers.iter().map(|x| x.name.as_str()).collect();
                prop(&mut props, "Triggers", names.join(", "));
            }
            ObjectDoc {
                kind: kind_label(t).into(),
                name: t.name.clone(),
                location: s.into(),
                comment: t.comment.clone(),
                ddl: generate::table(d, s, t),
                properties: props,
            }
        }
        "column" => {
            let t = table()?;
            let c = t.column(&obj.name)?;
            prop(&mut props, "Type", c.data_type.clone());
            prop(
                &mut props,
                "Nullable",
                if c.nullable { "yes" } else { "no" },
            );
            if let Some(def) = &c.default {
                prop(&mut props, "Default", def.clone());
            }
            if c.auto_increment {
                prop(&mut props, "Auto increment", "yes");
            }
            if t.primary_key.iter().any(|k| k == &c.name) {
                prop(&mut props, "Primary key", "yes");
            }
            for f in t
                .foreign_keys
                .iter()
                .filter(|f| f.columns.contains(&c.name))
            {
                let i = f.columns.iter().position(|x| x == &c.name).unwrap_or(0);
                prop(
                    &mut props,
                    if f.is_virtual {
                        "References (virtual)"
                    } else {
                        "References"
                    },
                    format!(
                        "{}.{}",
                        f.ref_table,
                        f.ref_columns.get(i).cloned().unwrap_or_default()
                    ),
                );
            }
            if let Some(e) = &c.expression {
                prop(&mut props, "Expression", e.clone());
            }
            ObjectDoc {
                kind: if c.is_virtual {
                    "virtual column".into()
                } else {
                    "column".into()
                },
                name: c.name.clone(),
                location: format!("{s}.{}", t.name),
                comment: c.comment.clone(),
                ddl: format!("{}\n", column_line(d, c)),
                properties: props,
            }
        }
        "key" => {
            let t = table()?;
            let name = t
                .indexes
                .iter()
                .find(|i| i.primary)
                .map(|i| i.name.clone())
                .unwrap_or_else(|| format!("{}_pk", t.name));
            prop(&mut props, "Columns", t.primary_key.join(", "));
            ObjectDoc {
                kind: "primary key".into(),
                ddl: format!(
                    "ALTER TABLE {} ADD CONSTRAINT {} PRIMARY KEY ({});\n",
                    d.qualified(s, &t.name),
                    d.quote_ident(&name),
                    t.primary_key
                        .iter()
                        .map(|c| d.quote_ident(c))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                name,
                location: format!("{s}.{}", t.name),
                comment: String::new(),
                properties: props,
            }
        }
        "fk" => {
            let t = table()?;
            let f = t.foreign_keys.iter().find(|f| f.name == obj.name)?;
            prop(&mut props, "Columns", f.columns.join(", "));
            prop(
                &mut props,
                "References",
                format!("{}({})", f.ref_table, f.ref_columns.join(", ")),
            );
            prop(&mut props, "On update", f.on_update.clone());
            prop(&mut props, "On delete", f.on_delete.clone());
            let qi = |cs: &[String]| {
                cs.iter()
                    .map(|c| d.quote_ident(c))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let mut ddl = format!(
                "ALTER TABLE {} ADD CONSTRAINT {} FOREIGN KEY ({}) REFERENCES {} ({});\n",
                d.qualified(s, &t.name),
                d.quote_ident(&f.name),
                qi(&f.columns),
                d.qualified(&f.ref_schema, &f.ref_table),
                qi(&f.ref_columns)
            );
            if f.is_virtual {
                ddl = format!(
                    "-- virtual foreign key, stored in the project (.forge/database/virtual.toml)\n-- {ddl}"
                );
            }
            ObjectDoc {
                kind: if f.is_virtual {
                    "virtual foreign key".into()
                } else {
                    "foreign key".into()
                },
                name: f.name.clone(),
                location: format!("{s}.{}", t.name),
                comment: String::new(),
                ddl,
                properties: props,
            }
        }
        "index" => {
            let t = table()?;
            let i = t.indexes.iter().find(|i| i.name == obj.name)?;
            prop(&mut props, "Columns", i.columns.join(", "));
            prop(&mut props, "Unique", if i.unique { "yes" } else { "no" });
            ObjectDoc {
                kind: "index".into(),
                name: i.name.clone(),
                location: format!("{s}.{}", t.name),
                comment: String::new(),
                ddl: format!(
                    "CREATE {}INDEX {} ON {} ({});\n",
                    if i.unique { "UNIQUE " } else { "" },
                    d.quote_ident(&i.name),
                    d.qualified(s, &t.name),
                    i.columns
                        .iter()
                        .map(|c| d.quote_ident(c))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                properties: props,
            }
        }
        "check" => {
            let t = table()?;
            let c = t.checks.iter().find(|c| c.name == obj.name)?;
            ObjectDoc {
                kind: "check".into(),
                name: c.name.clone(),
                location: format!("{s}.{}", t.name),
                comment: String::new(),
                ddl: format!(
                    "ALTER TABLE {} ADD CONSTRAINT {} CHECK ({});\n",
                    d.qualified(s, &t.name),
                    d.quote_ident(&c.name),
                    c.expression
                        .trim()
                        .trim_start_matches('(')
                        .trim_end_matches(')')
                ),
                properties: props,
            }
        }
        "trigger" => {
            let t = table()?;
            let tr = t.triggers.iter().find(|x| x.name == obj.name)?;
            prop(&mut props, "Timing", tr.timing.clone());
            prop(&mut props, "Events", tr.events.clone());
            ObjectDoc {
                kind: "trigger".into(),
                name: tr.name.clone(),
                location: format!("{s}.{}", t.name),
                comment: String::new(),
                ddl: generate::trigger(d, s, &t.name, tr),
                properties: props,
            }
        }
        "routine" => {
            let r = schema.routines.iter().find(|r| r.name == obj.name)?;
            prop(&mut props, "Arguments", r.signature());
            prop(&mut props, "Returns", r.returns.clone());
            prop(&mut props, "Language", r.language.clone());
            ObjectDoc {
                kind: r.kind.clone(),
                name: r.name.clone(),
                location: s.into(),
                comment: String::new(),
                ddl: generate::routine(d, s, r),
                properties: props,
            }
        }
        "sequence" => {
            let q = schema.sequences.iter().find(|q| q.name == obj.name)?;
            prop(&mut props, "Type", q.data_type.clone());
            ObjectDoc {
                kind: "sequence".into(),
                name: q.name.clone(),
                location: s.into(),
                comment: String::new(),
                ddl: generate::sequence(d, s, q),
                properties: props,
            }
        }
        "type" => {
            let ty = schema.types.iter().find(|t| t.name == obj.name)?;
            prop(&mut props, "Kind", ty.kind.clone());
            prop(&mut props, "Labels", ty.labels.join(", "));
            ObjectDoc {
                kind: format!("{} type", ty.kind),
                name: ty.name.clone(),
                location: s.into(),
                comment: String::new(),
                ddl: generate::user_type(d, s, ty),
                properties: props,
            }
        }
        _ => return None,
    };
    Some(doc)
}

/// Documentation of a server role or user.
pub fn document_role(r: &RoleInfo) -> ObjectDoc {
    let mut props = Vec::new();
    prop(&mut props, "Kind", r.kind.clone());
    prop(
        &mut props,
        "Superuser",
        if r.superuser { "yes" } else { "no" },
    );
    prop(
        &mut props,
        "Can log in",
        if r.can_login { "yes" } else { "no" },
    );
    ObjectDoc {
        kind: r.kind.clone(),
        name: r.name.clone(),
        location: String::new(),
        comment: String::new(),
        ddl: format!(
            "CREATE {} {};\n",
            if r.kind == "user" { "USER" } else { "ROLE" },
            r.name
        ),
        properties: props,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ForeignKey;

    fn schema() -> SchemaInfo {
        SchemaInfo {
            name: "shop".into(),
            tables: vec![
                TableInfo {
                    name: "orders".into(),
                    columns: vec![
                        ColumnInfo {
                            name: "id".into(),
                            data_type: "bigint".into(),
                            auto_increment: true,
                            ..Default::default()
                        },
                        ColumnInfo {
                            name: "user_id".into(),
                            data_type: "bigint".into(),
                            nullable: true,
                            comment: "buyer".into(),
                            ..Default::default()
                        },
                    ],
                    primary_key: vec!["id".into()],
                    foreign_keys: vec![ForeignKey {
                        name: "vfk".into(),
                        columns: vec!["user_id".into()],
                        ref_schema: "shop".into(),
                        ref_table: "users".into(),
                        ref_columns: vec!["id".into()],
                        is_virtual: true,
                        ..Default::default()
                    }],
                    row_estimate: Some(42),
                    ..Default::default()
                },
                TableInfo {
                    name: "users".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }
    }

    #[test]
    fn documents_columns_and_tables() {
        let s = schema();
        let c = document(
            Dialect::MySql,
            &s,
            &ObjectRef {
                kind: "column".into(),
                table: "orders".into(),
                name: "user_id".into(),
            },
        )
        .unwrap();
        assert_eq!(c.location, "shop.orders");
        assert_eq!(c.comment, "buyer");
        assert_eq!(c.ddl, "`user_id` bigint\n");
        assert!(c.properties.contains(&DocProperty {
            name: "References (virtual)".into(),
            value: "users.id".into()
        }));
        let t = document(
            Dialect::MySql,
            &s,
            &ObjectRef {
                kind: "table".into(),
                table: String::new(),
                name: "users".into(),
            },
        )
        .unwrap();
        assert!(t
            .properties
            .iter()
            .any(|p| p.name == "Referenced by" && p.value == "orders"));
        let fk = document(
            Dialect::MySql,
            &s,
            &ObjectRef {
                kind: "fk".into(),
                table: "orders".into(),
                name: "vfk".into(),
            },
        )
        .unwrap();
        assert_eq!(fk.kind, "virtual foreign key");
        assert!(fk.ddl.starts_with("-- virtual"));
        assert!(document(
            Dialect::MySql,
            &s,
            &ObjectRef {
                kind: "column".into(),
                table: "orders".into(),
                name: "nope".into()
            }
        )
        .is_none());
    }
}
