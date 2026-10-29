//! A tolerant structural scanner for Java sources: package, imports, type declarations
//! (classes, interfaces, enums, records, annotation types, nested types) and their methods
//! with annotations and modifiers. It skips comments, strings, text blocks and method
//! bodies, so half-typed code still yields what the gutter needs (main methods, tests).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TypeKind {
    Class,
    Interface,
    Enum,
    Record,
    Annotation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MethodDecl {
    pub name: String,
    /// 0-based line of the name.
    pub line: u32,
    /// Annotations as written (`Test`, `org.junit.jupiter.api.Test`).
    pub annotations: Vec<String>,
    pub modifiers: Vec<String>,
    /// The return type as written (`void`, `List<String>`); empty for constructors.
    pub return_type: String,
    /// The parameter list without parentheses (`String[] args`).
    pub params: String,
}

impl MethodDecl {
    pub fn has_modifier(&self, m: &str) -> bool {
        self.modifiers.iter().any(|x| x == m)
    }

    /// Annotated with `@<simple>` (or its qualified name).
    pub fn has_annotation(&self, simple: &str) -> bool {
        has_annotation(&self.annotations, simple)
    }
}

fn has_annotation(list: &[String], simple: &str) -> bool {
    list.iter()
        .any(|a| a == simple || a.rsplit('.').next() == Some(simple))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeDecl {
    pub name: String,
    /// The binary name: `com.acme.Outer$Inner`.
    pub binary_name: String,
    pub kind: TypeKind,
    pub line: u32,
    pub annotations: Vec<String>,
    pub modifiers: Vec<String>,
    /// The superclass as written after `extends` (classes only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extends: Option<String>,
    /// Index of the enclosing type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<usize>,
    pub methods: Vec<MethodDecl>,
}

impl TypeDecl {
    pub fn has_modifier(&self, m: &str) -> bool {
        self.modifiers.iter().any(|x| x == m)
    }

    pub fn has_annotation(&self, simple: &str) -> bool {
        has_annotation(&self.annotations, simple)
    }
}

/// What a Java file declares.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaFile {
    pub package: Option<String>,
    /// Imports as written, without `import` and `;` (`static org.junit.Assert.*`).
    pub imports: Vec<String>,
    pub types: Vec<TypeDecl>,
    /// Methods outside any type (Java 25 compact source files).
    pub top_level_methods: Vec<MethodDecl>,
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Ident(String),
    Punct(char),
    /// A string, character or number literal (its text does not matter).
    Literal,
}

#[derive(Debug, Clone)]
struct Token {
    tok: Tok,
    line: u32,
}

fn lex(text: &str) -> Vec<Token> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    let mut line = 0u32;
    while i < chars.len() {
        let c = chars[i];
        if c == '\n' {
            line += 1;
            i += 1;
            continue;
        }
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'*') {
            i += 2;
            while i < chars.len() && !(chars[i] == '*' && chars.get(i + 1) == Some(&'/')) {
                if chars[i] == '\n' {
                    line += 1;
                }
                i += 1;
            }
            i += 2;
            continue;
        }
        if c == '"' {
            let start_line = line;
            if chars.get(i + 1) == Some(&'"') && chars.get(i + 2) == Some(&'"') {
                // Text block.
                i += 3;
                while i < chars.len() {
                    if chars[i] == '\\' {
                        i += 2;
                        continue;
                    }
                    if chars[i] == '\n' {
                        line += 1;
                    }
                    if chars[i] == '"'
                        && chars.get(i + 1) == Some(&'"')
                        && chars.get(i + 2) == Some(&'"')
                    {
                        i += 3;
                        break;
                    }
                    i += 1;
                }
            } else {
                i += 1;
                while i < chars.len() && chars[i] != '"' && chars[i] != '\n' {
                    if chars[i] == '\\' {
                        i += 1;
                    }
                    i += 1;
                }
                i += 1;
            }
            out.push(Token {
                tok: Tok::Literal,
                line: start_line,
            });
            continue;
        }
        if c == '\'' {
            i += 1;
            while i < chars.len() && chars[i] != '\'' && chars[i] != '\n' {
                if chars[i] == '\\' {
                    i += 1;
                }
                i += 1;
            }
            i += 1;
            out.push(Token {
                tok: Tok::Literal,
                line,
            });
            continue;
        }
        if c.is_ascii_digit() {
            while i < chars.len()
                && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '.')
            {
                i += 1;
            }
            out.push(Token {
                tok: Tok::Literal,
                line,
            });
            continue;
        }
        if c.is_alphabetic() || c == '_' || c == '$' {
            let start = i;
            while i < chars.len()
                && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '$')
            {
                i += 1;
            }
            out.push(Token {
                tok: Tok::Ident(chars[start..i].iter().collect()),
                line,
            });
            continue;
        }
        out.push(Token {
            tok: Tok::Punct(c),
            line,
        });
        i += 1;
    }
    out
}

