//! A structural scanner for Kotlin files: package, imports, file annotations
//! (`@file:JvmName`), classes, objects and interfaces (nested ones with their JVM binary
//! names), functions (backtick names too) with annotations and modifiers, `init` blocks and
//! the lambdas passed to supertype constructors (Kotest's `FunSpec({ … })`). It tolerates
//! half-typed code: whatever cannot be read is skipped.

use serde::{Deserialize, Serialize};

/// A token of a Kotlin file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tok {
    /// An identifier or keyword; `quoted` for a backtick name (`` `adds stock` ``).
    Ident {
        text: String,
        quoted: bool,
    },
    /// A string literal's content (templates kept as written).
    Str(String),
    /// A number or character literal.
    Lit,
    Punct(char),
}

/// A token with its 0-based line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub tok: Tok,
    pub line: u32,
}

impl Token {
    pub fn ident(&self) -> Option<&str> {
        match &self.tok {
            Tok::Ident { text, .. } => Some(text),
            _ => None,
        }
    }

    /// An unquoted identifier (keywords are never quoted).
    pub fn keyword(&self) -> Option<&str> {
        match &self.tok {
            Tok::Ident {
                text,
                quoted: false,
            } => Some(text),
            _ => None,
        }
    }

    pub fn is(&self, c: char) -> bool {
        self.tok == Tok::Punct(c)
    }

    pub fn string(&self) -> Option<&str> {
        match &self.tok {
            Tok::Str(s) => Some(s),
            _ => None,
        }
    }
}

/// Split Kotlin source into tokens (comments dropped, strings kept as one token).
pub fn tokenize(text: &str) -> Vec<Token> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    let mut line = 0u32;
    while i < chars.len() {
        let c = chars[i];
        if c == '\n' {
            line += 1;
            i += 1;
        } else if c.is_whitespace() {
            i += 1;
        } else if c == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && chars.get(i + 1) == Some(&'*') {
            // Kotlin block comments nest.
            let mut depth = 0;
            while i < chars.len() {
                if chars[i] == '/' && chars.get(i + 1) == Some(&'*') {
                    depth += 1;
                    i += 2;
                } else if chars[i] == '*' && chars.get(i + 1) == Some(&'/') {
                    depth -= 1;
                    i += 2;
                    if depth == 0 {
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
            let start_line = line;
            let raw = chars.get(i + 1) == Some(&'"') && chars.get(i + 2) == Some(&'"');
            let (s, next, lines) = read_string(&chars, i, raw);
            out.push(Token {
                tok: Tok::Str(s),
                line: start_line,
            });
            line += lines;
            i = next;
        } else if c == '\'' {
            // A character literal: '\n', 'a', 'A'.
            i += 1;
            while i < chars.len() && chars[i] != '\'' && chars[i] != '\n' {
                if chars[i] == '\\' {
                    i += 1;
                }
                i += 1;
            }
            i += 1;
            out.push(Token {
                tok: Tok::Lit,
                line,
            });
        } else if c == '`' {
            let start = i + 1;
            let mut j = start;
            while j < chars.len() && chars[j] != '`' && chars[j] != '\n' {
                j += 1;
            }
            out.push(Token {
                tok: Tok::Ident {
                    text: chars[start..j].iter().collect(),
                    quoted: true,
                },
                line,
            });
            i = j + 1;
        } else if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            out.push(Token {
                tok: Tok::Ident {
                    text: chars[start..i].iter().collect(),
                    quoted: false,
                },
                line,
            });
        } else if c.is_ascii_digit() {
            while i < chars.len()
                && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '.')
                && !(chars[i] == '.' && chars.get(i + 1) == Some(&'.'))
            {
                i += 1;
            }
            out.push(Token {
                tok: Tok::Lit,
                line,
            });
        } else {
            out.push(Token {
                tok: Tok::Punct(c),
                line,
            });
            i += 1;
        }
    }
    out
}

