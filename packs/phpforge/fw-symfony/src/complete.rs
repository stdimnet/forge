//! Completion and Go to Declaration for Symfony references in PHP, Twig and YAML files:
//! route names (`generateUrl`, `redirectToRoute`, Twig `path()`/`url()`), templates
//! (`render()`, `{% extends %}`), service ids (`@id`, `#[Autowire(service:)]`,
//! `$container->get()`), parameters (`%name%`, `getParameter()`) and Twig tags, filters,
//! functions and tests. Offsets are byte offsets into the given text.

use serde::{Deserialize, Serialize};

use crate::strings::{call_context, string_at, string_at_from, word_before, StringAt};
use crate::twig::TwigSymbolKind;
use crate::{Location, Snapshot};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CompletionKind {
    Route,
    Service,
    Template,
    Parameter,
    Tag,
    Filter,
    Function,
    Test,
    Variable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionItem {
    pub label: String,
    pub kind: CompletionKind,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub detail: String,
    /// Longer text for the documentation popup.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub info: String,
    /// Text to insert when it differs from the label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub apply: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionList {
    /// Start of the text the completion replaces (up to the cursor).
    pub from: usize,
    pub items: Vec<CompletionItem>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Php,
    Twig,
    Yaml,
    Other,
}

impl FileKind {
    pub fn of(path: &str) -> Self {
        let p = path.to_ascii_lowercase();
        if p.ends_with(".twig") {
            Self::Twig
        } else if p.ends_with(".php") {
            Self::Php
        } else if p.ends_with(".yaml") || p.ends_with(".yml") {
            Self::Yaml
        } else {
            Self::Other
        }
    }
}

/// What a string or word at the cursor refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefKind {
    Route,
    Service,
    Template,
    Parameter,
    TwigTag,
    TwigFilter,
    TwigFunction,
    TwigTest,
}

/// A reference at the cursor: kind, replaced range start, typed prefix and whole value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    pub kind: RefKind,
    pub from: usize,
    pub prefix: String,
    pub value: String,
}

const ROUTE_FUNCTIONS: &[&str] = &["generateUrl", "redirectToRoute", "generate", "path", "url"];
const TEMPLATE_FUNCTIONS: &[&str] = &[
    "render",
    "renderView",
    "renderBlock",
    "renderBlockView",
    "stream",
    "Template",
    "include",
    "source",
];

/// Reference inside a `%parameter%` or `@service` string (YAML, Autowire).
fn sigil_reference(s: &StringAt) -> Option<Reference> {
    if let Some(rest) = s.prefix.strip_prefix('@') {
        if !rest.contains(['%', ' ']) {
            return Some(Reference {
                kind: RefKind::Service,
                from: s.start + 1,
                prefix: rest.to_string(),
                value: s.value[1..].to_string(),
            });
        }
    }
    let pct = s.prefix.matches('%').count();
    if pct % 2 == 1 {
        let last = s.prefix.rfind('%')?;
        let prefix = &s.prefix[last + 1..];
        if prefix.contains(' ') {
            return None;
        }
        let value_rest = &s.value[last + 1..];
        let value = value_rest.split('%').next().unwrap_or("").to_string();
        if prefix.starts_with("env(") {
            return None;
        }
        return Some(Reference {
            kind: RefKind::Parameter,
            from: s.start + last + 1,
            prefix: prefix.to_string(),
            value,
        });
    }
    None
}

fn plain(kind: RefKind, s: &StringAt) -> Reference {
    Reference {
        kind,
        from: s.start,
        prefix: s.prefix.clone(),
        value: s.value.clone(),
    }
}

/// Open Twig delimiter before `offset`: `Some(true)` for `{%`, `Some(false)` for `{{`, with
/// the byte offset just after it.
fn twig_block(text: &str, offset: usize) -> Option<(bool, usize)> {
    let before = &text[..offset];
    let tag = before.rfind("{%");
    let expr = before.rfind("{{");
    let (pos, is_tag) = match (tag, expr) {
        (Some(t), Some(e)) => {
            if t > e {
                (t, true)
            } else {
                (e, false)
            }
        }
        (Some(t), None) => (t, true),
        (None, Some(e)) => (e, false),
        (None, None) => return None,
    };
    let inside = &before[pos + 2..];
    let close = if is_tag { "%}" } else { "}}" };
    if inside.contains(close) {
        return None;
    }
    Some((is_tag, pos + 2))
}