const MODIFIERS: &[&str] = &[
    "public",
    "protected",
    "private",
    "static",
    "abstract",
    "final",
    "native",
    "synchronized",
    "transient",
    "volatile",
    "strictfp",
    "default",
    "sealed",
    "non-sealed",
];

const KEYWORDS_NOT_NAMES: &[&str] = &[
    "if",
    "for",
    "while",
    "switch",
    "catch",
    "return",
    "new",
    "throw",
    "synchronized",
    "try",
    "else",
    "do",
    "case",
    "assert",
    "super",
    "this",
];

#[derive(Debug)]
enum Scope {
    /// A type body (index into `types`); enums start with their constants.
    Type { index: usize, in_constants: bool },
    /// Any other block (method bodies, initializers, lambdas, array initializers).
    Block,
}

struct Parser {
    toks: Vec<Token>,
    i: usize,
    file: JavaFile,
    stack: Vec<Scope>,
    annotations: Vec<String>,
    modifiers: Vec<String>,
    /// Tokens of the current member header (since the last `;`, `{` or `}`).
    header: Vec<Token>,
    /// An `=` was seen in the header: a field initializer, not a method.
    initializer: bool,
}

impl Parser {
    fn peek(&self, n: usize) -> Option<&Tok> {
        self.toks.get(self.i + n).map(|t| &t.tok)
    }

    fn ident_at(&self, n: usize) -> Option<&str> {
        match self.peek(n) {
            Some(Tok::Ident(s)) => Some(s.as_str()),
            _ => None,
        }
    }

    fn reset_header(&mut self) {
        self.annotations.clear();
        self.modifiers.clear();
        self.header.clear();
        self.initializer = false;
    }

    /// At a level where declarations happen (top level or a type body, not in enum
    /// constants).
    fn declaration_level(&self) -> bool {
        match self.stack.last() {
            None => true,
            Some(Scope::Type { in_constants, .. }) => !in_constants,
            Some(Scope::Block) => false,
        }
    }

    /// Skip a balanced `( … )` starting at the current `(`; returns the tokens inside.
    fn skip_parens(&mut self) -> Vec<Token> {
        let mut depth = 0;
        let mut inner = Vec::new();
        while self.i < self.toks.len() {
            let t = self.toks[self.i].clone();
            self.i += 1;
            match t.tok {
                Tok::Punct('(') => {
                    depth += 1;
                    if depth == 1 {
                        continue;
                    }
                }
                Tok::Punct(')') => {
                    depth -= 1;
                    if depth == 0 {
                        return inner;
                    }
                }
                _ => {}
            }
            inner.push(t);
        }
        inner
    }

    /// `@Name`, `@a.b.Name`, `@Name(…)`. Returns false for `@interface`.
    fn annotation(&mut self) -> bool {
        if self.ident_at(1) == Some("interface") || self.ident_at(1).is_none() {
            return false;
        }
        self.i += 1;
        let mut name = String::new();
        while let Some(Tok::Ident(s)) = self.peek(0) {
            name.push_str(s);
            self.i += 1;
            if self.peek(0) == Some(&Tok::Punct('.')) && matches!(self.peek(1), Some(Tok::Ident(_)))
            {
                name.push('.');
                self.i += 1;
            } else {
                break;
            }
        }
        if self.peek(0) == Some(&Tok::Punct('(')) {
            self.skip_parens();
        }
        self.annotations.push(name);
        true
    }

