//! A tolerant scanner for Groovy sources: enough structure for run gutters, Spock discovery
//! and run configurations, without a full parser. Comments are skipped, string literals
//! keep their text (Spock features are named by strings), slashy strings and GString
//! interpolations are treated as plain text.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Tok {
    Ident(String),
    Str(String),
    Sym(char),
}

#[derive(Debug, Clone)]
struct Token {
    tok: Tok,
    /// 0-based line.
    line: u32,
    /// Brace depth before the token.
    depth: u32,
}

/// Split Groovy source into identifiers, strings and symbols.
fn lex(text: &str) -> Vec<Token> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    let mut line = 0u32;
    let mut depth = 0u32;
    // A shebang line (`#!/usr/bin/env groovy`).
    if text.starts_with("#!") {
        while i < chars.len() && chars[i] != '\n' {
            i += 1;
        }
    }
    let starts_with = |i: usize, s: &str| {
        s.chars()
            .enumerate()
            .all(|(k, c)| chars.get(i + k) == Some(&c))
    };
    while i < chars.len() {
        let c = chars[i];
        if c == '\n' {
            line += 1;
            i += 1;
        } else if c.is_whitespace() {
            i += 1;
        } else if starts_with(i, "//") {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else if starts_with(i, "/*") {
            i += 2;
            while i < chars.len() && !starts_with(i, "*/") {
                if chars[i] == '\n' {
                    line += 1;
                }
                i += 1;
            }
            i += 2;
        } else if c == '"' || c == '\'' {
            let start_line = line;
            let triple = starts_with(i, &c.to_string().repeat(3));
            i += if triple { 3 } else { 1 };
            let mut s = String::new();
            while i < chars.len() {
                let d = chars[i];
                if d == '\\' && i + 1 < chars.len() {
                    s.push(chars[i + 1]);
                    i += 2;
                    continue;
                }
                if triple && starts_with(i, &c.to_string().repeat(3)) {
                    i += 3;
                    break;
                }
                if !triple && d == c {
                    i += 1;
                    break;
                }
                if !triple && d == '\n' {
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
                line: start_line,
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

/// A method (or a Spock feature named by a string).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Method {
    pub name: String,
    /// 0-based line of the name.
    pub line: u32,
    /// Named by a string literal (`def "adds items"()`).
    pub quoted: bool,
    pub is_static: bool,
    pub annotations: Vec<String>,
    /// Spock block labels in the body (`given`, `when`, `then`, `expect`, `where`, …).
    pub labels: Vec<String>,
}

/// A class, interface, trait or enum.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeDecl {
    pub name: String,
    /// Binary name (`com.acme.Outer$Inner`).
    pub class: String,
    pub kind: String,
    pub line: u32,
    /// The supertype after `extends` (as written).
    pub extends: Option<String>,
    pub annotations: Vec<String>,
    pub methods: Vec<Method>,
    pub nested: bool,
}

/// What the scanner found in a Groovy file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroovyFile {
    pub package: Option<String>,
    pub imports: Vec<String>,
    pub types: Vec<TypeDecl>,
    /// Statements outside of type declarations: the file is a script.
    pub script: bool,
}

const TYPE_KEYWORDS: [&str; 5] = ["class", "interface", "trait", "enum", "record"];
const MODIFIERS: [&str; 10] = [
    "public",
    "private",
    "protected",
    "static",
    "final",
    "abstract",
    "synchronized",
    "native",
    "transient",
    "strictfp",
];
/// Words that start statements, never declarations.
const STATEMENTS: [&str; 14] = [
    "if", "for", "while", "switch", "return", "throw", "try", "new", "assert", "println", "print",
    "else", "do", "catch",
];
/// Spock's block labels.
pub const SPOCK_LABELS: [&str; 8] = [
    "given", "setup", "when", "then", "expect", "where", "cleanup", "and",
];

fn ident(t: Option<&Token>) -> Option<&str> {
    match t.map(|t| &t.tok) {
        Some(Tok::Ident(s)) => Some(s),
        _ => None,
    }
}

fn is_sym(t: Option<&Token>, c: char) -> bool {
    matches!(t.map(|t| &t.tok), Some(Tok::Sym(x)) if *x == c)
}

/// A dotted name starting at `i` (`spock.lang.Specification`), and the index after it.
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

/// The index of the token that closes the bracket opened at `open`.
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

struct Open {
    /// Index into `GroovyFile::types`.
    index: usize,
    /// Depth of the type's members.
    depth: u32,
    /// Binary name, for nested types.
    class: String,
}

/// Scan a Groovy file.
pub fn parse(text: &str) -> GroovyFile {
    let tokens = lex(text);
    let mut file = GroovyFile::default();
    let mut stack: Vec<Open> = Vec::new();
    let mut annotations: Vec<String> = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let t = &tokens[i];
        while stack.last().is_some_and(|o| t.depth < o.depth) {
            stack.pop();
        }
        let member_level = stack.last().is_some_and(|o| t.depth == o.depth);
        let top = t.depth == 0;
        if !(top || member_level) {
            i += 1;
            continue;
        }
        // Annotations: `@Unroll`, `@Timeout(5)`.
        if is_sym(Some(t), '@') {
            let (name, mut k) = dotted(&tokens, i + 1);
            if name == "interface" {
                // `@interface` declares an annotation type.
                i += 1;
                continue;
            }
            if is_sym(tokens.get(k), '(') {
                k = closing(&tokens, k, ('(', ')')) + 1;
            }
            annotations.push(name.rsplit('.').next().unwrap_or(&name).to_string());
            i = k.max(i + 1);
            continue;
        }
        match &t.tok {
            Tok::Ident(w) if top && (w == "package" || w == "import") => {
                let mut k = i + 1;
                if w == "import" && ident(tokens.get(k)) == Some("static") {
                    k += 1;
                }
                let (name, end) = dotted(&tokens, k);
                let mut end = end;
                // `import a.b.*`
                if is_sym(tokens.get(end), '.') && is_sym(tokens.get(end + 1), '*') {
                    end += 2;
                }
                if w == "package" {
                    file.package = Some(name).filter(|n| !n.is_empty());
                } else {
                    file.imports.push(name);
                }
                annotations.clear();
                i = end.max(i + 1);
                continue;
            }
            Tok::Ident(w) if TYPE_KEYWORDS.contains(&w.as_str()) => {
                let Some(name) = ident(tokens.get(i + 1)).map(str::to_string) else {
                    i += 1;
                    continue;
                };
                let mut k = i + 2;
                // Generic parameters.
                if is_sym(tokens.get(k), '<') {
                    k = closing(&tokens, k, ('<', '>')) + 1;
                }
                let mut extends = None;
                while k < tokens.len() && !is_sym(tokens.get(k), '{') {
                    if ident(tokens.get(k)) == Some("extends") && extends.is_none() {
                        let (sup, _) = dotted(&tokens, k + 1);
                        extends = Some(sup).filter(|s| !s.is_empty());
                    }
                    k += 1;
                }
                let nested = !stack.is_empty();
                let class = match stack.last() {
                    Some(o) => format!("{}${name}", o.class),
                    None => match &file.package {
                        Some(p) => format!("{p}.{name}"),
                        None => name.clone(),
                    },
                };
                file.types.push(TypeDecl {
                    name,
                    class: class.clone(),
                    kind: w.clone(),
                    line: t.line,
                    extends,
                    annotations: std::mem::take(&mut annotations),
                    methods: Vec::new(),
                    nested,
                });
                if k < tokens.len() {
                    stack.push(Open {
                        index: file.types.len() - 1,
                        depth: tokens[k].depth + 1,
                        class,
                    });
                }
                i = k + 1;
                continue;
            }
            _ => {}
        }
        if member_level {
            if let Some((method, next)) = method_at(&tokens, i, &annotations) {
                let o = stack.last().expect("member level has a type");
                file.types[o.index].methods.push(method);
                annotations.clear();
                i = next;
                continue;
            }
        } else if top {
            // Anything else at the top level is a script statement (or a script method).
            match &t.tok {
                Tok::Ident(w) if MODIFIERS.contains(&w.as_str()) => {}
                Tok::Sym(';') => {}
                _ => file.script = true,
            }
        }
        if !matches!(&t.tok, Tok::Ident(w) if MODIFIERS.contains(&w.as_str())) {
            annotations.clear();
        }
        i += 1;
    }
    file
}

/// A method declaration starting at `i` (modifiers, `def` or a type, the name, the
/// parameter list and the body), with the index after its body.
fn method_at(tokens: &[Token], i: usize, annotations: &[String]) -> Option<(Method, usize)> {
    let depth = tokens[i].depth;
    let mut k = i;
    let mut is_static = false;
    let mut words = 0;
    // Modifiers and the return type (`def`, `void`, `List<String>`, `int[]`).
    while k < tokens.len() {
        match &tokens[k].tok {
            Tok::Ident(w) if STATEMENTS.contains(&w.as_str()) => return None,
            Tok::Ident(w) if MODIFIERS.contains(&w.as_str()) => {
                is_static |= w == "static";
                k += 1;
            }
            Tok::Ident(_) | Tok::Str(_) if is_sym(tokens.get(k + 1), '(') => break,
            Tok::Ident(_) => {
                words += 1;
                k += 1;
                if is_sym(tokens.get(k), '<') {
                    k = closing(tokens, k, ('<', '>')) + 1;
                }
                while is_sym(tokens.get(k), '.')
                    || is_sym(tokens.get(k), '[')
                    || is_sym(tokens.get(k), ']')
                {
                    k += 1;
                    if ident(tokens.get(k)).is_some() && is_sym(tokens.get(k - 1), '.') {
                        k += 1;
                    }
                }
            }
            _ => return None,
        }
        if words > 3 {
            return None;
        }
    }
    let name_tok = tokens.get(k)?;
    let (name, quoted) = match &name_tok.tok {
        Tok::Ident(n) => (n.clone(), false),
        Tok::Str(s) => (s.clone(), true),
        Tok::Sym(_) => return None,
    };
    // A call (`println(x)`) has no declaration words before it; a method needs one, a
    // modifier, or a string name (`def "x"()` always has `def`).
    if words == 0 && k == i {
        return None;
    }
    let close = closing(tokens, k + 1, ('(', ')'));
    let mut b = close + 1;
    // `throws X, Y`
    if ident(tokens.get(b)) == Some("throws") {
        while b < tokens.len() && !is_sym(tokens.get(b), '{') && !is_sym(tokens.get(b), ';') {
            b += 1;
        }
    }
    let mut labels = Vec::new();
    let next = if is_sym(tokens.get(b), '{') {
        let end = closing(tokens, b, ('{', '}'));
        for j in b + 1..end.min(tokens.len()) {
            if tokens[j].depth != depth + 1 {
                continue;
            }
            if let Some(w) = ident(tokens.get(j)) {
                if SPOCK_LABELS.contains(&w)
                    && is_sym(tokens.get(j + 1), ':')
                    && !is_sym(tokens.get(j + 2), ':')
                    && !labels.iter().any(|l| l == w)
                {
                    labels.push(w.to_string());
                }
            }
        }
        end + 1
    } else {
        b
    };
    Some((
        Method {
            name,
            line: name_tok.line,
            quoted,
            is_static,
            annotations: annotations.to_vec(),
            labels,
        },
        next,
    ))
}

impl GroovyFile {
    /// Whether the file imports something from `prefix` (`spock.lang`).
    pub fn imports_from(&self, prefix: &str) -> bool {
        self.imports.iter().any(|i| i.starts_with(prefix))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_packages_classes_methods_and_strings() {
        let src = r#"package com.acme.cart

import spock.lang.Specification
import spock.lang.Unroll
import static java.lang.Math.max

/* A spec. { not a block } */
class CartSpec extends Specification {
    def cart = new Cart()

    def setup() { cart.clear() }

    def "adds items"() {
        given: "an empty cart" // { comment
        def item = new Item('apple', 2)

        when:
        cart.add(item)

        then:
        cart.size() == 1
    }

    @Unroll
    def 'totals #items items'(int items, int total) {
        expect:
        cart.total(items) == total

        where:
        items | total
        1     | 10
    }

    static class Helper {
        static void main(String[] args) { println "x" }
    }
}
"#;
        let f = parse(src);
        assert_eq!(f.package.as_deref(), Some("com.acme.cart"));
        assert_eq!(
            f.imports,
            [
                "spock.lang.Specification",
                "spock.lang.Unroll",
                "java.lang.Math.max"
            ]
        );
        assert!(!f.script);
        let spec = &f.types[0];
        assert_eq!(spec.class, "com.acme.cart.CartSpec");
        assert_eq!(spec.extends.as_deref(), Some("Specification"));
        assert_eq!(spec.line, 7);
        let names: Vec<(&str, u32, bool)> = spec
            .methods
            .iter()
            .map(|m| (m.name.as_str(), m.line, m.quoted))
            .collect();
        assert_eq!(
            names,
            [
                ("setup", 10, false),
                ("adds items", 12, true),
                ("totals #items items", 24, true)
            ]
        );
        assert_eq!(spec.methods[1].labels, ["given", "when", "then"]);
        assert_eq!(spec.methods[2].labels, ["expect", "where"]);
        assert_eq!(spec.methods[2].annotations, ["Unroll"]);
        let helper = &f.types[1];
        assert_eq!(helper.class, "com.acme.cart.CartSpec$Helper");
        assert!(helper.nested);
        assert_eq!(helper.methods[0].name, "main");
        assert!(helper.methods[0].is_static);
    }

    #[test]
    fn scripts_and_shebangs() {
        let script = "#!/usr/bin/env groovy\nimport groovy.json.JsonSlurper\n\ndef version = args ? args[0] : '1.0'\nprintln \"Releasing $version\"\n\ndef bump(String v) { v + '.1' }\n";
        let f = parse(script);
        assert!(f.script);
        assert_eq!(f.imports, ["groovy.json.JsonSlurper"]);
        assert!(f.types.is_empty());

        let class_only =
            "package a\nclass App {\n  static main(args) {\n    println 'hi'\n  }\n}\n";
        let f = parse(class_only);
        assert!(!f.script);
        assert_eq!(f.types[0].methods[0].name, "main");
        assert!(f.types[0].methods[0].is_static);

        // Calls inside methods are not methods; generic return types are.
        let f = parse("class A {\n  List<String> names() { foo(1); return [] }\n  void run() throws IOException { bar() }\n}\n");
        let names: Vec<&str> = f.types[0].methods.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, ["names", "run"]);
    }
}