/// A string literal starting at `i`: its text, the index after it and the lines it spans.
fn read_string(chars: &[char], i: usize, raw: bool) -> (String, usize, u32) {
    let mut s = String::new();
    let mut lines = 0;
    let mut j = if raw { i + 3 } else { i + 1 };
    while j < chars.len() {
        let c = chars[j];
        if raw {
            if c == '"' && chars.get(j + 1) == Some(&'"') && chars.get(j + 2) == Some(&'"') {
                j += 3;
                // `""""` ends with extra quotes that belong to the string.
                while chars.get(j) == Some(&'"') {
                    s.push('"');
                    j += 1;
                }
                return (s, j, lines);
            }
        } else if c == '"' {
            return (s, j + 1, lines);
        } else if c == '\n' {
            // An unterminated string ends at the line end.
            return (s, j, lines);
        } else if c == '\\' {
            if let Some(&n) = chars.get(j + 1) {
                s.push(match n {
                    'n' => '\n',
                    't' => '\t',
                    other => other,
                });
                j += 2;
                continue;
            }
        }
        if c == '$' && chars.get(j + 1) == Some(&'{') {
            // A template: keep it as written, braces balanced.
            let mut depth = 0;
            while j < chars.len() {
                let d = chars[j];
                s.push(d);
                j += 1;
                if d == '{' {
                    depth += 1;
                } else if d == '}' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                } else if d == '\n' {
                    lines += 1;
                }
            }
            continue;
        }
        if c == '\n' {
            lines += 1;
        }
        s.push(c);
        j += 1;
    }
    (s, j, lines)
}

/// What a class-like declaration is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ClassKind {
    Class,
    Interface,
    Object,
    CompanionObject,
    Enum,
    Annotation,
}

/// An annotation: its simple or qualified name and its first string argument.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Annotation {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub argument: Option<String>,
}

/// A supertype: `FunSpec({ … })` has a constructor call with a lambda.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SuperType {
    /// As written (`FunSpec`, `io.kotest.core.spec.style.StringSpec`).
    pub name: String,
    /// Token range of the constructor arguments (inside the parentheses).
    #[serde(skip)]
    pub arguments: Option<(usize, usize)>,
}

