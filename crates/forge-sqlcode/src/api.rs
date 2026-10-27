//! The requests and answers of the IPC commands and their WebAssembly mirrors. Offsets
//! here are UTF-16 code units (editor positions); the modules work in bytes.

use forge_dbmodel::ddl::DdlFile;
use forge_dbmodel::DbModel;
use serde::{Deserialize, Serialize};

use crate::inspect::{inspect, Edit, Fix, Problem};
use crate::lang::SqlDialect;
use crate::navigate::{self, DdlLocation, Occurrence, Symbol};
use crate::params::{find_params, param_names, ParamPatterns};
use crate::resolve::{analyze, Analysis};

/// Byte ↔ UTF-16 offsets of one text.
pub struct Offsets {
    /// UTF-16 offset of every byte offset (`len + 1` entries).
    to16: Vec<usize>,
    len16: usize,
}

impl Offsets {
    pub fn new(text: &str) -> Self {
        let mut to16 = Vec::with_capacity(text.len() + 1);
        let mut u = 0;
        for c in text.chars() {
            for _ in 0..c.len_utf8() {
                to16.push(u);
            }
            u += c.len_utf16();
        }
        to16.push(u);
        Self { to16, len16: u }
    }

    pub fn utf16(&self, byte: usize) -> usize {
        self.to16.get(byte).copied().unwrap_or(self.len16)
    }

    pub fn byte(&self, utf16: usize) -> usize {
        self.to16
            .partition_point(|u| *u < utf16)
            .min(self.to16.len() - 1)
    }
}

/// A script with what resolving it needs.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SqlContext {
    pub text: String,
    pub dialect: SqlDialect,
    /// The model of the attached data source (none: only the script's own names resolve).
    pub model: Option<DbModel>,
    pub default_schema: Option<String>,
}

impl SqlContext {
    pub fn analysis(&self) -> Analysis {
        analyze(
            &self.text,
            self.dialect,
            self.model.as_ref(),
            self.default_schema.as_deref(),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyzeAnswer {
    pub dialect: SqlDialect,
    pub problems: Vec<Problem>,
    /// Whether a data source model was used (unresolved tables are only known then).
    pub has_model: bool,
}

fn edit16(o: &Offsets, e: &Edit) -> Edit {
    Edit {
        from: o.utf16(e.from),
        to: o.utf16(e.to),
        text: e.text.clone(),
    }
}

/// The SQL inspections' problems (UTF-16 offsets).
pub fn analyze_text(ctx: &SqlContext) -> AnalyzeAnswer {
    let a = ctx.analysis();
    let o = Offsets::new(&ctx.text);
    let problems = inspect(&a, &ctx.text)
        .into_iter()
        .map(|p| Problem {
            from: o.utf16(p.from),
            to: o.utf16(p.to),
            fixes: p
                .fixes
                .iter()
                .map(|f| Fix {
                    title: f.title.clone(),
                    edits: f.edits.iter().map(|e| edit16(&o, e)).collect(),
                    select: f.select.map(|(a, b)| (o.utf16(a), o.utf16(b))),
                })
                .collect(),
            ..p
        })
        .collect();
    AnalyzeAnswer {
        dialect: ctx.dialect,
        problems,
        has_model: a.has_model,
    }
}

/// Go to Declaration: the symbol at `offset` (UTF-16) and, for a DDL data source, where its
/// files define it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeclarationAnswer {
    pub symbol: Symbol,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ddl: Option<DdlLocation>,
}

pub fn declaration(
    ctx: &SqlContext,
    offset: usize,
    ddl_files: &[DdlFile],
) -> Option<DeclarationAnswer> {
    let o = Offsets::new(&ctx.text);
    let a = ctx.analysis();
    let mut symbol = navigate::symbol_at(&a, o.byte(offset))?;
    symbol.local = symbol.local.map(|(f, t)| (o.utf16(f), o.utf16(t)));
    let ddl = if ddl_files.is_empty() || symbol.local.is_some() {
        None
    } else {
        navigate::ddl_location(
            ddl_files,
            ctx.dialect,
            symbol.schema.as_deref(),
            symbol.table.as_deref(),
            symbol.column.as_deref(),
            symbol.routine.as_deref(),
        )
    };
    Some(DeclarationAnswer { symbol, ddl })
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsagesAnswer {
    pub name: String,
    pub occurrences: Vec<Occurrence>,
}

fn occ16(o: &Offsets, list: Vec<Occurrence>) -> Vec<Occurrence> {
    list.into_iter()
        .map(|x| Occurrence {
            from: o.utf16(x.from),
            to: o.utf16(x.to),
            declaration: x.declaration,
        })
        .collect()
}

/// Find Usages in the file of the symbol at `offset`.
pub fn usages(ctx: &SqlContext, offset: usize) -> Option<UsagesAnswer> {
    let o = Offsets::new(&ctx.text);
    let (name, list) = navigate::occurrences(&ctx.analysis(), o.byte(offset))?;
    Some(UsagesAnswer {
        name,
        occurrences: occ16(&o, list),
    })
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameAnswer {
    pub name: String,
    pub occurrences: Vec<Occurrence>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// The occurrences an in-place rename of an alias or CTE edits.
pub fn rename(ctx: &SqlContext, offset: usize) -> RenameAnswer {
    let o = Offsets::new(&ctx.text);
    match navigate::rename_ranges(&ctx.analysis(), o.byte(offset)) {
        Ok((name, list)) => RenameAnswer {
            name,
            occurrences: occ16(&o, list),
            error: None,
        },
        Err(e) => RenameAnswer {
            name: String::new(),
            occurrences: Vec::new(),
            error: Some(e),
        },
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParamsAnswer {
    pub params: Vec<crate::params::Param>,
    /// Distinct names in order: the rows of the dialog.
    pub names: Vec<String>,
}

/// The user parameters of a text (UTF-16 offsets).
pub fn params(text: &str, d: SqlDialect, patterns: &ParamPatterns) -> ParamsAnswer {
    let o = Offsets::new(text);
    let found = find_params(text, d, patterns);
    let names = param_names(&found);
    ParamsAnswer {
        params: found
            .into_iter()
            .map(|p| crate::params::Param {
                from: o.utf16(p.from),
                to: o.utf16(p.to),
                ..p
            })
            .collect(),
        names,
    }
}

/// One statement of a Run SQL file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedStatement {
    pub index: usize,
    pub text: String,
    /// 0-based line of its start.
    pub line: u32,
}

/// The statements Run SQL file executes, in order.
pub fn plan_run(text: &str, d: SqlDialect) -> Vec<PlannedStatement> {
    forge_sql::split_statements(text, d.db())
        .into_iter()
        .enumerate()
        .map(|(index, st)| PlannedStatement {
            index,
            line: text[..st.start].matches('\n').count() as u32,
            text: st.text,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_offsets() {
        let o = Offsets::new("a😀b");
        assert_eq!(o.utf16(1), 1);
        assert_eq!(o.utf16(5), 3);
        assert_eq!(o.byte(3), 5);
        assert_eq!(o.byte(99), 6);
    }

    #[test]
    fn plans_statements_with_lines() {
        let plan = plan_run("SELECT 1;\n\n-- two\nSELECT 2;\nGO", SqlDialect::Postgres);
        assert_eq!(plan.len(), 3);
        assert_eq!(plan[1].text, "-- two\nSELECT 2");
        assert_eq!(plan[1].line, 2);
    }
}