    fn type_declaration(&mut self, kind: TypeKind) {
        // At `class`/`interface`/`enum`/`record` (or `@` `interface`).
        if kind == TypeKind::Annotation {
            self.i += 2;
        } else {
            self.i += 1;
        }
        let Some(Tok::Ident(name)) = self.peek(0).cloned() else {
            return;
        };
        let line = self.toks[self.i].line;
        self.i += 1;
        let mut extends = None;
        // Up to the body: type parameters, record components, extends, implements, permits.
        while self.i < self.toks.len() {
            match self.peek(0) {
                Some(Tok::Punct('{')) => break,
                Some(Tok::Punct(';')) => {
                    self.reset_header();
                    return;
                }
                Some(Tok::Punct('(')) => {
                    self.skip_parens();
                }
                Some(Tok::Ident(k)) if k == "extends" && kind == TypeKind::Class => {
                    self.i += 1;
                    let mut sup = String::new();
                    while let Some(t) = self.peek(0) {
                        match t {
                            Tok::Ident(s) if s != "implements" && s != "permits" => sup.push_str(s),
                            Tok::Punct('.') => sup.push('.'),
                            _ => break,
                        }
                        self.i += 1;
                    }
                    extends = Some(sup).filter(|s| !s.is_empty());
                }
                _ => self.i += 1,
            }
        }
        let parent = self.stack.iter().rev().find_map(|s| match s {
            Scope::Type { index, .. } => Some(*index),
            Scope::Block => None,
        });
        let binary_name = match parent {
            Some(p) => format!("{}${name}", self.file.types[p].binary_name),
            None => match &self.file.package {
                Some(pkg) => format!("{pkg}.{name}"),
                None => name.clone(),
            },
        };
        self.file.types.push(TypeDecl {
            name,
            binary_name,
            kind,
            line,
            annotations: std::mem::take(&mut self.annotations),
            modifiers: std::mem::take(&mut self.modifiers),
            extends,
            parent,
            methods: Vec::new(),
        });
        let index = self.file.types.len() - 1;
        self.reset_header();
        // Consume `{`.
        if self.peek(0) == Some(&Tok::Punct('{')) {
            self.i += 1;
            self.stack.push(Scope::Type {
                index,
                in_constants: kind == TypeKind::Enum,
            });
        }
    }

    /// At `(` after a member header ending in a name: a method or constructor.
    fn method(&mut self) {
        let Some(name_tok) = self.header.last().cloned() else {
            return;
        };
        let Tok::Ident(name) = name_tok.tok else {
            return;
        };
        let return_type: Vec<Token> = self.header[..self.header.len() - 1]
            .iter()
            .filter(|t| match &t.tok {
                Tok::Ident(s) => !MODIFIERS.contains(&s.as_str()),
                Tok::Punct(c) => "<>[],.?&".contains(*c),
                Tok::Literal => false,
            })
            .cloned()
            .collect();
        let return_type = join_tokens(&return_type);
        let params = join_tokens(&self.skip_parens());
        let decl = MethodDecl {
            name,
            line: name_tok.line,
            annotations: std::mem::take(&mut self.annotations),
            modifiers: std::mem::take(&mut self.modifiers),
            return_type,
            params,
        };
        match self.stack.last() {
            Some(Scope::Type { index, .. }) => self.file.types[*index].methods.push(decl),
            None => self.file.top_level_methods.push(decl),
            Some(Scope::Block) => {}
        }
        self.reset_header();
        // `throws …`, then a body or `;` (abstract / interface methods).
        while self.i < self.toks.len() {
            match self.peek(0) {
                Some(Tok::Punct('{')) => {
                    self.i += 1;
                    self.stack.push(Scope::Block);
                    return;
                }
                Some(Tok::Punct(';')) => {
                    self.i += 1;
                    return;
                }
                Some(Tok::Punct('}')) => return,
                _ => self.i += 1,
            }
        }
    }

