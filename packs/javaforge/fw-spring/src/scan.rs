//! A tolerant structural scanner for Java and Kotlin sources, made for framework analysis:
//! package, imports, classes (with supertypes and their type arguments), constructors
//! (Kotlin primary constructors too), methods / functions and fields / properties, each with
//! its annotations **and their arguments** (string literals, constants, class literals).
//! Method bodies, initializers and expressions are skipped, so half-typed code still yields
//! the declarations around it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Lang {
    Java,
    Kotlin,
}

impl Lang {
    /// `.java` or `.kt` / `.kts` files.
    pub fn of_path(path: &str) -> Option<Lang> {
        let lower = path.to_ascii_lowercase();
        if lower.ends_with(".java") {
            Some(Lang::Java)
        } else if lower.ends_with(".kt")
            || (lower.ends_with(".kts") && !lower.ends_with(".gradle.kts"))
        {
            Some(Lang::Kotlin)
        } else {
            None
        }
    }
}

/// A value inside an annotation's arguments.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "text")]
pub enum Val {
    /// A string literal (or a concatenation of literals).
    Str(String),
    /// Anything else as written: `RequestMethod.GET`, `Order.class`, `BASE + "/x"`.
    Expr(String),
}

impl Val {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Val::Str(s) => Some(s),
            Val::Expr(_) => None,
        }
    }

    /// The literal, or an expression made of literals and constants of `consts` (simple
    /// names; `Paths.BASE` looks up `BASE`).
    pub fn resolve(&self, consts: &BTreeMap<String, String>) -> Option<String> {
        match self {
            Val::Str(s) => Some(s.clone()),
            Val::Expr(e) => {
                let mut out = String::new();
                for part in split_plus(e) {
                    let part = part.trim();
                    if let Some(lit) = part.strip_prefix('"').and_then(|p| p.strip_suffix('"')) {
                        out.push_str(lit);
                    } else {
                        let simple = part.rsplit('.').next().unwrap_or(part);
                        out.push_str(consts.get(simple)?);
                    }
                }
                Some(out)
            }
        }
    }

    /// The last segment of an expression (`RequestMethod.GET` → `GET`).
    pub fn simple(&self) -> &str {
        match self {
            Val::Str(s) => s,
            Val::Expr(e) => e.rsplit(['.', ':']).next().unwrap_or(e),
        }
    }
}

fn split_plus(e: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut in_str = false;
    let bytes = e.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if in_str => i += 1,
            b'"' => in_str = !in_str,
            b'+' if !in_str => {
                parts.push(&e[start..i]);
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    parts.push(&e[start..]);
    parts
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnArg {
    /// `value` for the unnamed argument.
    pub name: String,
    /// Array arguments are flattened (`{"/a", "/b"}`, `["/a"]`, `arrayOf("/a")`).
    pub values: Vec<Val>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Annotation {
    /// As written: `GetMapping` or `org.springframework.web.bind.annotation.GetMapping`.
    pub name: String,
    /// 0-based line of the `@`.
    pub line: u32,
    pub args: Vec<AnnArg>,
}

impl Annotation {
    /// The simple name (`GetMapping`).
    pub fn simple(&self) -> &str {
        self.name.rsplit('.').next().unwrap_or(&self.name)
    }

    /// Values of the named argument.
    pub fn arg(&self, name: &str) -> Option<&[Val]> {
        self.args
            .iter()
            .find(|a| a.name == name)
            .map(|a| a.values.as_slice())
    }

    /// Values of the first argument among `names` that is present (`value`, `path`).
    pub fn first_arg(&self, names: &[&str]) -> Option<&[Val]> {
        names.iter().find_map(|n| self.arg(n))
    }

    /// The first string literal of the first argument among `names`.
    pub fn string(&self, names: &[&str]) -> Option<String> {
        self.first_arg(names)?
            .iter()
            .find_map(|v| v.as_str().map(str::to_string))
    }
}

/// Annotated with `@<simple>` (simple or qualified name).
pub fn find<'a>(list: &'a [Annotation], simple: &str) -> Option<&'a Annotation> {
    list.iter().find(|a| a.simple() == simple)
}

pub fn has(list: &[Annotation], simple: &str) -> bool {
    find(list, simple).is_some()
}

/// A type as written, split into its raw name and type arguments.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeRef {
    /// `JpaRepository`, `java.util.List` (without type arguments, `?` or `[]`).
    pub name: String,
    /// Type arguments as written (`Order`, `Long`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
}

impl TypeRef {
    pub fn simple(&self) -> &str {
        self.name.rsplit('.').next().unwrap_or(&self.name)
    }

    /// Parse `Map<String, List<Foo>>?` / `Foo[]` / `Foo...`.
    pub fn parse(text: &str) -> TypeRef {
        let text = text.trim().trim_end_matches('?').trim();
        let text = text.trim_end_matches("...").trim_end_matches("[]").trim();
        match text.find('<') {
            Some(lt) => {
                let inner = text[lt + 1..].trim_end().trim_end_matches('?');
                let inner = inner.strip_suffix('>').unwrap_or(inner);
                TypeRef {
                    name: text[..lt].trim().to_string(),
                    args: split_top(inner)
                        .into_iter()
                        .map(|s| {
                            s.trim()
                                .trim_start_matches("out ")
                                .trim_start_matches("in ")
                                .to_string()
                        })
                        .filter(|s| !s.is_empty())
                        .collect(),
                }
            }
            None => TypeRef {
                name: text.to_string(),
                args: Vec::new(),
            },
        }
    }
}

