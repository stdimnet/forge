//! A tolerant scanner for C# and F# sources: namespaces, (nested) classes, xUnit / NUnit /
//! MSTest tests and `Main` entry points, with their lines, for the gutter's ▶ icons and
//! the test runner's filters. It does not parse C#: comments, strings (verbatim, raw,
//! interpolated) and character literals are skipped, braces are counted.

use serde::{Deserialize, Serialize};

use crate::msbuild::TestFramework;

/// A test method.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestMethod {
    pub name: String,
    /// 0-based line of the method name.
    pub line: u32,
    pub framework: TestFramework,
    /// `[Fact(Skip = …)]`, `[Ignore]`.
    #[serde(default)]
    pub skipped: bool,
    /// `[Theory]`, `[TestCase]`, `[DataRow]`: one test per data row.
    #[serde(default)]
    pub parameterized: bool,
}

/// A class with tests (or a test fixture).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestClass {
    pub name: String,
    /// `Namespace.Outer+Inner`, as test results name it.
    pub full_name: String,
    pub line: u32,
    pub tests: Vec<TestMethod>,
}

/// A `static … Main(…)` entry point.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryPoint {
    pub class_full_name: String,
    pub line: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceScan {
    pub classes: Vec<TestClass>,
    pub entry_points: Vec<EntryPoint>,
    /// Every class declared in the file (`Namespace.Outer+Inner`), for locating test
    /// results.
    pub declared: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Ident(String),
    Punct(char),
    /// A string or character literal (its text is not needed).
    Literal,
}

