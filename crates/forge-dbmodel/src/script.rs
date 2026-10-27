//! "SQL Scripts › Generate DDL" for objects selected in the Database tool window: one
//! script in dependency order (types, sequences, tables ordered by their foreign keys,
//! views ordered by the views they read, routines, indexes, triggers), with optional
//! `DROP` statements (in reverse order), `IF NOT EXISTS` / `OR REPLACE` and qualified or
//! plain names. Foreign keys of tables that reference each other in a cycle are added by
//! `ALTER TABLE` after all tables exist. The caller formats the script with the SQL
//! formatter.

use std::collections::{BTreeSet, HashMap};

use forge_sql::Dialect;
use serde::{Deserialize, Serialize};

use crate::design::{
    self, add_foreign_key, create_index, create_statements, drop_statements, supports_if_not_exists,
    CreateOptions, DropRequest, IndexSpec, TableSpec,
};
use crate::generate;
use crate::model::{DbModel, SchemaInfo, TableInfo, TableKind};

/// A selected tree object: `kind` is `schema` (everything in it), `table`, `view`,
/// `matview`, `routine`, `sequence`, `type`, `index` or `trigger`; `table` names the table
/// of an index or trigger.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct DdlObject {
    pub schema: String,
    pub kind: String,
    pub table: String,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DdlOptions {
    /// `DROP … IF EXISTS` before the `CREATE` statements.
    pub include_drops: bool,
    /// `IF NOT EXISTS` / `OR REPLACE` where the dialect has it.
    pub if_not_exists: bool,
    /// `schema.name` instead of plain names.
    pub qualify: bool,
}

impl Default for DdlOptions {
    fn default() -> Self {
        Self {
            include_drops: false,
            if_not_exists: false,
            qualify: true,
        }
    }
}

