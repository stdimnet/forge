//! A tolerant scanner for Scala sources (Scala 2 and 3 with braces): packages, objects,
//! classes and traits with their parents, `main` methods (`def main(args: Array[String])`,
//! `extends App`, Scala 3's `@main def`), and the tests of ScalaTest and MUnit suites.
//! Comments are skipped (also nested ones), string literals keep their text.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Tok {
    Ident(String),
    Str(String),
    Sym(char),
}

#[derive(Debug, Clone)]
pub(crate) struct Token {
    pub tok: Tok,
    /// 0-based line.
    pub line: u32,
    /// Brace depth before the token.
    pub depth: u32,
}

pub(crate) fn lex(text: &str) -> Vec<Token> {
    let chars: Vec<char> = text.chars().collect();
    let at = |i: usize, s: &str| {
        s.chars()
            .enumerate()
            .all(|(k, c)| chars.get(i + k) == Some(&c))
    };
    let mut out = Vec::new();
    let (mut i, mut line, mut depth) = (0usize, 0u32, 0u32);
    while i < chars.len() {
        let c = chars[i];
        if c == '\n' {
            line += 1;
            i += 1;
        } else if c.is_whitespace() {
            i += 1;
        } else if at(i, "//") {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else if at(i, "/*") {
            let mut level = 0;
            while i < chars.len() {
                if at(i, "/*") {
                    level += 1;
                    i += 2;
                } else if at(i, "*/") {
                    level -= 1;
                    i += 2;
                    if level == 0 {
                        break;
                    }
                } else {
                    if chars[i] == '\n' {
                        line += 1;
                    }
                    i += 1;
                }
            }
        } else if c == '"' {
            let start = line;
            let triple = at(i, "\"\"\"");
            i += if triple { 3 } else { 1 };
            let mut s = String::new();
            while i < chars.len() {
                if triple && at(i, "\"\"\"") {
                    i += 3;
                    // `""""` ends with the last three quotes.
                    while i < chars.len() && chars[i] == '"' {
                        s.push('"');
                        i += 1;
                    }
                    break;
                }
                let d = chars[i];
                if !triple && d == '\\' && i + 1 < chars.len() {
                    s.push(chars[i + 1]);
                    i += 2;
                    continue;
                }
                if !triple && (d == '"' || d == '\n') {
                    i += 1;
                    break;
                }
                if d == '\n' {
                    line += 1;
                }
                s.push(d);
                i += 1;
            }
            out.push(Token {
                tok: Tok::Str(s),
                line: start,
                depth,
            });
        } else if c == '\'' {
            // A character literal (`'a'`, `'\n'`), else a Scala 2 symbol or a quote.
            if chars.get(i + 2) == Some(&'\'') {
                i += 3;
            } else if chars.get(i + 1) == Some(&'\\') && chars.get(i + 3) == Some(&'\'') {
                i += 4;
            } else {
                i += 1;
            }
        } else if c == '`' {
            let mut s = String::new();
            i += 1;
            while i < chars.len() && chars[i] != '`' && chars[i] != '\n' {
                s.push(chars[i]);
                i += 1;
            }
            i += 1;
            out.push(Token {
                tok: Tok::Ident(s),
                line,
                depth,
            });
        } else if c.is_alphanumeric() || c == '_' || c == '$' {
            let mut s = String::new();
            while i < chars.len()
                && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '$')
            {
                s.push(chars[i]);
                i += 1;
            }
            out.push(Token {
                tok: Tok::Ident(s),
                line,
                depth,
            });
        } else {
            out.push(Token {
                tok: Tok::Sym(c),
                line,
                depth,
            });
            match c {
                '{' => depth += 1,
                '}' => depth = depth.saturating_sub(1),
                _ => {}
            }
            i += 1;
        }
    }
    out
}

pub(crate) fn ident(t: Option<&Token>) -> Option<&str> {
    match t.map(|t| &t.tok) {
        Some(Tok::Ident(s)) => Some(s),
        _ => None,
    }
}

pub(crate) fn string(t: Option<&Token>) -> Option<&str> {
    match t.map(|t| &t.tok) {
        Some(Tok::Str(s)) => Some(s),
        _ => None,
    }
}

pub(crate) fn is_sym(t: Option<&Token>, c: char) -> bool {
    matches!(t.map(|t| &t.tok), Some(Tok::Sym(x)) if *x == c)
}

fn dotted(tokens: &[Token], mut i: usize) -> (String, usize) {
    let mut name = String::new();
    while let Some(id) = ident(tokens.get(i)) {
        name.push_str(id);
        i += 1;
        if is_sym(tokens.get(i), '.') && ident(tokens.get(i + 1)).is_some() {
            name.push('.');
            i += 1;
        } else {
            break;
        }
    }
    (name, i)
}