/// Split at top-level commas (outside `<>`, `()`, `[]`, `{}`).
pub fn split_top(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    for (i, c) in s.char_indices() {
        match c {
            '<' | '(' | '[' | '{' => depth += 1,
            '>' | ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                out.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    if !s[start..].trim().is_empty() {
        out.push(&s[start..]);
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ClassKind {
    Class,
    Interface,
    Enum,
    Record,
    Annotation,
    /// Kotlin `object` / `companion object`.
    Object,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Param {
    pub name: String,
    /// As written, without annotations and modifiers (`List<Order>`, `String?`).
    #[serde(rename = "type")]
    pub type_name: String,
    pub annotations: Vec<Annotation>,
    pub line: u32,
    pub column: u32,
    /// Kotlin `val` / `var` constructor parameter (also a property).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub property: bool,
    /// The doc comment before the parameter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    /// A literal default value (Kotlin `= 10`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Callable {
    pub name: String,
    pub line: u32,
    pub column: u32,
    pub annotations: Vec<Annotation>,
    pub modifiers: Vec<String>,
    /// Empty for constructors and Kotlin functions without a declared type.
    #[serde(rename = "returnType")]
    pub return_type: String,
    pub params: Vec<Param>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
    pub line: u32,
    pub column: u32,
    pub annotations: Vec<Annotation>,
    pub modifiers: Vec<String>,
    /// Has an initializer (`= …`, Kotlin `by …`).
    pub initialized: bool,
    /// The doc comment before the declaration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    /// A literal initializer (`10`, `"EUR"`, `true`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
}

impl Field {
    pub fn has_modifier(&self, m: &str) -> bool {
        self.modifiers.iter().any(|x| x == m)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassInfo {
    pub name: String,
    /// `com.acme.Outer.Inner`.
    pub qualified: String,
    pub kind: ClassKind,
    pub line: u32,
    pub column: u32,
    pub annotations: Vec<Annotation>,
    pub modifiers: Vec<String>,
    /// `extends` / `implements` / Kotlin `:` supertypes.
    pub supertypes: Vec<TypeRef>,
    /// Java constructors, the Kotlin primary constructor (first) and secondary ones.
    pub constructors: Vec<Callable>,
    pub methods: Vec<Callable>,
    pub fields: Vec<Field>,
    /// Enum constants.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub constants: Vec<String>,
    /// Index of the enclosing class in [`SourceFile::classes`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<usize>,
}

impl ClassInfo {
    pub fn has_modifier(&self, m: &str) -> bool {
        self.modifiers.iter().any(|x| x == m)
    }

    pub fn annotation(&self, simple: &str) -> Option<&Annotation> {
        find(&self.annotations, simple)
    }
}

/// What a Java or Kotlin file declares.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceFile {
    pub path: String,
    pub lang: Lang,
    pub package: Option<String>,
    /// As written, without `import` / `;` (`jakarta.persistence.*`).
    pub imports: Vec<String>,
    pub classes: Vec<ClassInfo>,
    /// String constants (`static final String X = "…"`, `const val X = "…"`) by simple name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub constants: BTreeMap<String, String>,
}

impl SourceFile {
    /// Whether an import starts with `prefix` (`jakarta.ws.rs`).
    pub fn imports_from(&self, prefix: &str) -> bool {
        self.imports.iter().any(|i| {
            i == prefix
                || i.strip_prefix(prefix)
                    .is_some_and(|rest| rest.starts_with('.'))
        })
    }
}

// ---- Lexer ----------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Ident(String),
    Str(String),
    /// Numbers and character literals, as written.
    Lit(String),
    Punct(char),
}

#[derive(Debug, Clone)]
struct Token {
    tok: Tok,
    line: u32,
    col: u32,
    /// The `/** … */` comment right before this token (its text without the stars).
    doc: Option<String>,
}

impl Token {
    fn ident(&self) -> Option<&str> {
        match &self.tok {
            Tok::Ident(s) => Some(s),
            _ => None,
        }
    }

    fn is(&self, c: char) -> bool {
        self.tok == Tok::Punct(c)
    }
}

/// The text of a doc comment: leading `*` and blank lines removed, `@param` tags dropped.
fn doc_text(body: &[char]) -> String {
    let text: String = body.iter().collect();
    let mut lines = Vec::new();
    for line in text.lines() {
        let l = line.trim().trim_start_matches('*').trim();
        if l.starts_with('@') {
            break;
        }
        lines.push(l);
    }
    lines
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn lex(text: &str) -> Vec<Token> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let (mut i, mut line, mut line_start) = (0usize, 0u32, 0usize);
    let n = chars.len();
    let mut doc: Option<String> = None;
    // Advance over `chars[i]`, counting lines.
    macro_rules! bump {
        () => {{
            if chars[i] == '\n' {
                line += 1;
                line_start = i + 1;
            }
            i += 1;
        }};
    }
    while i < n {
        let c = chars[i];
        let col = (i - line_start) as u32;
        if c.is_whitespace() {
            bump!();
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'/') {
            while i < n && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'*') {
            let is_doc = chars.get(i + 2) == Some(&'*') && chars.get(i + 3) != Some(&'/');
            i += 2;
            let start = i;
            while i < n && !(chars[i] == '*' && chars.get(i + 1) == Some(&'/')) {
                bump!();
            }
            if is_doc {
                doc = Some(doc_text(&chars[start + 1..i.min(n)]));
            }
            i = (i + 2).min(n);
            continue;
        }
        if c == '"' {
            let (start_line, start_col) = (line, col);
            let mut s = String::new();
            if chars.get(i + 1) == Some(&'"') && chars.get(i + 2) == Some(&'"') {
                i += 3;
                while i < n {
                    if chars[i] == '"'
                        && chars.get(i + 1) == Some(&'"')
                        && chars.get(i + 2) == Some(&'"')
                    {
                        i += 3;
                        while i < n && chars[i] == '"' {
                            i += 1;
                        }
                        break;
                    }
                    s.push(chars[i]);
                    bump!();
                }
            } else {
                i += 1;
                while i < n && chars[i] != '"' && chars[i] != '\n' {
                    if chars[i] == '\\' && i + 1 < n {
                        s.push(match chars[i + 1] {
                            'n' => '\n',
                            't' => '\t',
                            other => other,
                        });
                        i += 2;
                        continue;
                    }
                    if chars[i] == '$' && chars.get(i + 1) == Some(&'{') {
                        // Kotlin template: keep it as written, skipping nested strings.
                        let mut depth = 0;
                        while i < n && chars[i] != '\n' {
                            s.push(chars[i]);
                            if chars[i] == '{' {
                                depth += 1;
                            } else if chars[i] == '}' {
                                depth -= 1;
                                if depth == 0 {
                                    i += 1;
                                    break;
                                }
                            }
                            i += 1;
                        }
                        continue;
                    }
                    s.push(chars[i]);
                    i += 1;
                }
                if i < n && chars[i] == '"' {
                    i += 1;
                }
            }
            out.push(Token {
                tok: Tok::Str(s),
                line: start_line,
                col: start_col,
                doc: doc.take(),
            });
            continue;
        }
        if c == '\'' {
            let lit_start = i;
            i += 1;
            while i < n && chars[i] != '\'' && chars[i] != '\n' {
                if chars[i] == '\\' {
                    i += 1;
                }
                i += 1;
            }
            i = (i + 1).min(n);
            out.push(Token {
                tok: Tok::Lit(chars[lit_start..i].iter().collect()),
                line,
                col,
                doc: doc.take(),
            });
            continue;
        }
        if c == '`' {
            let mut s = String::new();
            i += 1;
            while i < n && chars[i] != '`' && chars[i] != '\n' {
                s.push(chars[i]);
                i += 1;
            }
            i = (i + 1).min(n);
            out.push(Token {
                tok: Tok::Ident(s),
                line,
                col,
                doc: doc.take(),
            });
            continue;
        }
        if c.is_ascii_digit() {
            let lit_start = i;
            while i < n
                && (chars[i].is_alphanumeric()
                    || chars[i] == '_'
                    || (chars[i] == '.' && chars.get(i + 1).is_some_and(|d| d.is_ascii_digit())))
            {
                i += 1;
            }
            out.push(Token {
                tok: Tok::Lit(chars[lit_start..i].iter().collect()),
                line,
                col,
                doc: doc.take(),
            });
            continue;
        }
        if c.is_alphabetic() || c == '_' || c == '$' {
            let mut s = String::new();
            while i < n && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '$') {
                s.push(chars[i]);
                i += 1;
            }
            out.push(Token {
                tok: Tok::Ident(s),
                line,
                col,
                doc: doc.take(),
            });
            continue;
        }
        out.push(Token {
            tok: Tok::Punct(c),
            line,
            col,
            doc: if c == '@' { doc.take() } else { None },
        });
        i += 1;
    }
    out
}

// ---- Parser ---------------------------------------------------------------------------------

const MODIFIERS: &[&str] = &[
    "public",
    "protected",
    "private",
    "internal",
    "static",
    "final",
    "abstract",
    "synchronized",
    "native",
    "transient",
    "volatile",
    "strictfp",
    "default",
    "sealed",
    "non-sealed",
    "open",
    "override",
    "data",
    "inline",
    "value",
    "lateinit",
    "const",
    "suspend",
    "external",
    "operator",
    "infix",
    "tailrec",
    "inner",
    "expect",
    "actual",
    "vararg",
    "noinline",
    "crossinline",
    "reified",
];

fn is_modifier(s: &str) -> bool {
    MODIFIERS.contains(&s)
}

struct Parser<'a> {
    toks: &'a [Token],
    i: usize,
    lang: Lang,
}

#[derive(Default)]
struct Header {
    annotations: Vec<Annotation>,
    modifiers: Vec<String>,
    doc: Option<String>,
}

impl<'a> Parser<'a> {
    fn new(toks: &'a [Token], lang: Lang) -> Self {
        Self { toks, i: 0, lang }
    }

    fn peek(&self, n: usize) -> Option<&'a Token> {
        self.toks.get(self.i + n)
    }

    fn ident_at(&self, n: usize) -> Option<&'a str> {
        self.peek(n).and_then(Token::ident)
    }

    fn punct_at(&self, n: usize, c: char) -> bool {
        self.peek(n).is_some_and(|t| t.is(c))
    }

    fn done(&self) -> bool {
        self.i >= self.toks.len()
    }

    /// At an opening bracket: the index after its match (or the end).
    fn skip_balanced(&mut self) {
        let mut depth = 0i32;
        while let Some(t) = self.peek(0) {
            self.i += 1;
            match t.tok {
                Tok::Punct('(' | '{' | '[') => depth += 1,
                Tok::Punct(')' | '}' | ']') => {
                    depth -= 1;
                    if depth <= 0 {
                        return;
                    }
                }
                _ => {}
            }
        }
    }

    /// At `(`: the tokens inside the parentheses; moves past the `)`.
    fn group(&mut self) -> &'a [Token] {
        let start = self.i + 1;
        self.skip_balanced();
        let end = self.i.saturating_sub(1).max(start);
        &self.toks[start.min(self.toks.len())..end.min(self.toks.len())]
    }

    /// At `<`: skip a type parameter / argument list.
    fn skip_angle(&mut self) {
        let mut depth = 0i32;
        while let Some(t) = self.peek(0) {
            match t.tok {
                Tok::Punct('<') => depth += 1,
                Tok::Punct('>') => {
                    depth -= 1;
                    if depth <= 0 {
                        self.i += 1;
                        return;
                    }
                }
                Tok::Punct('{' | ';' | '=') => return,
                Tok::Punct('(') => {
                    self.skip_balanced();
                    continue;
                }
                _ => {}
            }
            self.i += 1;
        }
    }

    /// At `@`: an annotation (`None` for `@interface`, which stays unconsumed).
    fn annotation(&mut self) -> Option<Annotation> {
        if self.ident_at(1) == Some("interface") {
            return None;
        }
        let line = self.peek(0)?.line;
        self.i += 1;
        let mut name = self.ident_at(0)?.to_string();
        self.i += 1;
        // Kotlin use-site targets: `@field:Value("…")`, `@param:Qualifier`.
        if self.punct_at(0, ':') && self.ident_at(1).is_some() && !self.punct_at(1, ':') {
            name = self.ident_at(1).unwrap_or_default().to_string();
            self.i += 2;
        }
        while self.punct_at(0, '.') && self.ident_at(1).is_some() {
            name.push('.');
            name.push_str(self.ident_at(1).unwrap_or_default());
            self.i += 2;
        }
        let mut args = Vec::new();
        if self.punct_at(0, '(') {
            let inner = self.group();
            args = annotation_args(inner);
        }
        Some(Annotation { name, line, args })
    }

    /// Annotations and modifiers before a declaration.
    fn header(&mut self) -> Header {
        let mut h = Header {
            doc: self.peek(0).and_then(|t| t.doc.clone()),
            ..Header::default()
        };
        loop {
            if self.punct_at(0, '@') {
                match self.annotation() {
                    Some(a) => h.annotations.push(a),
                    None => break,
                }
                continue;
            }
            match self.ident_at(0) {
                Some("non") if self.punct_at(1, '-') && self.ident_at(2) == Some("sealed") => {
                    h.modifiers.push("non-sealed".into());
                    self.i += 3;
                }
                // Kotlin `enum class`, `annotation class`, `fun interface`, `companion object`.
                Some(m @ ("enum" | "annotation" | "fun" | "companion"))
                    if self.lang == Lang::Kotlin
                        && matches!(self.ident_at(1), Some("class" | "interface" | "object")) =>
                {
                    h.modifiers.push(m.to_string());
                    self.i += 1;
                }
                Some(m) if is_modifier(m) => {
                    // `value` / `data` / `open` are also ordinary names in Kotlin.
                    if self.lang == Lang::Kotlin
                        && matches!(m, "value" | "data" | "open" | "inline" | "default")
                        && !self
                            .ident_at(1)
                            .is_some_and(|n| is_modifier(n) || is_class_keyword(n) || n == "fun")
                    {
                        break;
                    }
                    if self.lang == Lang::Java && m == "default" && self.punct_at(1, ':') {
                        break;
                    }
                    h.modifiers.push(m.to_string());
                    self.i += 1;
                }
                _ => break,
            }
        }
        h
    }

    /// A type at the cursor, as written (`Map<String, List<Foo>>`, `String?`, `int[]`,
    /// `(A) -> B`).
    fn type_text(&mut self) -> String {
        let mut out = String::new();
        if self.punct_at(0, '(') {
            let start = self.i;
            self.skip_balanced();
            out.push_str(&render(&self.toks[start..self.i]));
            if self.punct_at(0, '-') && self.punct_at(1, '>') {
                self.i += 2;
                out.push_str(" -> ");
                out.push_str(&self.type_text());
            }
            return out;
        }
        let Some(first) = self.ident_at(0) else {
            return out;
        };
        out.push_str(first);
        self.i += 1;
        loop {
            if self.punct_at(0, '.') && self.ident_at(1).is_some() {
                out.push('.');
                out.push_str(self.ident_at(1).unwrap_or_default());
                self.i += 2;
            } else if self.punct_at(0, '<') {
                let start = self.i;
                self.skip_angle();
                out.push_str(&render(&self.toks[start..self.i]));
            } else if self.punct_at(0, '[') && self.punct_at(1, ']') {
                out.push_str("[]");
                self.i += 2;
            } else if self.punct_at(0, '?') && !self.punct_at(1, '.') && !self.punct_at(1, ':') {
                out.push('?');
                self.i += 1;
            } else if self.punct_at(0, '.') && self.punct_at(1, '.') && self.punct_at(2, '.') {
                out.push_str("...");
                self.i += 3;
            } else {
                break;
            }
        }
        out
    }

    fn file(mut self, path: &str) -> SourceFile {
        let mut file = SourceFile {
            path: path.to_string(),
            lang: self.lang,
            package: None,
            imports: Vec::new(),
            classes: Vec::new(),
            constants: BTreeMap::new(),
        };
        while !self.done() {
            match self.ident_at(0) {
                Some("package") => {
                    self.i += 1;
                    file.package = Some(self.dotted());
                    continue;
                }
                Some("import") => {
                    self.i += 1;
                    let mut name = self.dotted();
                    if self.punct_at(0, '.') && self.punct_at(1, '*') {
                        name.push_str(".*");
                        self.i += 2;
                    }
                    if name.starts_with("static.") {
                        name = name.trim_start_matches("static.").to_string();
                    }
                    if !name.is_empty() {
                        file.imports.push(name);
                    }
                    continue;
                }
                _ => {}
            }
            // `@file:JvmName("…")` and other file annotations.
            if self.punct_at(0, '@') && self.ident_at(1) == Some("file") && self.punct_at(2, ':') {
                self.annotation();
                continue;
            }
            let start = self.i;
            let header = self.header();
            if self.member(&mut file, None, header) {
                continue;
            }
            if self.i == start {
                self.advance_unknown();
            }
        }
        file
    }

    /// `a.b.c` (also `static a.b`), stopping before `.*`.
    fn dotted(&mut self) -> String {
        let mut out = String::new();
        if self.ident_at(0) == Some("static") {
            out.push_str("static.");
            self.i += 1;
        }
        while let Some(id) = self.ident_at(0) {
            out.push_str(id);
            self.i += 1;
            if self.punct_at(0, '.') && self.ident_at(1).is_some() {
                out.push('.');
                self.i += 1;
            } else {
                break;
            }
        }
        if self.punct_at(0, ';') {
            self.i += 1;
        }
        out
    }

    fn advance_unknown(&mut self) {
        match self.peek(0).map(|t| &t.tok) {
            Some(Tok::Punct('(' | '{' | '[')) => self.skip_balanced(),
            Some(_) => self.i += 1,
            None => {}
        }
    }

    /// A declaration after its header. Returns whether something was consumed.
    fn member(&mut self, file: &mut SourceFile, owner: Option<usize>, header: Header) -> bool {
        let Some(word) = self.ident_at(0) else {
            if self.punct_at(0, '@') && self.ident_at(1) == Some("interface") {
                self.i += 2;
                self.class(file, owner, header, ClassKind::Annotation);
                return true;
            }
            return false;
        };
        if let Some(kind) = class_keyword(word, self.lang, &header) {
            // Kotlin `object : Foo {}` in an expression position is not a declaration; a
            // named object or a companion is.
            self.i += 1;
            self.class(file, owner, header, kind);
            return true;
        }
        match (self.lang, word) {
            (Lang::Kotlin, "fun") => {
                self.i += 1;
                self.kotlin_fun(file, owner, header);
                true
            }
            (Lang::Kotlin, "val" | "var") => {
                self.i += 1;
                self.kotlin_property(file, owner, header);
                true
            }
            (Lang::Kotlin, "constructor") if self.punct_at(1, '(') => {
                self.i += 1;
                let (line, column) = self.peek(0).map(|t| (t.line, t.col)).unwrap_or_default();
                let params = self.params();
                if self.punct_at(0, ':') {
                    self.i += 1;
                    self.i += 1; // this / super
                    if self.punct_at(0, '(') {
                        self.skip_balanced();
                    }
                }
                if self.punct_at(0, '{') {
                    self.skip_balanced();
                }
                if let Some(o) = owner {
                    let name = file.classes[o].name.clone();
                    file.classes[o].constructors.push(Callable {
                        name,
                        line,
                        column,
                        annotations: header.annotations,
                        modifiers: header.modifiers,
                        return_type: String::new(),
                        params,
                    });
                }
                true
            }
            (Lang::Kotlin, "init") if self.punct_at(1, '{') => {
                self.i += 1;
                self.skip_balanced();
                true
            }
            (Lang::Java, _) if owner.is_some() => self.java_member(file, owner, header),
            _ => false,
        }
    }

    fn class(
        &mut self,
        file: &mut SourceFile,
        owner: Option<usize>,
        header: Header,
        kind: ClassKind,
    ) {
        let (name, line, column) = match self.peek(0) {
            Some(t) if t.ident().is_some() && !self.punct_at(0, '{') => {
                self.i += 1;
                (t.ident().unwrap_or_default().to_string(), t.line, t.col)
            }
            Some(t) => ("Companion".to_string(), t.line, t.col),
            None => return,
        };
        let qualified = match owner {
            Some(o) => format!("{}.{name}", file.classes[o].qualified),
            None => match &file.package {
                Some(p) if !p.is_empty() => format!("{p}.{name}"),
                _ => name.clone(),
            },
        };
        let index = file.classes.len();
        file.classes.push(ClassInfo {
            name: name.clone(),
            qualified,
            kind,
            line,
            column,
            annotations: header.annotations,
            modifiers: header.modifiers,
            supertypes: Vec::new(),
            constructors: Vec::new(),
            methods: Vec::new(),
            fields: Vec::new(),
            constants: Vec::new(),
            parent: owner,
        });
        if self.punct_at(0, '<') {
            self.skip_angle();
        }
        // Kotlin primary constructor (`private constructor(…)`, `@Inject constructor(…)`)
        // and Java record headers.
        let ctor_header = if self.lang == Lang::Kotlin {
            let before = self.i;
            let h = self.header();
            if self.ident_at(0) == Some("constructor") {
                self.i += 1;
                h
            } else {
                // Annotations of the next declaration, not of a constructor.
                self.i = before;
                Header::default()
            }
        } else {
            Header::default()
        };
        if self.punct_at(0, '(') {
            let (pline, pcol) = self
                .peek(0)
                .map(|t| (t.line, t.col))
                .unwrap_or((line, column));
            let params = self.params();
            for p in &params {
                if p.property || kind == ClassKind::Record {
                    file.classes[index].fields.push(Field {
                        name: p.name.clone(),
                        type_name: p.type_name.clone(),
                        line: p.line,
                        column: p.column,
                        annotations: p.annotations.clone(),
                        modifiers: Vec::new(),
                        initialized: p.default.is_some(),
                        doc: p.doc.clone(),
                        default: p.default.clone(),
                    });
                }
            }
            file.classes[index].constructors.push(Callable {
                name: name.clone(),
                line: pline,
                column: pcol,
                annotations: ctor_header.annotations,
                modifiers: ctor_header.modifiers,
                return_type: String::new(),
                params,
            });
        }
        // Supertypes.
        let mut supertypes = Vec::new();
        loop {
            match (self.lang, self.ident_at(0)) {
                (Lang::Java, Some("extends" | "implements" | "permits")) => {
                    let permits = self.ident_at(0) == Some("permits");
                    self.i += 1;
                    loop {
                        let t = self.type_text();
                        if t.is_empty() {
                            break;
                        }
                        if !permits {
                            supertypes.push(TypeRef::parse(&t));
                        }
                        if self.punct_at(0, ',') {
                            self.i += 1;
                        } else {
                            break;
                        }
                    }
                }
                (Lang::Kotlin, _) if self.punct_at(0, ':') => {
                    self.i += 1;
                    loop {
                        while self.punct_at(0, '@') {
                            self.annotation();
                        }
                        let t = self.type_text();
                        if t.is_empty() {
                            break;
                        }
                        supertypes.push(TypeRef::parse(&t));
                        if self.punct_at(0, '(') {
                            self.skip_balanced();
                        }
                        if self.ident_at(0) == Some("by") {
                            self.i += 1;
                            self.kotlin_skip_expr();
                        }
                        if self.punct_at(0, ',') {
                            self.i += 1;
                        } else {
                            break;
                        }
                    }
                }
                (Lang::Kotlin, Some("where")) => {
                    self.i += 1;
                    while !self.done() && !self.punct_at(0, '{') && !self.punct_at(0, '}') {
                        if self.ident_at(0).is_some()
                            && self.peek(0).map(|t| t.line) != self.peek(1).map(|t| t.line)
                            && !self.punct_at(1, ',')
                        {
                            self.i += 1;
                            break;
                        }
                        self.i += 1;
                    }
                }
                _ => break,
            }
        }
        file.classes[index].supertypes = supertypes;
        if !self.punct_at(0, '{') {
            if self.punct_at(0, ';') {
                self.i += 1;
            }
            return;
        }
        self.i += 1;
        if kind == ClassKind::Enum {
            self.enum_constants(file, index);
        }
        self.body(file, index);
    }

    /// Enum constants up to the `;` (or the end of the body).
    fn enum_constants(&mut self, file: &mut SourceFile, index: usize) {
        let mut expect = true;
        while let Some(t) = self.peek(0) {
            match &t.tok {
                Tok::Punct(';') => {
                    self.i += 1;
                    return;
                }
                Tok::Punct('}') => return,
                Tok::Punct('@') => {
                    self.annotation();
                    continue;
                }
                Tok::Punct(',') => expect = true,
                Tok::Punct('(' | '{') => {
                    self.skip_balanced();
                    continue;
                }
                Tok::Ident(name) if expect => {
                    // A member declaration (Kotlin enum without `;`) ends the constants.
                    if is_modifier(name)
                        || matches!(name.as_str(), "fun" | "val" | "var" | "init" | "companion")
                    {
                        return;
                    }
                    file.classes[index].constants.push(name.clone());
                    expect = false;
                }
                _ => {}
            }
            self.i += 1;
        }
    }

    /// Members up to the closing `}` (consumed).
    fn body(&mut self, file: &mut SourceFile, index: usize) {
        while !self.done() {
            if self.punct_at(0, '}') {
                self.i += 1;
                return;
            }
            if self.punct_at(0, ';') || self.punct_at(0, ',') {
                self.i += 1;
                continue;
            }
            if self.punct_at(0, '{') {
                // Java initializer block.
                self.skip_balanced();
                continue;
            }
            let start = self.i;
            let header = self.header();
            if self.lang == Lang::Java && self.punct_at(0, '{') {
                // `static { … }`
                self.skip_balanced();
                continue;
            }
            if self.member(file, Some(index), header) {
                continue;
            }
            if self.i == start {
                self.advance_unknown();
            }
        }
    }

    /// At `(`: the parameters of a constructor or method.
    fn params(&mut self) -> Vec<Param> {
        let inner = self.group();
        let mut out = Vec::new();
        for slice in split_tokens(inner) {
            let mut p = Parser::new(slice, self.lang);
            let header = p.header();
            if p.done() {
                continue;
            }
            let param = match self.lang {
                Lang::Kotlin => {
                    let mut property = false;
                    if matches!(p.ident_at(0), Some("val" | "var")) {
                        property = true;
                        p.i += 1;
                    }
                    let Some(name_tok) = p.peek(0).filter(|t| t.ident().is_some()) else {
                        continue;
                    };
                    p.i += 1;
                    let type_name = if p.punct_at(0, ':') {
                        p.i += 1;
                        p.type_text()
                    } else {
                        String::new()
                    };
                    let default = if p.punct_at(0, '=') {
                        p.i += 1;
                        p.single_literal()
                    } else {
                        None
                    };
                    Param {
                        name: name_tok.ident().unwrap_or_default().to_string(),
                        type_name,
                        annotations: header.annotations,
                        line: name_tok.line,
                        column: name_tok.col,
                        property,
                        doc: header.doc,
                        default,
                    }
                }
                Lang::Java => {
                    let type_name = p.type_text();
                    // Receiver parameters (`Foo this`) are not parameters.
                    let Some(name_tok) =
                        p.peek(0).filter(|t| t.ident().is_some_and(|n| n != "this"))
                    else {
                        continue;
                    };
                    Param {
                        name: name_tok.ident().unwrap_or_default().to_string(),
                        type_name,
                        annotations: header.annotations,
                        line: name_tok.line,
                        column: name_tok.col,
                        property: false,
                        doc: header.doc,
                        default: None,
                    }
                }
            };
            out.push(param);
        }
        out
    }

    fn java_member(&mut self, file: &mut SourceFile, owner: Option<usize>, header: Header) -> bool {
        let Some(o) = owner else {
            return false;
        };
        if self.punct_at(0, '<') {
            self.skip_angle();
        }
        let class_name = file.classes[o].name.clone();
        // Constructor.
        if self.ident_at(0) == Some(class_name.as_str()) && self.punct_at(1, '(') {
            let t = &self.toks[self.i];
            let (line, column) = (t.line, t.col);
            self.i += 1;
            let params = self.params();
            self.skip_to_body_end();
            file.classes[o].constructors.push(Callable {
                name: class_name,
                line,
                column,
                annotations: header.annotations,
                modifiers: header.modifiers,
                return_type: String::new(),
                params,
            });
            return true;
        }
        // Compact record constructor (`public Foo { … }`).
        if self.ident_at(0) == Some(class_name.as_str()) && self.punct_at(1, '{') {
            self.i += 1;
            self.skip_balanced();
            return true;
        }
        let start = self.i;
        let type_name = self.type_text();
        if type_name.is_empty() {
            self.i = start;
            return false;
        }
        let Some(name_tok) = self.peek(0).filter(|t| t.ident().is_some()) else {
            return self.i != start;
        };
        let name = name_tok.ident().unwrap_or_default().to_string();
        self.i += 1;
        if self.punct_at(0, '(') {
            let params = self.params();
            self.skip_to_body_end();
            file.classes[o].methods.push(Callable {
                name,
                line: name_tok.line,
                column: name_tok.col,
                annotations: header.annotations,
                modifiers: header.modifiers,
                return_type: type_name,
                params,
            });
            return true;
        }
        // Fields: `Type a = 1, b;`
        let mut current = Some((name, name_tok.line, name_tok.col));
        while let Some((fname, line, column)) = current.take() {
            let mut initialized = false;
            let mut literal: Option<String> = None;
            let mut default = None;
            if self.punct_at(0, '=') {
                initialized = true;
                self.i += 1;
                if let (Some(Tok::Str(s)), true) = (
                    self.peek(0).map(|t| &t.tok),
                    self.punct_at(1, ';') || self.punct_at(1, ','),
                ) {
                    literal = Some(s.clone());
                }
                default = self.single_literal();
                self.skip_initializer();
            }
            if let Some(s) = literal {
                if header.modifiers.iter().any(|m| m == "final") {
                    file.constants.insert(fname.clone(), s);
                }
            }
            file.classes[o].fields.push(Field {
                name: fname,
                type_name: type_name.clone(),
                line,
                column,
                annotations: header.annotations.clone(),
                modifiers: header.modifiers.clone(),
                initialized,
                doc: header.doc.clone(),
                default,
            });
            if self.punct_at(0, ',') {
                self.i += 1;
                if let Some(t) = self.peek(0).filter(|t| t.ident().is_some()) {
                    self.i += 1;
                    current = Some((t.ident().unwrap_or_default().to_string(), t.line, t.col));
                }
            }
        }
        if self.punct_at(0, ';') {
            self.i += 1;
        }
        true
    }

    /// After a method's parameters: `throws …`, `default …;`, then a body or `;`.
    fn skip_to_body_end(&mut self) {
        while let Some(t) = self.peek(0) {
            match t.tok {
                Tok::Punct('{') => {
                    self.skip_balanced();
                    return;
                }
                Tok::Punct(';') => {
                    self.i += 1;
                    return;
                }
                Tok::Punct('}') => return,
                Tok::Punct('(') => self.skip_balanced(),
                _ => self.i += 1,
            }
        }
    }

    /// A Java field initializer up to `,` / `;` at depth 0.
    fn skip_initializer(&mut self) {
        while let Some(t) = self.peek(0) {
            match t.tok {
                Tok::Punct('(' | '{' | '[') => self.skip_balanced(),
                Tok::Punct(',' | ';' | '}') => return,
                _ => self.i += 1,
            }
        }
    }

    /// A Kotlin expression (initializer, expression body, delegate): up to the end of the
    /// statement, which in Kotlin is the end of the line unless the expression continues.
    fn kotlin_skip_expr(&mut self) {
        let mut last_line = self.peek(0).map(|t| t.line).unwrap_or_default();
        let mut last_continues = true;
        while let Some(t) = self.peek(0) {
            let continues_here = matches!(
                t.tok,
                Tok::Punct(
                    '.' | '?' | ':' | '+' | '-' | '*' | '/' | '&' | '|' | '=' | '<' | '>' | '%'
                )
            ) || matches!(t.ident(), Some("as" | "else" | "is" | "in"));
            if t.line > last_line && !last_continues && !continues_here {
                return;
            }
            match t.tok {
                Tok::Punct(';' | '}' | ')' | ']') => return,
                Tok::Punct(',') if !last_continues => return,
                Tok::Punct('(' | '{' | '[') => {
                    self.skip_balanced();
                    last_line = self.toks[self.i - 1].line;
                    last_continues = false;
                    continue;
                }
                _ => {}
            }
            last_continues = matches!(
                t.tok,
                Tok::Punct(
                    '.' | '+'
                        | '-'
                        | '*'
                        | '/'
                        | '&'
                        | '|'
                        | '='
                        | '<'
                        | '>'
                        | ','
                        | '%'
                        | ':'
                        | '?'
                )
            ) || matches!(t.ident(), Some("as" | "else" | "is" | "in" | "return"));
            last_line = t.line;
            self.i += 1;
        }
    }

    fn kotlin_fun(&mut self, file: &mut SourceFile, owner: Option<usize>, header: Header) {
        if self.punct_at(0, '<') {
            self.skip_angle();
        }
        // `fun Receiver.name(…)`: the name is the identifier before `(`.
        let mut name_tok = None;
        while let Some(t) = self.peek(0) {
            if t.ident().is_some() {
                name_tok = Some(t);
                self.i += 1;
                if self.punct_at(0, '<') {
                    self.skip_angle();
                }
            } else if t.is('.') || t.is('?') {
                self.i += 1;
            } else {
                break;
            }
        }
        let Some(name_tok) = name_tok else {
            return;
        };
        if !self.punct_at(0, '(') {
            return;
        }
        let params = self.params();
        let mut return_type = String::new();
        if self.punct_at(0, ':') {
            self.i += 1;
            return_type = self.type_text();
        }
        if self.ident_at(0) == Some("where") {
            while !self.done() && !self.punct_at(0, '{') && !self.punct_at(0, '=') {
                self.i += 1;
            }
        }
        if self.punct_at(0, '{') {
            self.skip_balanced();
        } else if self.punct_at(0, '=') {
            self.i += 1;
            self.kotlin_skip_expr();
        }
        if let Some(o) = owner {
            file.classes[o].methods.push(Callable {
                name: name_tok.ident().unwrap_or_default().to_string(),
                line: name_tok.line,
                column: name_tok.col,
                annotations: header.annotations,
                modifiers: header.modifiers,
                return_type,
                params,
            });
        }
    }

    fn kotlin_property(&mut self, file: &mut SourceFile, owner: Option<usize>, header: Header) {
        if self.punct_at(0, '<') {
            self.skip_angle();
        }
        let mut name_tok = None;
        while let Some(t) = self.peek(0) {
            if t.ident().is_some() {
                name_tok = Some(t);
                self.i += 1;
            } else if t.is('.') {
                self.i += 1;
            } else {
                break;
            }
            if !self.punct_at(0, '.') {
                break;
            }
        }
        let Some(name_tok) = name_tok else {
            return;
        };
        let mut type_name = String::new();
        if self.punct_at(0, ':') {
            self.i += 1;
            type_name = self.type_text();
        }
        let mut initialized = false;
        let mut literal = None;
        let mut default = None;
        if self.punct_at(0, '=') || self.ident_at(0) == Some("by") {
            initialized = true;
            self.i += 1;
            default = self.single_literal();
            if let Some(Tok::Str(s)) = self.peek(0).map(|t| &t.tok) {
                let next_line = self.peek(1).map(|t| t.line);
                if next_line != Some(self.toks[self.i].line) || self.punct_at(1, ';') {
                    literal = Some(s.clone());
                }
            }
            self.kotlin_skip_expr();
        }
        let name = name_tok.ident().unwrap_or_default().to_string();
        if let Some(s) = literal {
            if header.modifiers.iter().any(|m| m == "const") {
                file.constants.insert(name.clone(), s);
            }
        }
        if let Some(o) = owner {
            file.classes[o].fields.push(Field {
                name,
                type_name,
                line: name_tok.line,
                column: name_tok.col,
                annotations: header.annotations,
                modifiers: header.modifiers,
                initialized,
                doc: header.doc,
                default,
            });
        }
    }

    /// A literal that is the whole initializer (`10`, `"EUR"`, `true`, `-1`).
    fn single_literal(&self) -> Option<String> {
        let (neg, t) = match self.peek(0) {
            Some(t) if t.is('-') => (true, self.peek(1)?),
            Some(t) => (false, t),
            None => return None,
        };
        let next = self.peek(if neg { 2 } else { 1 });
        let ends = match next {
            None => true,
            Some(n) => matches!(n.tok, Tok::Punct(';' | ',' | ')' | '}')) || n.line > t.line,
        };
        if !ends {
            return None;
        }
        let text = match &t.tok {
            Tok::Str(s) if !neg => s.clone(),
            Tok::Lit(l) => l
                .trim_end_matches(['L', 'l', 'f', 'F', 'd', 'D'])
                .replace('_', ""),
            Tok::Ident(i) if !neg && matches!(i.as_str(), "true" | "false") => i.clone(),
            _ => return None,
        };
        Some(if neg { format!("-{text}") } else { text })
    }
}