fn twig_string_reference(text: &str, s: &StringAt) -> Option<Reference> {
    let (is_tag, start) = twig_block(text, s.open)?;
    if let Some(call) = call_context(text, s.open) {
        if call.arg == 0 {
            if call.function == "path" || call.function == "url" {
                return Some(plain(RefKind::Route, s));
            }
            if TEMPLATE_FUNCTIONS.contains(&call.function.as_str()) {
                return Some(plain(RefKind::Template, s));
            }
        }
    }
    if is_tag {
        let head = text[start..s.open].trim_start_matches('-').trim();
        let first = head.split_whitespace().next().unwrap_or("");
        let template_tag = matches!(
            first,
            "extends" | "include" | "embed" | "import" | "from" | "use"
        ) && head.split_whitespace().count() == 1;
        let theme = first == "form_theme" && head.split_whitespace().count() >= 2;
        if template_tag || theme {
            return Some(plain(RefKind::Template, s));
        }
    }
    None
}

fn php_string_reference(text: &str, s: &StringAt) -> Option<Reference> {
    let call = call_context(text, s.open);
    if let Some(call) = &call {
        if call.function == "Autowire" {
            return match call.named.as_deref() {
                Some("service") => Some(plain(RefKind::Service, s)),
                Some("param") => Some(plain(RefKind::Parameter, s)),
                _ => sigil_reference(s),
            };
        }
    }
    if let Some(r) = sigil_reference(s) {
        if r.kind == RefKind::Parameter {
            return Some(r);
        }
    }
    let call = call?;
    if call.arg != 0 {
        return None;
    }
    let f = call.function.as_str();
    if ROUTE_FUNCTIONS.contains(&f) && (call.is_method || f == "path" || f == "url") {
        if f == "generate" || f == "path" || f == "url" {
            // `->generate()` on url generators/routers only; skip unrelated generators.
            let head = &text[s.open.saturating_sub(80)..s.open].to_ascii_lowercase();
            if f == "generate" && !(head.contains("router") || head.contains("url")) {
                return None;
            }
        }
        return Some(plain(RefKind::Route, s));
    }
    if TEMPLATE_FUNCTIONS.contains(&f) && (call.is_method || f == "Template") && f != "include" {
        return Some(plain(RefKind::Template, s));
    }
    if (f == "get" || f == "has") && call.is_method {
        let head = &text[s.open.saturating_sub(60)..s.open];
        if head.contains("ontainer") || head.contains("$this->get(") || head.contains("locator") {
            return Some(plain(RefKind::Service, s));
        }
    }
    if matches!(f, "Reference" | "service" | "ServiceLocatorArgument") {
        return Some(plain(RefKind::Service, s));
    }
    if matches!(f, "getParameter" | "hasParameter" | "param") {
        return Some(plain(RefKind::Parameter, s));
    }
    None
}

/// Twig tag/filter/function/test at the cursor (outside strings).
fn twig_word_reference(text: &str, offset: usize) -> Option<Reference> {
    let (is_tag, start) = twig_block(text, offset)?;
    let (from, word) = word_before(text, offset, &[]);
    let value_end = text[offset..]
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .map(|i| offset + i)
        .unwrap_or(text.len());
    let value = text[from..value_end].to_string();
    let before = text[start..from].trim_end();
    let before = before.trim_start_matches('-');
    let make = |kind| {
        Some(Reference {
            kind,
            from,
            prefix: word.clone(),
            value: value.clone(),
        })
    };
    if before.ends_with('|') {
        return make(RefKind::TwigFilter);
    }
    if before.ends_with('.') {
        return None;
    }
    if is_tag && before.trim().is_empty() {
        return make(RefKind::TwigTag);
    }
    if before.ends_with(" is") || before.ends_with(" is not") {
        return make(RefKind::TwigTest);
    }
    if before.ends_with(['(', ',', '[', '{', ':', '=', '+', '-', '*', '/', '~', '?'])
        || before.trim().is_empty()
        || before.ends_with(" and")
        || before.ends_with(" or")
        || before.ends_with(" not")
        || before.ends_with(" in")
        || before.ends_with(" if")
        || (is_tag && before.split_whitespace().count() >= 1 && !word.is_empty())
    {
        return make(RefKind::TwigFunction);
    }
    None
}