fn tokenize(src: &str) -> Vec<(Tok, u32)> {
    let chars: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    let mut line = 0u32;
    let n = chars.len();
    while i < n {
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
        if c == '/' && i + 1 < n && chars[i + 1] == '/' {
            while i < n && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if c == '/' && i + 1 < n && chars[i + 1] == '*' {
            i += 2;
            while i < n && !(chars[i] == '*' && i + 1 < n && chars[i + 1] == '/') {
                if chars[i] == '\n' {
                    line += 1;
                }
                i += 1;
            }
            i += 2;
            continue;
        }
        // Strings: prefixes $, @, $@, @$, and raw strings (three or more quotes).
        let mut j = i;
        let mut verbatim = false;
        while j < n && (chars[j] == '$' || chars[j] == '@') {
            verbatim |= chars[j] == '@';
            j += 1;
        }
        if j < n && chars[j] == '"' && (j > i || c == '"') {
            let start_line = line;
            let mut quotes = 0;
            while j + quotes < n && chars[j + quotes] == '"' {
                quotes += 1;
            }
            if quotes >= 3 {
                // Raw string literal: ends at the same number of quotes.
                let mut k = j + quotes;
                loop {
                    if k >= n {
                        break;
                    }
                    if chars[k] == '\n' {
                        line += 1;
                    }
                    if chars[k] == '"' {
                        let mut q = 0;
                        while k + q < n && chars[k + q] == '"' {
                            q += 1;
                        }
                        if q >= quotes {
                            k += q;
                            break;
                        }
                        k += q;
                        continue;
                    }
                    k += 1;
                }
                i = k;
            } else if quotes == 2 {
                // Empty string.
                i = j + 2;
            } else {
                let mut k = j + 1;
                while k < n {
                    let ch = chars[k];
                    if ch == '\n' {
                        line += 1;
                    }
                    if verbatim {
                        if ch == '"' {
                            if k + 1 < n && chars[k + 1] == '"' {
                                k += 2;
                                continue;
                            }
                            break;
                        }
                    } else {
                        if ch == '\\' {
                            k += 2;
                            continue;
                        }
                        if ch == '"' || ch == '\n' {
                            break;
                        }
                    }
                    k += 1;
                }
                i = k + 1;
            }
            out.push((Tok::Literal, start_line));
            continue;
        }
        if c == '\'' {
            // Character literal ('a', '\n', 'A'); an F# type parameter ('T) has no
            // closing quote nearby.
            let mut k = i + 1;
            if k < n && chars[k] == '\\' {
                k += 2;
            } else {
                k += 1;
            }
            while k < n && k < i + 10 && chars[k] != '\'' && chars[k] != '\n' {
                k += 1;
            }
            if k < n && chars[k] == '\'' {
                out.push((Tok::Literal, line));
                i = k + 1;
            } else {
                i += 1;
            }
            continue;
        }
        if c == '`' && i + 1 < n && chars[i + 1] == '`' {
            // F# ``identifier with spaces``.
            let mut k = i + 2;
            while k + 1 < n && !(chars[k] == '`' && chars[k + 1] == '`') && chars[k] != '\n' {
                k += 1;
            }
            let ident: String = chars[i + 2..k.min(n)].iter().collect();
            out.push((Tok::Ident(ident), line));
            i = (k + 2).min(n);
            continue;
        }
        if c.is_alphanumeric() || c == '_' {
            let mut k = i;
            while k < n && (chars[k].is_alphanumeric() || chars[k] == '_') {
                k += 1;
            }
            let ident: String = chars[i..k].iter().collect();
            out.push((Tok::Ident(ident), line));
            i = k;
            continue;
        }
        if c == '@' && i + 1 < n && (chars[i + 1].is_alphabetic() || chars[i + 1] == '_') {
            // @class (escaped keyword identifier).
            i += 1;
            continue;
        }
        out.push((Tok::Punct(c), line));
        i += 1;
    }
    out
}

/// Attribute names of one `[...]` list (without the `Attribute` suffix), and whether one
/// has a `Skip` argument.
fn attribute_names(tokens: &[(Tok, u32)]) -> (Vec<String>, bool) {
    let mut names = Vec::new();
    let mut skip = false;
    let mut depth = 0;
    let mut expecting = true;
    let mut current = String::new();
    for (t, _) in tokens {
        match t {
            Tok::Punct('(') => {
                depth += 1;
                if !current.is_empty() {
                    names.push(std::mem::take(&mut current));
                }
                expecting = false;
            }
            Tok::Punct(')') => depth -= 1,
            Tok::Punct(',') if depth == 0 => {
                if !current.is_empty() {
                    names.push(std::mem::take(&mut current));
                }
                expecting = true;
            }
            Tok::Punct(':') if depth == 0 => {
                // `[assembly: …]`, `[return: …]`.
                current.clear();
                expecting = true;
            }
            Tok::Punct('.') | Tok::Punct('<') | Tok::Punct('>') if depth == 0 => {}
            Tok::Ident(s) if depth == 0 && expecting => current = s.clone(),
            Tok::Ident(s) if depth > 0 && s == "Skip" => skip = true,
            _ => {}
        }
    }
    if !current.is_empty() {
        names.push(current);
    }
    let names = names
        .into_iter()
        .map(|n| n.strip_suffix("Attribute").map(str::to_string).unwrap_or(n))
        .collect();
    (names, skip)
}

/// The framework of a test attribute (`None`: not a test attribute). `hint` decides the
/// ambiguous `Theory` (xUnit and NUnit).
fn test_attribute(name: &str, hint: Option<TestFramework>) -> Option<(TestFramework, bool)> {
    Some(match name {
        "Fact" | "SkippableFact" => (TestFramework::Xunit, false),
        "Theory" | "SkippableTheory" if hint == Some(TestFramework::Nunit) => {
            (TestFramework::Nunit, true)
        }
        "Theory" | "SkippableTheory" => (TestFramework::Xunit, true),
        "Test" | "UnityTest" => (TestFramework::Nunit, false),
        "TestCase" | "TestCaseSource" | "Combinatorial" | "Pairwise" | "Sequential" => {
            (TestFramework::Nunit, true)
        }
        "TestMethod" => (TestFramework::Mstest, false),
        "DataTestMethod" => (TestFramework::Mstest, true),
        _ => return None,
    })
}

const TYPE_KEYWORDS: &[&str] = &["class", "struct", "record", "interface", "enum"];
const MODIFIERS_BEFORE_NAME: &[&str] = &["class", "struct"];

#[derive(Debug, Clone)]
enum Scope {
    Namespace(String),
    Type {
        full_name: String,
        line: u32,
        index: Option<usize>,
    },
    Other,
}

/// Scan a C# file. `hint` is the project's test framework (for `[Theory]`).
pub fn scan_csharp(src: &str, hint: Option<TestFramework>) -> SourceScan {
    let tokens = tokenize(src);
    let mut out = SourceScan::default();
    let mut scopes: Vec<Scope> = Vec::new();
    let mut file_namespace = String::new();
    // The current statement: its tokens since the last `;`, `{` or `}`.
    let mut stmt: Vec<(Tok, u32)> = Vec::new();
    let mut attrs: Vec<String> = Vec::new();
    let mut skip = false;
    let mut i = 0;
    let namespace_of = |scopes: &[Scope], file_ns: &str| -> String {
        let mut parts: Vec<String> = Vec::new();
        if !file_ns.is_empty() {
            parts.push(file_ns.to_string());
        }
        for s in scopes {
            if let Scope::Namespace(n) = s {
                parts.push(n.clone());
            }
        }
        parts.join(".")
    };
    let enclosing_type = |scopes: &[Scope]| -> Option<(String, u32, Option<usize>)> {
        match scopes.last() {
            Some(Scope::Type {
                full_name,
                line,
                index,
            }) => Some((full_name.clone(), *line, *index)),
            _ => None,
        }
    };
    while i < tokens.len() {
        let (tok, line) = tokens[i].clone();
        match &tok {
            Tok::Punct('[') if stmt.is_empty() => {
                // An attribute list: up to the matching `]`.
                let mut depth = 0;
                let mut j = i;
                while j < tokens.len() {
                    match tokens[j].0 {
                        Tok::Punct('[') => depth += 1,
                        Tok::Punct(']') => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                    j += 1;
                }
                let (names, s) = attribute_names(&tokens[i + 1..j.min(tokens.len())]);
                attrs.extend(names);
                skip |= s;
                i = j + 1;
                continue;
            }
            Tok::Punct(';') => {
                // File-scoped namespace.
                if let Some(Tok::Ident(k)) = stmt.first().map(|t| &t.0) {
                    if k == "namespace" {
                        file_namespace = dotted(&stmt[1..]);
                    }
                }
                // A positional record without a body.
                if let Some(name) = type_name(&stmt) {
                    let full = qualified(&scopes, &file_namespace, &namespace_of, &name);
                    out.declared.push(full);
                }
                stmt.clear();
                attrs.clear();
                skip = false;
            }
            Tok::Punct('{') => {
                let scope = classify(&stmt, &scopes, &file_namespace, &namespace_of, &mut out);
                // A method body: was it a test?
                scopes.push(scope);
                stmt.clear();
                attrs.clear();
                skip = false;
            }
            Tok::Punct('}') => {
                scopes.pop();
                stmt.clear();
                attrs.clear();
                skip = false;
            }
            Tok::Punct('(') => {
                // `Name(` or `Name<T>(` in a type body: a method (not after `=`, `new`, `=>`).
                if let Some((owner, class_line, index)) = enclosing_type(&scopes) {
                    let assigned = stmt.iter().any(|(t, _)| {
                        matches!(t, Tok::Punct('=')) || *t == Tok::Ident("new".into())
                    });
                    let name = method_name(&stmt);
                    if let (false, Some((name, name_line))) = (assigned, name) {
                        let is_static = stmt.iter().any(|(t, _)| *t == Tok::Ident("static".into()));
                        if name == "Main" && is_static {
                            out.entry_points.push(EntryPoint {
                                class_full_name: owner.clone(),
                                line: name_line,
                            });
                        }
                        let found = attrs.iter().find_map(|a| test_attribute(a, hint));
                        if let Some((framework, parameterized)) = found {
                            let ignored = attrs.iter().any(|a| a == "Ignore");
                            let parameterized = parameterized
                                || attrs.iter().any(|a| {
                                    matches!(
                                        a.as_str(),
                                        "TestCase"
                                            | "DataRow"
                                            | "InlineData"
                                            | "MemberData"
                                            | "ClassData"
                                            | "TestCaseSource"
                                    )
                                });
                            let idx = match index {
                                Some(ix) => ix,
                                None => {
                                    let ix = out.classes.len();
                                    out.classes.push(TestClass {
                                        name: owner
                                            .rsplit(['.', '+'])
                                            .next()
                                            .unwrap_or(&owner)
                                            .to_string(),
                                        full_name: owner.clone(),
                                        line: class_line,
                                        tests: Vec::new(),
                                    });
                                    if let Some(Scope::Type { index, .. }) = scopes.last_mut() {
                                        *index = Some(ix);
                                    }
                                    ix
                                }
                            };
                            out.classes[idx].tests.push(TestMethod {
                                name,
                                line: name_line,
                                framework,
                                skipped: skip || ignored,
                                parameterized,
                            });
                        }
                        // Skip the parameter list (attributes on parameters are not tests).
                        let mut depth = 0;
                        let mut j = i;
                        while j < tokens.len() {
                            match tokens[j].0 {
                                Tok::Punct('(') => depth += 1,
                                Tok::Punct(')') => {
                                    depth -= 1;
                                    if depth == 0 {
                                        break;
                                    }
                                }
                                _ => {}
                            }
                            j += 1;
                        }
                        stmt.push((Tok::Punct('('), line));
                        stmt.push((Tok::Punct(')'), line));
                        attrs.clear();
                        skip = false;
                        i = j + 1;
                        continue;
                    }
                }
                stmt.push((tok, line));
            }
            _ => stmt.push((tok, line)),
        }
        i += 1;
    }
    // Classes with test attributes only on the class ([TestFixture] without tests) are
    // not listed; keep the declaration order of the classes.
    out.classes.sort_by_key(|c| c.line);
    for c in &mut out.classes {
        c.tests.sort_by_key(|t| t.line);
    }
    out
}

fn dotted(tokens: &[(Tok, u32)]) -> String {
    let mut s = String::new();
    for (t, _) in tokens {
        match t {
            Tok::Ident(x) => s.push_str(x),
            Tok::Punct('.') => s.push('.'),
            _ => break,
        }
    }
    s
}

/// The identifier before `(` (skipping `<…>` type arguments).
fn method_name(stmt: &[(Tok, u32)]) -> Option<(String, u32)> {
    let mut j = stmt.len();
    if j == 0 {
        return None;
    }
    if matches!(stmt[j - 1].0, Tok::Punct('>')) {
        let mut depth = 0;
        while j > 0 {
            j -= 1;
            match stmt[j].0 {
                Tok::Punct('>') => depth += 1,
                Tok::Punct('<') => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
        }
    }
    match stmt.get(j.checked_sub(1)?) {
        Some((Tok::Ident(name), line)) if !is_keyword(name) => {
            // A declaration has a return type (or `void`) before the name.
            let has_type = j >= 2;
            has_type.then(|| (name.clone(), *line))
        }
        _ => None,
    }
}

fn is_keyword(s: &str) -> bool {
    matches!(
        s,
        "if" | "for"
            | "foreach"
            | "while"
            | "switch"
            | "using"
            | "lock"
            | "catch"
            | "return"
            | "nameof"
            | "typeof"
            | "sizeof"
            | "default"
            | "base"
            | "this"
            | "fixed"
            | "when"
    )
}

/// The name a type declaration statement declares (`class X`, `record struct X`).
fn type_name(stmt: &[(Tok, u32)]) -> Option<String> {
    for (k, (t, _)) in stmt.iter().enumerate() {
        let Tok::Ident(word) = t else { continue };
        if !TYPE_KEYWORDS.contains(&word.as_str()) {
            continue;
        }
        // `record class X`, `record struct X`.
        let mut n = k + 1;
        if word == "record" {
            if let Some((Tok::Ident(next), _)) = stmt.get(n) {
                if MODIFIERS_BEFORE_NAME.contains(&next.as_str()) {
                    n += 1;
                }
            }
        }
        if let Some((Tok::Ident(name), _)) = stmt.get(n) {
            return Some(name.clone());
        }
    }
    None
}

/// `Namespace.Outer+Inner` of a type declared in the current scopes.
fn qualified(
    scopes: &[Scope],
    file_ns: &str,
    namespace_of: &dyn Fn(&[Scope], &str) -> String,
    name: &str,
) -> String {
    let outer = scopes.iter().rev().find_map(|s| match s {
        Scope::Type { full_name, .. } => Some(full_name.clone()),
        _ => None,
    });
    match outer {
        Some(o) => format!("{o}+{name}"),
        None => {
            let ns = namespace_of(scopes, file_ns);
            if ns.is_empty() {
                name.to_string()
            } else {
                format!("{ns}.{name}")
            }
        }
    }
}

/// What a `{` opens, from the statement before it; records test classes it declares.
fn classify(
    stmt: &[(Tok, u32)],
    scopes: &[Scope],
    file_ns: &str,
    namespace_of: &dyn Fn(&[Scope], &str) -> String,
    out: &mut SourceScan,
) -> Scope {
    if let Some((Tok::Ident(k), _)) = stmt.first() {
        if k == "namespace" {
            return Scope::Namespace(dotted(&stmt[1..]));
        }
    }
    if let Some(name) = type_name(stmt) {
        let full_name = qualified(scopes, file_ns, namespace_of, &name);
        out.declared.push(full_name.clone());
        let line = stmt
            .iter()
            .find(|(t, _)| matches!(t, Tok::Ident(w) if TYPE_KEYWORDS.contains(&w.as_str())))
            .map(|(_, l)| *l)
            .unwrap_or(0);
        return Scope::Type {
            full_name,
            line,
            index: None,
        };
    }
    Scope::Other
}

/// Scan an F# file: `[<Fact>]` / `[<Test>]` / `[<TestMethod>]` on `let` functions of
/// modules and `member` methods of types.
pub fn scan_fsharp(src: &str, hint: Option<TestFramework>) -> SourceScan {
    let mut out = SourceScan::default();
    let mut namespace = String::new();
    // (indent, full name)
    let mut containers: Vec<(usize, String)> = Vec::new();
    let mut attrs: Vec<String> = Vec::new();
    let mut skip = false;
    let qualify = |ns: &str, containers: &[(usize, String)], name: &str| -> String {
        match containers.last() {
            Some((_, outer)) => format!("{outer}+{name}"),
            None if ns.is_empty() => name.to_string(),
            None => format!("{ns}.{name}"),
        }
    };
    for (ln, raw) in src.lines().enumerate() {
        let line = ln as u32;
        let indent = raw.len() - raw.trim_start().len();
        let t = raw.trim();
        if t.is_empty() || t.starts_with("//") {
            continue;
        }
        if !t.starts_with("[<") {
            containers.retain(|(i, _)| *i <= indent);
        }
        if let Some(rest) = t.strip_prefix("namespace ") {
            namespace = rest.trim().trim_start_matches("rec ").trim().to_string();
            containers.clear();
            continue;
        }
        if let Some(rest) = t.strip_prefix("module ") {
            let rest = rest.trim_start_matches("rec ").trim();
            let (name, top) = match rest.split_once('=') {
                Some((n, _)) => (n.trim(), false),
                None => (rest, true),
            };
            if top {
                // `module A.B.C` at the top: namespace A.B, module C.
                let (ns, m) = match name.rsplit_once('.') {
                    Some((ns, m)) => (ns.to_string(), m.to_string()),
                    None => (String::new(), name.to_string()),
                };
                namespace = ns;
                containers = vec![(0, qualify(&namespace, &[], &m))];
                // Top-level modules contain everything below, whatever the indentation.
                containers[0].0 = 0;
            } else {
                let full = qualify(&namespace, &containers, name);
                out.declared.push(full.clone());
                containers.push((indent + 1, full));
            }
            attrs.clear();
            continue;
        }
        if t.starts_with("[<") {
            let inner = t.trim_start_matches("[<");
            let inner = inner.split(">]").next().unwrap_or(inner);
            let toks = tokenize(inner);
            let (names, s) = attribute_names(&toks);
            attrs.extend(names);
            skip |= s;
            continue;
        }
        if let Some(rest) = t.strip_prefix("type ") {
            let name: String = rest
                .trim()
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() {
                let full = qualify(&namespace, &containers, &name);
                out.declared.push(full.clone());
                containers.push((indent + 1, full));
            }
            attrs.clear();
            continue;
        }
        let decl = t
            .strip_prefix("let ")
            .or_else(|| t.strip_prefix("member "))
            .or_else(|| t.strip_prefix("static member "));
        if let Some(rest) = decl {
            let found = attrs.iter().find_map(|a| test_attribute(a, hint));
            if let (Some((framework, parameterized)), Some((_, owner))) = (found, containers.last())
            {
                let rest = rest.trim_start_matches("async ").trim_start_matches("rec ");
                // `_.Name`, `this.Name`, ``name with spaces``, `name`.
                let rest = match rest.split_once('.') {
                    Some((self_id, r)) if !self_id.contains(' ') && !self_id.contains('`') => r,
                    _ => rest,
                };
                let toks = tokenize(rest);
                if let Some((Tok::Ident(name), _)) = toks.first() {
                    let owner = owner.clone();
                    let idx = match out.classes.iter().position(|c| c.full_name == owner) {
                        Some(i) => i,
                        None => {
                            out.classes.push(TestClass {
                                name: owner
                                    .rsplit(['.', '+'])
                                    .next()
                                    .unwrap_or(&owner)
                                    .to_string(),
                                full_name: owner.clone(),
                                line,
                                tests: Vec::new(),
                            });
                            out.classes.len() - 1
                        }
                    };
                    out.classes[idx].tests.push(TestMethod {
                        name: name.clone(),
                        line,
                        framework,
                        skipped: skip || attrs.iter().any(|a| a == "Ignore"),
                        parameterized: parameterized
                            || attrs.iter().any(|a| a == "InlineData" || a == "TestCase"),
                    });
                }
            }
            if attrs.iter().any(|a| a == "EntryPoint") {
                out.entry_points.push(EntryPoint {
                    class_full_name: containers
                        .last()
                        .map(|(_, n)| n.clone())
                        .unwrap_or_default(),
                    line,
                });
            }
        }
        attrs.clear();
        skip = false;
    }
    out
}

/// Scan a source file by its extension.
pub fn scan_source(path: &str, src: &str, hint: Option<TestFramework>) -> SourceScan {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".fs") || lower.ends_with(".fsx") {
        scan_fsharp(src, hint)
    } else if lower.ends_with(".cs") {
        scan_csharp(src, hint)
    } else {
        SourceScan::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::fixture;

    #[test]
    fn xunit_tests_with_nested_classes() {
        let s = scan_csharp(
            &fixture("bookstore/tests/Bookstore.Tests/CartTests.cs"),
            None,
        );
        assert_eq!(s.classes.len(), 2);
        let c = &s.classes[0];
        assert_eq!(c.full_name, "Bookstore.Tests.CartTests");
        let names: Vec<&str> = c.tests.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "Total_IsZero_ForAnEmptyCart",
                "Total_AddsLines",
                "Add_RejectsNonPositiveQuantities",
                "Total_AppliesVouchers"
            ]
        );
        assert_eq!(c.tests[0].line, 11);
        assert!(c.tests[2].parameterized && !c.tests[0].parameterized);
        assert!(c.tests[3].skipped);
        assert_eq!(c.tests[0].framework, TestFramework::Xunit);
        assert_eq!(
            s.classes[1].full_name,
            "Bookstore.Tests.CartTests+WhenEmpty"
        );
        assert_eq!(s.classes[1].tests[0].name, "HasNoLines");
        assert!(s
            .declared
            .contains(&"Bookstore.Tests.CartTests".to_string()));
    }

    #[test]
    fn nunit_and_mstest() {
        let nunit = r#"
using NUnit.Framework;
namespace Shop.Tests
{
    [TestFixture]
    public class PriceTests
    {
        private const string Weird = "}{ [Test] void Fake() {";
        private readonly string _v = @"multi
line ""quoted"" }";

        [SetUp] public void Init() { }

        [Test]
        public void Rounds() { Assert.That(1, Is.EqualTo(1)); }

        [TestCase(1, 2)]
        [TestCase(2, 3)]
        public void Adds(int a, int b) { }

        [Test, Ignore("later")]
        public void Later() { }

        [Theory]
        public void Holds(int x) { }
    }
}"#;
        let s = scan_csharp(nunit, Some(TestFramework::Nunit));
        assert_eq!(s.classes.len(), 1);
        let t = &s.classes[0].tests;
        let names: Vec<&str> = t.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["Rounds", "Adds", "Later", "Holds"]);
        assert_eq!(t[0].framework, TestFramework::Nunit);
        assert!(t[1].parameterized);
        assert!(t[2].skipped);
        assert_eq!(t[3].framework, TestFramework::Nunit);
        assert_eq!(s.classes[0].full_name, "Shop.Tests.PriceTests");

        let mstest = r#"namespace Shop.Tests;
[TestClass]
public sealed class StockTests
{
    [TestMethod]
    public async Task Reserves() => await Task.Delay(1);

    [DataTestMethod]
    [DataRow(1)]
    public void Releases(int n) { var x = new List<int>(); }

    [TestMethod, Ignore]
    public void Old() { }

    public static void Main(string[] args) { }
    public record struct Line(int Qty);
}
"#;
        let s = scan_csharp(mstest, None);
        let t = &s.classes[0].tests;
        assert_eq!(t.len(), 3);
        assert_eq!(t[0].framework, TestFramework::Mstest);
        assert!(t[1].parameterized);
        assert!(t[2].skipped);
        assert_eq!(
            s.entry_points,
            vec![EntryPoint {
                class_full_name: "Shop.Tests.StockTests".into(),
                line: 14
            }]
        );
        assert!(s
            .declared
            .contains(&"Shop.Tests.StockTests+Line".to_string()));
    }

    #[test]
    fn raw_strings_generics_and_no_tests() {
        let src = "namespace A { class B {\n string s = \"\"\"\n } [Fact] void X() {\n \"\"\";\n [Fact] public void Generic<T>() where T : new() { }\n static async Task<int> Main() => 0; } }";
        let s = scan_csharp(src, None);
        assert_eq!(s.classes[0].tests.len(), 1);
        assert_eq!(s.classes[0].tests[0].name, "Generic");
        assert_eq!(s.classes[0].tests[0].line, 4);
        assert_eq!(s.entry_points.len(), 1);
        let plain = scan_csharp(
            &fixture("bookstore/src/Bookstore.Core/Catalog/Cart.cs"),
            None,
        );
        assert!(plain.classes.is_empty());
        assert_eq!(plain.declared, vec!["Bookstore.Core.Catalog.Cart"]);
    }

    #[test]
    fn fsharp_tests() {
        let src = r#"module Bookstore.Reports.Tests

open Xunit

[<Fact>]
let ``revenue is the sum of sales`` () =
    Assert.Equal(3, 1 + 2)

[<Theory>]
[<InlineData(1)>]
let doubles (n: int) = ()

type CartSpec() =
    [<Fact>]
    member _.``adds a line`` () = ()

    [<Fact(Skip = "slow")>]
    member this.Slow() = ()

[<EntryPoint>]
let main argv = 0
"#;
        let s = scan_fsharp(src, None);
        assert_eq!(s.classes.len(), 2, "{s:#?}");
        assert_eq!(s.classes[0].full_name, "Bookstore.Reports.Tests");
        assert_eq!(s.classes[0].tests[0].name, "revenue is the sum of sales");
        assert_eq!(s.classes[0].tests[0].line, 5);
        assert!(s.classes[0].tests[1].parameterized);
        assert_eq!(s.classes[1].full_name, "Bookstore.Reports.Tests+CartSpec");
        assert_eq!(s.classes[1].tests[0].name, "adds a line");
        assert!(s.classes[1].tests[1].skipped);
        assert_eq!(s.entry_points.len(), 1);
        assert!(scan_source("x.vb", "", None).classes.is_empty());
    }
}