    fn statement_at_top(&mut self, keyword: &str) {
        // `package a.b;` / `import [static] a.b.*;`
        self.i += 1;
        let mut text = String::new();
        while self.i < self.toks.len() {
            match &self.toks[self.i].tok {
                Tok::Punct(';') => {
                    self.i += 1;
                    break;
                }
                Tok::Ident(s) => {
                    if s == "static" && text.is_empty() {
                        text.push_str("static ");
                    } else {
                        text.push_str(s);
                    }
                }
                Tok::Punct(c) => text.push(*c),
                Tok::Literal => {}
            }
            self.i += 1;
        }
        if keyword == "package" {
            self.file.package = Some(text);
        } else {
            self.file.imports.push(text);
        }
        self.reset_header();
    }

    fn run(mut self) -> JavaFile {
        while self.i < self.toks.len() {
            let tok = self.toks[self.i].tok.clone();
            let level = self.declaration_level();
            match tok {
                Tok::Punct('{') => {
                    self.i += 1;
                    self.stack.push(Scope::Block);
                    if level {
                        self.reset_header();
                    }
                }
                Tok::Punct('}') => {
                    self.i += 1;
                    self.stack.pop();
                    if self.declaration_level() {
                        self.reset_header();
                    }
                }
                Tok::Punct(';') => {
                    self.i += 1;
                    if let Some(Scope::Type { in_constants, .. }) = self.stack.last_mut() {
                        if *in_constants {
                            *in_constants = false;
                        }
                    }
                    if self.declaration_level() {
                        self.reset_header();
                    }
                }
                _ if !level => {
                    // Enum constants with arguments or bodies: `A(1) { … }`.
                    if self.peek(0) == Some(&Tok::Punct('(')) {
                        self.skip_parens();
                    } else {
                        self.i += 1;
                    }
                }
                Tok::Punct('@') => {
                    if self.ident_at(1) == Some("interface") {
                        self.type_declaration(TypeKind::Annotation);
                    } else if !self.annotation() {
                        self.i += 1;
                    }
                }
                Tok::Ident(ref s)
                    if self.stack.is_empty()
                        && (s == "package" || s == "import")
                        && self.header.is_empty() =>
                {
                    let s = s.clone();
                    self.statement_at_top(&s);
                }
                Tok::Ident(ref s)
                    if matches!(s.as_str(), "class" | "interface" | "enum" | "record")
                        && !self.initializer
                        && matches!(self.peek(1), Some(Tok::Ident(_)))
                        && !matches!(self.header.last().map(|t| &t.tok), Some(Tok::Punct('.'))) =>
                {
                    let kind = match s.as_str() {
                        "class" => TypeKind::Class,
                        "interface" => TypeKind::Interface,
                        "enum" => TypeKind::Enum,
                        _ => TypeKind::Record,
                    };
                    self.type_declaration(kind);
                }
                Tok::Ident(ref s) if MODIFIERS.contains(&s.as_str()) && self.header.is_empty() => {
                    self.modifiers.push(s.clone());
                    self.i += 1;
                }
                Tok::Punct('=') => {
                    self.initializer = true;
                    self.header.push(self.toks[self.i].clone());
                    self.i += 1;
                }
                Tok::Punct('(') => {
                    let named = matches!(
                        self.header.last().map(|t| &t.tok),
                        Some(Tok::Ident(n)) if !KEYWORDS_NOT_NAMES.contains(&n.as_str())
                    );
                    if named && !self.initializer {
                        self.method();
                    } else {
                        self.skip_parens();
                    }
                }
                _ => {
                    self.header.push(self.toks[self.i].clone());
                    self.i += 1;
                }
            }
        }
        self.file
    }
}

/// Source text of tokens: a space between words and after `,`, `>`, `]` and `...`
/// before a word.
fn join_tokens(tokens: &[Token]) -> String {
    let mut out = String::new();
    let mut prev: Option<&Tok> = None;
    let mut dots = 0;
    for t in tokens {
        let word = matches!(t.tok, Tok::Ident(_) | Tok::Literal);
        let space = match prev {
            Some(Tok::Ident(_) | Tok::Literal) => word,
            Some(Tok::Punct(',')) => true,
            Some(Tok::Punct('>' | ']')) => word,
            Some(Tok::Punct('.')) => word && dots == 3,
            _ => false,
        };
        if space {
            out.push(' ');
        }
        match &t.tok {
            Tok::Ident(s) => out.push_str(s),
            Tok::Punct(c) => out.push(*c),
            Tok::Literal => out.push('_'),
        }
        dots = if t.tok == Tok::Punct('.') {
            dots + 1
        } else {
            0
        };
        prev = Some(&t.tok);
    }
    out
}

