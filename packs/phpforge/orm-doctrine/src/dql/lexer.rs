//! DQL tokenizer. Path expressions (`o.customer.email`) are single identifier tokens;
//! [`super::highlight`] splits them into alias and field parts.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TokenKind {
    Keyword,
    Function,
    /// Entity names, aliases and path expressions.
    Identifier,
    String,
    Number,
    /// `:name` or `?1`.
    Parameter,
    Operator,
    Comma,
    LParen,
    RParen,
    Whitespace,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub start: usize,
    pub end: usize,
    pub text: String,
}

impl Token {
    /// Upper-cased text for keyword comparisons.
    pub fn upper(&self) -> String {
        self.text.to_ascii_uppercase()
    }

    pub fn is_keyword(&self, kw: &str) -> bool {
        self.kind == TokenKind::Keyword && self.text.eq_ignore_ascii_case(kw)
    }
}

/// Reserved DQL keywords.
pub const KEYWORDS: &[&str] = &[
    "ALL", "AND", "ANY", "AS", "ASC", "AVG", "BETWEEN", "BOTH", "BY", "CASE", "COALESCE", "DELETE",
    "DESC", "DISTINCT", "ELSE", "EMPTY", "END", "ESCAPE", "EXISTS", "FALSE", "FROM", "GROUP",
    "HAVING", "HIDDEN", "IN", "INDEX", "INNER", "INSTANCE", "IS", "JOIN", "LEADING", "LEFT",
    "LIKE", "MEMBER", "NEW", "NOT", "NULL", "NULLIF", "OF", "OR", "ORDER", "OUTER", "PARTIAL",
    "SELECT", "SET", "SOME", "THEN", "TRAILING", "TRUE", "UPDATE", "WHEN", "WHERE", "WITH",
];

/// Built-in DQL functions with their signatures.
pub const FUNCTIONS: &[(&str, &str)] = &[
    ("ABS", "ABS(arithmetic)"),
    ("AVG", "AVG(field)"),
    ("BIT_AND", "BIT_AND(a, b)"),
    ("BIT_OR", "BIT_OR(a, b)"),
    ("CONCAT", "CONCAT(str1, str2, ...)"),
    ("COUNT", "COUNT([DISTINCT] expr)"),
    ("CURRENT_DATE", "CURRENT_DATE()"),
    ("CURRENT_TIME", "CURRENT_TIME()"),
    ("CURRENT_TIMESTAMP", "CURRENT_TIMESTAMP()"),
    ("DATE_ADD", "DATE_ADD(date, days, unit)"),
    ("DATE_DIFF", "DATE_DIFF(date1, date2)"),
    ("DATE_SUB", "DATE_SUB(date, days, unit)"),
    ("IDENTITY", "IDENTITY(association [, field])"),
    ("LENGTH", "LENGTH(str)"),
    ("LOCATE", "LOCATE(needle, haystack [, offset])"),
    ("LOWER", "LOWER(str)"),
    ("MAX", "MAX(field)"),
    ("MIN", "MIN(field)"),
    ("MOD", "MOD(a, b)"),
    ("SIZE", "SIZE(collection)"),
    ("SQRT", "SQRT(q)"),
    ("SUBSTRING", "SUBSTRING(str, start [, length])"),
    ("SUM", "SUM(field)"),
    ("TRIM", "TRIM([LEADING | TRAILING | BOTH] [char FROM] str)"),
    ("UPPER", "UPPER(str)"),
    ("COALESCE", "COALESCE(expr1, expr2, ...)"),
    ("NULLIF", "NULLIF(expr1, expr2)"),
];

pub fn is_keyword(word: &str) -> bool {
    KEYWORDS.iter().any(|k| k.eq_ignore_ascii_case(word))
}

pub fn function_signature(word: &str) -> Option<&'static str> {
    FUNCTIONS
        .iter()
        .find(|(f, _)| f.eq_ignore_ascii_case(word))
        .map(|(_, s)| *s)
}

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_' || c == '\\'
}

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '\\' || c == '.' || c == ':'
}