impl SuperType {
    /// The simple name (`StringSpec`).
    pub fn simple(&self) -> &str {
        self.name.rsplit('.').next().unwrap_or(&self.name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FunDecl {
    pub name: String,
    /// 0-based line of the name.
    pub line: u32,
    pub annotations: Vec<Annotation>,
    pub modifiers: Vec<String>,
    /// The parameter list as written, without parentheses.
    pub params: String,
    /// An extension function's receiver (`String` in `fun String.shout()`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receiver: Option<String>,
}

impl FunDecl {
    pub fn has_modifier(&self, m: &str) -> bool {
        self.modifiers.iter().any(|x| x == m)
    }

    pub fn has_annotation(&self, simple: &str) -> bool {
        has_annotation(&self.annotations, simple)
    }
}

fn has_annotation(list: &[Annotation], simple: &str) -> bool {
    list.iter()
        .any(|a| a.name == simple || a.name.rsplit('.').next() == Some(simple))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassDecl {
    pub name: String,
    /// JVM binary name: `com.acme.Cart$Totals`, `com.acme.App$Companion`.
    pub binary_name: String,
    pub kind: ClassKind,
    pub line: u32,
    pub annotations: Vec<Annotation>,
    pub modifiers: Vec<String>,
    pub supertypes: Vec<SuperType>,
    pub functions: Vec<FunDecl>,
    /// Index of the enclosing class in [`KotlinFile::classes`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<usize>,
    /// Token ranges of the `init { … }` blocks.
    #[serde(skip)]
    pub init_blocks: Vec<(usize, usize)>,
}

impl ClassDecl {
    pub fn has_modifier(&self, m: &str) -> bool {
        self.modifiers.iter().any(|x| x == m)
    }

    pub fn has_annotation(&self, simple: &str) -> bool {
        has_annotation(&self.annotations, simple)
    }
}

/// The structure of a Kotlin file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KotlinFile {
    pub package: Option<String>,
    pub imports: Vec<String>,
    /// `@file:JvmName("Tools")` and friends.
    pub file_annotations: Vec<Annotation>,
    pub classes: Vec<ClassDecl>,
    pub functions: Vec<FunDecl>,
    #[serde(skip)]
    pub tokens: Vec<Token>,
}

impl KotlinFile {
    /// The class that holds the top-level functions of `path`: `@file:JvmName`, else the
    /// file name + `Kt` (`main.kt` → `MainKt`), in the file's package.
    pub fn facade_class(&self, path: &str) -> String {
        let simple = self
            .file_annotations
            .iter()
            .find(|a| a.name == "JvmName")
            .and_then(|a| a.argument.clone())
            .unwrap_or_else(|| facade_name(path));
        match &self.package {
            Some(p) if !p.is_empty() => format!("{p}.{simple}"),
            _ => simple,
        }
    }
}

/// `main.kt` → `MainKt`, `app-tools.kt` → `App_toolsKt` (as kotlinc names file facades).
pub fn facade_name(path: &str) -> String {
    let file = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let stem = file
        .strip_suffix(".kts")
        .or_else(|| file.strip_suffix(".kt"))
        .unwrap_or(file);
    let mut out = String::new();
    for (i, c) in stem.chars().enumerate() {
        let c = if c.is_alphanumeric() || c == '_' {
            c
        } else {
            '_'
        };
        if i == 0 {
            out.extend(c.to_uppercase());
        } else {
            out.push(c);
        }
    }
    format!("{out}Kt")
}

const MODIFIERS: &[&str] = &[
    "public",
    "private",
    "internal",
    "protected",
    "open",
    "abstract",
    "final",
    "sealed",
    "data",
    "inner",
    "enum",
    "annotation",
    "value",
    "override",
    "suspend",
    "inline",
    "noinline",
    "crossinline",
    "operator",
    "infix",
    "tailrec",
    "external",
    "const",
    "lateinit",
    "actual",
    "expect",
    "companion",
    "fun",
];

const DECLARATIONS: &[&str] = &[
    "fun",
    "val",
    "var",
    "class",
    "interface",
    "object",
    "init",
    "constructor",
    "typealias",
    "package",
    "import",
];

struct Parser<'a> {
    toks: &'a [Token],
    i: usize,
    file: KotlinFile,
}

/// Index after the bracket that closes the one at `open` (`(`, `{`, `[` or `<`).
pub fn skip_balanced(toks: &[Token], open: usize) -> usize {
    let (o, c) = match &toks[open].tok {
        Tok::Punct('(') => ('(', ')'),
        Tok::Punct('[') => ('[', ']'),
        Tok::Punct('<') => ('<', '>'),
        _ => ('{', '}'),
    };
    let mut depth = 0i32;
    let mut i = open;
    while i < toks.len() {
        if toks[i].is(o) {
            depth += 1;
        } else if toks[i].is(c) {
            depth -= 1;
            if depth == 0 {
                return i + 1;
            }
        } else if o == '<' && (toks[i].is('{') || toks[i].is(';') || toks[i].is('=')) {
            // Not a type argument list after all (`a < b`).
            return open + 1;
        }
        i += 1;
    }
    toks.len()
}

impl<'a> Parser<'a> {
    fn tok(&self, i: usize) -> Option<&'a Token> {
        self.toks.get(i)
    }

    fn kw(&self, i: usize) -> Option<&'a str> {
        self.tok(i).and_then(Token::keyword)
    }

    fn punct(&self, i: usize, c: char) -> bool {
        self.tok(i).is_some_and(|t| t.is(c))
    }

    /// A dotted name at `i` (`com.acme.App`), and the index after it.
    fn dotted(&self, mut i: usize) -> (String, usize) {
        let mut name = String::new();
        while let Some(id) = self.tok(i).and_then(Token::ident) {
            name.push_str(id);
            if self.punct(i + 1, '.') && self.tok(i + 2).and_then(Token::ident).is_some() {
                name.push('.');
                i += 2;
            } else {
                i += 1;
                break;
            }
        }
        (name, i)
    }