fn closing(tokens: &[Token], open: usize, (o, c): (char, char)) -> usize {
    let mut level = 0i32;
    for (k, t) in tokens.iter().enumerate().skip(open) {
        if let Tok::Sym(s) = t.tok {
            if s == o {
                level += 1;
            } else if s == c {
                level -= 1;
                if level == 0 {
                    return k;
                }
            }
        }
    }
    tokens.len()
}

/// An object, class or trait.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Template {
    pub name: String,
    /// Fully qualified name (`com.acme.Main`).
    pub class: String,
    /// `object`, `class`, `trait` or `enum`.
    pub kind: String,
    pub line: u32,
    /// Parents after `extends` and `with` (as written, without type arguments).
    pub parents: Vec<String>,
    /// Index of the first and after the last token of the body (`{` … `}`).
    #[serde(skip)]
    pub body: Option<(usize, usize)>,
    /// `def main(args: Array[String])` (line).
    pub main_line: Option<u32>,
}

/// What the scanner found in a Scala file.
#[derive(Debug, Clone, Default)]
pub struct ScalaFile {
    pub package: Option<String>,
    pub imports: Vec<String>,
    pub templates: Vec<Template>,
    /// Scala 3 `@main` methods: (name, line).
    pub main_methods: Vec<(String, u32)>,
    pub(crate) tokens: Vec<Token>,
}

impl ScalaFile {
    pub fn imports_from(&self, prefix: &str) -> bool {
        self.imports.iter().any(|i| i.starts_with(prefix))
    }
}

const TEMPLATE_KEYWORDS: [&str; 4] = ["object", "class", "trait", "enum"];

/// Scan a Scala file.
pub fn parse(text: &str) -> ScalaFile {
    let tokens = lex(text);
    let mut file = ScalaFile::default();
    let mut packages: Vec<String> = Vec::new();
    // (depth of the body, qualified prefix) of enclosing objects.
    let mut scopes: Vec<(u32, String)> = Vec::new();
    let mut i = 0;
    let mut main_annotation = false;
    while i < tokens.len() {
        let t = &tokens[i];
        while scopes.last().is_some_and(|(d, _)| t.depth < *d) {
            scopes.pop();
        }
        match &t.tok {
            Tok::Ident(w) if w == "package" && ident(tokens.get(i + 1)) != Some("object") => {
                let (name, end) = dotted(&tokens, i + 1);
                if !name.is_empty() {
                    packages.push(name);
                }
                i = end.max(i + 1);
                continue;
            }
            Tok::Ident(w) if w == "import" => {
                let (name, end) = dotted(&tokens, i + 1);
                file.imports.push(name);
                i = end.max(i + 1);
                continue;
            }
            Tok::Sym('@') if ident(tokens.get(i + 1)) == Some("main") => {
                main_annotation = true;
                i += 2;
                continue;
            }
            Tok::Ident(w) if w == "def" => {
                let name = ident(tokens.get(i + 1)).unwrap_or_default().to_string();
                if main_annotation && !name.is_empty() {
                    file.main_methods.push((name.clone(), tokens[i + 1].line));
                }
                main_annotation = false;
                // `def main(args: Array[String])` in an object.
                if name == "main" && is_sym(tokens.get(i + 2), '(') {
                    let close = closing(&tokens, i + 2, ('(', ')'));
                    let params: Vec<&str> = tokens[i + 3..close.min(tokens.len())]
                        .iter()
                        .filter_map(|t| ident(Some(t)))
                        .collect();
                    if params.contains(&"Array") && params.contains(&"String") {
                        if let Some(tpl) = file.templates.iter_mut().rev().find(|tp| {
                            tp.kind == "object" && tp.body.is_some_and(|(s, e)| s < i && i < e)
                        }) {
                            tpl.main_line.get_or_insert(tokens[i + 1].line);
                        }
                    }
                }
            }
            Tok::Ident(w) if TEMPLATE_KEYWORDS.contains(&w.as_str()) => {
                // `case class`, `case object` are templates too; `enum` cases are not.
                let Some(name) = ident(tokens.get(i + 1)).map(str::to_string) else {
                    i += 1;
                    continue;
                };
                let mut k = i + 2;
                let mut parents = Vec::new();
                // Up to the body or the end of the declaration line.
                let decl_line = t.line;
                while k < tokens.len() {
                    let tk = &tokens[k];
                    if is_sym(Some(tk), '{') || (tk.line > decl_line + 3) {
                        break;
                    }
                    if is_sym(Some(tk), '(') || is_sym(Some(tk), '[') {
                        let pair = if is_sym(Some(tk), '(') {
                            ('(', ')')
                        } else {
                            ('[', ']')
                        };
                        k = closing(&tokens, k, pair) + 1;
                        continue;
                    }
                    if matches!(ident(Some(tk)), Some("extends") | Some("with")) {
                        let (p, end) = dotted(&tokens, k + 1);
                        if !p.is_empty() {
                            parents.push(p);
                        }
                        k = end.max(k + 1);
                        continue;
                    }
                    if ident(Some(tk)).is_some_and(|w| {
                        TEMPLATE_KEYWORDS.contains(&w) || w == "def" || w == "val" || w == "case"
                    }) || is_sym(Some(tk), '}')
                        || is_sym(Some(tk), '=')
                    {
                        break;
                    }
                    k += 1;
                }
                let prefix = match scopes.last() {
                    Some((_, p)) => p.clone(),
                    None => packages.join("."),
                };
                let class = if prefix.is_empty() {
                    name.clone()
                } else {
                    format!("{prefix}.{name}")
                };
                let body = if is_sym(tokens.get(k), '{') {
                    let end = closing(&tokens, k, ('{', '}'));
                    scopes.push((tokens[k].depth + 1, class.clone()));
                    Some((k, end))
                } else {
                    None
                };
                file.templates.push(Template {
                    name,
                    class,
                    kind: w.clone(),
                    line: t.line,
                    parents,
                    body,
                    main_line: None,
                });
                i = k.max(i + 1);
                if body.is_some() {
                    i += 1;
                }
                continue;
            }
            _ => {}
        }
        if !is_sym(Some(t), '@') && ident(Some(t)) != Some("def") {
            // Other annotations and modifiers between `@main` and `def`.
            if !matches!(
                ident(Some(t)),
                Some("private") | Some("protected") | Some("final")
            ) {
                main_annotation &= is_sym(tokens.get(i.wrapping_sub(1)), '@');
            }
        }
        i += 1;
    }
    file.package = Some(packages.join(".")).filter(|p| !p.is_empty());
    file.tokens = tokens;
    file
}

