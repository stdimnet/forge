//! SQL inspections (registered as `[[contributions.inspections]]` of the database plugin):
//! problems of an [`Analysis`] with their quick fixes. Ids follow DataGrip's where it has
//! one; the inspection profile sets the final severity and turns them off.

use serde::Serialize;

use crate::resolve::{Analysis, DefKind, FindingFix, Role, State};

/// The SQL inspections: id, name, default severity and description.
pub const INSPECTIONS: &[(&str, &str, &str, &str)] = &[
    (
        "SqlResolve",
        "Unresolved reference",
        "ERROR",
        "Reports tables, columns, schemas and aliases that do not exist in the data source attached to the file or console, or in the script itself.",
    ),
    (
        "SqlAmbiguousColumn",
        "Ambiguous column reference",
        "WARNING",
        "Reports unqualified columns that exist in more than one table of the query. The quick fix qualifies the column.",
    ),
    (
        "SqlWithoutWhere",
        "Delete or update statement without where clauses",
        "WARNING",
        "Reports DELETE and UPDATE statements without WHERE, which change every row of the table. The quick fix adds a WHERE placeholder.",
    ),
    (
        "SqlUnusedAlias",
        "Unused table alias",
        "WEAK WARNING",
        "Reports table aliases that no column of the query uses. The quick fix removes the alias.",
    ),
    (
        "SqlUnusedCte",
        "Unused common table expression",
        "WARNING",
        "Reports WITH queries that the statement never reads. The quick fix removes the CTE.",
    ),
    (
        "SqlType",
        "Types compatibility",
        "WARNING",
        "Reports comparisons of a column with a value of another type family (a number column with a string, a text column with a number), where the model knows the column's type.",
    ),
    (
        "SqlRedundantOrderingDirection",
        "Redundant ordering direction",
        "WEAK WARNING",
        "Reports ASC in ORDER BY: ascending is the default.",
    ),
    (
        "SqlRedundantElseNull",
        "Redundant ELSE NULL clause",
        "WEAK WARNING",
        "Reports ELSE NULL in CASE expressions: a CASE without ELSE gives NULL.",
    ),
    (
        "SqlRedundantAlias",
        "Redundant alias expressions",
        "WEAK WARNING",
        "Reports aliases that repeat the table's or column's own name.",
    ),
];

/// One text edit (byte offsets until converted by [`crate::api`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Edit {
    pub from: usize,
    pub to: usize,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Fix {
    pub title: String,
    pub edits: Vec<Edit>,
    /// Selection after applying (e.g. the WHERE placeholder).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub select: Option<(usize, usize)>,
}

/// A problem of one inspection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    /// The inspection id (`SqlResolve`); the diagnostic's code.
    pub id: String,
    pub from: usize,
    pub to: usize,
    pub message: String,
    /// `error`, `warning` or `weak` (the editor's severities; the profile may change them).
    pub severity: String,
    /// Shown greyed out (unused and redundant code).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub unnecessary: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fixes: Vec<Fix>,
}

fn severity(id: &str) -> &'static str {
    match INSPECTIONS.iter().find(|i| i.0 == id).map(|i| i.2) {
        Some("ERROR") => "error",
        Some("WARNING") => "warning",
        _ => "weak",
    }
}

fn problem(id: &str, from: usize, to: usize, message: String, fixes: Vec<Fix>) -> Problem {
    Problem {
        id: id.into(),
        from,
        to,
        message,
        severity: severity(id).into(),
        unnecessary: matches!(id, "SqlUnusedAlias" | "SqlUnusedCte")
            || id.starts_with("SqlRedundant"),
        fixes,
    }
}

fn remove(title: &str, from: usize, to: usize) -> Fix {
    Fix {
        title: title.into(),
        edits: vec![Edit {
            from,
            to,
            text: String::new(),
        }],
        select: None,
    }
}

/// Every problem of the analysis, by position.
pub fn inspect(a: &Analysis, src: &str) -> Vec<Problem> {
    let mut out = Vec::new();
    for r in &a.refs {
        match r.state {
            State::Unresolved => {
                let what = match r.role {
                    Role::Schema => "schema",
                    Role::Table => "table",
                    Role::Column => "column",
                    Role::Qualifier => "symbol",
                    Role::Function => "routine",
                };
                out.push(problem(
                    "SqlResolve",
                    r.from,
                    r.to,
                    format!("Cannot resolve {what} '{}'", r.name),
                    Vec::new(),
                ));
            }
            State::Ambiguous => {
                let fixes = r
                    .candidates
                    .iter()
                    .map(|q| Fix {
                        title: format!("Qualify with '{q}'"),
                        edits: vec![Edit {
                            from: r.from,
                            to: r.from,
                            text: format!("{q}."),
                        }],
                        select: None,
                    })
                    .collect();
                let list = r
                    .candidates
                    .iter()
                    .map(|c| format!("'{c}'"))
                    .collect::<Vec<_>>()
                    .join(", ");
                out.push(problem(
                    "SqlAmbiguousColumn",
                    r.from,
                    r.to,
                    format!(
                        "Ambiguous column reference '{}': it exists in {list}",
                        r.name
                    ),
                    fixes,
                ));
            }
            _ => {}
        }
    }
    for d in &a.defs {
        if d.used {
            continue;
        }
        match d.kind {
            DefKind::TableAlias => out.push(problem(
                "SqlUnusedAlias",
                d.from,
                d.to,
                format!("Unused alias '{}'", d.name),
                vec![remove("Remove alias", d.remove.0, d.remove.1)],
            )),
            DefKind::Cte => out.push(problem(
                "SqlUnusedCte",
                d.from,
                d.to,
                format!("Unused CTE '{}'", d.name),
                vec![remove("Remove unused CTE", d.remove.0, d.remove.1)],
            )),
            _ => {}
        }
    }
    for f in &a.findings {
        let fixes = match &f.fix {
            None => Vec::new(),
            Some(FindingFix::Remove(from, to)) => {
                let title = match f.id {
                    "SqlRedundantOrderingDirection" => "Remove redundant ASC",
                    "SqlRedundantElseNull" => "Remove ELSE NULL",
                    _ => "Remove redundant alias",
                };
                vec![remove(title, *from, *to)]
            }
            Some(FindingFix::Insert { at, text, select }) => vec![Fix {
                title: "Add WHERE clause".into(),
                edits: vec![Edit {
                    from: *at,
                    to: *at,
                    text: text.clone(),
                }],
                select: Some((at + select.0, at + select.1)),
            }],
            Some(FindingFix::Quote(from, to)) => vec![Fix {
                title: "Quote the value".into(),
                edits: vec![Edit {
                    from: *from,
                    to: *to,
                    text: format!("'{}'", &src[*from..*to]),
                }],
                select: None,
            }],
        };
        out.push(problem(f.id, f.from, f.to, f.message.clone(), fixes));
    }
    out.sort_by_key(|p| (p.from, p.to));
    out.dedup_by(|a, b| a.id == b.id && a.from == b.from && a.to == b.to);
    out
}
