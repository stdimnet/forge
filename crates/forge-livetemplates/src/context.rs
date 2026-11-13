//! Template contexts ("Applicable in") and what the code around a position knows.
//!
//! Context ids follow PHPStorm's tree: `PHP` with `PHP_STATEMENT`, `PHP_CLASS_MEMBER`,
//! `PHP_EXPRESSION`, `PHP_COMMENT` and `PHP_STRING`; `JAVASCRIPT` ("JavaScript and
//! TypeScript") with `JS_STATEMENT` and `JS_EXPRESSION`; `TYPESCRIPT`, `HTML`, `CSS`, `LUA`,
//! `SQL`, `BLADE`, `TWIG` and `OTHER`. A template enabled for a parent applies in its children,
//! except comments and strings, which only match when chosen explicitly.

use serde::Serialize;

use crate::lex::{lex, Kind, Syntax, Tok};

/// A node of the "Applicable in" tree.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContextNode {
    pub id: String,
    pub label: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<ContextNode>,
}

fn node(id: &str, label: &str, children: Vec<ContextNode>) -> ContextNode {
    ContextNode {
        id: id.into(),
        label: label.into(),
        children,
    }
}

/// The contexts a template can be applicable in, as PHPStorm's tree.
pub fn context_tree() -> Vec<ContextNode> {
    vec![
        node("BLADE", "Blade", vec![]),
        node("CSS", "CSS", vec![]),
        node("HTML", "HTML", vec![]),
        node(
            "JAVASCRIPT",
            "JavaScript and TypeScript",
            vec![
                node("JS_EXPRESSION", "Expression", vec![]),
                node("JS_STATEMENT", "Statement", vec![]),
            ],
        ),
        node("LUA", "Lua", vec![]),
        node(
            "PHP",
            "PHP",
            vec![
                node("PHP_CLASS_MEMBER", "Class Member", vec![]),
                node("PHP_COMMENT", "Comment", vec![]),
                node("PHP_EXPRESSION", "Expression", vec![]),
                node("PHP_STATEMENT", "Statement", vec![]),
                node("PHP_STRING", "String", vec![]),
            ],
        ),
        node("SQL", "SQL", vec![]),
        node("TWIG", "Twig", vec![]),
        node("TYPESCRIPT", "TypeScript", vec![]),
        node("OTHER", "Other", vec![]),
    ]
}

/// Parent context of a child id (`PHP_STATEMENT` → `PHP`); comments and strings have none
/// so that templates for "PHP" do not expand inside them.
fn parent_of(id: &str) -> Option<&'static str> {
    match id {
        "PHP_STATEMENT" | "PHP_CLASS_MEMBER" | "PHP_EXPRESSION" => Some("PHP"),
        "JS_STATEMENT" | "JS_EXPRESSION" => Some("JAVASCRIPT"),
        _ => None,
    }
}

/// Whether a template with `template_contexts` applies at a position with `caret_contexts`.
pub fn applies(template_contexts: &[String], caret_contexts: &[String]) -> bool {
    caret_contexts.iter().any(|c| {
        template_contexts
            .iter()
            .any(|t| t == c || parent_of(c) == Some(t.as_str()))
    })
}

/// Human-readable "Applicable in" summary, e.g. "PHP: statement, class member".
pub fn describe(contexts: &[String]) -> String {
    let tree = context_tree();
    let mut parts: Vec<String> = Vec::new();
    for top in &tree {
        if contexts.contains(&top.id) {
            parts.push(top.label.clone());
            continue;
        }
        let kids: Vec<String> = top
            .children
            .iter()
            .filter(|c| contexts.contains(&c.id))
            .map(|c| c.label.to_lowercase())
            .collect();
        if !kids.is_empty() {
            parts.push(format!("{}: {}", top.label, kids.join(", ")));
        }
    }
    parts.join("; ")
}

/// Language family of a file, from its file type id (or extension).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Lang {
    Php,
    Html,
    Css,
    JavaScript,
    TypeScript,
    Lua,
    Sql,
    Blade,
    Twig,
    Other,
}

