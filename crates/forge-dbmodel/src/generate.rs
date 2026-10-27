//! DDL generated from the model: tables (columns, keys, checks, indexes, comments), views,
//! sequences, types, routines and triggers. Used where the server does not keep the source
//! and for DDL data sources and Quick Documentation.

use forge_sql::Dialect;

use crate::model::{RoutineInfo, SequenceInfo, TableInfo, TableKind, TriggerInfo, TypeInfo};

fn cols(d: Dialect, cs: &[String]) -> String {
    cs.iter()
        .map(|c| d.quote_ident(c))
        .collect::<Vec<_>>()
        .join(", ")
}

/// `CREATE TABLE` (plus `CREATE INDEX` for non-unique indexes) of a table, or
/// `CREATE VIEW` of a view with a known query.
pub fn table(d: Dialect, schema: &str, t: &TableInfo) -> String {
    let name = d.qualified(schema, &t.name);
    if t.kind.is_view() {
        let kw = if t.kind == TableKind::MaterializedView {
            "MATERIALIZED VIEW"
        } else {
            "VIEW"
        };
        let body = if t.definition.trim().is_empty() {
            "SELECT …".to_string()
        } else {
            t.definition.trim().trim_end_matches(';').to_string()
        };
        return format!("CREATE {kw} {name} AS\n{body};\n");
    }
    let mut lines: Vec<String> = t
        .columns
        .iter()
        .filter(|c| !c.is_virtual)
        .map(|c| {
            let mut l = format!("    {} {}", d.quote_ident(&c.name), c.data_type);
            if !c.nullable {
                l.push_str(" NOT NULL");
            }
            if let Some(def) = &c.default {
                l.push_str(&format!(" DEFAULT {def}"));
            }
            l
        })
        .collect();
    if !t.primary_key.is_empty() {
        let pk = t.indexes.iter().find(|i| i.primary).map(|i| i.name.clone());
        lines.push(match pk {
            Some(n) => format!(
                "    CONSTRAINT {} PRIMARY KEY ({})",
                d.quote_ident(&n),
                cols(d, &t.primary_key)
            ),
            None => format!("    PRIMARY KEY ({})", cols(d, &t.primary_key)),
        });
    }
    for i in t.indexes.iter().filter(|i| i.unique && !i.primary) {
        lines.push(format!(
            "    CONSTRAINT {} UNIQUE ({})",
            d.quote_ident(&i.name),
            cols(d, &i.columns)
        ));
    }
    for f in t.foreign_keys.iter().filter(|f| !f.is_virtual) {
        let mut l = format!(
            "    CONSTRAINT {} FOREIGN KEY ({}) REFERENCES {} ({})",
            d.quote_ident(&f.name),
            cols(d, &f.columns),
            d.qualified(&f.ref_schema, &f.ref_table),
            cols(d, &f.ref_columns)
        );
        if !f.on_update.is_empty() && f.on_update != "NO ACTION" {
            l.push_str(&format!(" ON UPDATE {}", f.on_update));
        }
        if !f.on_delete.is_empty() && f.on_delete != "NO ACTION" {
            l.push_str(&format!(" ON DELETE {}", f.on_delete));
        }
        lines.push(l);
    }
    for c in &t.checks {
        let expr = c.expression.trim();
        let expr = if expr.starts_with('(') {
            expr.to_string()
        } else {
            format!("({expr})")
        };
        lines.push(if c.name.is_empty() {
            format!("    CHECK {expr}")
        } else {
            format!("    CONSTRAINT {} CHECK {expr}", d.quote_ident(&c.name))
        });
    }
    let mut out = format!("CREATE TABLE {name} (\n{}\n);\n", lines.join(",\n"));
    for i in t.indexes.iter().filter(|i| !i.unique && !i.primary) {
        out.push_str(&format!(
            "\nCREATE INDEX {} ON {name} ({});\n",
            d.quote_ident(&i.name),
            cols(d, &i.columns)
        ));
    }
    if !t.comment.is_empty() && matches!(d, Dialect::Postgres | Dialect::Oracle) {
        out.push_str(&format!(
            "\nCOMMENT ON TABLE {name} IS {};\n",
            d.quote_str(&t.comment)
        ));
    }
    out
}

