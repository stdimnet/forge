//! Modify Table: the statements that turn the old table into the edited one. Columns are
//! matched by their original name (set by the dialog), so a changed name is a rename; specs
//! without original names fall back to pairing a removed and an added column at the same
//! position with the same type. Destructive changes (dropped columns, type changes, a
//! rebuilt SQLite table) come with warnings.
//!
//! Order of the statements: rename the table; drop changed or removed foreign keys,
//! checks, unique keys, indexes and the primary key; rename, drop, add and modify columns;
//! add keys, checks, foreign keys and indexes; then comments and table options.

use forge_sql::Dialect;
use serde::{Deserialize, Serialize};

use super::{
    check_clause, column_def, column_type, create_index, create_statements, fk_clause,
    implicit_identity, pk_clause, sqlserver_comment, unique_clause, ColumnSpec, CreateOptions,
    IndexSpec, KeySpec, TableSpec,
};
use crate::model::{CheckInfo, ForeignKey};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesignWarning {
    pub message: String,
    /// Data may be lost (a dropped column, a type change, a rebuilt table).
    pub destructive: bool,
}

/// The Modify Table preview.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AlterPlan {
    pub statements: Vec<String>,
    pub warnings: Vec<DesignWarning>,
}

impl AlterPlan {
    pub fn script(&self) -> String {
        super::join(&self.statements)
    }

    pub fn destructive(&self) -> bool {
        self.warnings.iter().any(|w| w.destructive)
    }

    fn warn(&mut self, message: impl Into<String>, destructive: bool) {
        self.warnings.push(DesignWarning {
            message: message.into(),
            destructive,
        });
    }
}

