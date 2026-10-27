//! User parameters: placeholders in SQL that Forge asks values for before running (`?`,
//! `:name`, `$1`, `${name}`, `#{name}`; each pattern can be turned off in Settings | Tools |
//! Database | User Parameters). Values are inserted as written (a string needs its quotes),
//! like DataGrip. Parameters in strings and comments are ignored.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::lang::SqlDialect;
use crate::lex::{lex, Kind};

/// Which placeholder patterns are user parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ParamPatterns {
    /// `?` (positional).
    pub question: bool,
    /// `:name`
    pub colon: bool,
    /// `$1`
    pub dollar: bool,
    /// `${name}`
    pub dollar_brace: bool,
    /// `#{name}`
    pub hash_brace: bool,
}

impl Default for ParamPatterns {
    fn default() -> Self {
        Self {
            question: true,
            colon: true,
            dollar: true,
            dollar_brace: true,
            hash_brace: true,
        }
    }
}

/// One placeholder in the text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Param {
    /// The key of its value: `:id`, `$1`, `${user}`, `#{user}`, or `?1`, `?2` … for `?`.
    pub name: String,
    pub from: usize,
    pub to: usize,
}

/// The parameters of `text` in order (byte offsets).
pub fn find_params(text: &str, d: SqlDialect, p: &ParamPatterns) -> Vec<Param> {
    let mut out = Vec::new();
    let mut question = 0;
    for t in lex(text, d) {
        if t.kind != Kind::Param {
            continue;
        }
        let s = t.text(text);
        let name = if s == "?" {
            if !p.question {
                continue;
            }
            question += 1;
            format!("?{question}")
        } else if s.starts_with("${") {
            if !p.dollar_brace {
                continue;
            }
            s.to_string()
        } else if s.starts_with("#{") {
            if !p.hash_brace {
                continue;
            }
            s.to_string()
        } else if s.starts_with('$') {
            if !p.dollar {
                continue;
            }
            s.to_string()
        } else if s.starts_with(':') {
            if !p.colon {
                continue;
            }
            s.to_string()
        } else {
            continue;
        };
        out.push(Param {
            name,
            from: t.start,
            to: t.end,
        });
    }
    out
}

/// The distinct parameter names in order of first use (the rows of the parameters dialog).
pub fn param_names(params: &[Param]) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for p in params {
        if !names.contains(&p.name) {
            names.push(p.name.clone());
        }
    }
    names
}

/// `text` with every parameter replaced by its value; the names without a value are the
/// error.
pub fn substitute(
    text: &str,
    d: SqlDialect,
    p: &ParamPatterns,
    values: &BTreeMap<String, String>,
) -> Result<String, Vec<String>> {
    let params = find_params(text, d, p);
    let missing: Vec<String> = param_names(&params)
        .into_iter()
        .filter(|n| !values.contains_key(n))
        .collect();
    if !missing.is_empty() {
        return Err(missing);
    }
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for prm in &params {
        out.push_str(&text[at..prm.from]);
        out.push_str(&values[&prm.name]);
        at = prm.to;
    }
    out.push_str(&text[at..]);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_every_pattern() {
        let sql = "SELECT * FROM t WHERE a = ? AND b = :id AND c = $1 AND d = ${user} AND e = #{org} AND f = ? AND g = :id -- :ignored\n AND h = '?' AND i::text = x";
        let found = find_params(sql, SqlDialect::Postgres, &ParamPatterns::default());
        let names: Vec<&str> = found.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["?1", ":id", "$1", "${user}", "#{org}", "?2", ":id"]);
        assert_eq!(
            param_names(&found),
            ["?1", ":id", "$1", "${user}", "#{org}", "?2"]
        );
    }

    #[test]
    fn patterns_can_be_turned_off() {
        let off = ParamPatterns {
            colon: false,
            question: false,
            ..ParamPatterns::default()
        };
        let found = find_params("SELECT :a, ?, $2", SqlDialect::Postgres, &off);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "$2");
    }

    #[test]
    fn substitutes_values() {
        let mut v = BTreeMap::new();
        v.insert(":id".to_string(), "42".to_string());
        v.insert("?1".to_string(), "'x'".to_string());
        let p = ParamPatterns::default();
        assert_eq!(
            substitute(
                "SELECT ? WHERE id = :id OR p = :id",
                SqlDialect::MySql,
                &p,
                &v
            )
            .unwrap(),
            "SELECT 'x' WHERE id = 42 OR p = 42"
        );
        assert_eq!(
            substitute("SELECT :other", SqlDialect::MySql, &p, &v).unwrap_err(),
            [":other"]
        );
    }
}