pub fn sequence(d: Dialect, schema: &str, s: &SequenceInfo) -> String {
    let mut out = format!("CREATE SEQUENCE {}", d.qualified(schema, &s.name));
    if !s.data_type.is_empty() && d != Dialect::Oracle {
        out.push_str(&format!(" AS {}", s.data_type));
    }
    if let Some(v) = s.start {
        out.push_str(&format!(" START WITH {v}"));
    }
    if let Some(v) = s.increment {
        out.push_str(&format!(" INCREMENT BY {v}"));
    }
    out.push_str(";\n");
    out
}

pub fn user_type(d: Dialect, schema: &str, t: &TypeInfo) -> String {
    let name = d.qualified(schema, &t.name);
    match t.kind.as_str() {
        "enum" => format!(
            "CREATE TYPE {name} AS ENUM ({});\n",
            t.labels
                .iter()
                .map(|l| d.quote_str(l))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        "domain" => format!("CREATE DOMAIN {name} AS {};\n", t.definition),
        "composite" => format!("CREATE TYPE {name} AS ({});\n", t.definition),
        _ if t.definition.is_empty() => format!("CREATE TYPE {name};\n"),
        _ => format!("CREATE TYPE {name} {};\n", t.definition),
    }
}

/// The routine's source, or its header when the catalog does not keep it.
pub fn routine(d: Dialect, schema: &str, r: &RoutineInfo) -> String {
    let def = r.definition.trim();
    if def.len() > 6 && def[..6].eq_ignore_ascii_case("create") {
        return format!("{}\n", def.trim_end());
    }
    let kind = if r.kind == "procedure" {
        "PROCEDURE"
    } else {
        "FUNCTION"
    };
    let mut out = format!(
        "CREATE {kind} {}{}",
        d.qualified(schema, &r.name),
        r.signature()
    );
    if !r.returns.is_empty() && kind == "FUNCTION" {
        out.push_str(&format!(" RETURNS {}", r.returns));
    }
    if !r.language.is_empty() {
        out.push_str(&format!(" LANGUAGE {}", r.language));
    }
    if def.is_empty() {
        out.push_str(";\n");
    } else {
        out.push_str(&format!("\nAS\n{def}\n"));
    }
    out
}

pub fn trigger(d: Dialect, schema: &str, table: &str, t: &TriggerInfo) -> String {
    let def = t.definition.trim();
    if def.len() > 6 && def[..6].eq_ignore_ascii_case("create") {
        return format!("{}\n", def.trim_end());
    }
    let mut out = format!(
        "CREATE TRIGGER {} {} {} ON {}",
        d.quote_ident(&t.name),
        if t.timing.is_empty() {
            "AFTER"
        } else {
            &t.timing
        },
        if t.events.is_empty() {
            "INSERT"
        } else {
            &t.events
        },
        d.qualified(schema, table)
    );
    if def.is_empty() {
        out.push_str(";\n");
    } else {
        out.push_str(&format!("\n{def}\n"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{CheckInfo, ColumnInfo, ForeignKey, IndexInfo, RoutineArg};

    #[test]
    fn generates_postgres_table() {
        let t = TableInfo {
            name: "orders".into(),
            columns: vec![
                ColumnInfo {
                    name: "id".into(),
                    data_type: "bigint".into(),
                    nullable: false,
                    default: Some("nextval('orders_id_seq'::regclass)".into()),
                    auto_increment: true,
                    ..Default::default()
                },
                ColumnInfo {
                    name: "user_id".into(),
                    data_type: "bigint".into(),
                    nullable: true,
                    ..Default::default()
                },
            ],
            primary_key: vec!["id".into()],
            indexes: vec![
                IndexInfo {
                    name: "orders_pkey".into(),
                    columns: vec!["id".into()],
                    unique: true,
                    primary: true,
                },
                IndexInfo {
                    name: "orders_user_id_index".into(),
                    columns: vec!["user_id".into()],
                    unique: false,
                    primary: false,
                },
            ],
            foreign_keys: vec![ForeignKey {
                name: "orders_user_id_foreign".into(),
                columns: vec!["user_id".into()],
                ref_schema: "public".into(),
                ref_table: "users".into(),
                ref_columns: vec!["id".into()],
                on_update: "NO ACTION".into(),
                on_delete: "CASCADE".into(),
                is_virtual: false,
            }],
            ..Default::default()
        };
        let ddl = table(Dialect::Postgres, "public", &t);
        assert_eq!(
            ddl,
            "CREATE TABLE \"public\".\"orders\" (\n    \"id\" bigint NOT NULL DEFAULT nextval('orders_id_seq'::regclass),\n    \"user_id\" bigint,\n    CONSTRAINT \"orders_pkey\" PRIMARY KEY (\"id\"),\n    CONSTRAINT \"orders_user_id_foreign\" FOREIGN KEY (\"user_id\") REFERENCES \"public\".\"users\" (\"id\") ON DELETE CASCADE\n);\n\nCREATE INDEX \"orders_user_id_index\" ON \"public\".\"orders\" (\"user_id\");\n"
        );
    }

    #[test]
    fn checks_views_and_other_objects() {
        let t = TableInfo {
            name: "items".into(),
            columns: vec![ColumnInfo {
                name: "qty".into(),
                data_type: "int".into(),
                ..Default::default()
            }],
            checks: vec![CheckInfo {
                name: "qty_positive".into(),
                expression: "qty > 0".into(),
            }],
            ..Default::default()
        };
        assert!(
            table(Dialect::MySql, "shop", &t).contains("CONSTRAINT `qty_positive` CHECK (qty > 0)")
        );
        let v = TableInfo {
            name: "totals".into(),
            kind: TableKind::MaterializedView,
            definition: "SELECT 1;".into(),
            ..Default::default()
        };
        assert_eq!(
            table(Dialect::Postgres, "public", &v),
            "CREATE MATERIALIZED VIEW \"public\".\"totals\" AS\nSELECT 1;\n"
        );
        let s = SequenceInfo {
            name: "seq".into(),
            data_type: "bigint".into(),
            start: Some(1),
            increment: Some(1),
        };
        assert_eq!(
            sequence(Dialect::Postgres, "public", &s),
            "CREATE SEQUENCE \"public\".\"seq\" AS bigint START WITH 1 INCREMENT BY 1;\n"
        );
        let e = TypeInfo {
            name: "status".into(),
            kind: "enum".into(),
            labels: vec!["new".into(), "paid".into()],
            definition: String::new(),
        };
        assert_eq!(
            user_type(Dialect::Postgres, "public", &e),
            "CREATE TYPE \"public\".\"status\" AS ENUM ('new', 'paid');\n"
        );
        let r = RoutineInfo {
            name: "total".into(),
            kind: "function".into(),
            returns: "numeric".into(),
            arguments: vec![RoutineArg {
                name: "o".into(),
                data_type: "bigint".into(),
                mode: String::new(),
            }],
            language: "sql".into(),
            definition: "SELECT 1".into(),
        };
        assert_eq!(
            routine(Dialect::Postgres, "public", &r),
            "CREATE FUNCTION \"public\".\"total\"(o bigint) RETURNS numeric LANGUAGE sql\nAS\nSELECT 1\n"
        );
        let tr = TriggerInfo {
            name: "t".into(),
            timing: "BEFORE".into(),
            events: "UPDATE".into(),
            definition: "EXECUTE FUNCTION touch()".into(),
        };
        assert_eq!(
            trigger(Dialect::Postgres, "public", "items", &tr),
            "CREATE TRIGGER \"t\" BEFORE UPDATE ON \"public\".\"items\"\nEXECUTE FUNCTION touch()\n"
        );
    }
}