impl Lang {
    pub fn of(language_id: Option<&str>, path: &str) -> Lang {
        let id = language_id.map(str::to_ascii_lowercase).unwrap_or_else(|| {
            let name = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
            if name.ends_with(".blade.php") {
                return "blade".into();
            }
            name.rsplit_once('.')
                .map(|(_, e)| e.to_string())
                .unwrap_or_default()
        });
        match id.as_str() {
            "php" | "phtml" | "inc" => Lang::Php,
            "blade" => Lang::Blade,
            "twig" => Lang::Twig,
            "html" | "htm" | "xhtml" | "vue" | "svelte" | "astro" => Lang::Html,
            "javascript" | "js" | "jsx" | "mjs" | "cjs" => Lang::JavaScript,
            "typescript" | "ts" | "tsx" | "mts" | "cts" => Lang::TypeScript,
            "css" | "scss" | "sass" | "less" | "pcss" => Lang::Css,
            "lua" => Lang::Lua,
            "sql" | "ddl" | "mysql" | "pgsql" | "plsql" => Lang::Sql,
            _ => Lang::Other,
        }
    }
}

/// What a position in a file knows: its contexts and the surrounding code.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CaretInfo {
    pub contexts: Vec<String>,
    /// Innermost class, interface, trait or enum.
    pub class_name: Option<String>,
    /// Innermost named function or method.
    pub function_name: Option<String>,
    pub namespace: Option<String>,
    /// Variables used before the position in the innermost function, most recent first
    /// (PHP: with `$`).
    pub variables: Vec<String>,
}

/// Contexts and surroundings at byte offset `at`.
pub fn analyze(text: &str, at: usize, language_id: Option<&str>, path: &str) -> CaretInfo {
    let at = at.min(text.len());
    match Lang::of(language_id, path) {
        Lang::Php => analyze_php(text, at),
        Lang::JavaScript => analyze_js(text, at, false),
        Lang::TypeScript => analyze_js(text, at, true),
        Lang::Lua => analyze_lua(text, at),
        Lang::Sql => analyze_sql(text, at),
        Lang::Css => contexts(&["CSS"]),
        Lang::Html => analyze_markup(text, at, None),
        Lang::Blade => analyze_markup(text, at, Some("BLADE")),
        Lang::Twig => analyze_markup(text, at, Some("TWIG")),
        Lang::Other => contexts(&["OTHER"]),
    }
}

/// SQL: everywhere except in comments and strings.
fn analyze_sql(text: &str, at: usize) -> CaretInfo {
    let line_start = text[..at].rfind('\n').map(|p| p + 1).unwrap_or(0);
    let line = &text[line_start..at];
    let quotes = line.matches('\'').count();
    let open_block = text[..at]
        .rfind("/*")
        .is_some_and(|p| !text[p..at].contains("*/"));
    if line.contains("--") || quotes % 2 == 1 || open_block {
        return CaretInfo::default();
    }
    contexts(&["SQL"])
}

fn contexts(ids: &[&str]) -> CaretInfo {
    CaretInfo {
        contexts: ids.iter().map(|s| s.to_string()).collect(),
        ..CaretInfo::default()
    }
}

/// HTML-like files: `<script>` is JavaScript (TypeScript with `lang="ts"`), `<style>` CSS.
fn analyze_markup(text: &str, at: usize, template: Option<&str>) -> CaretInfo {
    let before = &text[..at];
    let lower = before.to_ascii_lowercase();
    for (tag, close) in [("<script", "</script"), ("<style", "</style")] {
        if let Some(open) = lower.rfind(tag) {
            let closed = lower[open..].contains(close);
            let tag_end = lower[open..].find('>').map(|p| open + p + 1);
            if let (false, Some(body)) = (closed, tag_end) {
                if tag == "<style" {
                    return contexts(&["CSS"]);
                }
                let header = &lower[open..body];
                let ts = header.contains("lang=\"ts\"")
                    || header.contains("lang=\"tsx\"")
                    || header.contains("lang='ts'");
                let mut info = analyze_js(&text[body..], at - body, ts);
                info.variables.clear();
                return info;
            }
        }
    }
    let mut ids = Vec::new();
    if let Some(t) = template {
        ids.push(t);
    }
    ids.push("HTML");
    contexts(&ids)
}