    /// Whether the token at `i` starts a new declaration (ends an expression or a type).
    fn starts_declaration(&self, i: usize) -> bool {
        match self.tok(i) {
            None => true,
            Some(t) if t.is('}') || t.is('@') || t.is(';') => true,
            Some(t) => t
                .keyword()
                .is_some_and(|k| DECLARATIONS.contains(&k) || MODIFIERS.contains(&k)),
        }
    }

    /// Skip an expression or a type that ends at the next declaration on a later line (or a
    /// closing brace of the enclosing scope).
    fn skip_expression(&mut self) {
        let start_line = self.tok(self.i).map_or(0, |t| t.line);
        let mut last_line = start_line;
        while let Some(t) = self.tok(self.i) {
            if t.is('}') || t.is(';') {
                return;
            }
            if t.line > last_line && self.starts_declaration(self.i) {
                // `.map { }` on the next line continues; a declaration does not.
                return;
            }
            if t.is('(') || t.is('{') || t.is('[') {
                self.i = skip_balanced(self.toks, self.i);
                last_line = self
                    .tok(self.i.saturating_sub(1))
                    .map_or(last_line, |t| t.line);
                continue;
            }
            last_line = t.line;
            self.i += 1;
        }
    }

    /// Annotations at `self.i` (`@Test`, `@file:JvmName("X")`, `@Tag("slow")`).
    fn annotations(&mut self, file_level: &mut Vec<Annotation>) -> Vec<Annotation> {
        let mut out = Vec::new();
        while self.punct(self.i, '@') {
            let mut j = self.i + 1;
            let mut target_file = false;
            // Use-site targets: `@file:`, `@get:`, `@param:`.
            if self.tok(j).and_then(Token::keyword).is_some() && self.punct(j + 1, ':') {
                target_file = self.kw(j) == Some("file");
                j += 2;
            }
            if self.punct(j, '[') {
                // `@[A B]`
                self.i = skip_balanced(self.toks, j);
                continue;
            }
            let (name, mut k) = self.dotted(j);
            if self.punct(k, '<') {
                k = skip_balanced(self.toks, k);
            }
            let mut argument = None;
            if self.punct(k, '(') {
                let end = skip_balanced(self.toks, k);
                argument = self.toks[k..end]
                    .iter()
                    .find_map(|t| t.string().map(str::to_string));
                k = end;
            }
            let a = Annotation { name, argument };
            if target_file {
                file_level.push(a);
            } else {
                out.push(a);
            }
            self.i = k.max(self.i + 1);
        }
        out
    }

    /// Declarations until the `}` that closes the scope (or the end of the file).
    fn scope(&mut self, parent: Option<usize>) -> Vec<FunDecl> {
        let mut functions = Vec::new();
        let mut annotations: Vec<Annotation> = Vec::new();
        let mut modifiers: Vec<String> = Vec::new();
        while let Some(t) = self.tok(self.i) {
            if t.is('}') {
                if parent.is_some() {
                    self.i += 1;
                    return functions;
                }
                self.i += 1;
                continue;
            }
            if t.is('@') {
                let mut file_level = Vec::new();
                let a = self.annotations(&mut file_level);
                self.file.file_annotations.extend(file_level);
                annotations.extend(a);
                continue;
            }
            let Some(k) = t.keyword() else {
                if t.is('{') || t.is('(') || t.is('[') {
                    self.i = skip_balanced(self.toks, self.i);
                } else {
                    self.i += 1;
                }
                annotations.clear();
                modifiers.clear();
                continue;
            };
            match k {
                "package" if parent.is_none() => {
                    let (name, next) = self.dotted(self.i + 1);
                    self.file.package = Some(name);
                    self.i = next;
                }
                "import" if parent.is_none() => {
                    let (mut name, mut next) = self.dotted(self.i + 1);
                    if self.punct(next, '.') && self.punct(next + 1, '*') {
                        name.push_str(".*");
                        next += 2;
                    }
                    self.file.imports.push(name);
                    self.i = next;
                }
                "class" | "interface" | "object" => {
                    let mods = std::mem::take(&mut modifiers);
                    let anns = std::mem::take(&mut annotations);
                    self.class(k, mods, anns, parent);
                    continue;
                }
                "fun"
                    if !self.punct(self.i + 1, '(') && self.kw(self.i + 1) != Some("interface") =>
                {
                    let mods = std::mem::take(&mut modifiers);
                    let anns = std::mem::take(&mut annotations);
                    if let Some(f) = self.function(mods, anns) {
                        functions.push(f);
                    }
                    continue;
                }
                "init" if parent.is_some() && self.punct(self.i + 1, '{') => {
                    let start = self.i + 1;
                    let end = skip_balanced(self.toks, start);
                    if let Some(p) = parent {
                        self.file.classes[p].init_blocks.push((start + 1, end - 1));
                    }
                    self.i = end;
                }
                "val" | "var" | "typealias" | "constructor" => {
                    self.i += 1;
                    self.skip_expression();
                }
                m if MODIFIERS.contains(&m) => {
                    modifiers.push(m.to_string());
                    self.i += 1;
                    continue;
                }
                _ => {
                    self.i += 1;
                }
            }
            annotations.clear();
            modifiers.clear();
        }
        functions
    }

