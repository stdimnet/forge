//! What a Rust file holds for the run gutter, found with a small lexer (strings, raw strings,
//! chars, lifetimes, nested block comments) and brace tracking, not a full parser:
//!
//! - `fn main` (the entry point of binaries and examples),
//! - tests: functions with `#[test]` or a `#[<runtime>::test]` attribute (`tokio::test`,
//!   `async_std::test`, `test_log::test`, …), benchmarks (`#[bench]`),
//! - test modules (`#[cfg(test)] mod tests { … }`, any inline module with tests),
//! - doctests: fenced code blocks in `///` and `//!` comments whose info string is Rust
//!   (empty, `rust`, `should_panic`, `no_run`, `compile_fail`, `edition2021`, …; not
//!   `ignore`, `text` or another language), named like rustdoc names them: the item path
//!   from the crate root (`cart::Cart::total`) and the line of the opening fence.
//!
//! Module paths are relative to the file's module (`module` argument); inline `mod`
//! blocks nest.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ItemKind {
    Main,
    Test,
    Bench,
    /// An inline module with tests.
    TestModule,
    Doctest,
}

/// A runnable item of a file. Lines are 0-based.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RustItem {
    pub kind: ItemKind,
    pub line: u32,
    /// The function, module or documented item name.
    pub name: String,
    /// Full path from the crate root, as libtest names tests (`cart::tests::adds_items`),
    /// the module path of test modules, rustdoc's doctest name for doctests
    /// (`cart::Cart::total`; empty for the crate's own docs).
    pub path: String,
    /// Doctests: the attributes of the fence (`no_run`, `should_panic`, …).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<String>,
    /// `#[ignore]`d tests.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub ignored: bool,
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Ident(String),
    Punct(char),
    /// `///` or `//!` text (without the marker), and whether it is inner (`//!`).
    Doc {
        text: String,
        inner: bool,
    },
    /// String or char literal; its content does not matter.
    Literal,
}

#[derive(Debug, Clone)]
struct Token {
    tok: Tok,
    line: u32,
}