#[derive(Debug, Clone, PartialEq)]
enum Frame {
    Class(Option<String>),
    Function(Option<String>),
    Block,
}

#[derive(Default)]
struct Pending {
    class: bool,
    function: bool,
    name: Option<String>,
    /// Token index of `function` (parameters belong to the function's scope).
    start: usize,
}

struct Scope {
    frames: Vec<Frame>,
    /// Token index where the innermost function body starts.
    function_starts: Vec<usize>,
}

/// Walk tokens before `at`, keeping the brace frames (class, function, block).
fn scan_frames(src: &str, toks: &[Tok], at: usize, php: bool) -> (Scope, Option<String>) {
    let mut scope = Scope {
        frames: Vec::new(),
        function_starts: vec![0],
    };
    let mut pending = Pending::default();
    let mut namespace: Option<String> = None;
    let mut reading_namespace = false;
    let mut prev: Option<&str> = None;
    for (i, t) in toks.iter().enumerate() {
        if t.start >= at {
            break;
        }
        if !t.significant() {
            continue;
        }
        let s = t.text(src);
        let after_access = matches!(prev, Some("->" | "::" | "?->" | "."));
        match t.kind {
            Kind::Ident if reading_namespace => {
                namespace = Some(s.trim_start_matches('\\').to_string());
                reading_namespace = false;
            }
            Kind::Ident if php && s.eq_ignore_ascii_case("namespace") && !after_access => {
                reading_namespace = true
            }
            Kind::Ident
                if !after_access
                    && matches!(s, "class" | "interface" | "trait" | "enum")
                    && prev != Some("new") =>
            {
                pending = Pending {
                    class: true,
                    ..Pending::default()
                }
            }
            Kind::Ident if s == "class" && prev == Some("new") => {
                pending = Pending {
                    class: true,
                    ..Pending::default()
                }
            }
            Kind::Ident if !after_access && s == "function" => {
                pending = Pending {
                    function: true,
                    start: i,
                    ..Pending::default()
                }
            }
            Kind::Ident
                if (pending.class || pending.function)
                    && pending.name.is_none()
                    && prev.is_some_and(|p| {
                        matches!(
                            p,
                            "class" | "interface" | "trait" | "enum" | "function" | "&" | "*"
                        )
                    }) =>
            {
                pending.name = Some(s.to_string())
            }
            // JavaScript methods: `name(…) {` inside a class body.
            Kind::Punct if s == "{" => {
                let frame = if pending.class {
                    Frame::Class(pending.name.take())
                } else if pending.function {
                    Frame::Function(pending.name.take())
                } else if !php
                    && matches!(scope.frames.last(), Some(Frame::Class(_)))
                    && prev == Some(")")
                {
                    Frame::Function(js_method_name(src, toks, i))
                } else if !php && prev == Some("=>") {
                    Frame::Function(None)
                } else {
                    Frame::Block
                };
                if matches!(frame, Frame::Function(_)) {
                    scope
                        .function_starts
                        .push(if pending.function { pending.start } else { i });
                }
                scope.frames.push(frame);
                pending = Pending::default();
            }
            Kind::Punct if s == "}" => {
                if matches!(scope.frames.pop(), Some(Frame::Function(_)))
                    && scope.function_starts.len() > 1
                {
                    scope.function_starts.pop();
                }
            }
            Kind::Punct if s == ";" => {
                pending = Pending::default();
                reading_namespace = false;
            }
            _ => {}
        }
        prev = Some(s);
    }
    (scope, namespace)
}