fn same(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

fn norm_type(t: &str) -> String {
    t.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn norm_default(d: &Option<String>) -> Option<String> {
    d.as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
}

/// Pairs (old index, new index) of matched columns; unmatched old columns are dropped and
/// unmatched new ones added.
pub fn match_columns(
    old: &[ColumnSpec],
    new: &[ColumnSpec],
) -> Vec<(Option<usize>, Option<usize>)> {
    let mut old_used = vec![false; old.len()];
    let mut new_match: Vec<Option<usize>> = vec![None; new.len()];
    let tracked = new.iter().any(|c| c.original.is_some());
    for (ni, c) in new.iter().enumerate() {
        let key = if tracked {
            match &c.original {
                Some(o) => o.as_str(),
                None => continue,
            }
        } else {
            c.name.as_str()
        };
        if let Some(oi) = (0..old.len()).find(|&oi| !old_used[oi] && same(&old[oi].name, key)) {
            old_used[oi] = true;
            new_match[ni] = Some(oi);
        }
    }
    if !tracked {
        // A removed and an added column at the same position with the same type: a rename.
        for ni in 0..new.len() {
            if new_match[ni].is_some() || ni >= old.len() || old_used[ni] {
                continue;
            }
            if norm_type(&old[ni].data_type) == norm_type(&new[ni].data_type) {
                old_used[ni] = true;
                new_match[ni] = Some(ni);
            }
        }
    }
    let mut out: Vec<(Option<usize>, Option<usize>)> = (0..old.len())
        .filter(|&oi| !old_used[oi])
        .map(|oi| (Some(oi), None))
        .collect();
    out.extend(new_match.iter().enumerate().map(|(ni, oi)| (*oi, Some(ni))));
    out
}

pub(crate) fn rename_table(d: Dialect, schema: &str, from: &str, to: &str) -> String {
    match d {
        Dialect::SqlServer => format!(
            "EXEC sp_rename {}, {};",
            d.quote_str(&format!(
                "{}.{}",
                if schema.is_empty() { "dbo" } else { schema },
                from
            )),
            d.quote_str(to)
        ),
        Dialect::MySql => format!(
            "RENAME TABLE {} TO {};",
            d.qualified(schema, from),
            d.qualified(schema, to)
        ),
        Dialect::ClickHouse => format!(
            "RENAME TABLE {} TO {};",
            d.qualified(schema, from),
            d.qualified(schema, to)
        ),
        _ => format!(
            "ALTER TABLE {} RENAME TO {};",
            d.qualified(schema, from),
            d.quote_ident(to)
        ),
    }
}

pub(crate) fn rename_column(d: Dialect, schema: &str, table: &str, from: &str, to: &str) -> String {
    match d {
        Dialect::SqlServer => format!(
            "EXEC sp_rename {}, {}, N'COLUMN';",
            d.quote_str(&format!(
                "{}.{}.{}",
                if schema.is_empty() { "dbo" } else { schema },
                table,
                from
            )),
            d.quote_str(to)
        ),
        _ => format!(
            "ALTER TABLE {} RENAME COLUMN {} TO {};",
            d.qualified(schema, table),
            d.quote_ident(from),
            d.quote_ident(to)
        ),
    }
}

pub(crate) fn drop_fk(d: Dialect, schema: &str, table: &str, name: &str) -> String {
    let t = d.qualified(schema, table);
    match d {
        Dialect::MySql => format!("ALTER TABLE {t} DROP FOREIGN KEY {};", d.quote_ident(name)),
        _ => format!("ALTER TABLE {t} DROP CONSTRAINT {};", d.quote_ident(name)),
    }
}

fn fk_eq(a: &ForeignKey, b: &ForeignKey) -> bool {
    let up = |s: &str| {
        let s = s.trim().to_ascii_uppercase();
        if s.is_empty() {
            "NO ACTION".to_string()
        } else {
            s
        }
    };
    a.columns == b.columns
        && same(&a.ref_table, &b.ref_table)
        && same(&a.ref_schema, &b.ref_schema)
        && a.ref_columns == b.ref_columns
        && up(&a.on_update) == up(&b.on_update)
        && up(&a.on_delete) == up(&b.on_delete)
}

fn check_eq(a: &CheckInfo, b: &CheckInfo) -> bool {
    let n = |s: &str| {
        s.trim()
            .trim_start_matches('(')
            .trim_end_matches(')')
            .split_whitespace()
            .collect::<String>()
    };
    n(&a.expression) == n(&b.expression)
}

/// Items of `old` missing from `new` (or changed), and items of `new` missing from `old`
/// (or changed), matched by name.
fn diff_named<'a, T>(
    old: &'a [T],
    new: &'a [T],
    name: impl Fn(&T) -> &str,
    eq: impl Fn(&T, &T) -> bool,
) -> (Vec<&'a T>, Vec<&'a T>) {
    let removed = old
        .iter()
        .filter(|o| {
            !new.iter()
                .any(|n| same(name(n), name(o)) && !name(n).is_empty() && eq(o, n))
        })
        .collect();
    let added = new
        .iter()
        .filter(|n| {
            !old.iter()
                .any(|o| same(name(n), name(o)) && !name(n).is_empty() && eq(o, n))
        })
        .collect();
    (removed, added)
}

/// Columns renamed by the diff, so keys can be compared under new names.
fn renamed_cols(cols: &[String], renames: &[(String, String)]) -> Vec<String> {
    cols.iter()
        .map(|c| {
            renames
                .iter()
                .find(|(from, _)| same(from, c))
                .map(|(_, to)| to.clone())
                .unwrap_or_else(|| c.clone())
        })
        .collect()
}

/// What SQLite's `ALTER TABLE` cannot do: everything except renames, plain added columns,
/// dropped columns and index changes needs a rebuilt table.
fn sqlite_needs_rebuild(
    old: &TableSpec,
    new: &TableSpec,
    pairs: &[(Option<usize>, Option<usize>)],
) -> bool {
    let keys_changed = old.primary_key.as_ref().map(|k| &k.columns)
        != new.primary_key.as_ref().map(|k| &k.columns)
        || old.unique_keys.len() != new.unique_keys.len()
        || old
            .unique_keys
            .iter()
            .zip(&new.unique_keys)
            .any(|(a, b)| a.columns != b.columns)
        || old.foreign_keys.len() != new.foreign_keys.len()
        || old
            .foreign_keys
            .iter()
            .zip(&new.foreign_keys)
            .any(|(a, b)| !fk_eq(a, b))
        || old.checks.len() != new.checks.len()
        || old
            .checks
            .iter()
            .zip(&new.checks)
            .any(|(a, b)| !check_eq(a, b))
        || old.options != new.options;
    if keys_changed {
        return true;
    }
    pairs.iter().any(|p| match p {
        (Some(oi), Some(ni)) => {
            let (o, n) = (&old.columns[*oi], &new.columns[*ni]);
            norm_type(&o.data_type) != norm_type(&n.data_type)
                || o.nullable != n.nullable
                || norm_default(&o.default) != norm_default(&n.default)
                || o.auto_increment != n.auto_increment
        }
        // An added NOT NULL column needs a default; a key column needs a rebuild.
        (None, Some(ni)) => {
            let n = &new.columns[*ni];
            (!n.nullable && norm_default(&n.default).is_none()) || n.auto_increment
        }
        _ => false,
    })
}

/// SQLite: create the new table, copy the rows, drop the old one and rename.
fn sqlite_rebuild(
    old: &TableSpec,
    new: &TableSpec,
    pairs: &[(Option<usize>, Option<usize>)],
    plan: &mut AlterPlan,
) {
    let d = Dialect::Sqlite;
    let tmp = format!("{}_new", new.name);
    let mut staged = new.clone();
    staged.name = tmp.clone();
    staged.indexes.clear();
    let mut targets = Vec::new();
    let mut sources = Vec::new();
    for p in pairs {
        if let (Some(oi), Some(ni)) = p {
            targets.push(d.quote_ident(&new.columns[*ni].name));
            sources.push(d.quote_ident(&old.columns[*oi].name));
        }
    }
    plan.statements.push("PRAGMA foreign_keys = OFF;".into());
    plan.statements
        .extend(create_statements(d, &staged, CreateOptions::default(), &[]));
    plan.statements.push(format!(
        "INSERT INTO {} ({}) SELECT {} FROM {};",
        d.qualified(&new.schema, &tmp),
        targets.join(", "),
        sources.join(", "),
        d.qualified(&old.schema, &old.name)
    ));
    plan.statements.push(format!(
        "DROP TABLE {};",
        d.qualified(&old.schema, &old.name)
    ));
    plan.statements
        .push(rename_table(d, &new.schema, &tmp, &new.name));
    for i in &new.indexes {
        plan.statements
            .push(create_index(d, &new.name, &new.schema, i, false));
    }
    plan.statements.push("PRAGMA foreign_keys = ON;".into());
    plan.warn(
        format!(
            "SQLite cannot alter {} in place: the table is rebuilt (copied into a new table, the old one dropped)",
            new.name
        ),
        true,
    );
}

/// Statements that change one matched column (type, nullability, default, identity).
fn modify_column(
    d: Dialect,
    t: &str,
    schema: &str,
    o: &ColumnSpec,
    n: &ColumnSpec,
    plan: &mut AlterPlan,
) {
    let target = d.qualified(schema, t);
    let col = d.quote_ident(&n.name);
    let type_changed = norm_type(&o.data_type) != norm_type(&n.data_type);
    let null_changed = o.nullable != n.nullable;
    let default_changed = norm_default(&o.default) != norm_default(&n.default);
    let identity_changed = o.auto_increment != n.auto_increment;
    let comment_changed = o.comment != n.comment;
    if type_changed {
        plan.warn(
            format!(
                "Changing {} from {} to {} may lose or fail to convert data",
                n.name,
                o.data_type.trim(),
                n.data_type.trim()
            ),
            true,
        );
    }
    if null_changed && !n.nullable {
        plan.warn(
            format!(
                "{} becomes NOT NULL: fails if the column has NULL values",
                n.name
            ),
            false,
        );
    }
    match d {
        Dialect::MySql => {
            // MODIFY restates the whole column; comments are part of it.
            if type_changed
                || null_changed
                || default_changed
                || identity_changed
                || comment_changed
            {
                plan.statements.push(format!(
                    "ALTER TABLE {target} MODIFY COLUMN {};",
                    column_def(d, n, false)
                ));
            }
        }
        Dialect::Postgres => {
            if type_changed {
                plan.statements.push(format!(
                    "ALTER TABLE {target} ALTER COLUMN {col} TYPE {} USING {col}::{};",
                    n.data_type.trim(),
                    n.data_type.trim()
                ));
            }
            if null_changed {
                plan.statements.push(format!(
                    "ALTER TABLE {target} ALTER COLUMN {col} {} NOT NULL;",
                    if n.nullable { "DROP" } else { "SET" }
                ));
            }
            if default_changed {
                plan.statements.push(match norm_default(&n.default) {
                    Some(v) => format!("ALTER TABLE {target} ALTER COLUMN {col} SET DEFAULT {v};"),
                    None => format!("ALTER TABLE {target} ALTER COLUMN {col} DROP DEFAULT;"),
                });
            }
            if identity_changed && !implicit_identity(d, n) && !implicit_identity(d, o) {
                plan.statements.push(if n.auto_increment {
                    format!("ALTER TABLE {target} ALTER COLUMN {col} ADD GENERATED BY DEFAULT AS IDENTITY;")
                } else {
                    format!("ALTER TABLE {target} ALTER COLUMN {col} DROP IDENTITY IF EXISTS;")
                });
            }
        }
        Dialect::SqlServer => {
            if type_changed || null_changed {
                plan.statements.push(format!(
                    "ALTER TABLE {target} ALTER COLUMN {col} {}{};",
                    column_type(d, n),
                    if n.nullable { " NULL" } else { " NOT NULL" }
                ));
            }
            if default_changed {
                if norm_default(&o.default).is_some() {
                    // Defaults are constraints with generated names: look the name up.
                    let obj = d.quote_str(&format!(
                        "{}.{}",
                        if schema.is_empty() { "dbo" } else { schema },
                        t
                    ));
                    plan.statements.push(format!(
                        "DECLARE @df sysname = (SELECT name FROM sys.default_constraints WHERE parent_object_id = OBJECT_ID({obj}) AND parent_column_id = COLUMNPROPERTY(OBJECT_ID({obj}), {}, 'ColumnId')) IF @df IS NOT NULL EXEC(N'ALTER TABLE {} DROP CONSTRAINT ' + QUOTENAME(@df));",
                        d.quote_str(&n.name),
                        target.replace('\'', "''")
                    ));
                }
                if let Some(v) = norm_default(&n.default) {
                    plan.statements
                        .push(format!("ALTER TABLE {target} ADD DEFAULT {v} FOR {col};"));
                }
            }
            if identity_changed {
                plan.warn(
                    format!(
                        "SQL Server cannot add or remove IDENTITY on {}: recreate the column",
                        n.name
                    ),
                    false,
                );
            }
        }
        Dialect::Oracle => {
            let mut parts = Vec::new();
            if type_changed {
                parts.push(n.data_type.trim().to_string());
            }
            if default_changed {
                parts.push(format!(
                    "DEFAULT {}",
                    norm_default(&n.default).unwrap_or_else(|| "NULL".into())
                ));
            }
            if null_changed {
                parts.push(if n.nullable { "NULL" } else { "NOT NULL" }.into());
            }
            if !parts.is_empty() {
                plan.statements.push(format!(
                    "ALTER TABLE {target} MODIFY ({col} {});",
                    parts.join(" ")
                ));
            }
            if identity_changed {
                plan.statements.push(if n.auto_increment {
                    format!("ALTER TABLE {target} MODIFY ({col} GENERATED BY DEFAULT AS IDENTITY);")
                } else {
                    format!("ALTER TABLE {target} MODIFY ({col} DROP IDENTITY);")
                });
            }
        }
        Dialect::ClickHouse => {
            if type_changed || null_changed || default_changed {
                let mut l = format!(
                    "ALTER TABLE {target} MODIFY COLUMN {col} {}",
                    column_type(d, n)
                );
                if let Some(v) = norm_default(&n.default) {
                    l.push_str(&format!(" DEFAULT {v}"));
                }
                l.push(';');
                plan.statements.push(l);
            }
            if comment_changed {
                plan.statements.push(format!(
                    "ALTER TABLE {target} COMMENT COLUMN {col} {};",
                    d.quote_str(&n.comment)
                ));
            }
        }
        _ => {}
    }
    if comment_changed {
        column_comment(d, schema, t, o, n, plan);
    }
}

fn column_comment(
    d: Dialect,
    schema: &str,
    t: &str,
    o: &ColumnSpec,
    n: &ColumnSpec,
    plan: &mut AlterPlan,
) {
    match d {
        Dialect::Postgres | Dialect::Oracle => plan.statements.push(format!(
            "COMMENT ON COLUMN {}.{} IS {};",
            d.qualified(schema, t),
            d.quote_ident(&n.name),
            if n.comment.is_empty() {
                "NULL".to_string()
            } else {
                d.quote_str(&n.comment)
            }
        )),
        Dialect::SqlServer => {
            let proc_name = match (o.comment.is_empty(), n.comment.is_empty()) {
                (true, _) => "sp_addextendedproperty",
                (false, true) => "sp_dropextendedproperty",
                (false, false) => "sp_updateextendedproperty",
            };
            plan.statements.push(sqlserver_comment(
                schema,
                t,
                Some(&n.name),
                &n.comment,
                proc_name,
            ));
        }
        _ => {}
    }
}

fn add_column(d: Dialect, t: &TableSpec, c: &ColumnSpec, plan: &mut AlterPlan) {
    let target = d.qualified(&t.schema, &t.name);
    let def = column_def(d, c, false);
    plan.statements.push(match d {
        Dialect::Oracle => format!("ALTER TABLE {target} ADD ({def});"),
        Dialect::SqlServer => format!("ALTER TABLE {target} ADD {def};"),
        _ => format!("ALTER TABLE {target} ADD COLUMN {def};"),
    });
    if !c.nullable && norm_default(&c.default).is_none() && !c.auto_increment {
        plan.warn(
            format!(
                "{} is NOT NULL without a default: adding it fails if the table has rows",
                c.name
            ),
            false,
        );
    }
    if !c.comment.is_empty() && !matches!(d, Dialect::MySql | Dialect::ClickHouse) {
        let empty = ColumnSpec::default();
        column_comment(d, &t.schema, &t.name, &empty, c, plan);
    }
}

fn drop_key(d: Dialect, target: &str, k: &KeySpec, primary: bool) -> String {
    match (d, primary) {
        (Dialect::MySql, true) => format!("ALTER TABLE {target} DROP PRIMARY KEY;"),
        (Dialect::MySql, false) => format!(
            "ALTER TABLE {target} DROP INDEX {};",
            d.quote_ident(&k.name)
        ),
        (Dialect::Oracle, true) if k.name.is_empty() => {
            format!("ALTER TABLE {target} DROP PRIMARY KEY;")
        }
        _ => format!(
            "ALTER TABLE {target} DROP CONSTRAINT {};",
            d.quote_ident(&k.name)
        ),
    }
}

fn drop_index(d: Dialect, schema: &str, table: &str, i: &IndexSpec) -> String {
    match d {
        Dialect::MySql | Dialect::SqlServer => format!(
            "DROP INDEX {} ON {};",
            d.quote_ident(&i.name),
            d.qualified(schema, table)
        ),
        _ => format!("DROP INDEX {};", d.qualified(schema, &i.name)),
    }
}

fn drop_check(d: Dialect, target: &str, c: &CheckInfo) -> String {
    match d {
        Dialect::MySql => format!(
            "ALTER TABLE {target} DROP CHECK {};",
            d.quote_ident(&c.name)
        ),
        _ => format!(
            "ALTER TABLE {target} DROP CONSTRAINT {};",
            d.quote_ident(&c.name)
        ),
    }
}

/// Table comment and options.
fn table_level(d: Dialect, old: &TableSpec, new: &TableSpec, plan: &mut AlterPlan) {
    let target = d.qualified(&new.schema, &new.name);
    let (o, n) = (&old.options, &new.options);
    if old.comment != new.comment {
        match d {
            Dialect::MySql => plan.statements.push(format!(
                "ALTER TABLE {target} COMMENT = {};",
                d.quote_str(&new.comment)
            )),
            Dialect::Postgres | Dialect::Oracle => plan.statements.push(format!(
                "COMMENT ON TABLE {target} IS {};",
                if new.comment.is_empty() {
                    "NULL".into()
                } else {
                    d.quote_str(&new.comment)
                }
            )),
            Dialect::ClickHouse => plan.statements.push(format!(
                "ALTER TABLE {target} MODIFY COMMENT {};",
                d.quote_str(&new.comment)
            )),
            Dialect::SqlServer => {
                let proc_name = match (old.comment.is_empty(), new.comment.is_empty()) {
                    (true, _) => "sp_addextendedproperty",
                    (false, true) => "sp_dropextendedproperty",
                    (false, false) => "sp_updateextendedproperty",
                };
                plan.statements.push(sqlserver_comment(
                    &new.schema,
                    &new.name,
                    None,
                    &new.comment,
                    proc_name,
                ));
            }
            _ => {}
        }
    }
    match d {
        Dialect::MySql => {
            if o.engine != n.engine && !n.engine.is_empty() {
                plan.statements
                    .push(format!("ALTER TABLE {target} ENGINE = {};", n.engine));
            }
            if (o.charset != n.charset || o.collation != n.collation) && !n.charset.is_empty() {
                let mut l = format!("ALTER TABLE {target} DEFAULT CHARACTER SET {}", n.charset);
                if !n.collation.is_empty() {
                    l.push_str(&format!(" COLLATE {}", n.collation));
                }
                l.push(';');
                plan.statements.push(l);
            }
        }
        Dialect::Postgres => {
            if o.tablespace != n.tablespace && !n.tablespace.is_empty() {
                plan.statements.push(format!(
                    "ALTER TABLE {target} SET TABLESPACE {};",
                    d.quote_ident(&n.tablespace)
                ));
            }
            if o.unlogged != n.unlogged {
                plan.statements.push(format!(
                    "ALTER TABLE {target} SET {};",
                    if n.unlogged { "UNLOGGED" } else { "LOGGED" }
                ));
            }
        }
        Dialect::ClickHouse if o.order_by != n.order_by && !n.order_by.is_empty() => {
            plan.statements.push(format!(
                "ALTER TABLE {target} MODIFY ORDER BY {};",
                n.order_by
            ));
        }
        _ => {}
    }
}

/// The Modify Table script: `old` as loaded, `new` as edited.
pub fn alter_table(d: Dialect, old: &TableSpec, new: &TableSpec) -> AlterPlan {
    let mut plan = AlterPlan::default();
    let pairs = match_columns(&old.columns, &new.columns);
    let schema = new.schema.as_str();
    if old.name != new.name && !new.name.trim().is_empty() {
        plan.statements
            .push(rename_table(d, &old.schema, &old.name, &new.name));
    }
    let t = new.name.as_str();
    let target = d.qualified(schema, t);
    if d == Dialect::Sqlite && sqlite_needs_rebuild(old, new, &pairs) {
        sqlite_rebuild(old, new, &pairs, &mut plan);
        for p in &pairs {
            if let (Some(oi), None) = p {
                plan.warn(
                    format!("Column {} is dropped with its data", old.columns[*oi].name),
                    true,
                );
            }
        }
        return plan;
    }
    // Renames, so keys are compared under the new column names.
    let renames: Vec<(String, String)> = pairs
        .iter()
        .filter_map(|p| match p {
            (Some(oi), Some(ni)) if old.columns[*oi].name != new.columns[*ni].name => {
                Some((old.columns[*oi].name.clone(), new.columns[*ni].name.clone()))
            }
            _ => None,
        })
        .collect();
    let mut old_r = old.clone();
    if let Some(pk) = old_r.primary_key.as_mut() {
        pk.columns = renamed_cols(&pk.columns, &renames);
    }
    for k in &mut old_r.unique_keys {
        k.columns = renamed_cols(&k.columns, &renames);
    }
    for f in &mut old_r.foreign_keys {
        f.columns = renamed_cols(&f.columns, &renames);
    }
    for i in &mut old_r.indexes {
        i.columns = renamed_cols(&i.columns, &renames);
    }
    let constraints = d != Dialect::ClickHouse;
    let (fk_gone, fk_new) = diff_named(&old_r.foreign_keys, &new.foreign_keys, |f| &f.name, fk_eq);
    let (ck_gone, ck_new) = diff_named(&old_r.checks, &new.checks, |c| &c.name, check_eq);
    let (uk_gone, uk_new) = diff_named(
        &old_r.unique_keys,
        &new.unique_keys,
        |k| &k.name,
        |a, b| a.columns == b.columns,
    );
    let (ix_gone, ix_new) = diff_named(
        &old_r.indexes,
        &new.indexes,
        |i| &i.name,
        |a, b| a.columns == b.columns && a.unique == b.unique && same(&a.method, &b.method),
    );
    let pk_changed = old_r.primary_key.as_ref().map(|k| &k.columns)
        != new.primary_key.as_ref().map(|k| &k.columns);
    // Drops first.
    if constraints {
        for f in &fk_gone {
            plan.statements.push(drop_fk(d, schema, t, &f.name));
        }
    } else if !new.foreign_keys.is_empty() || !old.foreign_keys.is_empty() {
        plan.warn("ClickHouse has no foreign keys; they are left out", false);
    }
    for c in &ck_gone {
        plan.statements.push(drop_check(d, &target, c));
    }
    if constraints {
        for k in &uk_gone {
            plan.statements.push(drop_key(d, &target, k, false));
        }
        for i in &ix_gone {
            plan.statements.push(drop_index(d, schema, t, i));
        }
        if pk_changed {
            if let Some(pk) = old_r.primary_key.as_ref() {
                plan.statements.push(drop_key(d, &target, pk, true));
            }
        }
    }
    // Columns: renames, drops, adds, changes.
    for (from, to) in &renames {
        let modified = pairs.iter().any(|p| match p {
            (Some(oi), Some(ni)) => {
                same(&old.columns[*oi].name, from) && {
                    let (o, n) = (&old.columns[*oi], &new.columns[*ni]);
                    norm_type(&o.data_type) != norm_type(&n.data_type)
                        || o.nullable != n.nullable
                        || norm_default(&o.default) != norm_default(&n.default)
                        || o.auto_increment != n.auto_increment
                        || o.comment != n.comment
                }
            }
            _ => false,
        });
        if d == Dialect::MySql && modified {
            // CHANGE renames and redefines in one go (see the modify step).
            continue;
        }
        plan.statements.push(rename_column(d, schema, t, from, to));
    }
    for p in &pairs {
        if let (Some(oi), None) = p {
            let c = &old.columns[*oi];
            plan.statements.push(format!(
                "ALTER TABLE {target} DROP COLUMN {};",
                d.quote_ident(&c.name)
            ));
            plan.warn(format!("Column {} is dropped with its data", c.name), true);
        }
    }
    for p in &pairs {
        if let (None, Some(ni)) = p {
            add_column(d, new, &new.columns[*ni], &mut plan);
        }
    }
    for p in &pairs {
        if let (Some(oi), Some(ni)) = p {
            let (o, n) = (&old.columns[*oi], &new.columns[*ni]);
            if d == Dialect::MySql && o.name != n.name {
                let changed = norm_type(&o.data_type) != norm_type(&n.data_type)
                    || o.nullable != n.nullable
                    || norm_default(&o.default) != norm_default(&n.default)
                    || o.auto_increment != n.auto_increment
                    || o.comment != n.comment;
                if changed {
                    if norm_type(&o.data_type) != norm_type(&n.data_type) {
                        plan.warn(
                            format!(
                                "Changing {} from {} to {} may lose or fail to convert data",
                                n.name,
                                o.data_type.trim(),
                                n.data_type.trim()
                            ),
                            true,
                        );
                    }
                    plan.statements.push(format!(
                        "ALTER TABLE {target} CHANGE COLUMN {} {};",
                        d.quote_ident(&o.name),
                        column_def(d, n, false)
                    ));
                    continue;
                }
            }
            modify_column(d, t, schema, o, n, &mut plan);
        }
    }
    // Adds.
    if constraints {
        if pk_changed {
            if let Some(pk) = new.primary_key.as_ref().filter(|k| !k.columns.is_empty()) {
                plan.statements
                    .push(format!("ALTER TABLE {target} ADD {};", pk_clause(d, pk)));
            }
        }
        for k in &uk_new {
            plan.statements
                .push(format!("ALTER TABLE {target} ADD {};", unique_clause(d, k)));
        }
    }
    for c in &ck_new {
        plan.statements
            .push(format!("ALTER TABLE {target} ADD {};", check_clause(d, c)));
    }
    if constraints {
        for f in &fk_new {
            plan.statements
                .push(format!("ALTER TABLE {target} ADD {};", fk_clause(d, f)));
        }
        for i in &ix_new {
            plan.statements.push(create_index(d, t, schema, i, false));
        }
    }
    table_level(d, old, new, &mut plan);
    plan
}