/// The Symfony reference at `offset`, if any.
pub fn reference_at(path: &str, text: &str, offset: usize) -> Option<Reference> {
    let offset = offset.min(text.len());
    let kind = FileKind::of(path);
    if kind == FileKind::Twig {
        // Only strings inside `{{ }}` / `{% %}`; quotes of HTML attributes do not count.
        let (_, start) = twig_block(text, offset)?;
        if let Some(s) = string_at_from(text, offset, start) {
            return twig_string_reference(text, &s);
        }
        return twig_word_reference(text, offset);
    }
    let s = string_at(text, offset)?;
    match kind {
        FileKind::Php => php_string_reference(text, &s),
        FileKind::Yaml => sigil_reference(&s),
        FileKind::Twig | FileKind::Other => None,
    }
}

fn matches_prefix(name: &str, prefix: &str) -> bool {
    if prefix.is_empty() {
        return true;
    }
    let n = name.to_ascii_lowercase();
    let p = prefix.to_ascii_lowercase();
    if n.contains(&p) {
        return true;
    }
    // Camel/underscore humps: "os" matches "order_show".
    let mut chars = p.chars().peekable();
    for c in n.chars() {
        if chars.peek() == Some(&c) {
            chars.next();
        }
    }
    chars.peek().is_none() && n.starts_with(p.chars().next().unwrap_or(' '))
}

fn twig_kind(k: RefKind) -> Option<TwigSymbolKind> {
    Some(match k {
        RefKind::TwigTag => TwigSymbolKind::Tag,
        RefKind::TwigFilter => TwigSymbolKind::Filter,
        RefKind::TwigFunction => TwigSymbolKind::Function,
        RefKind::TwigTest => TwigSymbolKind::Test,
        _ => return None,
    })
}

/// Completion items for the reference at the cursor.
pub fn complete(snap: &Snapshot, path: &str, text: &str, offset: usize) -> Option<CompletionList> {
    let r = reference_at(path, text, offset)?;
    let mut items: Vec<CompletionItem> = Vec::new();
    match r.kind {
        RefKind::Route => {
            for route in snap
                .routes
                .iter()
                .filter(|x| matches_prefix(&x.name, &r.prefix))
            {
                let methods = if route.methods.is_empty() {
                    "ANY".to_string()
                } else {
                    route.methods.join("|")
                };
                items.push(CompletionItem {
                    label: route.name.clone(),
                    kind: CompletionKind::Route,
                    detail: format!("{methods} {}", route.path),
                    info: route.controller.clone().unwrap_or_default(),
                    apply: None,
                });
            }
        }
        RefKind::Service => {
            for s in snap
                .services
                .iter()
                .filter(|x| matches_prefix(&x.id, &r.prefix))
            {
                let detail = match (&s.alias, &s.class) {
                    (Some(a), _) => format!("alias for {a}"),
                    (None, Some(c)) if c != &s.id => c.clone(),
                    _ => String::new(),
                };
                items.push(CompletionItem {
                    label: s.id.clone(),
                    kind: CompletionKind::Service,
                    detail,
                    info: if s.public {
                        "public".into()
                    } else {
                        String::new()
                    },
                    apply: None,
                });
            }
        }
        RefKind::Template => {
            for t in snap
                .templates
                .iter()
                .filter(|x| matches_prefix(&x.name, &r.prefix))
            {
                items.push(CompletionItem {
                    label: t.name.clone(),
                    kind: CompletionKind::Template,
                    detail: t.path.clone(),
                    info: String::new(),
                    apply: None,
                });
            }
        }
        RefKind::Parameter => {
            // `'%app.cur|'` gets its closing `%`.
            let closing = text[..r.from].ends_with('%') && !text[offset..].starts_with('%');
            for p in snap
                .parameters
                .iter()
                .filter(|x| matches_prefix(&x.name, &r.prefix))
            {
                items.push(CompletionItem {
                    label: p.name.clone(),
                    kind: CompletionKind::Parameter,
                    detail: p.value.clone(),
                    info: String::new(),
                    apply: closing.then(|| format!("{}%", p.name)),
                });
            }
        }
        k => {
            let tk = twig_kind(k)?;
            for s in snap.twig.iter().filter(|x| {
                (x.kind == tk
                    || (tk == TwigSymbolKind::Function && x.kind == TwigSymbolKind::Variable))
                    && matches_prefix(&x.name, &r.prefix)
            }) {
                items.push(CompletionItem {
                    label: s.name.clone(),
                    kind: match s.kind {
                        TwigSymbolKind::Tag => CompletionKind::Tag,
                        TwigSymbolKind::Filter => CompletionKind::Filter,
                        TwigSymbolKind::Function => CompletionKind::Function,
                        TwigSymbolKind::Test => CompletionKind::Test,
                        TwigSymbolKind::Variable => CompletionKind::Variable,
                    },
                    detail: if s.signature.is_empty() {
                        s.provider.clone()
                    } else {
                        s.signature.clone()
                    },
                    info: s.provider.clone(),
                    apply: None,
                });
            }
        }
    }
    // Exact prefix matches first, then alphabetical.
    let p = r.prefix.to_ascii_lowercase();
    items.sort_by(|a, b| {
        let ap = !a.label.to_ascii_lowercase().starts_with(&p);
        let bp = !b.label.to_ascii_lowercase().starts_with(&p);
        ap.cmp(&bp).then(a.label.cmp(&b.label))
    });
    items.dedup_by(|a, b| a.label == b.label && a.kind == b.kind);
    Some(CompletionList {
        from: r.from,
        items,
    })
}