    fn class(
        &mut self,
        keyword: &str,
        modifiers: Vec<String>,
        annotations: Vec<Annotation>,
        parent: Option<usize>,
    ) {
        let line = self.toks[self.i].line;
        self.i += 1;
        let companion = keyword == "object" && modifiers.iter().any(|m| m == "companion");
        let (name, line) = match self.tok(self.i) {
            Some(t) if t.ident().is_some() && !t.is('{') && t.keyword() != Some("fun") => {
                self.i += 1;
                (t.ident().unwrap_or_default().to_string(), t.line)
            }
            _ if companion => ("Companion".to_string(), line),
            // `object : Runnable { }` expressions have no name.
            _ => (String::new(), line),
        };
        let kind = match keyword {
            "interface" => ClassKind::Interface,
            "object" if companion => ClassKind::CompanionObject,
            "object" => ClassKind::Object,
            _ if modifiers.iter().any(|m| m == "enum") => ClassKind::Enum,
            _ if modifiers.iter().any(|m| m == "annotation") => ClassKind::Annotation,
            _ => ClassKind::Class,
        };
        if self.punct(self.i, '<') {
            self.i = skip_balanced(self.toks, self.i);
        }
        // Primary constructor: `private constructor(…)`, `@Inject constructor(…)`, `(…)`.
        let mut ignored = Vec::new();
        let _ = self.annotations(&mut ignored);
        while self
            .kw(self.i)
            .is_some_and(|k| MODIFIERS.contains(&k) && k != "fun")
        {
            self.i += 1;
        }
        if self.kw(self.i) == Some("constructor") {
            self.i += 1;
        }
        if self.punct(self.i, '(') {
            self.i = skip_balanced(self.toks, self.i);
        }
        let mut supertypes = Vec::new();
        if self.punct(self.i, ':') {
            self.i += 1;
            loop {
                let _ = self.annotations(&mut ignored);
                let (st, mut next) = self.dotted(self.i);
                if st.is_empty() {
                    break;
                }
                if self.punct(next, '<') {
                    next = skip_balanced(self.toks, next);
                }
                let mut arguments = None;
                if self.punct(next, '(') {
                    let end = skip_balanced(self.toks, next);
                    arguments = Some((next + 1, end - 1));
                    next = end;
                }
                // `by delegate`
                if self.kw(next) == Some("by") {
                    let (_, mut n) = self.dotted(next + 1);
                    while self.punct(n, '(') || self.punct(n, '<') {
                        n = skip_balanced(self.toks, n);
                    }
                    next = n;
                }
                supertypes.push(SuperType {
                    name: st,
                    arguments,
                });
                self.i = next;
                if self.punct(self.i, ',') {
                    self.i += 1;
                } else {
                    break;
                }
            }
        }
        if self.kw(self.i) == Some("where") {
            while self.tok(self.i).is_some_and(|t| !t.is('{')) && !self.starts_declaration(self.i) {
                self.i += 1;
            }
        }
        let binary_name = match parent {
            Some(p) => format!("{}${name}", self.file.classes[p].binary_name),
            None => match &self.file.package {
                Some(pkg) if !pkg.is_empty() => format!("{pkg}.{name}"),
                _ => name.clone(),
            },
        };
        let index = self.file.classes.len();
        self.file.classes.push(ClassDecl {
            name,
            binary_name,
            kind,
            line,
            annotations,
            modifiers,
            supertypes,
            functions: Vec::new(),
            parent,
            init_blocks: Vec::new(),
        });
        if self.punct(self.i, '{') {
            self.i += 1;
            let functions = self.scope(Some(index));
            self.file.classes[index].functions = functions;
        }
    }