fn is_class_keyword(s: &str) -> bool {
    matches!(s, "class" | "interface" | "enum" | "record" | "object")
}

fn class_keyword(word: &str, lang: Lang, header: &Header) -> Option<ClassKind> {
    let has = |m: &str| header.modifiers.iter().any(|x| x == m);
    match (lang, word) {
        (_, "interface") => Some(ClassKind::Interface),
        (Lang::Java, "class") => Some(ClassKind::Class),
        (Lang::Java, "enum") => Some(ClassKind::Enum),
        (Lang::Java, "record") => Some(ClassKind::Record),
        (Lang::Kotlin, "class") if has("enum") => Some(ClassKind::Enum),
        (Lang::Kotlin, "class") if has("annotation") => Some(ClassKind::Annotation),
        (Lang::Kotlin, "class") => Some(ClassKind::Class),
        (Lang::Kotlin, "object") => Some(ClassKind::Object),
        _ => None,
    }
}

/// Split tokens at top-level commas.
fn split_tokens(toks: &[Token]) -> Vec<&[Token]> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    for (i, t) in toks.iter().enumerate() {
        match t.tok {
            Tok::Punct('(' | '{' | '[' | '<') => depth += 1,
            Tok::Punct(')' | '}' | ']' | '>') => depth -= 1,
            Tok::Punct(',') if depth == 0 => {
                out.push(&toks[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    if start < toks.len() {
        out.push(&toks[start..]);
    }
    out
}

/// Split annotation arguments at top-level commas (`<` / `>` are operators here).
fn split_args(toks: &[Token]) -> Vec<&[Token]> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    for (i, t) in toks.iter().enumerate() {
        match t.tok {
            Tok::Punct('(' | '{' | '[') => depth += 1,
            Tok::Punct(')' | '}' | ']') => depth -= 1,
            Tok::Punct(',') if depth == 0 => {
                out.push(&toks[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    if start < toks.len() {
        out.push(&toks[start..]);
    }
    out
}

fn annotation_args(toks: &[Token]) -> Vec<AnnArg> {
    let mut out = Vec::new();
    for slice in split_args(toks) {
        if slice.is_empty() {
            continue;
        }
        let (name, rest) = match (slice.first().and_then(Token::ident), slice.get(1)) {
            (Some(n), Some(t)) if t.is('=') && !slice.get(2).is_some_and(|t| t.is('=')) => {
                (n.to_string(), &slice[2..])
            }
            _ => ("value".to_string(), slice),
        };
        out.push(AnnArg {
            name,
            values: values(rest),
        });
    }
    out
}

/// Flatten `{a, b}`, `[a, b]`, `arrayOf(a, b)` into values.
fn values(toks: &[Token]) -> Vec<Val> {
    let inner = match toks {
        [first, .., last] if (first.is('{') && last.is('}')) || (first.is('[') && last.is(']')) => {
            Some(&toks[1..toks.len() - 1])
        }
        [f, p, .., last]
            if matches!(f.ident(), Some("arrayOf" | "listOf" | "setOf"))
                && p.is('(')
                && last.is(')') =>
        {
            Some(&toks[2..toks.len() - 1])
        }
        _ => None,
    };
    if let Some(inner) = inner {
        return split_args(inner)
            .into_iter()
            .filter(|s| !s.is_empty())
            .flat_map(values)
            .collect();
    }
    if toks.is_empty() {
        return Vec::new();
    }
    // Literal concatenations stay strings.
    let all_strings = toks.iter().enumerate().all(|(i, t)| {
        if i % 2 == 0 {
            matches!(t.tok, Tok::Str(_))
        } else {
            t.is('+')
        }
    });
    if all_strings {
        let s = toks
            .iter()
            .filter_map(|t| match &t.tok {
                Tok::Str(s) => Some(s.as_str()),
                _ => None,
            })
            .collect::<String>();
        return vec![Val::Str(s)];
    }
    vec![Val::Expr(render(toks))]
}

/// Tokens as compact source text.
fn render(toks: &[Token]) -> String {
    let mut out = String::new();
    let mut prev_word = false;
    for t in toks {
        match &t.tok {
            Tok::Ident(s) => {
                if prev_word {
                    out.push(' ');
                }
                out.push_str(s);
                prev_word = true;
            }
            Tok::Str(s) => {
                if prev_word {
                    out.push(' ');
                }
                out.push('"');
                out.push_str(&s.replace('\\', "\\\\").replace('"', "\\\""));
                out.push('"');
                prev_word = true;
            }
            Tok::Lit(l) => {
                if prev_word {
                    out.push(' ');
                }
                out.push_str(l);
                prev_word = true;
            }
            Tok::Punct(c) => {
                if *c == '+' {
                    out.push_str(" + ");
                } else {
                    out.push(*c);
                    if *c == ',' {
                        out.push(' ');
                    }
                }
                prev_word = false;
            }
        }
    }
    out
}

/// Scan a Java or Kotlin file (the language follows the extension; Java otherwise).
pub fn parse(path: &str, text: &str) -> SourceFile {
    let lang = Lang::of_path(path).unwrap_or(Lang::Java);
    let toks = lex(text);
    Parser::new(&toks, lang).file(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_classes_members_and_annotation_arguments() {
        let src = r#"
package com.acme.orders;

import org.springframework.web.bind.annotation.*;
import static java.util.Objects.requireNonNull;

/** Orders. */
@RestController
@RequestMapping(path = {"/api", "/v1"}, produces = MediaType.APPLICATION_JSON_VALUE)
public class OrderController extends Base<Long> implements Api, Other<String> {
    static final String ITEMS = "/items";
    private final OrderService service;
    @Autowired private Clock clock = Clock.systemUTC(), other;

    public OrderController(OrderService service, @Qualifier("fast") Repo repo) {
        this.service = requireNonNull(service);
        String s = "}";
    }

    @GetMapping("/{id}")
    public ResponseEntity<Order> find(@PathVariable("id") Long id) { return null; }

    @RequestMapping(value = ITEMS + "/all", method = {RequestMethod.GET, RequestMethod.HEAD})
    List<Item> items() throws IOException;

    enum Status { NEW, PAID("p") { }, SHIPPED; int code; }

    record Line(String sku, int qty) {}
}
"#;
        let f = parse("OrderController.java", src);
        assert_eq!(f.package.as_deref(), Some("com.acme.orders"));
        assert!(f.imports_from("org.springframework.web.bind.annotation"));
        assert!(f
            .imports
            .contains(&"java.util.Objects.requireNonNull".to_string()));
        assert_eq!(f.constants.get("ITEMS").map(String::as_str), Some("/items"));
        let c = &f.classes[0];
        assert_eq!(c.qualified, "com.acme.orders.OrderController");
        assert_eq!(
            c.supertypes[0],
            TypeRef {
                name: "Base".into(),
                args: vec!["Long".into()]
            }
        );
        assert_eq!(c.supertypes.len(), 3);
        let rm = c.annotation("RequestMapping").unwrap();
        assert_eq!(
            rm.arg("path").unwrap(),
            &[Val::Str("/api".into()), Val::Str("/v1".into())]
        );
        assert_eq!(
            rm.arg("produces").unwrap()[0],
            Val::Expr("MediaType.APPLICATION_JSON_VALUE".into())
        );
        assert_eq!(
            c.fields.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
            ["ITEMS", "service", "clock", "other"]
        );
        assert!(!c.fields[1].initialized && c.fields[2].initialized);
        assert!(has(&c.fields[3].annotations, "Autowired"));
        let ctor = &c.constructors[0];
        assert_eq!(ctor.params.len(), 2);
        assert_eq!(ctor.params[1].type_name, "Repo");
        assert_eq!(
            find(&ctor.params[1].annotations, "Qualifier")
                .unwrap()
                .string(&["value"])
                .as_deref(),
            Some("fast")
        );
        let find_m = &c.methods[0];
        assert_eq!((find_m.name.as_str(), find_m.line), ("find", 20));
        assert_eq!(find_m.return_type, "ResponseEntity<Order>");
        let items = &c.methods[1];
        let ann = &items.annotations[0];
        assert_eq!(
            ann.arg("value").unwrap()[0]
                .resolve(&f.constants)
                .as_deref(),
            Some("/items/all")
        );
        assert_eq!(
            ann.arg("method")
                .unwrap()
                .iter()
                .map(Val::simple)
                .collect::<Vec<_>>(),
            ["GET", "HEAD"]
        );
        let status = f.classes.iter().find(|c| c.name == "Status").unwrap();
        assert_eq!(status.constants, ["NEW", "PAID", "SHIPPED"]);
        assert_eq!(status.fields[0].name, "code");
        let line = f.classes.iter().find(|c| c.name == "Line").unwrap();
        assert_eq!(line.kind, ClassKind::Record);
        assert_eq!(line.fields.len(), 2);
        assert_eq!(line.qualified, "com.acme.orders.OrderController.Line");
    }

    #[test]
    fn kotlin_primary_constructors_properties_and_functions() {
        let src = r#"
package com.acme.orders

import jakarta.persistence.Entity

const val BASE = "/api/orders"

@Service
class OrderService(
    private val repository: OrderRepository,
    @Qualifier("audit") val audit: AuditLog,
    clock: Clock = Clock.systemUTC(),
) : Base(), Api<Order> {
    @Autowired
    lateinit var events: ApplicationEventPublisher

    private val cache = mutableMapOf<Long, Order>()
        get() = field

    @GetMapping("$BASE/{id}")
    fun find(@PathVariable id: Long): Order? = repository
        .findById(id)
        .orElse(null)

    @Bean
    fun clock(): Clock {
        val x = "}"
        return Clock.systemUTC()
    }

    companion object {
        const val LIMIT = "50"
    }
}

interface OrderRepository : JpaRepository<Order, Long>

@ConfigurationProperties(prefix = "shop.orders")
data class OrderProperties(val limit: Int = 10, val currency: String)

enum class Status { NEW, PAID }
"#;
        let f = parse("OrderService.kt", src);
        assert_eq!(f.lang, Lang::Kotlin);
        assert_eq!(
            f.constants.get("BASE").map(String::as_str),
            Some("/api/orders")
        );
        assert_eq!(f.constants.get("LIMIT").map(String::as_str), Some("50"));
        let s = &f.classes[0];
        assert_eq!(s.name, "OrderService");
        assert_eq!(
            s.supertypes
                .iter()
                .map(|t| t.name.as_str())
                .collect::<Vec<_>>(),
            ["Base", "Api"]
        );
        let ctor = &s.constructors[0];
        assert_eq!(
            ctor.params
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>(),
            ["repository", "audit", "clock"]
        );
        assert_eq!(ctor.params[0].type_name, "OrderRepository");
        assert!(ctor.params[0].property && !ctor.params[2].property);
        assert_eq!(
            s.fields.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
            ["repository", "audit", "events", "cache"]
        );
        assert!(has(&s.fields[2].annotations, "Autowired"));
        assert_eq!(
            s.methods
                .iter()
                .map(|m| m.name.as_str())
                .collect::<Vec<_>>(),
            ["find", "clock"]
        );
        assert_eq!(s.methods[0].return_type, "Order?");
        assert_eq!(s.methods[1].return_type, "Clock");
        let get = &s.methods[0].annotations[0];
        assert_eq!(get.arg("value").unwrap()[0], Val::Str("$BASE/{id}".into()));
        let repo = f
            .classes
            .iter()
            .find(|c| c.name == "OrderRepository")
            .unwrap();
        assert_eq!(repo.kind, ClassKind::Interface);
        assert_eq!(repo.supertypes[0].args, ["Order", "Long"]);
        let props = f
            .classes
            .iter()
            .find(|c| c.name == "OrderProperties")
            .unwrap();
        assert_eq!(props.fields.len(), 2);
        assert_eq!(
            props
                .annotation("ConfigurationProperties")
                .unwrap()
                .string(&["prefix", "value"])
                .as_deref(),
            Some("shop.orders")
        );
        let status = f.classes.iter().find(|c| c.name == "Status").unwrap();
        assert_eq!(
            (status.kind, status.constants.clone()),
            (ClassKind::Enum, vec!["NEW".to_string(), "PAID".to_string()])
        );
        assert!(f.classes.iter().any(|c| c.name == "Companion"));
    }

    #[test]
    fn doc_comments_and_literal_defaults() {
        let f = parse(
            "P.java",
            "class P {\n  /**\n   * Maximum number of orders.\n   * @since 2\n   */\n  private int limit = 10_000;\n  /** Currency. */ String currency = \"EUR\";\n  Duration d = Duration.ofSeconds(1);\n}",
        );
        let fields = &f.classes[0].fields;
        assert_eq!(fields[0].doc.as_deref(), Some("Maximum number of orders."));
        assert_eq!(fields[0].default.as_deref(), Some("10000"));
        assert_eq!(
            (fields[1].doc.as_deref(), fields[1].default.as_deref()),
            (Some("Currency."), Some("EUR"))
        );
        assert_eq!(fields[2].default, None);
        let k = parse(
            "P.kt",
            "class P(\n  /** Retries. */\n  val retries: Int = 3,\n  val on: Boolean = true\n)",
        );
        let ps = &k.classes[0].fields;
        assert_eq!(
            (ps[0].doc.as_deref(), ps[0].default.as_deref()),
            (Some("Retries."), Some("3"))
        );
        assert_eq!(ps[1].default.as_deref(), Some("true"));
    }

    #[test]
    fn tolerates_broken_code() {
        let f = parse(
            "A.java",
            "@Service class A { @Autowired B b; void x( { if (",
        );
        assert_eq!(f.classes[0].name, "A");
        assert_eq!(f.classes[0].fields[0].type_name, "B");
        let k = parse("A.kt", "class A(val b: B { fun x(");
        assert_eq!(k.classes[0].name, "A");
        assert!(parse("x.kt", "\"unterminated").classes.is_empty());
        assert_eq!(
            TypeRef::parse("Map<String, List<Foo>>?").args,
            ["String", "List<Foo>"]
        );
        assert_eq!(TypeRef::parse("Foo[]").name, "Foo");
    }
}
