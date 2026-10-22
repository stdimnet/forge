//! SQL: keyword case (also run by Reformat Code) and, for the preview, the spaces and the
//! indentation of clauses and parentheses.

use super::lexer::{join, lex, Kind, Lang, Tok};
use super::Opts;

const KEYWORDS: &[&str] = &[
    "add",
    "all",
    "alter",
    "and",
    "any",
    "as",
    "asc",
    "between",
    "by",
    "case",
    "check",
    "column",
    "constraint",
    "create",
    "cross",
    "database",
    "default",
    "delete",
    "desc",
    "distinct",
    "drop",
    "else",
    "end",
    "exists",
    "foreign",
    "from",
    "full",
    "group",
    "having",
    "if",
    "in",
    "index",
    "inner",
    "insert",
    "into",
    "is",
    "join",
    "key",
    "left",
    "like",
    "limit",
    "not",
    "null",
    "offset",
    "on",
    "or",
    "order",
    "outer",
    "primary",
    "references",
    "returning",
    "right",
    "select",
    "set",
    "table",
    "then",
    "union",
    "unique",
    "update",
    "using",
    "values",
    "view",
    "when",
    "where",
    "with",
    "begin",
    "commit",
    "rollback",
    "transaction",
    "truncate",
    "replace",
    "ilike",
    "except",
    "intersect",
    "lateral",
    "natural",
    "partition",
    "over",
    "window",
    "recursive",
    "cascade",
    "temporary",
    "temp",
    "sequence",
    "trigger",
    "procedure",
    "function",
    "returns",
    "declare",
    "explain",
    "analyze",
    "grant",
    "revoke",
    "auto_increment",
    "unsigned",
];

/// Clauses that start a line at the statement's indentation.
const CLAUSES: &[&str] = &[
    "select",
    "from",
    "where",
    "group",
    "order",
    "having",
    "limit",
    "offset",
    "union",
    "insert",
    "values",
    "update",
    "set",
    "delete",
    "create",
    "alter",
    "drop",
    "with",
    "returning",
    "join",
    "inner",
    "left",
    "right",
    "full",
    "cross",
    "except",
    "intersect",
    "window",
    "on",
];

fn is_keyword(t: &[Tok], i: usize) -> bool {
    let tok = &t[i];
    tok.kind == Kind::Word
        && KEYWORDS.iter().any(|k| tok.text.eq_ignore_ascii_case(k))
        && !(i > 0 && t[i - 1].is(".") && t[i - 1].ws.is_empty())
        && !t.get(i + 1).is_some_and(|n| n.is(".") && n.ws.is_empty())
}

/// Format SQL: `layout` for the preview, else only the keyword case.
pub fn format(text: &str, o: &Opts<'_>, layout: bool) -> String {
    let mut t = lex(text, Lang::Sql);
    let case = o.str("case.keywords");
    for i in 0..t.len() {
        if is_keyword(&t, i) {
            match case.as_str() {
                "upper" => t[i].text = t[i].text.to_ascii_uppercase(),
                "lower" => t[i].text = t[i].text.to_ascii_lowercase(),
                _ => {}
            }
        }
    }
    if !layout {
        return join(&t);
    }
    let ind = o.indent();
    let comma = o.bool("spaces.afterComma");
    let ops = o.bool("spaces.aroundOperators");
    let within = o.bool("spaces.withinParentheses");
    let mut out = String::new();
    let mut depth = 0usize;
    let mut stmt_start = true;
    for i in 0..t.len() {
        let tok = &t[i];
        if tok.text.is_empty() {
            continue;
        }
        if i > 0 && tok.nl() > 0 {
            for _ in 0..tok.nl().min(o.int("blankLines.keepMax") + 1) {
                out.push('\n');
            }
            let clause =
                tok.kind == Kind::Word && CLAUSES.iter().any(|c| tok.text.eq_ignore_ascii_case(c));
            let mut cols = depth * ind.continuation;
            if !(stmt_start || clause || tok.kind == Kind::Close) {
                cols += ind.size;
            }
            out.push_str(&ind.render(cols));
        } else if i > 0 {
            let p = &t[i - 1];
            let operator = |x: &Tok| {
                x.kind == Kind::Op
                    && ["=", "<>", "!=", "<", ">", "<=", ">=", "||", "+", "-", "/"]
                        .contains(&x.text.as_str())
            };
            let space = if tok.is(",") || tok.is(";") || p.is(".") || tok.is(".") {
                false
            } else if p.is(",") {
                comma
            } else if p.is("(") {
                within && !tok.is(")")
            } else if tok.is(")") {
                within && !p.is("(")
            } else if tok.is("(") {
                !(p.kind == Kind::Word && !is_keyword(&t, i - 1))
            } else if operator(tok) || operator(p) {
                ops
            } else {
                !tok.ws.is_empty()
            };
            if space {
                out.push(' ');
            }
        }
        out.push_str(&tok.text);
        match tok.kind {
            Kind::Open => depth += 1,
            Kind::Close => depth = depth.saturating_sub(1),
            _ => {}
        }
        stmt_start = tok.is(";");
    }
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::model;
    use serde_json::json;

    #[test]
    fn keyword_case_and_layout() {
        let mut v = model().defaults();
        v.insert("sql.case.keywords".into(), json!("upper"));
        let o = Opts::new("sql", &v);
        assert_eq!(
            format(
                "select a.from, b from t\nwhere x='y' -- select\n",
                &o,
                false
            ),
            "SELECT a.from, b FROM t\nWHERE x='y' -- select\n"
        );
        assert_eq!(
            format("select a,b\nfrom t\nwhere x>1\nand y in (1,2);\n", &o, true),
            "SELECT a, b\nFROM t\nWHERE x > 1\n    AND y IN (1, 2);\n"
        );
    }
}