/// `name` of `name(args) {` (the `{` is token `open`).
fn js_method_name(src: &str, toks: &[Tok], open: usize) -> Option<String> {
    let mut depth = 0;
    let mut i = open;
    while i > 0 {
        i -= 1;
        match toks[i].text(src) {
            ")" => depth += 1,
            "(" => {
                depth -= 1;
                if depth == 0 {
                    return toks[..i]
                        .iter()
                        .rev()
                        .find(|t| t.significant())
                        .filter(|t| t.kind == Kind::Ident)
                        .map(|t| t.text(src).to_string());
                }
            }
            _ => {}
        }
    }
    None
}

/// Index of the token containing or ending before `at`, and whether `at` is inside it.
fn token_at(toks: &[Tok], at: usize) -> Option<&Tok> {
    toks.iter().find(|t| {
        t.start < at && at < t.end || (t.kind == Kind::Comment && t.start < at && at == t.end)
    })
}

fn statement_position(src: &str, toks: &[Tok], at: usize, php: bool) -> bool {
    let prev = toks.iter().rev().find(|t| t.end <= at && t.significant());
    match prev {
        None => true,
        Some(t) => match t.kind {
            Kind::OpenTag => true,
            Kind::Punct => {
                matches!(t.text(src), ";" | "{" | "}" | ")")
                    || (php && t.text(src) == ":")
                    // After a PHP attribute line: `#[Route('/')]`.
                    || (php && t.text(src) == "]" && src[crate::text::line_start(src, t.start)..].trim_start().starts_with("#["))
            }
            Kind::Ident => matches!(t.text(src), "else" | "do"),
            _ => false,
        },
    }
}

fn names(src: &str, toks: &[Tok], from: usize, at: usize, kind: Kind) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for t in toks[from..]
        .iter()
        .take_while(|t| t.start < at)
        .filter(|t| t.kind == kind)
    {
        let name = t.text(src).to_string();
        if name == "$this" {
            continue;
        }
        out.retain(|n| *n != name);
        out.push(name);
    }
    out.reverse();
    out
}

fn innermost<T>(frames: &[Frame], pick: impl Fn(&Frame) -> Option<T>) -> Option<T> {
    frames.iter().rev().find_map(pick)
}

fn analyze_php(text: &str, at: usize) -> CaretInfo {
    let toks = lex(text, Syntax::Php);
    if let Some(t) = token_at(&toks, at) {
        match t.kind {
            Kind::Comment => return contexts(&["PHP_COMMENT"]),
            Kind::Str => return contexts(&["PHP_STRING"]),
            _ => {}
        }
    }
    // Inside inline HTML?
    let last = toks.iter().rev().find(|t| t.start < at);
    if matches!(
        last.map(|t| t.kind),
        None | Some(Kind::Html | Kind::CloseTag)
    ) {
        let html_start = last.map_or(0, |t| t.start);
        let mut info = analyze_markup(&text[html_start..], at - html_start, None);
        info.contexts.retain(|c| c != "BLADE");
        return info;
    }
    let (scope, namespace) = scan_frames(text, &toks, at, true);
    let ctx = if !statement_position(text, &toks, at, true) {
        "PHP_EXPRESSION"
    } else if matches!(scope.frames.last(), Some(Frame::Class(_))) {
        "PHP_CLASS_MEMBER"
    } else {
        "PHP_STATEMENT"
    };
    let function_start = *scope.function_starts.last().unwrap_or(&0);
    CaretInfo {
        contexts: vec![ctx.into()],
        class_name: innermost(&scope.frames, |f| match f {
            Frame::Class(n) => Some(n.clone()),
            _ => None,
        })
        .flatten(),
        function_name: innermost(&scope.frames, |f| match f {
            Frame::Function(Some(n)) => Some(n.clone()),
            _ => None,
        }),
        namespace,
        variables: names(text, &toks, function_start, at, Kind::Var),
    }
}

