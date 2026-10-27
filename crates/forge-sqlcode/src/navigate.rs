//! Navigation in SQL: Go to Declaration targets (a tree node of the data source, the
//! `CREATE` statement of a DDL data source, or a definition in the script), Find Usages in
//! the file, and the occurrences an in-place rename of an alias or CTE edits.

use forge_dbmodel::ddl::DdlFile;
use serde::Serialize;

use crate::lang::SqlDialect;
use crate::lex::{lex, Kind, Token};
use crate::resolve::{Analysis, Def, Target};

/// A position in a DDL file: 0-based line and UTF-16 character.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DdlLocation {
    pub path: String,
    pub line: u32,
    pub character: u32,
}

/// What a name at the caret is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Symbol {
    pub name: String,
    /// `table`, `view`, `column`, `schema`, `routine`, `alias`, `cte`, `selectAlias`.
    pub kind: String,
    /// A definition in the script: its name range.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local: Option<(usize, usize)>,
    /// A database object: where it is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub table: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub routine: Option<String>,
}

fn def_kind(d: &Def) -> &'static str {
    match d.kind {
        crate::resolve::DefKind::TableAlias | crate::resolve::DefKind::SubqueryAlias => "alias",
        crate::resolve::DefKind::Cte => "cte",
        crate::resolve::DefKind::SelectAlias => "selectAlias",
    }
}

fn local(a: &Analysis, def: usize, name: &str) -> Option<Symbol> {
    let d = a.defs.get(def)?;
    Some(Symbol {
        name: name.to_string(),
        kind: def_kind(d).into(),
        local: Some((d.from, d.to)),
        schema: None,
        table: None,
        column: None,
        routine: None,
    })
}

fn object(kind: &str, name: &str) -> Symbol {
    Symbol {
        name: name.to_string(),
        kind: kind.into(),
        local: None,
        schema: None,
        table: None,
        column: None,
        routine: None,
    }
}

/// The symbol at a byte offset: a resolved reference's target or a definition.
pub fn symbol_at(a: &Analysis, at: usize) -> Option<Symbol> {
    if let Some(d) = a.def_at(at) {
        return local(a, d.id, &d.name);
    }
    let r = a.ref_at(at)?;
    match r.target.as_ref()? {
        Target::Local { def } => local(a, *def, &r.name),
        Target::LocalColumn { def, column } => {
            let d = a.defs.get(*def)?;
            Some(Symbol {
                kind: "column".into(),
                local: Some((d.from, d.to)),
                column: Some(column.clone()),
                ..object("column", &r.name)
            })
        }
        Target::Schema { schema } => Some(Symbol {
            schema: Some(schema.clone()),
            ..object("schema", &r.name)
        }),
        Target::Table {
            schema,
            table,
            view,
        } => Some(Symbol {
            schema: Some(schema.clone()),
            table: Some(table.clone()),
            ..object(if *view { "view" } else { "table" }, &r.name)
        }),
        Target::Column {
            schema,
            table,
            column,
            ..
        } => Some(Symbol {
            schema: Some(schema.clone()),
            table: Some(table.clone()),
            column: Some(column.clone()),
            ..object("column", &r.name)
        }),
        Target::Routine { schema, name } => Some(Symbol {
            schema: Some(schema.clone()),
            routine: Some(name.clone()),
            ..object("routine", &r.name)
        }),
    }
}

/// One occurrence of a symbol in the script.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Occurrence {
    pub from: usize,
    pub to: usize,
    /// The definition itself (alias or CTE name).
    pub declaration: bool,
}

/// Every occurrence of the symbol at `at` in the script (Find Usages in the file, rename,
/// highlight usages): the definition and the references with the same target.
pub fn occurrences(a: &Analysis, at: usize) -> Option<(String, Vec<Occurrence>)> {
    let target = if let Some(d) = a.def_at(at) {
        Target::Local { def: d.id }
    } else {
        a.ref_at(at)?.target.clone()?
    };
    let mut out = Vec::new();
    let name;
    if let Target::Local { def } = target {
        let d = a.defs.get(def)?;
        name = d.name.clone();
        out.push(Occurrence {
            from: d.from,
            to: d.to,
            declaration: true,
        });
    } else {
        name = a.ref_at(at).map(|r| r.name.clone()).unwrap_or_default();
    }
    for r in &a.refs {
        if r.target.as_ref().is_some_and(|t| t.same(&target)) {
            out.push(Occurrence {
                from: r.from,
                to: r.to,
                declaration: false,
            });
        }
    }
    out.sort_by_key(|o| o.from);
    out.dedup_by_key(|o| o.from);
    Some((name, out))
}

/// The occurrences an in-place rename edits: only definitions of the script (aliases, CTEs,
/// select-list aliases) can be renamed in the editor.
pub fn rename_ranges(a: &Analysis, at: usize) -> Result<(String, Vec<Occurrence>), String> {
    let local = a.def_at(at).is_some()
        || matches!(
            a.ref_at(at).and_then(|r| r.target.as_ref()),
            Some(Target::Local { .. })
        );
    if !local {
        return Err("Only aliases and common table expressions can be renamed in the editor; rename database objects in the Database tool window".into());
    }
    occurrences(a, at).ok_or_else(|| {
        "Cannot perform refactoring. Caret should be positioned at the symbol to be renamed".into()
    })
}