/// Scan a Java source file.
pub fn parse(text: &str) -> JavaFile {
    Parser {
        toks: lex(text),
        i: 0,
        file: JavaFile::default(),
        stack: Vec::new(),
        annotations: Vec::new(),
        modifiers: Vec::new(),
        header: Vec::new(),
        initializer: false,
    }
    .run()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = r#"package com.acme.shop;

import java.util.List;
import static org.junit.jupiter.api.Assertions.*;

/** A cart. "quoted" { */
@SuppressWarnings({"unchecked", "rawtypes"})
public final class Cart<T extends Item> extends Base implements Iterable<T> {
    private static final String NAME = "cart { ( ";
    private final List<T> items = new ArrayList<>() {{ add(null); }};
    enum State { OPEN("o") { void x() {} }, CLOSED("c"); State(String s) {} String code() { return ""; } }
    record Line(String sku, int qty) { Line { if (qty < 0) throw new IllegalArgumentException(); } int total() { return qty; } }

    public Cart() { this(1); }

    @Override
    public <R> java.util.Map<String, List<R>> group(Function<T, R> f) throws IOException {
        Runnable r = () -> { int x = 1; };
        String block = """
            text { with braces
            """;
        return null;
    }

    abstract static class Inner { abstract void run(); }

    public static void main(String... args) {}
}

@interface Marker { String value() default ""; }
"#;

    #[test]
    fn scans_types_methods_and_imports() {
        let f = parse(SOURCE);
        assert_eq!(f.package.as_deref(), Some("com.acme.shop"));
        assert_eq!(
            f.imports,
            [
                "java.util.List",
                "static org.junit.jupiter.api.Assertions.*"
            ]
        );
        let names: Vec<&str> = f.types.iter().map(|t| t.binary_name.as_str()).collect();
        assert_eq!(
            names,
            [
                "com.acme.shop.Cart",
                "com.acme.shop.Cart$State",
                "com.acme.shop.Cart$Line",
                "com.acme.shop.Cart$Inner",
                "com.acme.shop.Marker"
            ]
        );
        let cart = &f.types[0];
        assert_eq!(cart.line, 7);
        assert_eq!(cart.extends.as_deref(), Some("Base"));
        assert!(cart.has_modifier("final") && cart.has_annotation("SuppressWarnings"));
        let methods: Vec<&str> = cart.methods.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(methods, ["Cart", "group", "main"]);
        let group = &cart.methods[1];
        assert!(group.has_annotation("Override"));
        assert_eq!(group.line, 16);
        assert_eq!(group.params, "Function<T, R> f");
        let main = &cart.methods[2];
        assert!(main.has_modifier("static"));
        assert_eq!(main.return_type, "void");
        assert_eq!(main.params, "String... args");
        let state: Vec<&str> = f.types[1].methods.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(state, ["State", "code"]);
        let line: Vec<&str> = f.types[2].methods.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(line, ["total"]);
        assert_eq!(f.types[2].kind, TypeKind::Record);
        assert!(f.types[3].has_modifier("abstract"));
        assert_eq!(f.types[4].kind, TypeKind::Annotation);
        assert_eq!(f.types[4].methods[0].name, "value");
    }

    #[test]
    fn tolerates_broken_code_and_compact_files() {
        let f = parse("void main() {\n    IO.println(\"hi\");\n}\n");
        assert_eq!(f.top_level_methods[0].name, "main");
        let f = parse("class A {\n  void a() {\n    if (x) {\n  }\n  @Test void b() {}\n");
        assert_eq!(f.types[0].methods[0].name, "a");
        assert!(parse("class").types.is_empty());
        assert!(parse("").types.is_empty());
    }
}