/// Tokenize a DQL string. Never fails: unknown characters become [`TokenKind::Unknown`],
/// unterminated strings run to the end.
pub fn tokenize(dql: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let chars: Vec<(usize, char)> = dql.char_indices().collect();
    let len = dql.len();
    let at = |i: usize| chars.get(i).map(|(_, c)| *c);
    let pos = |i: usize| chars.get(i).map(|(p, _)| *p).unwrap_or(len);
    let mut i = 0;
    // Upper-cased previous keyword: an entity name after FROM may look like a keyword (`Order`).
    let mut prev_kw = String::new();
    while i < chars.len() {
        let start = pos(i);
        let c = chars[i].1;
        let mut j = i + 1;
        let kind = if c.is_whitespace() {
            while at(j).is_some_and(char::is_whitespace) {
                j += 1;
            }
            TokenKind::Whitespace
        } else if c == '\'' || c == '"' {
            loop {
                match at(j) {
                    None => break,
                    Some(q) if q == c => {
                        // `''` escapes a quote inside a string.
                        if at(j + 1) == Some(c) {
                            j += 2;
                            continue;
                        }
                        j += 1;
                        break;
                    }
                    Some(_) => j += 1,
                }
            }
            TokenKind::String
        } else if c.is_ascii_digit() {
            while at(j).is_some_and(|d| d.is_ascii_digit() || d == '.') {
                j += 1;
            }
            TokenKind::Number
        } else if c == ':' && at(j).is_some_and(|d| d.is_alphanumeric() || d == '_') {
            while at(j).is_some_and(|d| d.is_alphanumeric() || d == '_') {
                j += 1;
            }
            TokenKind::Parameter
        } else if c == '?' {
            while at(j).is_some_and(|d| d.is_ascii_digit()) {
                j += 1;
            }
            TokenKind::Parameter
        } else if is_ident_start(c) {
            while at(j).is_some_and(|d| {
                is_ident_char(d) && !(d == ':' && at(j + 1).is_some_and(|n| !n.is_alphabetic()))
            }) {
                j += 1;
            }
            let word = &dql[start..pos(j)];
            let next_paren = {
                let mut k = j;
                while at(k).is_some_and(char::is_whitespace) {
                    k += 1;
                }
                at(k) == Some('(')
            };
            let declares = matches!(prev_kw.as_str(), "FROM" | "JOIN" | "UPDATE" | "DELETE");
            if declares {
                TokenKind::Identifier
            } else if function_signature(word).is_some()
                && (next_paren || word.to_ascii_uppercase().starts_with("CURRENT_"))
            {
                TokenKind::Function
            } else if is_keyword(word) {
                TokenKind::Keyword
            } else {
                TokenKind::Identifier
            }
        } else if c == ',' {
            TokenKind::Comma
        } else if c == '(' {
            TokenKind::LParen
        } else if c == ')' {
            TokenKind::RParen
        } else if "=<>!+-*/".contains(c) {
            while at(j).is_some_and(|d| "=<>".contains(d)) {
                j += 1;
            }
            TokenKind::Operator
        } else if c == '{' || c == '}' || c == '.' {
            TokenKind::Operator
        } else {
            TokenKind::Unknown
        };
        let end = pos(j);
        if kind == TokenKind::Keyword {
            prev_kw = dql[start..end].to_ascii_uppercase();
        } else if kind != TokenKind::Whitespace {
            prev_kw.clear();
        }
        out.push(Token {
            kind,
            start,
            end,
            text: dql[start..end].to_string(),
        });
        i = j;
    }
    out
}

/// Tokens without whitespace.
pub fn significant(tokens: &[Token]) -> Vec<&Token> {
    tokens
        .iter()
        .filter(|t| t.kind != TokenKind::Whitespace)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(dql: &str) -> Vec<(TokenKind, &str)> {
        tokenize(dql)
            .into_iter()
            .filter(|t| t.kind != TokenKind::Whitespace)
            .map(|t| (t.kind, &dql[t.start..t.end]))
            .collect()
    }

    #[test]
    fn tokenizes_select() {
        let k = kinds("SELECT o, COUNT(i.id) FROM App\\Entity\\Order o WHERE o.status = :s AND o.total > 10.5");
        assert_eq!(k[0], (TokenKind::Keyword, "SELECT"));
        assert_eq!(k[1], (TokenKind::Identifier, "o"));
        assert_eq!(k[2], (TokenKind::Comma, ","));
        assert_eq!(k[3], (TokenKind::Function, "COUNT"));
        assert_eq!(k[5], (TokenKind::Identifier, "i.id"));
        assert_eq!(k[8], (TokenKind::Identifier, "App\\Entity\\Order"));
        assert_eq!(k[12], (TokenKind::Operator, "="));
        assert_eq!(k[13], (TokenKind::Parameter, ":s"));
        assert_eq!(k[17], (TokenKind::Number, "10.5"));
    }

    #[test]
    fn strings_parameters_and_case() {
        let k = kinds("where o.name LIKE 'it''s' OR o.id = ?1 and current_date");
        assert_eq!(k[0], (TokenKind::Keyword, "where"));
        assert_eq!(k[3], (TokenKind::String, "'it''s'"));
        assert_eq!(k[7], (TokenKind::Parameter, "?1"));
        assert_eq!(k[9], (TokenKind::Function, "current_date"));
    }

    #[test]
    fn unterminated_string() {
        let k = kinds("WHERE o.name = 'abc");
        assert_eq!(k.last().unwrap(), &(TokenKind::String, "'abc"));
    }

    #[test]
    fn legacy_alias_and_operators() {
        let k = kinds("FROM App:Order o WHERE o.a <> o.b");
        assert_eq!(k[1], (TokenKind::Identifier, "App:Order"));
        assert_eq!(k[5], (TokenKind::Operator, "<>"));
    }
}