fn lex(src: &str) -> Vec<Token> {
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
            let start = i;
            while i < n && chars[i] != '\n' {
                i += 1;
            }
            let text: String = chars[start..i].iter().collect();
            if let Some(rest) = text.strip_prefix("///") {
                if !rest.starts_with('/') {
                    out.push(Token {
                        tok: Tok::Doc {
                            text: rest.to_string(),
                            inner: false,
                        },
                        line,
                    });
                }
            } else if let Some(rest) = text.strip_prefix("//!") {
                out.push(Token {
                    tok: Tok::Doc {
                        text: rest.to_string(),
                        inner: true,
                    },
                    line,
                });
            }
            continue;
        }
        if c == '/' && i + 1 < n && chars[i + 1] == '*' {
            let mut depth = 0;
            while i < n {
                if chars[i] == '/' && i + 1 < n && chars[i + 1] == '*' {
                    depth += 1;
                    i += 2;
                } else if chars[i] == '*' && i + 1 < n && chars[i + 1] == '/' {
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
            continue;
        }
        // Raw strings: r"…", r#"…"#, br#"…"#.
        if (c == 'r' || (c == 'b' && i + 1 < n && chars[i + 1] == 'r'))
            && !(i > 0 && (chars[i - 1].is_alphanumeric() || chars[i - 1] == '_'))
        {
            let mut j = i + if c == 'b' { 2 } else { 1 };
            let mut hashes = 0;
            while j < n && chars[j] == '#' {
                hashes += 1;
                j += 1;
            }
            if j < n && chars[j] == '"' && (hashes > 0 || j == i + 1 || c == 'b') {
                let tok_line = line;
                j += 1;
                loop {
                    if j >= n {
                        break;
                    }
                    if chars[j] == '\n' {
                        line += 1;
                    }
                    if chars[j] == '"'
                        && (0..hashes).all(|h| j + 1 + h < n && chars[j + 1 + h] == '#')
                    {
                        j += 1 + hashes;
                        break;
                    }
                    j += 1;
                }
                out.push(Token {
                    tok: Tok::Literal,
                    line: tok_line,
                });
                i = j;
                continue;
            }
        }
        if c == '"' {
            let tok_line = line;
            i += 1;
            while i < n && chars[i] != '"' {
                if chars[i] == '\\' {
                    i += 1;
                }
                if i < n && chars[i] == '\n' {
                    line += 1;
                }
                i += 1;
            }
            i += 1;
            out.push(Token {
                tok: Tok::Literal,
                line: tok_line,
            });
            continue;
        }
        if c == '\'' {
            // A char literal ('a', '\n', '\u{1F600}') or a lifetime ('a).
            if i + 2 < n && chars[i + 1] == '\\' {
                let mut j = i + 2;
                while j < n && chars[j] != '\'' && chars[j] != '\n' {
                    j += 1;
                }
                i = j + 1;
                out.push(Token {
                    tok: Tok::Literal,
                    line,
                });
                continue;
            }
            if i + 2 < n && chars[i + 2] == '\'' {
                i += 3;
                out.push(Token {
                    tok: Tok::Literal,
                    line,
                });
                continue;
            }
            i += 1;
            while i < n && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            continue;
        }
        if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < n && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            out.push(Token {
                tok: Tok::Ident(chars[start..i].iter().collect()),
                line,
            });
            continue;
        }
        if c.is_ascii_digit() {
            while i < n && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '.') {
                if chars[i] == '.' && i + 1 < n && !chars[i + 1].is_ascii_digit() {
                    break;
                }
                i += 1;
            }
            out.push(Token {
                tok: Tok::Literal,
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

/// Words of a fence info string that keep a block a Rust doctest.
const RUST_ATTRS: &[&str] = &[
    "rust",
    "should_panic",
    "no_run",
    "compile_fail",
    "test_harness",
    "allow_fail",
    "standalone_crate",
];

/// The attributes of a doctest fence, `None` when the block is no Rust test.
fn fence_attributes(info: &str) -> Option<Vec<String>> {
    let words: Vec<String> = info
        .split([',', ' ', '\t'])
        .map(|w| {
            w.trim()
                .trim_start_matches('{')
                .trim_end_matches('}')
                .trim_start_matches('.')
        })
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect();
    let mut attrs = Vec::new();
    for w in words {
        if w == "ignore" || w.starts_with("ignore-") || w == "text" {
            return None;
        }
        if RUST_ATTRS.contains(&w.as_str())
            || w.starts_with("edition")
            || w.starts_with('E') && w[1..].chars().all(|c| c.is_ascii_digit())
        {
            if w != "rust" {
                attrs.push(w);
            }
        } else {
            // Another language (`toml`, `sh`, `json`).
            return None;
        }
    }
    Some(attrs)
}

/// Doctests in a block of doc comment lines: (0-based line of the opening fence, attributes).
fn doctests_in(lines: &[(u32, String)]) -> Vec<(u32, Vec<String>)> {
    let mut out = Vec::new();
    let mut open: Option<String> = None;
    for (line, text) in lines {
        let t = text.trim_start();
        let fence = t.starts_with("```") || t.starts_with("~~~");
        if !fence {
            continue;
        }
        let marker: String = t.chars().take_while(|c| *c == '`' || *c == '~').collect();
        match &open {
            None => {
                if let Some(attrs) = fence_attributes(&t[marker.len()..]) {
                    out.push((*line, attrs));
                }
                open = Some(marker);
            }
            Some(m)
                if t.trim() == m.as_str()
                    || t.trim_end()
                        .chars()
                        .all(|c| c == m.chars().next().unwrap_or('`'))
                        && t.trim().len() >= m.len() =>
            {
                open = None;
            }
            Some(_) => {}
        }
    }
    out
}

fn join(a: &str, b: &str) -> String {
    match (a.is_empty(), b.is_empty()) {
        (true, _) => b.to_string(),
        (_, true) => a.to_string(),
        _ => format!("{a}::{b}"),
    }
}

#[derive(Debug)]
enum Scope {
    Module {
        name: String,
        tests: bool,
        line: u32,
        index: Option<usize>,
    },
    Impl {
        ty: String,
    },
    Other,
}

/// The runnable items of a file whose module path (from the crate root) is `module`.
pub fn scan(src: &str, module: &str) -> Vec<RustItem> {
    let toks = lex(src);
    let mut items: Vec<RustItem> = Vec::new();
    let mut scopes: Vec<Scope> = Vec::new();
    let mut attrs: Vec<String> = Vec::new();
    let mut docs: Vec<(u32, String)> = Vec::new();
    let mut inner_docs: Vec<(u32, String)> = Vec::new();
    let mut i = 0;
    let ident = |k: usize| match toks.get(k).map(|t| &t.tok) {
        Some(Tok::Ident(s)) => Some(s.as_str()),
        _ => None,
    };
    let punct =
        |k: usize, c: char| matches!(toks.get(k).map(|t| &t.tok), Some(Tok::Punct(p)) if *p == c);
    let module_path = |scopes: &[Scope]| {
        let mut p = module.to_string();
        for s in scopes {
            if let Scope::Module { name, .. } = s {
                p = join(&p, name);
            }
        }
        p
    };
    let impl_type = |scopes: &[Scope]| {
        scopes.iter().rev().find_map(|s| match s {
            Scope::Impl { ty } => Some(ty.clone()),
            Scope::Module { .. } => Some(String::new()),
            Scope::Other => None,
        })
    };
    let push_doctests =
        |items: &mut Vec<RustItem>, docs: &[(u32, String)], path: &str, name: &str| {
            for (line, attributes) in doctests_in(docs) {
                items.push(RustItem {
                    kind: ItemKind::Doctest,
                    line,
                    name: name.to_string(),
                    path: path.to_string(),
                    attributes,
                    ignored: false,
                });
            }
        };
    // Where the next `{` opens (`mod x {`, `impl … {`), set when the item header is read.
    let mut pending: Option<Scope> = None;
    while i < toks.len() {
        let t = &toks[i];
        match &t.tok {
            Tok::Doc { text, inner: true } => {
                inner_docs.push((t.line, text.clone()));
                i += 1;
            }
            Tok::Doc { text, inner: false } => {
                docs.push((t.line, text.clone()));
                i += 1;
            }
            Tok::Punct('#') if punct(i + 1, '[') || (punct(i + 1, '!') && punct(i + 2, '[')) => {
                let inner = punct(i + 1, '!');
                let mut j = i + if inner { 3 } else { 2 };
                let mut depth = 1;
                let mut text = String::new();
                while j < toks.len() && depth > 0 {
                    match &toks[j].tok {
                        Tok::Punct('[') => depth += 1,
                        Tok::Punct(']') => depth -= 1,
                        _ => {}
                    }
                    if depth > 0 {
                        match &toks[j].tok {
                            Tok::Ident(s) => text.push_str(s),
                            Tok::Punct(c) => text.push(*c),
                            Tok::Literal => text.push('"'),
                            Tok::Doc { .. } => {}
                        }
                    }
                    j += 1;
                }
                if !inner {
                    attrs.push(text);
                }
                i = j;
            }
            Tok::Punct('{') => {
                // Inner docs of the file end with the first item.
                scopes.push(pending.take().unwrap_or(Scope::Other));
                attrs.clear();
                docs.clear();
                i += 1;
            }
            Tok::Punct('}') => {
                if let Some(Scope::Module {
                    tests: true,
                    line,
                    name,
                    index: None,
                }) = scopes.last()
                {
                    let path = module_path(&scopes);
                    let (line, name) = (*line, name.clone());
                    items.push(RustItem {
                        kind: ItemKind::TestModule,
                        line,
                        name,
                        path,
                        attributes: Vec::new(),
                        ignored: false,
                    });
                }
                let closed = scopes.pop();
                // A module with tests makes its parents test modules too (not the file).
                if let Some(Scope::Module { tests: true, .. }) = closed {
                    if let Some(Scope::Module { tests, .. }) = scopes.last_mut() {
                        *tests = true;
                    }
                }
                attrs.clear();
                docs.clear();
                i += 1;
            }
            Tok::Punct(';') => {
                pending = None;
                attrs.clear();
                docs.clear();
                i += 1;
            }
            Tok::Ident(word) => {
                let word = word.as_str();
                // Item headers: skip visibility and qualifiers.
                if matches!(
                    word,
                    "pub" | "async" | "const" | "unsafe" | "extern" | "default"
                ) && !(word == "const" && ident(i + 1).is_some() && punct(i + 2, ':'))
                {
                    if word == "pub" && punct(i + 1, '(') {
                        // pub(crate)
                        let mut j = i + 2;
                        while j < toks.len() && !punct(j, ')') {
                            j += 1;
                        }
                        i = j + 1;
                    } else {
                        i += 1;
                    }
                    if word == "extern" && matches!(toks.get(i).map(|t| &t.tok), Some(Tok::Literal))
                    {
                        i += 1;
                    }
                    continue;
                }
                let doc_path = |own: &str, scopes: &[Scope]| {
                    let base = module_path(scopes);
                    match impl_type(scopes).filter(|t| !t.is_empty()) {
                        Some(ty) => join(&join(&base, &ty), own),
                        None => join(&base, own),
                    }
                };
                match word {
                    "fn" => {
                        let name = ident(i + 1).unwrap_or("").to_string();
                        let line = t.line;
                        let is_test = attrs.iter().any(|a| {
                            a == "test"
                                || a.ends_with("::test")
                                || a.starts_with("test(")
                                || a.contains("::test(")
                        });
                        let is_bench = attrs.iter().any(|a| a == "bench");
                        let ignored = attrs
                            .iter()
                            .any(|a| a == "ignore" || a.starts_with("ignore="));
                        let in_fn = scopes.iter().any(|s| matches!(s, Scope::Other));
                        let path = join(&module_path(&scopes), &name);
                        if !docs.is_empty() {
                            push_doctests(&mut items, &docs, &doc_path(&name, &scopes), &name);
                        }
                        if !in_fn && (is_test || is_bench) {
                            items.push(RustItem {
                                kind: if is_bench {
                                    ItemKind::Bench
                                } else {
                                    ItemKind::Test
                                },
                                line,
                                name: name.clone(),
                                path,
                                attributes: Vec::new(),
                                ignored,
                            });
                            mark_tests(&mut scopes);
                        } else if name == "main" && scopes.is_empty() && module.is_empty() {
                            items.push(RustItem {
                                kind: ItemKind::Main,
                                line,
                                name,
                                path: String::new(),
                                attributes: Vec::new(),
                                ignored: false,
                            });
                        }
                        attrs.clear();
                        docs.clear();
                        pending = Some(Scope::Other);
                        i += 2;
                    }
                    "mod" => {
                        let name = ident(i + 1).unwrap_or("").to_string();
                        let line = t.line;
                        if !docs.is_empty() {
                            push_doctests(&mut items, &docs, &doc_path(&name, &scopes), &name);
                        }
                        let cfg_test = attrs.iter().any(|a| a == "cfg(test)");
                        pending = Some(Scope::Module {
                            name,
                            tests: false,
                            line,
                            index: None,
                        });
                        let _ = cfg_test;
                        attrs.clear();
                        docs.clear();
                        i += 2;
                    }
                    // `-> impl Trait` of a function header.
                    "impl" if matches!(pending, Some(Scope::Other)) => {
                        i += 1;
                    }
                    "impl" => {
                        // The implemented type: after `for` if there is one, else the first
                        // path; its last segment without generics.
                        let mut j = i + 1;
                        let mut depth = 0i32;
                        let mut ty = String::new();
                        let mut after_for = false;
                        while j < toks.len() {
                            match &toks[j].tok {
                                Tok::Punct('{') if depth == 0 => break,
                                Tok::Punct(';') if depth == 0 => break,
                                Tok::Punct('<') => depth += 1,
                                Tok::Punct('>') => depth -= 1,
                                Tok::Ident(s) if depth == 0 && s == "for" => {
                                    after_for = true;
                                    ty.clear();
                                }
                                Tok::Ident(s) if depth == 0 && s == "where" => {
                                    while j < toks.len() && !punct(j, '{') {
                                        j += 1;
                                    }
                                    break;
                                }
                                Tok::Ident(s) if depth == 0 && s != "dyn" && s != "mut" => {
                                    ty = s.clone();
                                }
                                _ => {}
                            }
                            j += 1;
                        }
                        let _ = after_for;
                        if !docs.is_empty() {
                            push_doctests(&mut items, &docs, &doc_path(&ty, &scopes), &ty);
                        }
                        pending = Some(Scope::Impl { ty });
                        attrs.clear();
                        docs.clear();
                        i = j;
                    }
                    "struct" | "enum" | "trait" | "union" | "type" | "static" | "macro_rules"
                    | "const" => {
                        let mut j = i + 1;
                        if word == "macro_rules" && punct(j, '!') {
                            j += 1;
                        }
                        let name = ident(j).unwrap_or("").to_string();
                        if !docs.is_empty() && !name.is_empty() && name != "_" {
                            push_doctests(&mut items, &docs, &doc_path(&name, &scopes), &name);
                        }
                        pending = Some(if word == "trait" {
                            Scope::Impl { ty: name }
                        } else {
                            Scope::Other
                        });
                        attrs.clear();
                        docs.clear();
                        i = j + 1;
                    }
                    _ => {
                        i += 1;
                    }
                }
            }
            _ => {
                i += 1;
            }
        }
    }
    // The crate's or module's own docs (`//!`).
    if !inner_docs.is_empty() {
        let mut found = Vec::new();
        push_doctests(
            &mut found,
            &inner_docs,
            module,
            module.rsplit("::").next().unwrap_or(""),
        );
        items.extend(found);
    }
    items.sort_by_key(|it| (it.line, it.kind != ItemKind::TestModule));
    items
}

fn mark_tests(scopes: &mut [Scope]) {
    if let Some(Scope::Module { tests, .. }) = scopes
        .iter_mut()
        .rev()
        .find(|s| matches!(s, Scope::Module { .. }))
    {
        *tests = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(items: &[RustItem]) -> Vec<(ItemKind, u32, &str)> {
        items
            .iter()
            .map(|i| (i.kind, i.line, i.path.as_str()))
            .collect()
    }

    #[test]
    fn tests_modules_and_doctests_of_the_demo_cart() {
        let src = include_str!("../tests/fixtures/shop/crates/shop-core/src/cart.rs");
        let items = scan(src, "cart");
        assert_eq!(
            rows(&items),
            [
                (ItemKind::Doctest, 30, "cart::Cart::total"),
                (ItemKind::TestModule, 54, "cart::tests"),
                (ItemKind::Test, 58, "cart::tests::adds_items"),
                (ItemKind::Test, 66, "cart::tests::rounds_to_cents"),
                (ItemKind::Test, 73, "cart::tests::vouchers"),
            ]
        );
        assert!(items[4].ignored);
        // rustdoc: `src/cart.rs - cart::Cart::total (line 31)`, 1-based.
        assert_eq!(items[0].line + 1, 31);
    }

    #[test]
    fn crate_docs_and_functions() {
        let src = include_str!("../tests/fixtures/shop/crates/shop-core/src/lib.rs");
        let items = scan(src, "");
        assert_eq!(
            rows(&items),
            [(ItemKind::Doctest, 2, ""), (ItemKind::Doctest, 17, "price")]
        );
        let main = scan(
            include_str!("../tests/fixtures/shop/crates/shop-cli/src/main.rs"),
            "",
        );
        assert_eq!(rows(&main), [(ItemKind::Main, 4, "")]);
        // `fn main` of a module is no entry point.
        assert!(scan("fn main() {}", "cli").is_empty());
    }

    #[test]
    fn runtimes_benches_and_tricky_tokens() {
        let src = r##"
const S: &str = "#[test] fn fake() {}";
const R: &str = r#"mod tests { #[test] fn raw() {} }"#;
/* #[test] fn commented() {} /* nested */ */
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test(flavor = "multi_thread")]
    async fn fetches() { let c = '{'; let l: &'static str = "}"; }
    #[bench]
    fn sums(b: &mut test::Bencher) { b.iter(|| 1 + 1) }
    mod nested {
        #[test_log::test]
        fn logs() {}
    }
    fn helper() {
        #[test]
        fn inside_a_function() {}
    }
}
"##;
        let items = scan(src, "");
        assert_eq!(
            rows(&items),
            [
                (ItemKind::TestModule, 5, "tests"),
                (ItemKind::Test, 8, "tests::fetches"),
                (ItemKind::Bench, 10, "tests::sums"),
                (ItemKind::TestModule, 11, "tests::nested"),
                (ItemKind::Test, 13, "tests::nested::logs"),
            ]
        );
    }

    #[test]
    fn fences_that_are_not_rust() {
        let src = "/// ```toml\n/// a = 1\n/// ```\n/// ```ignore\n/// x\n/// ```\n/// ```no_run,edition2021\n/// x();\n/// ```\npub fn x() {}\n\n/// ```text\n/// ```\nimpl<T: Clone> Into<u8> for Wrapper<T> where T: Copy {\n    /// ```\n    /// y();\n    /// ```\n    fn into(self) -> u8 { 0 }\n}\n";
        let items = scan(src, "util");
        assert_eq!(
            items
                .iter()
                .map(|i| (i.line, i.path.as_str(), i.attributes.clone()))
                .collect::<Vec<_>>(),
            [
                (
                    6,
                    "util::x",
                    vec!["no_run".to_string(), "edition2021".to_string()]
                ),
                (14, "util::Wrapper::into", vec![]),
            ]
        );
    }
}