/// Follow aliases to the service that is actually declared.
fn service_target(snap: &Snapshot, id: &str) -> Option<Location> {
    let mut id = id.trim_start_matches('@').to_string();
    for _ in 0..5 {
        let s = snap.service(&id)?;
        if let Some(a) = &s.alias {
            if let Some(loc) = &s.location {
                if s.source == crate::ServiceSource::Yaml && snap.service(a).is_none() {
                    return Some(loc.clone());
                }
            }
            id = a.clone();
            continue;
        }
        if let Some(c) = &s.class {
            if let Some(loc) = snap.class_location(c) {
                return Some(loc);
            }
        }
        return s.location.clone();
    }
    None
}

/// Declarations the reference at `offset` points to.
pub fn goto(snap: &Snapshot, path: &str, text: &str, offset: usize) -> Vec<Location> {
    let offset = offset.min(text.len());
    if let Some(r) = reference_at(path, text, offset) {
        let found = match r.kind {
            RefKind::Route => snap.route(&r.value).and_then(|route| {
                route
                    .controller
                    .as_deref()
                    .and_then(|c| snap.method_location(c))
                    .or_else(|| route.location.clone())
            }),
            RefKind::Service => service_target(snap, &r.value),
            RefKind::Template => snap
                .template(&r.value)
                .map(|t| Location::new(&t.path, 0, 0)),
            RefKind::Parameter => snap.parameter(&r.value).and_then(|p| p.location.clone()),
            k => {
                let tk = twig_kind(k);
                snap.twig
                    .iter()
                    .find(|s| Some(s.kind) == tk && s.name == r.value)
                    .and_then(|s| s.location.clone())
            }
        };
        return found.into_iter().collect();
    }
    // YAML: `controller: App\Controller\X::method` and service class keys.
    if FileKind::of(path) == FileKind::Yaml {
        let (from, _) = word_before(text, offset, &['\\', ':']);
        let end = text[offset..]
            .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '\\' || c == ':'))
            .map(|i| offset + i)
            .unwrap_or(text.len());
        let word = text[from..end].trim_matches(':');
        if word.contains('\\') {
            let loc = if word.contains("::") {
                snap.method_location(word)
            } else {
                snap.class_location(word.trim_end_matches('\\'))
            };
            return loc.into_iter().collect();
        }
    }
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{fixture, scan, Config};

    fn at(src: &str) -> (String, usize) {
        let pos = src.find('¦').unwrap();
        (src.replacen('¦', "", 1), pos)
    }

    fn labels(list: &CompletionList) -> Vec<&str> {
        list.items.iter().map(|i| i.label.as_str()).collect()
    }

    #[test]
    fn route_names_in_php_and_twig() {
        let dir = fixture::project();
        let snap = scan(dir.path(), &Config::default());
        let (src, pos) = at("<?php $this->redirectToRoute('order_¦');");
        let list = complete(&snap, "src/X.php", &src, pos).unwrap();
        assert_eq!(
            labels(&list),
            vec!["order_index", "order_pay", "order_show"]
        );
        assert_eq!(list.items[2].detail, "GET /orders/{id}");
        assert_eq!(list.from, src.find("order_").unwrap());

        let (src, pos) = at("<a href=\"{{ path('order_sh¦') }}\">");
        let list = complete(&snap, "templates/x.html.twig", &src, pos).unwrap();
        assert_eq!(labels(&list), vec!["order_show"]);
    }

    #[test]
    fn templates_services_and_parameters() {
        let dir = fixture::project();
        let snap = scan(dir.path(), &Config::default());
        let (src, pos) = at("return $this->render('order/¦');");
        let list = complete(&snap, "src/X.php", &src, pos).unwrap();
        assert_eq!(
            labels(&list),
            vec!["order/index.html.twig", "order/show.html.twig"]
        );

        let (src, pos) = at("{% extends 'ba¦' %}");
        let list = complete(&snap, "templates/x.html.twig", &src, pos).unwrap();
        assert_eq!(labels(&list), vec!["base.html.twig"]);

        let (src, pos) = at("        arguments: ['@app.m¦']");
        let list = complete(&snap, "config/services.yaml", &src, pos).unwrap();
        assert_eq!(labels(&list)[0], "app.mailer");
        assert_eq!(list.from, src.find("app.m").unwrap());

        let (src, pos) = at("    #[Autowire('%app.cur¦')] private string $c,");
        let list = complete(&snap, "src/X.php", &src, pos).unwrap();
        assert_eq!(labels(&list), vec!["app.currency"]);
        assert_eq!(list.items[0].apply.as_deref(), Some("app.currency%"));

        let (src, pos) = at("$this->getParameter('kernel.pro¦')");
        let list = complete(&snap, "src/X.php", &src, pos).unwrap();
        assert_eq!(labels(&list), vec!["kernel.project_dir"]);
    }

    #[test]
    fn twig_filters_tags_functions_tests() {
        let dir = fixture::project();
        let snap = scan(dir.path(), &Config::default());
        let (src, pos) = at("{{ order.total|pr¦ }}");
        let list = complete(&snap, "t.html.twig", &src, pos).unwrap();
        assert_eq!(labels(&list)[0], "price");

        let (src, pos) = at("{% bl¦ %}");
        let list = complete(&snap, "t.html.twig", &src, pos).unwrap();
        assert_eq!(labels(&list)[0], "block");

        let (src, pos) = at("{{ form_¦ }}");
        let list = complete(&snap, "t.html.twig", &src, pos).unwrap();
        assert!(labels(&list).contains(&"form_start"));
        assert!(list
            .items
            .iter()
            .all(|i| i.kind == CompletionKind::Function));

        let (src, pos) = at("{% if items is em¦ %}");
        let list = complete(&snap, "t.html.twig", &src, pos).unwrap();
        assert_eq!(labels(&list), vec!["empty"]);

        let (src, pos) = at("<p>plain text¦ here</p>");
        assert!(complete(&snap, "t.html.twig", &src, pos).is_none());
    }

    #[test]
    fn goto_targets() {
        let dir = fixture::project();
        let snap = scan(dir.path(), &Config::default());
        let (src, pos) = at("{{ path('order_s¦how') }}");
        let loc = &goto(&snap, "t.html.twig", &src, pos)[0];
        assert_eq!(loc.path, "src/Controller/OrderController.php");
        assert_eq!(loc.line, 20);

        let (src, pos) = at("$this->render('order/sh¦ow.html.twig')");
        assert_eq!(
            goto(&snap, "src/X.php", &src, pos)[0].path,
            "templates/order/show.html.twig"
        );

        let (src, pos) = at("['@mailer.def¦ault']");
        let loc = &goto(&snap, "config/services.yaml", &src, pos)[0];
        assert_eq!(loc.path, "src/Service/Mailer.php");

        let (src, pos) = at("$currency: '%app.curr¦ency%'");
        assert_eq!(goto(&snap, "config/services.yaml", &src, pos)[0].line, 1);

        let (src, pos) = at("{{ total|pri¦ce }}");
        assert_eq!(
            goto(&snap, "t.html.twig", &src, pos)[0].path,
            "src/Twig/AppExtension.php"
        );

        let (src, pos) = at("    controller: App\\Controller\\OrderController::sh¦ow");
        assert_eq!(goto(&snap, "config/routes.yaml", &src, pos)[0].line, 20);
    }

    #[test]
    fn ignores_unrelated_strings() {
        let (src, pos) = at("$request->get('pa¦ge')");
        assert!(reference_at("src/X.php", &src, pos).is_none());
        let (src, pos) = at("$faker->generate('x¦')");
        assert!(reference_at("src/X.php", &src, pos).is_none());
    }
}