#[derive(Clone)]
enum Item<'a> {
    Type(&'a str, &'a crate::model::TypeInfo),
    Sequence(&'a str, &'a crate::model::SequenceInfo),
    Table(&'a str, &'a TableInfo),
    View(&'a str, &'a TableInfo),
    Routine(&'a str, &'a crate::model::RoutineInfo),
    Index(&'a str, &'a TableInfo, &'a crate::model::IndexInfo),
    Trigger(&'a str, &'a TableInfo, &'a crate::model::TriggerInfo),
}

fn key(schema: &str, name: &str) -> String {
    format!("{}.{}", schema.to_lowercase(), name.to_lowercase())
}

/// Order `nodes` so every node comes after the ones it depends on; nodes in a cycle keep
/// their (name) order, and the edges that close a cycle are returned.
fn topo(nodes: &[String], deps: &HashMap<String, Vec<String>>) -> (Vec<String>, Vec<(String, String)>) {
    #[derive(Clone, Copy, PartialEq)]
    enum Mark {
        New,
        Active,
        Done,
    }
    fn visit(
        n: &str,
        deps: &HashMap<String, Vec<String>>,
        marks: &mut HashMap<String, Mark>,
        out: &mut Vec<String>,
        back: &mut Vec<(String, String)>,
    ) {
        marks.insert(n.to_string(), Mark::Active);
        for d in deps.get(n).into_iter().flatten() {
            match marks.get(d.as_str()).copied() {
                Some(Mark::New) => visit(d, deps, marks, out, back),
                Some(Mark::Active) => back.push((n.to_string(), d.clone())),
                _ => {}
            }
        }
        marks.insert(n.to_string(), Mark::Done);
        out.push(n.to_string());
    }
    let mut marks: HashMap<String, Mark> = nodes.iter().map(|n| (n.clone(), Mark::New)).collect();
    let mut out = Vec::new();
    let mut back = Vec::new();
    for n in nodes {
        if marks.get(n.as_str()) == Some(&Mark::New) {
            visit(n, deps, &mut marks, &mut out, &mut back);
        }
    }
    (out, back)
}

/// Words of a view's query that may name other objects.
fn mentions(sql: &str) -> BTreeSet<String> {
    sql.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.'))
        .filter(|w| !w.is_empty())
        .flat_map(|w| {
            let w = w.to_lowercase();
            let last = w.rsplit('.').next().unwrap_or("").to_string();
            [w, last]
        })
        .collect()
}

/// The objects of `sel` resolved against the loaded schemas.
fn collect<'a>(model: &'a DbModel, sel: &[DdlObject]) -> Vec<Item<'a>> {
    let mut items = Vec::new();
    let mut seen = BTreeSet::new();
    let mut push = |id: String, it: Item<'a>, items: &mut Vec<Item<'a>>| {
        if seen.insert(id) {
            items.push(it);
        }
    };
    for o in sel {
        let Some(s) = model
            .loaded
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(&o.schema))
            .map(|(_, v)| v)
        else {
            continue;
        };
        let sn = s.name.as_str();
        let all = o.kind == "schema";
        for t in &s.types {
            if all || (o.kind == "type" && t.name.eq_ignore_ascii_case(&o.name)) {
                push(format!("type:{sn}.{}", t.name), Item::Type(sn, t), &mut items);
            }
        }
        for q in &s.sequences {
            if all || (o.kind == "sequence" && q.name.eq_ignore_ascii_case(&o.name)) {
                push(format!("seq:{sn}.{}", q.name), Item::Sequence(sn, q), &mut items);
            }
        }
        for t in &s.tables {
            let hit = all
                || (matches!(o.kind.as_str(), "table" | "view" | "matview" | "collection")
                    && t.name.eq_ignore_ascii_case(&o.name));
            if hit {
                let it = if t.kind.is_view() {
                    Item::View(sn, t)
                } else {
                    Item::Table(sn, t)
                };
                push(format!("tbl:{sn}.{}", t.name), it, &mut items);
                for tr in &t.triggers {
                    push(
                        format!("trg:{sn}.{}.{}", t.name, tr.name),
                        Item::Trigger(sn, t, tr),
                        &mut items,
                    );
                }
            }
            if o.kind == "index" && t.name.eq_ignore_ascii_case(&o.table) {
                if let Some(i) = t.indexes.iter().find(|i| i.name.eq_ignore_ascii_case(&o.name)) {
                    push(
                        format!("idx:{sn}.{}.{}", t.name, i.name),
                        Item::Index(sn, t, i),
                        &mut items,
                    );
                }
            }
            if o.kind == "trigger" && t.name.eq_ignore_ascii_case(&o.table) {
                if let Some(tr) = t.triggers.iter().find(|x| x.name.eq_ignore_ascii_case(&o.name)) {
                    push(
                        format!("trg:{sn}.{}.{}", t.name, tr.name),
                        Item::Trigger(sn, t, tr),
                        &mut items,
                    );
                }
            }
        }
        for r in &s.routines {
            if all || (o.kind == "routine" && r.name.eq_ignore_ascii_case(&o.name)) {
                push(format!("rtn:{sn}.{}", r.name), Item::Routine(sn, r), &mut items);
            }
        }
    }
    items
}

fn rank(it: &Item) -> u8 {
    match it {
        Item::Type(..) => 0,
        Item::Sequence(..) => 1,
        Item::Table(..) => 2,
        Item::View(..) => 3,
        Item::Routine(..) => 4,
        Item::Index(..) => 5,
        Item::Trigger(..) => 6,
    }
}

fn or_replace(d: Dialect, sql: &str, kind: &str) -> String {
    let trimmed = sql.trim_start();
    let head = format!("CREATE {kind}");
    if trimmed.len() < head.len() || !trimmed[..head.len()].eq_ignore_ascii_case(&head) {
        return sql.to_string();
    }
    let rest = &trimmed[head.len()..];
    match d {
        Dialect::Postgres | Dialect::Oracle | Dialect::MySql if kind == "VIEW" => {
            format!("CREATE OR REPLACE {kind}{rest}")
        }
        Dialect::Postgres | Dialect::Oracle if kind != "VIEW" => format!("CREATE OR REPLACE {kind}{rest}"),
        Dialect::SqlServer => format!("CREATE OR ALTER {kind}{rest}"),
        Dialect::Sqlite | Dialect::ClickHouse if kind == "VIEW" => {
            format!("CREATE {kind} IF NOT EXISTS{rest}")
        }
        _ => sql.to_string(),
    }
}