fn position(text: &str, at: usize) -> (u32, u32) {
    let before = &text[..at.min(text.len())];
    let line = before.matches('\n').count() as u32;
    let start = before.rfind('\n').map(|p| p + 1).unwrap_or(0);
    (line, before[start..].encode_utf16().count() as u32)
}

/// Where a DDL data source defines an object: the column in its `CREATE TABLE` (or `ALTER
/// TABLE … ADD`), the table or view, or the routine. The last definition wins (migrations).
pub fn ddl_location(
    files: &[DdlFile],
    d: SqlDialect,
    schema: Option<&str>,
    table: Option<&str>,
    column: Option<&str>,
    routine: Option<&str>,
) -> Option<DdlLocation> {
    let eq = |a: &str, b: &str| a.eq_ignore_ascii_case(b);
    let mut found: Option<DdlLocation> = None;
    for f in files {
        let toks: Vec<Token> = lex(&f.text, d)
            .into_iter()
            .filter(Token::is_significant)
            .collect();
        let at = |i: usize| -> DdlLocation {
            let (line, character) = position(&f.text, toks[i].start);
            DdlLocation {
                path: f.path.clone(),
                line,
                character,
            }
        };
        let mut i = 0;
        while i < toks.len() {
            let create = toks[i].is_kw(&f.text, "CREATE");
            let alter = toks[i].is_kw(&f.text, "ALTER");
            if !create && !alter {
                i += 1;
                continue;
            }
            // CREATE [OR REPLACE] [TEMP …] TABLE|VIEW|FUNCTION|PROCEDURE [IF NOT EXISTS] name
            let mut j = i + 1;
            while j < toks.len()
                && j < i + 6
                && !toks[j].is_kw(&f.text, "TABLE")
                && !toks[j].is_kw(&f.text, "VIEW")
                && !toks[j].is_kw(&f.text, "FUNCTION")
                && !toks[j].is_kw(&f.text, "PROCEDURE")
            {
                j += 1;
            }
            if j >= toks.len() || j >= i + 6 {
                i += 1;
                continue;
            }
            let is_routine =
                toks[j].is_kw(&f.text, "FUNCTION") || toks[j].is_kw(&f.text, "PROCEDURE");
            j += 1;
            while j < toks.len()
                && ["IF", "NOT", "EXISTS", "ONLY"]
                    .iter()
                    .any(|k| toks[j].is_kw(&f.text, k))
            {
                j += 1;
            }
            // The dotted name.
            let mut parts = Vec::new();
            while j < toks.len() && toks[j].is_name() {
                parts.push(j);
                if toks.get(j + 1).is_some_and(|t| t.kind == Kind::Dot)
                    && toks.get(j + 2).is_some_and(Token::is_name)
                {
                    j += 2;
                } else {
                    j += 1;
                    break;
                }
            }
            let Some(&last) = parts.last() else {
                i += 1;
                continue;
            };
            let name = toks[last].name(&f.text);
            let sch = (parts.len() >= 2).then(|| toks[parts[parts.len() - 2]].name(&f.text));
            let schema_ok = match (schema, &sch) {
                (Some(s), Some(x)) => eq(s, x),
                _ => true,
            };
            if is_routine {
                if create && schema_ok && routine.is_some_and(|r| eq(r, &name)) {
                    found = Some(at(last));
                }
                i = j;
                continue;
            }
            if !schema_ok || !table.is_some_and(|t| eq(t, &name)) {
                i = j;
                continue;
            }
            match column {
                None if create => found = Some(at(last)),
                None => {}
                Some(col) => {
                    let mut hit = None;
                    if create && toks.get(j).is_some_and(|t| t.kind == Kind::Open) {
                        // Items of the column list start after `(` or a depth-1 comma.
                        let mut depth = 0;
                        let mut k = j;
                        let mut item_start = true;
                        while k < toks.len() {
                            match toks[k].kind {
                                Kind::Open => {
                                    depth += 1;
                                    item_start = depth == 1;
                                    k += 1;
                                    continue;
                                }
                                Kind::Close => {
                                    depth -= 1;
                                    if depth == 0 {
                                        break;
                                    }
                                }
                                Kind::Comma if depth == 1 => {
                                    item_start = true;
                                    k += 1;
                                    continue;
                                }
                                _ => {}
                            }
                            if item_start
                                && depth == 1
                                && toks[k].is_name()
                                && eq(&toks[k].name(&f.text), col)
                            {
                                hit = Some(at(k));
                            }
                            item_start = false;
                            k += 1;
                        }
                    } else if alter {
                        // ALTER TABLE t ADD [COLUMN] [IF NOT EXISTS] col
                        let mut k = j;
                        while k < toks.len() && toks[k].kind != Kind::Semi {
                            if toks[k].is_kw(&f.text, "ADD") {
                                let mut c = k + 1;
                                while c < toks.len()
                                    && ["COLUMN", "IF", "NOT", "EXISTS"]
                                        .iter()
                                        .any(|w| toks[c].is_kw(&f.text, w))
                                {
                                    c += 1;
                                }
                                if toks
                                    .get(c)
                                    .is_some_and(|t| t.is_name() && eq(&t.name(&f.text), col))
                                {
                                    hit = Some(at(c));
                                }
                            }
                            k += 1;
                        }
                    }
                    if let Some(h) = hit {
                        found = Some(h);
                    } else if create && found.is_none() {
                        found = Some(at(last));
                    }
                }
            }
            i = j;
        }
    }
    found
}