    fn function(
        &mut self,
        modifiers: Vec<String>,
        annotations: Vec<Annotation>,
    ) -> Option<FunDecl> {
        self.i += 1;
        if self.punct(self.i, '<') {
            self.i = skip_balanced(self.toks, self.i);
        }
        // `fun Receiver<T>.name(` / `fun name(`: the name is the last identifier before `(`.
        let mut receiver = String::new();
        let mut name: Option<(String, u32)> = None;
        while let Some(t) = self.tok(self.i) {
            if t.is('(') {
                break;
            }
            if let Some(id) = t.ident() {
                if let Some((prev, _)) = name.take() {
                    receiver.push_str(&prev);
                }
                name = Some((id.to_string(), t.line));
                self.i += 1;
            } else if t.is('.') || t.is('?') {
                if name.is_some() {
                    receiver.push_str(&name.take().map(|n| n.0).unwrap_or_default());
                }
                receiver.push(if t.is('.') { '.' } else { '?' });
                self.i += 1;
            } else if t.is('<') {
                let end = skip_balanced(self.toks, self.i);
                if let Some((n, _)) = name.take() {
                    receiver.push_str(&n);
                }
                receiver.push_str("<…>");
                self.i = end;
            } else {
                return None;
            }
        }
        let (name, line) = name?;
        let open = self.i;
        let close = skip_balanced(self.toks, open);
        let params = render(&self.toks[open + 1..close.saturating_sub(1).max(open + 1)]);
        self.i = close;
        // Return type, `where` clauses, then a body or an expression.
        while let Some(t) = self.tok(self.i) {
            if t.is('{') {
                self.i = skip_balanced(self.toks, self.i);
                break;
            }
            if t.is('=') {
                self.i += 1;
                self.skip_expression();
                break;
            }
            if t.is('(') || t.is('<') {
                self.i = skip_balanced(self.toks, self.i);
                continue;
            }
            if t.line > line && self.starts_declaration(self.i) || t.is('}') || t.is(';') {
                break;
            }
            self.i += 1;
        }
        let receiver = receiver.trim_end_matches('.').to_string();
        Some(FunDecl {
            name,
            line,
            annotations,
            modifiers,
            params,
            receiver: (!receiver.is_empty()).then_some(receiver),
        })
    }
}

/// Tokens back to compact text (`args: Array<String>`).
pub fn render(toks: &[Token]) -> String {
    let mut out = String::new();
    for t in toks {
        match &t.tok {
            Tok::Ident { text, quoted } => {
                if out.ends_with(|c: char| c.is_alphanumeric() || c == '_' || c == '`') {
                    out.push(' ');
                }
                if *quoted {
                    out.push('`');
                    out.push_str(text);
                    out.push('`');
                } else {
                    out.push_str(text);
                }
            }
            Tok::Str(s) => {
                out.push('"');
                out.push_str(s);
                out.push('"');
            }
            Tok::Lit => out.push('0'),
            Tok::Punct(c) => {
                if *c == ':' || *c == ',' {
                    out.push(*c);
                    out.push(' ');
                } else {
                    out.push(*c);
                }
            }
        }
    }
    out.trim().replace(" :", ":").replace(", )", ")")
}