/// A runnable entry point.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MainObject {
    /// The class `runMain` takes (`com.acme.Main`, a Scala 3 `@main def report` → `com.acme.report`).
    pub class: String,
    pub name: String,
    /// 0-based line for the gutter.
    pub line: u32,
}

/// `main` methods, `App` objects and Scala 3 `@main` methods of a Scala file.
pub fn main_objects(text: &str) -> Vec<MainObject> {
    let file = parse(text);
    let mut out = Vec::new();
    for t in &file.templates {
        if t.kind != "object" {
            continue;
        }
        let app = t.parents.iter().any(|p| p == "App" || p == "scala.App");
        if let Some(line) = t.main_line.or(app.then_some(t.line)) {
            out.push(MainObject {
                class: t.class.clone(),
                name: t.name.clone(),
                line,
            });
        }
    }
    for (name, line) in &file.main_methods {
        out.push(MainObject {
            class: match &file.package {
                Some(p) => format!("{p}.{name}"),
                None => name.clone(),
            },
            name: name.clone(),
            line: *line,
        });
    }
    out.sort_by_key(|m| m.line);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packages_templates_and_mains() {
        let src = r#"package com.acme
package shop

import cats.effect.IO
import scala.util.{Try, Success}

/** The /* nested */ entry point. */
object Main {
  def main(args: Array[String]): Unit = {
    val c = 'x'
    println(s"Hello ${args.mkString}")
  }
}

object Tool extends App {
  println("tool")
}

case class Item(name: String, price: BigDecimal) extends Product with Serializable

trait Repo[A] { def all: List[A] }

@main def report(month: Int): Unit =
  println(month)
"#;
        let f = parse(src);
        assert_eq!(f.package.as_deref(), Some("com.acme.shop"));
        assert_eq!(f.imports, ["cats.effect.IO", "scala.util"]);
        let names: Vec<(&str, &str)> = f
            .templates
            .iter()
            .map(|t| (t.kind.as_str(), t.class.as_str()))
            .collect();
        assert_eq!(
            names,
            [
                ("object", "com.acme.shop.Main"),
                ("object", "com.acme.shop.Tool"),
                ("class", "com.acme.shop.Item"),
                ("trait", "com.acme.shop.Repo"),
            ]
        );
        assert_eq!(f.templates[2].parents, ["Product", "Serializable"]);
        let mains: Vec<(String, u32)> = main_objects(src)
            .into_iter()
            .map(|m| (m.class, m.line))
            .collect();
        assert_eq!(
            mains,
            [
                ("com.acme.shop.Main".to_string(), 8),
                ("com.acme.shop.Tool".to_string(), 14),
                ("com.acme.shop.report".to_string(), 22),
            ]
        );
    }

    #[test]
    fn nested_objects_and_non_mains() {
        let src = "object Outer {\n  object Inner {\n    def main(args: Array[String]): Unit = ()\n  }\n  def main(x: Int) = x\n}\n";
        let mains = main_objects(src);
        assert_eq!(mains.len(), 1);
        assert_eq!(mains[0].class, "Outer.Inner");
    }
}
