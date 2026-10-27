//! Migration scripts: the statements that turn the target schema into the source schema, in
//! the target's dialect. The script is only text; Forge shows it in a console and never runs
//! it without the user's explicit confirmation (and the console's read-only / production
//! guard still applies).
//!
//! Statements are ordered so each one can run: foreign keys and views that change are
//! dropped first, then indexes, primary keys and checks, then tables; new tables, columns,
//! keys, indexes and checks follow, and foreign keys and views come last. SQLite cannot alter
//! most of a table, so a changed SQLite table is rebuilt (create the new table, copy the
//! common columns, drop the old one, rename) with foreign key enforcement off, and its
//! indexes and triggers are created again. Drops are marked destructive and can be left out.

use std::collections::BTreeSet;

use forge_dbmodel::{
    generate, ColumnInfo, ForeignKey, IndexInfo, SchemaInfo, TableInfo, TableKind,
};
use forge_sql::{split_statements, Dialect};
use serde::{Deserialize, Serialize};

use crate::compare::{self, CompareOptions, DiffKind, ItemCategory};

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationOptions {
    /// Drop tables, views and columns that exist only in the target.
    #[serde(default = "yes")]
    pub drops: bool,
    #[serde(default)]
    pub compare: CompareOptions,
}

impl Default for MigrationOptions {
    fn default() -> Self {
        Self {
            drops: true,
            compare: CompareOptions::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationStatement {
    /// Without the terminating `;`.
    pub sql: String,
    /// Loses data (drops a table or column, or rebuilds a table).
    #[serde(default)]
    pub destructive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Migration {
    pub dialect: Dialect,
    /// The whole script, with a header and the warnings as comments.
    pub sql: String,
    pub statements: Vec<MigrationStatement>,
    /// What needs a look before running (or could not be generated).
    pub warnings: Vec<String>,
    pub destructive: bool,
}

const PHASES: usize = 8;

struct Gen<'a> {
    d: Dialect,
    /// Schema the statements qualify names with (the target's).
    schema: &'a str,
    /// The source schema's name, mapped to `schema` in foreign keys.
    from_schema: &'a str,
    phases: [Vec<MigrationStatement>; PHASES],
    warnings: Vec<String>,
    opts: &'a MigrationOptions,
    rebuilt: bool,
}

impl<'a> Gen<'a> {
    fn push(&mut self, phase: usize, sql: impl Into<String>, destructive: bool) {
        let sql = sql.into();
        let sql = sql.trim().trim_end_matches(';').trim_end().to_string();
        if !sql.is_empty() {
            self.phases[phase].push(MigrationStatement { sql, destructive });
        }
    }

    fn warn(&mut self, w: impl Into<String>) {
        self.warnings.push(w.into());
    }

    fn q(&self, table: &str) -> String {
        self.d.qualified(self.schema, table)
    }

    fn id(&self, name: &str) -> String {
        self.d.quote_ident(name)
    }

    fn cols(&self, cs: &[String]) -> String {
        cs.iter().map(|c| self.id(c)).collect::<Vec<_>>().join(", ")
    }

    /// A foreign key of the source pointing into the target schema.
    fn remap(&self, f: &ForeignKey) -> ForeignKey {
        let mut f = f.clone();
        if f.ref_schema.is_empty() || f.ref_schema.eq_ignore_ascii_case(self.from_schema) {
            f.ref_schema = self.schema.to_string();
        }
        f
    }

    /// `"c" type DEFAULT x NOT NULL` (Oracle wants the default before constraints).
    fn col_def(&self, c: &ColumnInfo) -> String {
        let mut s = format!("{} {}", self.id(&c.name), c.data_type);
        if let Some(def) = &c.default {
            s.push_str(&format!(" DEFAULT {def}"));
        }
        if !c.nullable && self.d != Dialect::ClickHouse {
            s.push_str(" NOT NULL");
        }
        if self.d == Dialect::MySql {
            if c.auto_increment {
                s.push_str(" AUTO_INCREMENT");
            }
            if !c.comment.is_empty() {
                s.push_str(&format!(" COMMENT {}", self.d.quote_str(&c.comment)));
            }
        }
        s
    }

    fn create_table(&mut self, phase: usize, schema: &str, t: &TableInfo) {
        for st in split_statements(&generate::table(self.d, schema, t), self.d) {
            self.push(phase, st.text, false);
        }
    }

    fn add_fk(&mut self, table: &str, f: &ForeignKey) {
        let f = self.remap(f);
        let mut s = format!(
            "ALTER TABLE {} ADD CONSTRAINT {} FOREIGN KEY ({}) REFERENCES {} ({})",
            self.q(table),
            self.id(&f.name),
            self.cols(&f.columns),
            self.d.qualified(&f.ref_schema, &f.ref_table),
            self.cols(&f.ref_columns)
        );
        if !f.on_delete.is_empty() && !f.on_delete.eq_ignore_ascii_case("NO ACTION") {
            s.push_str(&format!(" ON DELETE {}", f.on_delete));
        }
        if self.d != Dialect::Oracle
            && !f.on_update.is_empty()
            && !f.on_update.eq_ignore_ascii_case("NO ACTION")
        {
            s.push_str(&format!(" ON UPDATE {}", f.on_update));
        }
        if self.d == Dialect::ClickHouse {
            self.warn(format!(
                "ClickHouse has no foreign keys: {table}.{} is left out",
                f.name
            ));
            return;
        }
        self.push(6, s, false);
    }

    fn drop_fk(&mut self, table: &str, f: &ForeignKey) {
        let sql = match self.d {
            Dialect::MySql => format!(
                "ALTER TABLE {} DROP FOREIGN KEY {}",
                self.q(table),
                self.id(&f.name)
            ),
            Dialect::ClickHouse => return,
            _ => format!(
                "ALTER TABLE {} DROP CONSTRAINT {}",
                self.q(table),
                self.id(&f.name)
            ),
        };
        self.push(1, sql, false);
    }

    fn index_name(table: &str, i: &IndexInfo) -> String {
        if compare::generated_index_name(&i.name) {
            format!(
                "{table}_{}_{}",
                i.columns.join("_"),
                if i.unique { "unique" } else { "index" }
            )
        } else {
            i.name.clone()
        }
    }

    fn create_index(&mut self, table: &str, i: &IndexInfo) {
        if self.d == Dialect::ClickHouse {
            self.warn(format!(
                "ClickHouse data skipping indexes are not generated: {table}.{}",
                i.name
            ));
            return;
        }
        let name = Self::index_name(table, i);
        let sql = format!(
            "CREATE {}INDEX {} ON {} ({})",
            if i.unique { "UNIQUE " } else { "" },
            self.id(&name),
            self.q(table),
            self.cols(&i.columns)
        );
        self.push(5, sql, false);
    }

    fn drop_index(&mut self, table: &str, i: &IndexInfo) {
        let name = self.id(&i.name);
        let sql = match self.d {
            Dialect::MySql | Dialect::SqlServer => {
                format!("DROP INDEX {name} ON {}", self.q(table))
            }
            Dialect::Sqlite => format!("DROP INDEX {name}"),
            Dialect::ClickHouse => format!("ALTER TABLE {} DROP INDEX {name}", self.q(table)),
            _ => format!("DROP INDEX {}", self.d.qualified(self.schema, &i.name)),
        };
        self.push(2, sql, false);
    }

    fn drop_pk(&mut self, t: &TableInfo) {
        let sql = match self.d {
            Dialect::MySql => format!("ALTER TABLE {} DROP PRIMARY KEY", self.q(&t.name)),
            Dialect::ClickHouse => {
                self.warn(format!(
                    "The primary key of ClickHouse table {} cannot be changed",
                    t.name
                ));
                return;
            }
            _ => {
                let name = t
                    .indexes
                    .iter()
                    .find(|i| i.primary && !i.name.is_empty())
                    .map(|i| i.name.clone())
                    .unwrap_or_else(|| format!("{}_pkey", t.name));
                format!(
                    "ALTER TABLE {} DROP CONSTRAINT {}",
                    self.q(&t.name),
                    self.id(&name)
                )
            }
        };
        self.push(2, sql, false);
    }

    fn add_pk(&mut self, table: &str, pk: &[String]) {
        if self.d == Dialect::ClickHouse {
            return;
        }
        let sql = format!(
            "ALTER TABLE {} ADD PRIMARY KEY ({})",
            self.q(table),
            self.cols(pk)
        );
        self.push(5, sql, false);
    }

    fn add_column(&mut self, table: &str, c: &ColumnInfo) {
        if !c.nullable && c.default.is_none() && !c.auto_increment {
            self.warn(format!(
                "{table}.{} is NOT NULL without a default: adding it fails when {table} has rows",
                c.name
            ));
        }
        let def = self.col_def(c);
        let t = self.q(table);
        let sql = match self.d {
            Dialect::SqlServer => format!("ALTER TABLE {t} ADD {def}"),
            Dialect::Oracle => format!("ALTER TABLE {t} ADD ({def})"),
            _ => format!("ALTER TABLE {t} ADD COLUMN {def}"),
        };
        self.push(4, sql, false);
    }

    fn drop_column(&mut self, table: &str, c: &ColumnInfo) {
        if !self.opts.drops {
            self.warn(format!(
                "Column {table}.{} exists only in the target; not dropped",
                c.name
            ));
            return;
        }
        let sql = format!(
            "ALTER TABLE {} DROP COLUMN {}",
            self.q(table),
            self.id(&c.name)
        );
        self.push(4, sql, true);
    }

    fn alter_column(&mut self, table: &str, s: &ColumnInfo, t: &ColumnInfo) {
        let type_changed =
            compare::normalize_type(&s.data_type) != compare::normalize_type(&t.data_type);
        let null_changed = s.nullable != t.nullable;
        let default_changed = !self.opts.compare.ignore_defaults && s.default != t.default;
        let comment_changed = !self.opts.compare.ignore_comments && s.comment != t.comment;
        let tq = self.q(table);
        let c = self.id(&s.name);
        if s.auto_increment != t.auto_increment && self.d != Dialect::MySql {
            self.warn(format!(
                "Auto increment of {table}.{} differs; change its identity or sequence by hand",
                s.name
            ));
        }
        match self.d {
            Dialect::Postgres => {
                if type_changed {
                    self.push(
                        4,
                        format!(
                            "ALTER TABLE {tq} ALTER COLUMN {c} TYPE {ty} USING {c}::{ty}",
                            ty = s.data_type
                        ),
                        false,
                    );
                }
                if null_changed {
                    let what = if s.nullable {
                        "DROP NOT NULL"
                    } else {
                        "SET NOT NULL"
                    };
                    self.push(
                        4,
                        format!("ALTER TABLE {tq} ALTER COLUMN {c} {what}"),
                        false,
                    );
                }
                if default_changed {
                    let what = match &s.default {
                        Some(d) => format!("SET DEFAULT {d}"),
                        None => "DROP DEFAULT".into(),
                    };
                    self.push(
                        4,
                        format!("ALTER TABLE {tq} ALTER COLUMN {c} {what}"),
                        false,
                    );
                }
            }
            Dialect::MySql => {
                if type_changed
                    || null_changed
                    || default_changed
                    || comment_changed
                    || s.auto_increment != t.auto_increment
                {
                    let def = self.col_def(s);
                    self.push(4, format!("ALTER TABLE {tq} MODIFY COLUMN {def}"), false);
                }
                return;
            }
            Dialect::SqlServer => {
                if type_changed || null_changed {
                    let null = if s.nullable { "NULL" } else { "NOT NULL" };
                    self.push(
                        4,
                        format!("ALTER TABLE {tq} ALTER COLUMN {c} {} {null}", s.data_type),
                        false,
                    );
                }
                if default_changed {
                    self.warn(format!(
                        "SQL Server keeps defaults as constraints: change the default of {table}.{} by hand",
                        s.name
                    ));
                }
            }
            Dialect::Oracle => {
                let mut parts = vec![c.clone()];
                if type_changed {
                    parts.push(s.data_type.clone());
                }
                if default_changed {
                    parts.push(format!(
                        "DEFAULT {}",
                        s.default.as_deref().unwrap_or("NULL")
                    ));
                }
                if null_changed {
                    parts.push(if s.nullable {
                        "NULL".into()
                    } else {
                        "NOT NULL".into()
                    });
                }
                if parts.len() > 1 {
                    self.push(
                        4,
                        format!("ALTER TABLE {tq} MODIFY ({})", parts.join(" ")),
                        false,
                    );
                }
            }
            Dialect::ClickHouse => {
                if type_changed || default_changed {
                    let mut s2 = format!("ALTER TABLE {tq} MODIFY COLUMN {c} {}", s.data_type);
                    if let Some(d) = &s.default {
                        s2.push_str(&format!(" DEFAULT {d}"));
                    }
                    self.push(4, s2, false);
                }
                if comment_changed {
                    self.push(
                        4,
                        format!(
                            "ALTER TABLE {tq} COMMENT COLUMN {c} {}",
                            self.d.quote_str(&s.comment)
                        ),
                        false,
                    );
                }
                return;
            }
            _ => {}
        }
        if comment_changed {
            self.column_comment(table, s);
        }
    }

    fn column_comment(&mut self, table: &str, c: &ColumnInfo) {
        match self.d {
            Dialect::Postgres | Dialect::Oracle => {
                let sql = format!(
                    "COMMENT ON COLUMN {}.{} IS {}",
                    self.q(table),
                    self.id(&c.name),
                    self.d.quote_str(&c.comment)
                );
                self.push(5, sql, false);
            }
            _ => self.warn(format!(
                "The comment of {table}.{} is not generated for this dialect",
                c.name
            )),
        }
    }

    fn table_comment(&mut self, t: &TableInfo) {
        let sql = match self.d {
            Dialect::Postgres | Dialect::Oracle => {
                format!(
                    "COMMENT ON TABLE {} IS {}",
                    self.q(&t.name),
                    self.d.quote_str(&t.comment)
                )
            }
            Dialect::MySql => format!(
                "ALTER TABLE {} COMMENT = {}",
                self.q(&t.name),
                self.d.quote_str(&t.comment)
            ),
            _ => {
                self.warn(format!(
                    "The comment of {} is not generated for this dialect",
                    t.name
                ));
                return;
            }
        };
        self.push(5, sql, false);
    }

    fn check_sql(&self, table: &str, name: &str, expr: &str) -> String {
        let e = expr.trim();
        let e = if e.starts_with('(') {
            e.to_string()
        } else {
            format!("({e})")
        };
        if name.is_empty() {
            format!("ALTER TABLE {} ADD CHECK {e}", self.q(table))
        } else {
            format!(
                "ALTER TABLE {} ADD CONSTRAINT {} CHECK {e}",
                self.q(table),
                self.id(name)
            )
        }
    }

    fn drop_view(&mut self, t: &TableInfo, destructive: bool) {
        let kw = if t.kind == TableKind::MaterializedView {
            "MATERIALIZED VIEW"
        } else {
            "VIEW"
        };
        let sql = format!("DROP {kw} {}", self.q(&t.name));
        self.push(1, sql, destructive);
    }

    /// The source table as it is created in the target: foreign keys into the target schema,
    /// optionally without them, and without the names SQLite generates.
    fn prepared(&self, s: &TableInfo, with_fks: bool) -> TableInfo {
        let mut t = s.clone();
        t.foreign_keys = if with_fks {
            s.foreign_keys
                .iter()
                .filter(|f| !f.is_virtual)
                .map(|f| self.remap(f))
                .collect()
        } else {
            Vec::new()
        };
        t.columns.retain(|c| !c.is_virtual);
        for i in &mut t.indexes {
            if i.name.starts_with("sqlite_autoindex_") {
                i.name.clear();
            }
        }
        t.triggers.clear();
        t
    }

    fn added_table(&mut self, s: &TableInfo) {
        if s.kind.is_view() {
            let t = self.prepared(s, false);
            for st in split_statements(&generate::table(self.d, self.schema, &t), self.d) {
                self.push(6, st.text, false);
            }
            return;
        }
        let inline = self.d == Dialect::Sqlite;
        let t = self.prepared(s, inline);
        self.create_table(4, self.schema, &t);
        if self.d == Dialect::ClickHouse {
            self.warn(format!("Add an ENGINE clause to CREATE TABLE {}", s.name));
        }
        if !inline {
            for f in compare::real_fks(s).cloned().collect::<Vec<_>>() {
                self.add_fk(&s.name, &f);
            }
        }
    }

    fn removed_table(&mut self, t: &TableInfo) {
        if !self.opts.drops {
            self.warn(format!("{} exists only in the target; not dropped", t.name));
            return;
        }
        if t.kind.is_view() {
            self.drop_view(t, false);
        } else {
            let sql = format!("DROP TABLE {}", self.q(&t.name));
            self.push(3, sql, true);
        }
    }

    /// SQLite: whether the changes need a rebuild of the table.
    fn sqlite_needs_rebuild(&self, s: &TableInfo, t: &TableInfo) -> bool {
        compare::table_items(s, t, &self.opts.compare)
            .iter()
            .any(|i| match i.category {
                ItemCategory::Column => match i.kind {
                    DiffKind::Added => s.column(&i.name).is_some_and(|c| {
                        (!c.nullable && c.default.is_none())
                            || s.primary_key
                                .iter()
                                .any(|p| p.eq_ignore_ascii_case(&c.name))
                    }),
                    DiffKind::Removed => self.opts.drops,
                    DiffKind::Changed => true,
                },
                ItemCategory::Index => {
                    compare::generated_index_name(&i.name)
                        || s.indexes
                            .iter()
                            .chain(&t.indexes)
                            .any(|x| x.name == i.name && x.unique)
                }
                ItemCategory::Comment => false,
                _ => true,
            })
    }

    fn sqlite_rebuild(&mut self, s: &TableInfo, t: &TableInfo) {
        self.rebuilt = true;
        let mut new = self.prepared(s, true);
        if !self.opts.drops {
            for c in t
                .columns
                .iter()
                .filter(|c| !c.is_virtual && s.column(&c.name).is_none())
            {
                self.warn(format!(
                    "Column {}.{} exists only in the target; kept",
                    t.name, c.name
                ));
                new.columns.push(c.clone());
            }
        }
        let indexes: Vec<IndexInfo> = new
            .indexes
            .iter()
            .filter(|i| !i.unique && !i.primary)
            .cloned()
            .collect();
        new.indexes.retain(|i| i.unique || i.primary);
        let tmp = format!("_forge_new_{}", s.name);
        new.name = tmp.clone();
        let common: Vec<String> = new
            .columns
            .iter()
            .filter(|c| t.column(&c.name).is_some_and(|x| !x.is_virtual))
            .map(|c| self.id(&c.name))
            .collect();
        for c in new.columns.iter().filter(|c| t.column(&c.name).is_none()) {
            if !c.nullable && c.default.is_none() {
                self.warn(format!(
                    "{}.{} is NOT NULL without a default: copying the rows of {} fails",
                    s.name, c.name, s.name
                ));
            }
        }
        self.create_table(4, self.schema, &new);
        let cols = common.join(", ");
        let copy = format!(
            "INSERT INTO {} ({cols}) SELECT {cols} FROM {}",
            self.id(&tmp),
            self.q(&s.name)
        );
        self.push(4, copy, false);
        let drop = format!("DROP TABLE {}", self.q(&s.name));
        self.push(4, drop, true);
        let rename = format!(
            "ALTER TABLE {} RENAME TO {}",
            self.id(&tmp),
            self.id(&s.name)
        );
        self.push(4, rename, false);
        for i in indexes {
            self.create_index(&s.name, &i);
        }
        for tr in &t.triggers {
            let def = tr.definition.trim();
            if def.len() > 6 && def[..6].eq_ignore_ascii_case("create") {
                self.push(6, def.to_string(), false);
            } else {
                self.warn(format!(
                    "Trigger {} of {} is dropped by the rebuild; create it again",
                    tr.name, t.name
                ));
            }
        }
    }

    fn changed_table(&mut self, s: &TableInfo, t: &TableInfo) {
        if s.kind.is_view() || t.kind.is_view() {
            if t.kind.is_view() {
                self.drop_view(t, false);
            } else {
                self.removed_table(t);
            }
            self.added_table(s);
            return;
        }
        if self.d == Dialect::Sqlite && self.sqlite_needs_rebuild(s, t) {
            self.sqlite_rebuild(s, t);
            return;
        }
        let table = s.name.clone();
        for i in compare::table_items(s, t, &self.opts.compare) {
            match i.category {
                ItemCategory::Column => match i.kind {
                    DiffKind::Added => {
                        let c = s.column(&i.name).cloned().expect("source column");
                        self.add_column(&table, &c);
                        if !c.comment.is_empty()
                            && self.d != Dialect::MySql
                            && !self.opts.compare.ignore_comments
                        {
                            self.column_comment(&table, &c);
                        }
                    }
                    DiffKind::Removed => {
                        let c = t.column(&i.name).cloned().expect("target column");
                        self.drop_column(&table, &c);
                    }
                    DiffKind::Changed => {
                        let (sc, tc) = (
                            s.column(&i.name).cloned().expect("column"),
                            t.column(&i.name).cloned().expect("column"),
                        );
                        self.alter_column(&table, &sc, &tc);
                    }
                },
                ItemCategory::PrimaryKey => {
                    if i.kind != DiffKind::Added {
                        self.drop_pk(t);
                    }
                    if i.kind != DiffKind::Removed {
                        self.add_pk(&table, &s.primary_key);
                    }
                }
                ItemCategory::Index => {
                    let si = compare::plain_indexes(s)
                        .find(|x| x.name == i.name)
                        .cloned();
                    let ti = match (i.kind, &si) {
                        (DiffKind::Removed, _) => compare::plain_indexes(t)
                            .find(|x| x.name == i.name)
                            .cloned(),
                        (DiffKind::Changed, Some(si)) => compare::plain_indexes(t)
                            .find(|x| compare::same_index(si, x))
                            .cloned(),
                        _ => None,
                    };
                    if let Some(x) = ti {
                        self.drop_index(&table, &x);
                    }
                    if let (true, Some(x)) = (i.kind != DiffKind::Removed, si) {
                        self.create_index(&table, &x);
                    }
                }
                ItemCategory::ForeignKey => {
                    if i.kind == DiffKind::Removed {
                        if let Some(f) = compare::real_fks(t).find(|f| f.name == i.name).cloned() {
                            self.drop_fk(&table, &f);
                        }
                    } else {
                        let sf = compare::real_fks(s)
                            .find(|f| f.name == i.name)
                            .cloned()
                            .expect("source fk");
                        if i.kind == DiffKind::Changed {
                            if let Some(tf) = compare::real_fks(t)
                                .find(|f| compare::same_fk(&sf, f))
                                .cloned()
                            {
                                self.drop_fk(&table, &tf);
                            }
                        }
                        self.add_fk(&table, &sf);
                    }
                }
                ItemCategory::Check => {
                    if i.kind == DiffKind::Removed {
                        if i.name.is_empty() {
                            self.warn(format!("An unnamed check of {table} exists only in the target; drop it by hand"));
                        } else {
                            let sql = match self.d {
                                Dialect::MySql => format!(
                                    "ALTER TABLE {} DROP CHECK {}",
                                    self.q(&table),
                                    self.id(&i.name)
                                ),
                                _ => format!(
                                    "ALTER TABLE {} DROP CONSTRAINT {}",
                                    self.q(&table),
                                    self.id(&i.name)
                                ),
                            };
                            self.push(2, sql, false);
                        }
                    } else {
                        let sql = self.check_sql(
                            &table,
                            &i.name,
                            i.source.as_deref().unwrap_or_default(),
                        );
                        self.push(5, sql, false);
                    }
                }
                ItemCategory::Comment => self.table_comment(s),
                ItemCategory::Definition => {}
            }
        }
    }
}

/// Tables in dependency order: referenced tables before the tables that reference them.
fn dependency_order(tables: Vec<&TableInfo>) -> Vec<&TableInfo> {
    let names: BTreeSet<String> = tables.iter().map(|t| t.name.to_ascii_lowercase()).collect();
    let mut done: BTreeSet<String> = BTreeSet::new();
    let mut out = Vec::new();
    let mut rest = tables;
    while !rest.is_empty() {
        let (ready, wait): (Vec<&TableInfo>, Vec<&TableInfo>) = rest.into_iter().partition(|t| {
            t.foreign_keys.iter().all(|f| {
                let r = f.ref_table.to_ascii_lowercase();
                r == t.name.to_ascii_lowercase() || !names.contains(&r) || done.contains(&r)
            })
        });
        if ready.is_empty() {
            // A cycle: take the rest as they are.
            out.extend(wait);
            break;
        }
        for t in &ready {
            done.insert(t.name.to_ascii_lowercase());
        }
        out.extend(ready);
        rest = wait;
    }
    out
}

/// The migration that makes `target` look like `source`, in dialect `d`.
pub fn migration(
    d: Dialect,
    source: &SchemaInfo,
    target: &SchemaInfo,
    opts: &MigrationOptions,
) -> Migration {
    let mut g = Gen {
        d,
        schema: &target.name,
        from_schema: &source.name,
        phases: Default::default(),
        warnings: Vec::new(),
        opts,
        rebuilt: false,
    };
    let pairs = compare::pairs(source, target, &opts.compare);
    let added = dependency_order(
        pairs
            .iter()
            .filter_map(|p| match p {
                (Some(s), None) => Some(*s),
                _ => None,
            })
            .collect(),
    );
    let mut removed = dependency_order(
        pairs
            .iter()
            .filter_map(|p| match p {
                (None, Some(t)) => Some(*t),
                _ => None,
            })
            .collect(),
    );
    removed.reverse();
    for t in removed {
        g.removed_table(t);
    }
    for s in added {
        g.added_table(s);
    }
    for (s, t) in &pairs {
        if let (Some(s), Some(t)) = (s, t) {
            if !compare::table_items(s, t, &opts.compare).is_empty() {
                g.changed_table(s, t);
            }
        }
    }
    if g.rebuilt {
        g.push(0, "PRAGMA foreign_keys = OFF", false);
        g.push(7, "PRAGMA foreign_keys = ON", false);
    }
    let statements: Vec<MigrationStatement> = g.phases.into_iter().flatten().collect();
    let warnings = g.warnings;
    let destructive = statements.iter().any(|s| s.destructive);
    let mut sql = format!(
        "-- Migration of {} to the structure of {}, generated by Forge.\n-- Review it before running it.\n",
        if target.name.is_empty() { "the target" } else { &target.name },
        if source.name.is_empty() { "the source" } else { &source.name },
    );
    for w in &warnings {
        sql.push_str(&format!("-- Warning: {w}\n"));
    }
    if statements.is_empty() {
        sql.push_str("-- The schemas are identical: nothing to migrate.\n");
    }
    for st in &statements {
        sql.push('\n');
        if st.destructive {
            sql.push_str("-- Destructive: drops data\n");
        }
        sql.push_str(&st.sql);
        sql.push_str(";\n");
    }
    Migration {
        dialect: d,
        sql,
        statements,
        warnings,
        destructive,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compare::tests::dev_and_prod;

    fn sqls(m: &Migration) -> Vec<&str> {
        m.statements.iter().map(|s| s.sql.as_str()).collect()
    }

    #[test]
    fn postgres_migration_in_dependency_order() {
        let (dev, prod) = dev_and_prod();
        let m = migration(Dialect::Postgres, &dev, &prod, &MigrationOptions::default());
        let s = sqls(&m);
        assert_eq!(
            s,
            [
                r#"ALTER TABLE "prod"."orders" DROP CONSTRAINT "orders_user_id_foreign""#,
                r#"DROP INDEX "prod"."orders_user_id_index""#,
                r#"DROP TABLE "prod"."legacy""#,
                "CREATE TABLE \"prod\".\"audit\" (\n    \"id\" integer NOT NULL,\n    \"message\" text,\n    PRIMARY KEY (\"id\")\n)",
                r#"ALTER TABLE "prod"."users" ALTER COLUMN "name" TYPE varchar(150) USING "name"::varchar(150)"#,
                r#"ALTER TABLE "prod"."users" ALTER COLUMN "name" SET NOT NULL"#,
                r#"ALTER TABLE "prod"."users" ADD COLUMN "last_login" timestamp DEFAULT CURRENT_TIMESTAMP"#,
                r#"ALTER TABLE "prod"."orders" ADD CONSTRAINT "orders_user_id_foreign" FOREIGN KEY ("user_id") REFERENCES "prod"."users" ("id") ON DELETE RESTRICT"#,
            ],
            "{}",
            m.sql
        );
        assert!(m.destructive);
        assert!(m.sql.contains("-- Destructive: drops data\nDROP TABLE"));
        assert!(m.warnings.is_empty(), "{:?}", m.warnings);
    }

    #[test]
    fn mysql_sqlserver_oracle_and_clickhouse_alter_syntax() {
        let (dev, prod) = dev_and_prod();
        let my = migration(Dialect::MySql, &dev, &prod, &MigrationOptions::default());
        let s = sqls(&my);
        assert!(
            s.contains(&"ALTER TABLE `prod`.`orders` DROP FOREIGN KEY `orders_user_id_foreign`"),
            "{s:?}"
        );
        assert!(s.contains(&"DROP INDEX `orders_user_id_index` ON `prod`.`orders`"));
        assert!(
            s.contains(&"ALTER TABLE `prod`.`users` MODIFY COLUMN `name` varchar(150) NOT NULL")
        );
        let ss = migration(
            Dialect::SqlServer,
            &dev,
            &prod,
            &MigrationOptions::default(),
        );
        let s = sqls(&ss);
        assert!(
            s.contains(&"ALTER TABLE [prod].[users] ALTER COLUMN [name] varchar(150) NOT NULL"),
            "{s:?}"
        );
        assert!(s.contains(
            &"ALTER TABLE [prod].[users] ADD [last_login] timestamp DEFAULT CURRENT_TIMESTAMP"
        ));
        let ora = migration(Dialect::Oracle, &dev, &prod, &MigrationOptions::default());
        let s = sqls(&ora);
        assert!(
            s.contains(&r#"ALTER TABLE "prod"."users" MODIFY ("name" varchar(150) NOT NULL)"#),
            "{s:?}"
        );
        assert!(s.contains(
            &r#"ALTER TABLE "prod"."users" ADD ("last_login" timestamp DEFAULT CURRENT_TIMESTAMP)"#
        ));
        let ch = migration(
            Dialect::ClickHouse,
            &dev,
            &prod,
            &MigrationOptions::default(),
        );
        assert!(
            ch.warnings.iter().any(|w| w.contains("ENGINE")),
            "{:?}",
            ch.warnings
        );
        assert!(!sqls(&ch).iter().any(|s| s.contains("FOREIGN KEY")));
    }

    #[test]
    fn without_drops_and_identical() {
        let (dev, prod) = dev_and_prod();
        let opts = MigrationOptions {
            drops: false,
            ..Default::default()
        };
        let m = migration(Dialect::Postgres, &dev, &prod, &opts);
        assert!(!m.destructive);
        assert!(!sqls(&m).iter().any(|s| s.starts_with("DROP TABLE")));
        assert!(m
            .warnings
            .iter()
            .any(|w| w.contains("legacy exists only in the target")));
        let same = migration(
            Dialect::Postgres,
            &prod,
            &prod,
            &MigrationOptions::default(),
        );
        assert!(same.statements.is_empty());
        assert!(same.sql.contains("identical"));
    }

    #[test]
    fn sqlite_rebuilds_changed_tables() {
        let (mut dev, mut prod) = dev_and_prod();
        dev.name = "main".into();
        prod.name = "main".into();
        dev.tables[1].foreign_keys[0].ref_schema = "main".into();
        prod.tables[2].triggers.push(forge_dbmodel::TriggerInfo {
            name: "users_touch".into(),
            definition: "CREATE TRIGGER users_touch AFTER UPDATE ON users BEGIN SELECT 1; END"
                .into(),
            ..Default::default()
        });
        let m = migration(Dialect::Sqlite, &dev, &prod, &MigrationOptions::default());
        let s = sqls(&m);
        assert_eq!(s.first(), Some(&"PRAGMA foreign_keys = OFF"));
        assert_eq!(s.last(), Some(&"PRAGMA foreign_keys = ON"));
        let at = s
            .iter()
            .position(|x| x.starts_with(r#"CREATE TABLE "_forge_new_users""#))
            .expect("rebuild");
        assert_eq!(
            &s[at + 1..at + 4],
            [
                r#"INSERT INTO "_forge_new_users" ("id", "email", "name") SELECT "id", "email", "name" FROM "users""#,
                r#"DROP TABLE "users""#,
                r#"ALTER TABLE "_forge_new_users" RENAME TO "users""#,
            ]
        );
        assert!(s.contains(&"CREATE TRIGGER users_touch AFTER UPDATE ON users BEGIN SELECT 1; END"));
        // The new audit table and orders' foreign key inline (orders is rebuilt too).
        assert!(s.iter().any(|x| x.starts_with(r#"CREATE TABLE "audit""#)));
        let orders = s
            .iter()
            .find(|x| x.starts_with(r#"CREATE TABLE "_forge_new_orders""#))
            .unwrap();
        assert!(
            orders.contains(r#"REFERENCES "users" ("id") ON DELETE RESTRICT"#),
            "{orders}"
        );
        // A nullable column alone is just added.
        let mut a = prod.clone();
        a.tables[2].triggers.clear();
        let mut b = a.clone();
        b.tables[1]
            .columns
            .push(compare::tests::col("note", "text", true));
        let m = migration(Dialect::Sqlite, &b, &a, &MigrationOptions::default());
        assert_eq!(sqls(&m), [r#"ALTER TABLE "orders" ADD COLUMN "note" text"#]);
    }
}