/// Scan a Kotlin file.
pub fn parse(text: &str) -> KotlinFile {
    let tokens = tokenize(text);
    let mut p = Parser {
        toks: &tokens,
        i: 0,
        file: KotlinFile::default(),
    };
    let functions = p.scope(None);
    let mut file = p.file;
    file.functions = functions;
    file.tokens = tokens;
    file
}

#[cfg(test)]
mod tests {
    use super::*;

    const APP: &str = r#"@file:JvmName("Tools")
package com.acme.inventory

import kotlinx.coroutines.*
import com.acme.model.Item as Thing

/* A /* nested */ comment with fun fake() {} */
data class Item(val name: String, val qty: Int = 0) : Comparable<Item> {
    override fun compareTo(other: Item) = qty - other.qty

    companion object {
        @JvmStatic
        fun main(args: Array<String>) {
            println("Hi ${args.size} {")
        }
    }

    inner class Stock { fun `add stock`(n: Int) {} }
}

fun String.shout(): String = uppercase()
    .plus("!")

suspend fun main() {
    val s = """
        fun notAFunction() {}
    """
    println(s)
}

object Registry : Map<String, Int> by emptyMap() {
    fun lookup(key: String): Int? = get(key)
}
"#;

    #[test]
    fn scans_declarations() {
        let f = parse(APP);
        assert_eq!(f.package.as_deref(), Some("com.acme.inventory"));
        assert_eq!(f.imports, ["kotlinx.coroutines.*", "com.acme.model.Item"]);
        assert_eq!(f.file_annotations[0].argument.as_deref(), Some("Tools"));
        assert_eq!(
            f.facade_class("src/main/kotlin/App.kt"),
            "com.acme.inventory.Tools"
        );
        let names: Vec<(&str, &str, ClassKind)> = f
            .classes
            .iter()
            .map(|c| (c.name.as_str(), c.binary_name.as_str(), c.kind))
            .collect();
        assert_eq!(
            names,
            [
                ("Item", "com.acme.inventory.Item", ClassKind::Class),
                (
                    "Companion",
                    "com.acme.inventory.Item$Companion",
                    ClassKind::CompanionObject
                ),
                ("Stock", "com.acme.inventory.Item$Stock", ClassKind::Class),
                ("Registry", "com.acme.inventory.Registry", ClassKind::Object),
            ]
        );
        assert!(f.classes[0].has_modifier("data"));
        assert_eq!(f.classes[0].supertypes[0].simple(), "Comparable");
        assert_eq!(f.classes[0].functions[0].name, "compareTo");
        let main = &f.classes[1].functions[0];
        assert_eq!((main.name.as_str(), main.line), ("main", 12));
        assert!(main.has_annotation("JvmStatic"));
        assert_eq!(main.params, "args: Array<String>");
        assert_eq!(f.classes[2].functions[0].name, "add stock");
        let top: Vec<&str> = f.functions.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(top, ["shout", "main"]);
        assert_eq!(f.functions[0].receiver.as_deref(), Some("String"));
        assert!(f.functions[1].has_modifier("suspend"));
        assert_eq!(f.functions[1].line, 23);
        assert_eq!(f.classes[3].functions[0].name, "lookup");
    }

    #[test]
    fn facade_names() {
        assert_eq!(facade_name("src/main.kt"), "MainKt");
        assert_eq!(facade_name("app-tools.kt"), "App_toolsKt");
        assert_eq!(facade_name("build.main.kts"), "Build_mainKt");
        assert_eq!(parse("fun main() {}").facade_class("x/Hello.kt"), "HelloKt");
    }

    #[test]
    fn tolerates_half_typed_code() {
        let f = parse("package a\nclass A {\n  fun b(\n  fun c() {}\n");
        assert_eq!(f.classes[0].name, "A");
        let f = parse("class\nfun x(");
        assert!(f.functions.is_empty() || f.functions[0].name == "x");
        assert!(tokenize("\"unterminated\nval x = 1").len() > 2);
    }
}