fn routine_kind(r: &crate::model::RoutineInfo) -> &'static str {
    if r.kind == "procedure" {
        "PROCEDURE"
    } else {
        "FUNCTION"
    }
}

/// The DDL script of the selected objects (unformatted).
pub fn generate_ddl(model: &DbModel, sel: &[DdlObject], opts: DdlOptions) -> String {
    let d = model.dialect;
    let mut items = collect(model, sel);
    items.sort_by_key(rank);
    let qual = |schema: &str| if opts.qualify { schema.to_string() } else { String::new() };

    // Tables by their foreign keys, views by what they read.
    let tables: Vec<(&str, &TableInfo)> = items
        .iter()
        .filter_map(|i| match i {
            Item::Table(s, t) => Some((*s, *t)),
            _ => None,
        })
        .collect();
    let table_keys: Vec<String> = tables.iter().map(|(s, t)| key(s, &t.name)).collect();
    let mut deps: HashMap<String, Vec<String>> = HashMap::new();
    for (s, t) in &tables {
        let k = key(s, &t.name);
        let mut ds = Vec::new();
        for f in t.foreign_keys.iter().filter(|f| !f.is_virtual) {
            let rs = if f.ref_schema.is_empty() { s } else { f.ref_schema.as_str() };
            let rk = key(rs, &f.ref_table);
            if rk != k && table_keys.contains(&rk) {
                ds.push(rk);
            }
        }
        deps.insert(k, ds);
    }
    let (table_order, back) = topo(&table_keys, &deps);
    let views: Vec<(&str, &TableInfo)> = items
        .iter()
        .filter_map(|i| match i {
            Item::View(s, t) => Some((*s, *t)),
            _ => None,
        })
        .collect();
    let view_keys: Vec<String> = views.iter().map(|(s, t)| key(s, &t.name)).collect();
    let mut vdeps: HashMap<String, Vec<String>> = HashMap::new();
    for (s, v) in &views {
        let m = mentions(&v.definition);
        let k = key(s, &v.name);
        vdeps.insert(
            k.clone(),
            views
                .iter()
                .filter(|(s2, v2)| {
                    let k2 = key(s2, &v2.name);
                    k2 != k && (m.contains(&k2) || m.contains(&v2.name.to_lowercase()))
                })
                .map(|(s2, v2)| key(s2, &v2.name))
                .collect(),
        );
    }
    let (view_order, _) = topo(&view_keys, &vdeps);

    // FKs closing a cycle are added after every table exists.
    let cyclic: Vec<(String, String)> = back;
    let mut deferred: Vec<String> = Vec::new();

    let mut creates: Vec<String> = Vec::new();
    let mut drops: Vec<Vec<String>> = Vec::new();
    let cr = CreateOptions {
        if_not_exists: opts.if_not_exists,
    };
    let drop = |kind: &str, schema: &str, name: &str, table: &str| {
        drop_statements(
            d,
            &DropRequest {
                schema: qual(schema),
                name: name.into(),
                kind: kind.into(),
                table: table.into(),
                cascade: false,
                if_exists: true,
                referencing: Vec::new(),
            },
        )
    };
    let ordered: Vec<Item> = {
        let mut out: Vec<Item> = Vec::new();
        let mut by_key: HashMap<String, Item> = HashMap::new();
        for it in &items {
            match it {
                Item::Table(s, t) | Item::View(s, t) => {
                    by_key.insert(key(s, &t.name), it.clone());
                }
                _ => {}
            }
        }
        for it in &items {
            match it {
                Item::Table(..) | Item::View(..) => {}
                other if rank(other) < 2 => out.push(other.clone()),
                _ => {}
            }
        }
        for k in table_order.iter().chain(view_order.iter()) {
            if let Some(it) = by_key.remove(k) {
                out.push(it);
            }
        }
        for it in &items {
            if rank(it) > 3 {
                out.push(it.clone());
            }
        }
        out
    };
    for it in &ordered {
        match it {
            Item::Type(s, t) => {
                creates.push(generate::user_type(d, &qual(s), t));
                drops.push(drop("type", s, &t.name, ""));
            }
            Item::Sequence(s, q) => {
                let mut sql = generate::sequence(d, &qual(s), q);
                if opts.if_not_exists && supports_if_not_exists(d, "sequence") {
                    sql = sql.replacen("CREATE SEQUENCE ", "CREATE SEQUENCE IF NOT EXISTS ", 1);
                }
                creates.push(sql);
                drops.push(drop("sequence", s, &q.name, ""));
            }
            Item::Table(s, t) => {
                let mut spec = TableSpec::from_table(&qual(s), t);
                for f in &mut spec.foreign_keys {
                    if !opts.qualify && f.ref_schema.eq_ignore_ascii_case(s) {
                        f.ref_schema.clear();
                    }
                }
                let me = key(s, &t.name);
                let skip: Vec<String> = t
                    .foreign_keys
                    .iter()
                    .filter(|f| {
                        let rs = if f.ref_schema.is_empty() { s } else { f.ref_schema.as_str() };
                        let rk = key(rs, &f.ref_table);
                        cyclic.iter().any(|(a, b)| *a == me && *b == rk)
                    })
                    .map(|f| f.name.clone())
                    .collect();
                for f in spec.foreign_keys.iter().filter(|f| skip.contains(&f.name)) {
                    deferred.push(add_foreign_key(d, &spec.schema, &spec.name, f));
                }
                creates.push(design::join(&create_statements(d, &spec, cr, &skip)));
                drops.push(drop("table", s, &t.name, ""));
            }
            Item::View(s, v) => {
                let mut sql = generate::table(d, &qual(s), v);
                if opts.if_not_exists {
                    let kind = if v.kind == TableKind::MaterializedView {
                        "MATERIALIZED VIEW"
                    } else {
                        "VIEW"
                    };
                    if kind == "VIEW" {
                        sql = or_replace(d, &sql, kind);
                    } else if d == Dialect::Postgres {
                        sql = sql.replacen(
                            "CREATE MATERIALIZED VIEW ",
                            "CREATE MATERIALIZED VIEW IF NOT EXISTS ",
                            1,
                        );
                    }
                }
                creates.push(sql);
                let kind = if v.kind == TableKind::MaterializedView {
                    "matview"
                } else {
                    "view"
                };
                drops.push(drop(kind, s, &v.name, ""));
            }
            Item::Routine(s, r) => {
                let mut sql = generate::routine(d, &qual(s), r);
                if opts.if_not_exists {
                    sql = or_replace(d, &sql, routine_kind(r));
                }
                if !sql.trim_end().ends_with(';') && !sql.trim_end().ends_with('$') {
                    sql = format!("{};\n", sql.trim_end());
                }
                creates.push(sql);
                drops.push(drop(
                    if r.kind == "procedure" { "procedure" } else { "routine" },
                    s,
                    &r.name,
                    "",
                ));
            }
            Item::Index(s, t, i) => {
                let spec = IndexSpec {
                    name: i.name.clone(),
                    columns: i.columns.clone(),
                    unique: i.unique,
                    method: String::new(),
                };
                if i.primary {
                    continue;
                }
                creates.push(format!(
                    "{}\n",
                    create_index(d, &t.name, &qual(s), &spec, opts.if_not_exists)
                ));
                drops.push(drop("index", s, &i.name, &t.name));
            }
            Item::Trigger(s, t, tr) => {
                let mut sql = generate::trigger(d, &qual(s), &t.name, tr);
                if !sql.trim_end().ends_with(';') {
                    sql = format!("{};\n", sql.trim_end());
                }
                creates.push(sql);
                drops.push(drop("trigger", s, &tr.name, &t.name));
            }
        }
    }
    let mut out = String::new();
    if opts.include_drops {
        for ds in drops.iter().rev() {
            for s in ds {
                out.push_str(s);
                out.push('\n');
            }
        }
        if !drops.is_empty() {
            out.push('\n');
        }
    }
    let mut parts: Vec<String> = creates
        .into_iter()
        .map(|c| c.trim_end().to_string())
        .filter(|c| !c.is_empty())
        .collect();
    parts.extend(deferred);
    out.push_str(&parts.join("\n\n"));
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// Every object of a loaded schema, for the Generate DDL dialog's object list.
pub fn schema_objects(s: &SchemaInfo) -> Vec<DdlObject> {
    let obj = |kind: &str, table: &str, name: &str| DdlObject {
        schema: s.name.clone(),
        kind: kind.into(),
        table: table.into(),
        name: name.into(),
    };
    let mut out = Vec::new();
    out.extend(s.types.iter().map(|t| obj("type", "", &t.name)));
    out.extend(s.sequences.iter().map(|q| obj("sequence", "", &q.name)));
    for t in &s.tables {
        let kind = match t.kind {
            TableKind::Table | TableKind::Collection => "table",
            TableKind::View => "view",
            TableKind::MaterializedView => "matview",
        };
        out.push(obj(kind, "", &t.name));
    }
    out.extend(s.routines.iter().map(|r| obj("routine", "", &r.name)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        ColumnInfo, ForeignKey, IndexInfo, RoutineInfo, SchemaInfo, SequenceInfo, TriggerInfo,
    };

    fn table(name: &str, refs: &[&str]) -> TableInfo {
        TableInfo {
            name: name.into(),
            columns: vec![ColumnInfo {
                name: "id".into(),
                data_type: "bigint".into(),
                ..Default::default()
            }],
            primary_key: vec!["id".into()],
            foreign_keys: refs
                .iter()
                .map(|r| ForeignKey {
                    name: format!("{name}_{r}_fk"),
                    columns: vec!["id".into()],
                    ref_schema: "shop".into(),
                    ref_table: (*r).into(),
                    ref_columns: vec!["id".into()],
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }
    }

    fn model() -> DbModel {
        let mut m = DbModel::new(Dialect::Postgres);
        let mut items = table("order_items", &["orders"]);
        items.indexes.push(IndexInfo {
            name: "items_order_idx".into(),
            columns: vec!["id".into()],
            unique: false,
            primary: false,
        });
        items.triggers.push(TriggerInfo {
            name: "items_touch".into(),
            timing: "BEFORE".into(),
            events: "UPDATE".into(),
            definition: "EXECUTE FUNCTION touch()".into(),
        });
        let schema = SchemaInfo {
            name: "shop".into(),
            tables: vec![
                items,
                table("orders", &["users"]),
                table("users", &[]),
                TableInfo {
                    name: "big_orders".into(),
                    kind: TableKind::View,
                    definition: "SELECT * FROM order_totals WHERE total > 100".into(),
                    ..Default::default()
                },
                TableInfo {
                    name: "order_totals".into(),
                    kind: TableKind::View,
                    definition: "SELECT id FROM orders".into(),
                    ..Default::default()
                },
                // A cycle: a ↔ b.
                table("a", &["b"]),
                table("b", &["a"]),
            ],
            routines: vec![RoutineInfo {
                name: "touch".into(),
                kind: "function".into(),
                returns: "trigger".into(),
                language: "plpgsql".into(),
                definition: "BEGIN NEW.updated_at = now(); RETURN NEW; END".into(),
                ..Default::default()
            }],
            sequences: vec![SequenceInfo {
                name: "order_no".into(),
                ..Default::default()
            }],
            types: vec![],
        };
        m.loaded.insert("shop".into(), schema);
        m
    }

    fn sel(kind: &str, name: &str) -> DdlObject {
        DdlObject {
            schema: "shop".into(),
            kind: kind.into(),
            table: String::new(),
            name: name.into(),
        }
    }

    fn pos(s: &str, what: &str) -> usize {
        s.find(what).unwrap_or_else(|| panic!("{what} missing in\n{s}"))
    }

    #[test]
    fn orders_tables_by_foreign_keys_and_views_by_reads() {
        let m = model();
        let out = generate_ddl(
            &m,
            &[
                sel("view", "big_orders"),
                sel("table", "order_items"),
                sel("view", "order_totals"),
                sel("table", "users"),
                sel("table", "orders"),
                sel("routine", "touch"),
                sel("sequence", "order_no"),
            ],
            DdlOptions::default(),
        );
        let seq = pos(&out, "CREATE SEQUENCE \"shop\".\"order_no\"");
        let users = pos(&out, "CREATE TABLE \"shop\".\"users\"");
        let orders = pos(&out, "CREATE TABLE \"shop\".\"orders\"");
        let items = pos(&out, "CREATE TABLE \"shop\".\"order_items\"");
        let totals = pos(&out, "CREATE VIEW \"shop\".\"order_totals\"");
        let big = pos(&out, "CREATE VIEW \"shop\".\"big_orders\"");
        let func = pos(&out, "CREATE FUNCTION \"shop\".\"touch\"");
        let trig = pos(&out, "CREATE TRIGGER \"items_touch\"");
        assert!(seq < users && users < orders && orders < items, "{out}");
        assert!(items < totals && totals < big && big < func && func < trig, "{out}");
        assert!(out.contains("CREATE INDEX \"items_order_idx\""));
    }

    #[test]
    fn cycles_add_foreign_keys_afterwards() {
        let m = model();
        let out = generate_ddl(&m, &[sel("table", "a"), sel("table", "b")], DdlOptions::default());
        let a = pos(&out, "CREATE TABLE \"shop\".\"a\"");
        let b = pos(&out, "CREATE TABLE \"shop\".\"b\"");
        let alter = pos(&out, "ALTER TABLE");
        assert!(a.max(b) < alter, "{out}");
        assert_eq!(out.matches("FOREIGN KEY").count(), 2);
    }

    #[test]
    fn drops_if_not_exists_and_plain_names() {
        let m = model();
        let out = generate_ddl(
            &m,
            &[sel("table", "orders"), sel("table", "users"), sel("view", "order_totals")],
            DdlOptions {
                include_drops: true,
                if_not_exists: true,
                qualify: false,
            },
        );
        let drop_view = pos(&out, "DROP VIEW IF EXISTS \"order_totals\";");
        let drop_orders = pos(&out, "DROP TABLE IF EXISTS \"orders\";");
        let drop_users = pos(&out, "DROP TABLE IF EXISTS \"users\";");
        assert!(drop_view < drop_orders && drop_orders < drop_users, "{out}");
        assert!(out.contains("CREATE TABLE IF NOT EXISTS \"users\""));
        assert!(out.contains("REFERENCES \"users\" (\"id\")"), "{out}");
        assert!(out.contains("CREATE OR REPLACE VIEW \"order_totals\""));
        assert!(!out.contains("\"shop\"."));
    }

    #[test]
    fn whole_schema_and_object_list() {
        let m = model();
        let out = generate_ddl(&m, &[sel("schema", "")], DdlOptions::default());
        assert_eq!(out.matches("CREATE TABLE").count(), 5);
        let objs = schema_objects(m.loaded.get("shop").unwrap());
        assert!(objs.iter().any(|o| o.kind == "view" && o.name == "big_orders"));
        assert!(objs.iter().any(|o| o.kind == "sequence"));
    }
}