fn analyze_js(text: &str, at: usize, typescript: bool) -> CaretInfo {
    let toks = lex(text, Syntax::Js);
    if token_at(&toks, at).is_some_and(|t| matches!(t.kind, Kind::Comment | Kind::Str)) {
        return CaretInfo::default();
    }
    let (scope, _) = scan_frames(text, &toks, at, false);
    let ctx = if statement_position(text, &toks, at, false)
        && !matches!(scope.frames.last(), Some(Frame::Class(_)))
    {
        "JS_STATEMENT"
    } else {
        "JS_EXPRESSION"
    };
    let mut ids = vec![ctx.to_string()];
    if typescript {
        ids.push("TYPESCRIPT".into());
    }
    CaretInfo {
        contexts: ids,
        class_name: innermost(&scope.frames, |f| match f {
            Frame::Class(n) => Some(n.clone()),
            _ => None,
        })
        .flatten(),
        function_name: innermost(&scope.frames, |f| match f {
            Frame::Function(Some(n)) => Some(n.clone()),
            _ => None,
        }),
        namespace: None,
        variables: Vec::new(),
    }
}

fn analyze_lua(text: &str, at: usize) -> CaretInfo {
    let toks = lex(text, Syntax::Lua);
    if token_at(&toks, at).is_some_and(|t| matches!(t.kind, Kind::Comment | Kind::Str)) {
        return CaretInfo::default();
    }
    // Function names on a block stack: `function a.b:c()` … `end`.
    let mut stack: Vec<Option<String>> = Vec::new();
    let mut loop_header = 0usize;
    for (i, t) in toks.iter().enumerate() {
        if t.start >= at || t.kind != Kind::Ident {
            continue;
        }
        match t.text(text) {
            "function" => {
                let mut name = String::new();
                for n in &toks[i + 1..] {
                    match n.text(text) {
                        "(" => break,
                        s => name.push_str(s),
                    }
                }
                stack.push(Some(name).filter(|n| !n.is_empty()));
            }
            "for" | "while" => loop_header += 1,
            "do" if loop_header > 0 => {
                loop_header -= 1;
                stack.push(None);
            }
            "if" | "do" | "repeat" => stack.push(None),
            "end" | "until" => {
                stack.pop();
            }
            _ => {}
        }
    }
    CaretInfo {
        contexts: vec!["LUA".into()],
        function_name: stack.iter().rev().find_map(|n| n.clone()),
        ..CaretInfo::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(src: &str) -> (String, usize) {
        let pos = src.find('|').unwrap();
        (src.replacen('|', "", 1), pos)
    }

    fn info(src: &str, lang: &str) -> CaretInfo {
        let (text, pos) = at(src);
        analyze(&text, pos, Some(lang), "")
    }

    #[test]
    fn finds_php_contexts() {
        let class = "<?php\nnamespace App\\Models;\n\nclass User extends Model\n{\n    |\n}";
        let i = info(class, "php");
        assert_eq!(i.contexts, ["PHP_CLASS_MEMBER"]);
        assert_eq!(i.class_name.as_deref(), Some("User"));
        assert_eq!(i.namespace.as_deref(), Some("App\\Models"));

        let method = "<?php class A { public function save(array $items) { $total = 0; | } }";
        let i = info(method, "php");
        assert_eq!(i.contexts, ["PHP_STATEMENT"]);
        assert_eq!(i.function_name.as_deref(), Some("save"));
        assert_eq!(i.variables, ["$total", "$items"]);
        let attr = "<?php class C {\n    #[Route('/a')]\n    |\n}";
        assert_eq!(info(attr, "php").contexts, ["PHP_CLASS_MEMBER"]);

        assert_eq!(info("<?php $x = |", "php").contexts, ["PHP_EXPRESSION"]);
        assert_eq!(info("<?php // |", "php").contexts, ["PHP_COMMENT"]);
        assert_eq!(info("<?php $a = 'x|y';", "php").contexts, ["PHP_STRING"]);
        assert_eq!(info("<p>|</p><?php", "php").contexts, ["HTML"]);
        assert_eq!(info("<?php if (1): ?>\n<b>|</b>", "php").contexts, ["HTML"]);
        assert_eq!(info("|", "php").contexts, ["HTML"]);
    }

    #[test]
    fn closures_and_static_class_constants_do_not_confuse_frames() {
        let src = "<?php class A { function f() { $c = X::class; $g = function () { | }; } }";
        let i = info(src, "php");
        assert_eq!(i.contexts, ["PHP_STATEMENT"]);
        assert_eq!(i.class_name.as_deref(), Some("A"));
        assert_eq!(i.function_name.as_deref(), Some("f"));
        assert!(i.variables.is_empty(), "{:?}", i.variables);
    }

    #[test]
    fn finds_javascript_and_markup_contexts() {
        assert_eq!(
            info("const a = 1;\n|", "javascript").contexts,
            ["JS_STATEMENT"]
        );
        assert_eq!(info("f(|)", "javascript").contexts, ["JS_EXPRESSION"]);
        assert_eq!(
            info("let x = 1;\n|", "typescript").contexts,
            ["JS_STATEMENT", "TYPESCRIPT"]
        );
        assert!(info("// |", "javascript").contexts.is_empty());
        let i = info("class Cart { total(a) { | } }", "javascript");
        assert_eq!(i.class_name.as_deref(), Some("Cart"));
        assert_eq!(i.function_name.as_deref(), Some("total"));
        assert_eq!(info("<div>|</div>", "html").contexts, ["HTML"]);
        assert_eq!(
            info("<script>\n|\n</script>", "html").contexts,
            ["JS_STATEMENT"]
        );
        assert_eq!(
            info("<script lang=\"ts\">|</script>", "vue").contexts,
            ["JS_STATEMENT", "TYPESCRIPT"]
        );
        assert_eq!(info("<style>|</style>", "html").contexts, ["CSS"]);
        assert_eq!(info("@if(1) | @endif", "blade").contexts, ["BLADE", "HTML"]);
        assert_eq!(info("{% block a %}|", "twig").contexts, ["TWIG", "HTML"]);
        assert_eq!(info("|", "markdown").contexts, ["OTHER"]);
    }

    #[test]
    fn finds_lua_contexts() {
        let i = info(
            "function M.load(path)\n  for i = 1, 2 do\n    |\n  end\nend",
            "lua",
        );
        assert_eq!(i.contexts, ["LUA"]);
        assert_eq!(i.function_name.as_deref(), Some("M.load"));
        assert!(info("-- |", "lua").contexts.is_empty());
    }

    #[test]
    fn matches_parent_contexts() {
        let php = vec!["PHP".to_string()];
        assert!(applies(&php, &["PHP_STATEMENT".into()]));
        assert!(!applies(&php, &["PHP_COMMENT".into()]));
        assert!(applies(&["PHP_COMMENT".into()], &["PHP_COMMENT".into()]));
        assert!(applies(
            &["JAVASCRIPT".into()],
            &["JS_STATEMENT".into(), "TYPESCRIPT".into()]
        ));
        assert!(!applies(
            &["JS_STATEMENT".into()],
            &["JS_EXPRESSION".into()]
        ));
        assert_eq!(
            describe(&[
                "PHP_STATEMENT".into(),
                "PHP_CLASS_MEMBER".into(),
                "HTML".into()
            ]),
            "HTML; PHP: class member, statement"
        );
        assert_eq!(Lang::of(None, "resources/views/a.blade.php"), Lang::Blade);
        assert_eq!(Lang::of(None, "src/app.ts"), Lang::TypeScript);
    }

    #[test]
    fn sql_context_outside_comments_and_strings() {
        assert_eq!(analyze("sel", 3, Some("sql"), "q.sql").contexts, ["SQL"]);
        assert!(analyze("-- sel", 6, None, "q.sql").contexts.is_empty());
        assert!(analyze("SELECT 'sel", 11, None, "q.sql")
            .contexts
            .is_empty());
    }
}
