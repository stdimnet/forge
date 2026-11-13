//! Twig: templates, the tags/filters/functions/tests available in templates (Twig core,
//! the Symfony bridge and the project's own extensions) and which controllers render which
//! template.

use std::path::Path;
use std::sync::OnceLock;

use phpforge_lang_php::SymbolKind;
use regex::Regex;
use serde::{Deserialize, Serialize};
use yaml_rust2::YamlLoader;

use crate::{line_col, rel_path, skip_dir, Location, PhpFile};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Template {
    /// Logical name: `order/show.html.twig` or `@email/welcome.html.twig`.
    pub name: String,
    /// File path relative to the project root.
    pub path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TwigSymbolKind {
    Tag,
    Filter,
    Function,
    Test,
    Variable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TwigSymbol {
    pub name: String,
    pub kind: TwigSymbolKind,
    /// `path(name, parameters = [], relative = false)`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub signature: String,
    /// `Twig`, `Symfony` or the extension class.
    pub provider: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<Location>,
}

/// A controller action rendering a template.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderRef {
    pub template: String,
    /// `App\Controller\OrderController::show`.
    pub controller: String,
    /// The `render(...)` call (or `#[Template]` attribute).
    pub location: Location,
}

fn walk_templates(root: &Path, dir: &Path, prefix: &str, out: &mut Vec<Template>) {
    if !dir.is_dir() {
        return;
    }
    for entry in walkdir::WalkDir::new(dir)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|e| !(e.file_type().is_dir() && skip_dir(&e.file_name().to_string_lossy())))
        .flatten()
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy();
        if !name.ends_with(".twig") {
            continue;
        }
        let rel = entry
            .path()
            .strip_prefix(dir)
            .unwrap_or(entry.path())
            .to_string_lossy()
            .replace('\\', "/");
        out.push(Template {
            name: format!("{prefix}{rel}"),
            path: rel_path(root, entry.path()),
        });
    }
}

/// Templates of the default directory plus namespaced paths from `config/packages/twig.yaml`.
pub fn templates(root: &Path, templates_dir: &str) -> Vec<Template> {
    let mut out = Vec::new();
    walk_templates(root, &root.join(templates_dir), "", &mut out);
    if let Ok(text) = std::fs::read_to_string(root.join("config/packages/twig.yaml")) {
        if let Ok(docs) = YamlLoader::load_from_str(&text) {
            let paths = docs
                .first()
                .and_then(|d| d["twig"]["paths"].as_hash().cloned());
            if let Some(paths) = paths {
                for (k, v) in paths {
                    let (Some(dir), ns) = (k.as_str(), v.as_str()) else {
                        continue;
                    };
                    let dir = dir
                        .replace("%kernel.project_dir%/", "")
                        .replace("%kernel.project_dir%", "");
                    let prefix = match ns {
                        Some(ns) if !ns.is_empty() => format!("@{ns}/"),
                        _ => String::new(),
                    };
                    walk_templates(root, &root.join(dir), &prefix, &mut out);
                }
            }
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out.dedup_by(|a, b| a.name == b.name);
    out
}

const TAGS: &[(&str, &str)] = &[
    ("apply", "apply filter"),
    ("autoescape", "autoescape 'html'"),
    ("block", "block name"),
    ("cache", "cache 'key'"),
    ("deprecated", "deprecated 'message'"),
    ("do", "do expression"),
    ("embed", "embed 'template'"),
    ("extends", "extends 'template'"),
    ("flush", "flush"),
    ("for", "for item in items"),
    ("from", "from 'template' import macro"),
    ("guard", "guard function name"),
    ("if", "if condition"),
    ("elseif", "elseif condition"),
    ("else", "else"),
    ("import", "import 'template' as name"),
    ("include", "include 'template'"),
    ("macro", "macro name(args)"),
    ("sandbox", "sandbox"),
    ("set", "set name = value"),
    ("types", "types {name: 'type'}"),
    ("use", "use 'template'"),
    ("verbatim", "verbatim"),
    ("with", "with {key: value}"),
    ("endblock", ""),
    ("endfor", ""),
    ("endif", ""),
    ("endapply", ""),
    ("endembed", ""),
    ("endmacro", ""),
    ("endset", ""),
    ("endwith", ""),
    ("endverbatim", ""),
];

const SYMFONY_TAGS: &[(&str, &str)] = &[
    ("form_theme", "form_theme form 'theme.html.twig'"),
    ("trans", "trans with {params} from 'domain'"),
    ("endtrans", ""),
    ("trans_default_domain", "trans_default_domain 'domain'"),
    ("stopwatch", "stopwatch 'event'"),
    ("endstopwatch", ""),
    ("dump", "dump variable"),
];

const FILTERS: &[(&str, &str)] = &[
    ("abs", "abs"),
    ("batch", "batch(size, fill = null, preserve_keys = true)"),
    ("capitalize", "capitalize"),
    ("column", "column(name, index = null)"),
    ("convert_encoding", "convert_encoding(to, from)"),
    ("country_name", "country_name(locale = null)"),
    ("currency_name", "currency_name(locale = null)"),
    ("data_uri", "data_uri(mime = null, parameters = [])"),
    ("date", "date(format = null, timezone = null)"),
    ("date_modify", "date_modify(modifier)"),
    ("default", "default(value = '')"),
    ("escape", "escape(strategy = 'html', charset = null)"),
    ("e", "e(strategy = 'html')"),
    ("filter", "filter(arrow)"),
    ("find", "find(arrow)"),
    ("first", "first"),
    ("format", "format(...values)"),
    (
        "format_currency",
        "format_currency(currency, attrs = [], locale = null)",
    ),
    (
        "format_date",
        "format_date(dateFormat = 'medium', pattern = '', timezone = null)",
    ),
    (
        "format_datetime",
        "format_datetime(dateFormat = 'medium', timeFormat = 'medium')",
    ),
    (
        "format_number",
        "format_number(attrs = [], style = 'decimal')",
    ),
    ("join", "join(glue = '', and = null)"),
    ("json_encode", "json_encode(options = null)"),
    ("keys", "keys"),
    ("language_name", "language_name(locale = null)"),
    ("last", "last"),
    ("length", "length"),
    ("lower", "lower"),
    ("map", "map(arrow)"),
    ("markdown_to_html", "markdown_to_html"),
    ("merge", "merge(array)"),
    ("nl2br", "nl2br"),
    (
        "number_format",
        "number_format(decimal = 0, decimal_point = '.', thousand_sep = ',')",
    ),
    ("raw", "raw"),
    ("reduce", "reduce(arrow, initial = null)"),
    ("replace", "replace(from)"),
    ("reverse", "reverse(preserve_keys = false)"),
    ("round", "round(precision = 0, method = 'common')"),
    ("shuffle", "shuffle"),
    (
        "slice",
        "slice(start, length = null, preserve_keys = false)",
    ),
    ("slug", "slug(separator = '-', locale = null)"),
    ("sort", "sort(arrow = null)"),
    ("spaceless", "spaceless"),
    ("split", "split(delimiter, limit = null)"),
    ("striptags", "striptags(allowable_tags = null)"),
    ("title", "title"),
    ("trim", "trim(character_mask = null, side = 'both')"),
    ("u", "u"),
    ("upper", "upper"),
    ("url_encode", "url_encode"),
];

const SYMFONY_FILTERS: &[(&str, &str)] = &[
    (
        "trans",
        "trans(arguments = [], domain = null, locale = null)",
    ),
    ("humanize", "humanize"),
    ("yaml_encode", "yaml_encode(inline = 0, dumpObjects = 0)"),
    ("yaml_dump", "yaml_dump(inline = 0)"),
    ("abbr_class", "abbr_class"),
    ("abbr_method", "abbr_method"),
    ("format_args", "format_args"),
    ("file_excerpt", "file_excerpt(line, srcContext = 3)"),
    ("format_file", "format_file(line, text = null)"),
    ("file_link", "file_link(line)"),
    ("sanitize_html", "sanitize_html(sanitizer = null)"),
    ("serialize", "serialize(format = 'json', context = [])"),
];

const FUNCTIONS: &[(&str, &str)] = &[
    ("attribute", "attribute(object, method, arguments = [])"),
    ("block", "block(name, template = null)"),
    ("constant", "constant(name, object = null)"),
    ("cycle", "cycle(values, position)"),
    ("date", "date(date = null, timezone = null)"),
    ("dump", "dump(...vars)"),
    ("enum", "enum(class)"),
    ("enum_cases", "enum_cases(class)"),
    ("html_classes", "html_classes(...classes)"),
    (
        "include",
        "include(template, variables = [], with_context = true)",
    ),
    ("max", "max(...values)"),
    ("min", "min(...values)"),
    ("parent", "parent()"),
    ("random", "random(values = null, max = null)"),
    ("range", "range(low, high, step = 1)"),
    ("source", "source(name, ignore_missing = false)"),
    (
        "template_from_string",
        "template_from_string(template, name = null)",
    ),
];

const SYMFONY_FUNCTIONS: &[(&str, &str)] = &[
    ("path", "path(name, parameters = [], relative = false)"),
    ("url", "url(name, parameters = [], schemeRelative = false)"),
    ("asset", "asset(path, packageName = null)"),
    ("asset_version", "asset_version(path, packageName = null)"),
    ("absolute_url", "absolute_url(path)"),
    ("relative_path", "relative_path(path)"),
    ("csrf_token", "csrf_token(tokenId)"),
    (
        "is_granted",
        "is_granted(role, object = null, field = null)",
    ),
    (
        "is_granted_for_user",
        "is_granted_for_user(user, attribute, subject = null)",
    ),
    ("logout_path", "logout_path(key = null)"),
    ("logout_url", "logout_url(key = null)"),
    (
        "impersonation_exit_path",
        "impersonation_exit_path(exitTo = null)",
    ),
    ("render", "render(uri, options = [])"),
    ("render_esi", "render_esi(uri, options = [])"),
    (
        "controller",
        "controller(controller, attributes = [], query = [])",
    ),
    ("form", "form(view, variables = [])"),
    ("form_start", "form_start(view, variables = [])"),
    ("form_end", "form_end(view, variables = [])"),
    ("form_widget", "form_widget(view, variables = [])"),
    ("form_row", "form_row(view, variables = [])"),
    (
        "form_label",
        "form_label(view, label = null, variables = [])",
    ),
    ("form_errors", "form_errors(view)"),
    ("form_help", "form_help(view)"),
    ("form_rest", "form_rest(view, variables = [])"),
    ("field_name", "field_name(view)"),
    ("field_value", "field_value(view)"),
    ("field_label", "field_label(view)"),
    ("t", "t(message, parameters = [], domain = null)"),
    ("importmap", "importmap(entryPoint = 'app')"),
    (
        "encore_entry_link_tags",
        "encore_entry_link_tags(entryName)",
    ),
    (
        "encore_entry_script_tags",
        "encore_entry_script_tags(entryName)",
    ),
    (
        "workflow_can",
        "workflow_can(subject, transitionName, name = null)",
    ),
    (
        "workflow_transitions",
        "workflow_transitions(subject, name = null)",
    ),
];

const TESTS: &[&str] = &[
    "constant",
    "defined",
    "divisible by",
    "empty",
    "even",
    "iterable",
    "mapping",
    "null",
    "odd",
    "same as",
    "sequence",
    "rootform",
    "selectedchoice",
];

/// Twig core and Symfony bridge tags, filters, functions, tests and the `app` variable.
pub fn builtin_symbols() -> Vec<TwigSymbol> {
    let mut out = Vec::new();
    let mut add = |list: &[(&str, &str)], kind: TwigSymbolKind, provider: &str| {
        for (name, sig) in list {
            out.push(TwigSymbol {
                name: (*name).into(),
                kind,
                signature: (*sig).into(),
                provider: provider.into(),
                location: None,
            });
        }
    };
    add(TAGS, TwigSymbolKind::Tag, "Twig");
    add(SYMFONY_TAGS, TwigSymbolKind::Tag, "Symfony");
    add(FILTERS, TwigSymbolKind::Filter, "Twig");
    add(SYMFONY_FILTERS, TwigSymbolKind::Filter, "Symfony");
    add(FUNCTIONS, TwigSymbolKind::Function, "Twig");
    add(SYMFONY_FUNCTIONS, TwigSymbolKind::Function, "Symfony");
    for t in TESTS {
        out.push(TwigSymbol {
            name: (*t).into(),
            kind: TwigSymbolKind::Test,
            signature: String::new(),
            provider: "Twig".into(),
            location: None,
        });
    }
    out.push(TwigSymbol {
        name: "app".into(),
        kind: TwigSymbolKind::Variable,
        signature:
            "AppVariable (user, request, session, flashes, environment, debug, current_route)"
                .into(),
        provider: "Symfony".into(),
        location: None,
    });
    out
}

fn extension_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r#"(?:new\s+\\?(?:Twig\\)?Twig(Filter|Function|Test)\s*\(\s*|#\[AsTwig(Filter|Function|Test)\s*\(\s*(?:name\s*:\s*)?)['"]([\w. ]+)['"]"#,
        )
        .expect("valid regex")
    })
}

/// Filters, functions and tests registered by the project's Twig extensions.
pub fn project_extensions(php: &[PhpFile]) -> Vec<TwigSymbol> {
    let mut out = Vec::new();
    for f in php {
        if !f.source.contains("Twig") {
            continue;
        }
        for cap in extension_re().captures_iter(&f.source) {
            let kind = match cap.get(1).or_else(|| cap.get(2)).map(|m| m.as_str()) {
                Some("Filter") => TwigSymbolKind::Filter,
                Some("Function") => TwigSymbolKind::Function,
                _ => TwigSymbolKind::Test,
            };
            let m = cap.get(3).expect("name group");
            let (line, column) = line_col(&f.source, m.start());
            let class = f
                .index
                .class_at(m.start())
                .map(|c| c.fqn.clone())
                .unwrap_or_else(|| f.path.clone());
            out.push(TwigSymbol {
                name: m.as_str().to_string(),
                kind,
                signature: String::new(),
                provider: class,
                location: Some(Location::new(&f.path, line, column)),
            });
        }
    }
    out
}

fn render_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r#"(?:->(?:render|renderView|renderBlock|renderBlockView|stream)\s*\(\s*|#\[Template\s*\(\s*(?:template\s*:\s*)?)['"]([^'"]+\.twig)['"]"#,
        )
        .expect("valid regex")
    })
}

/// Template references in controller code: `$this->render('x.html.twig')`, `#[Template]`.
pub fn render_refs_in(file: &PhpFile) -> Vec<RenderRef> {
    let mut out = Vec::new();
    for cap in render_re().captures_iter(&file.source) {
        let m = cap.get(1).expect("template group");
        let Some(class) = file.index.class_at(m.start()) else {
            continue;
        };
        let method = class
            .members
            .iter()
            .filter(|mb| mb.kind == SymbolKind::Method)
            .find(|mb| {
                mb.range.contains(m.start())
                    || (mb.range.start > m.start()
                        && mb.range.start - m.start() < 400
                        && cap.get(0).is_some_and(|c| c.as_str().starts_with("#[")))
            });
        let controller = match method {
            Some(mb) => format!("{}::{}", class.fqn, mb.name),
            None => class.fqn.clone(),
        };
        let (line, column) = line_col(&file.source, m.start());
        out.push(RenderRef {
            template: m.as_str().to_string(),
            controller,
            location: Location::new(&file.path, line, column),
        });
    }
    out
}

pub fn render_refs(php: &[PhpFile]) -> Vec<RenderRef> {
    php.iter().flat_map(render_refs_in).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture;

    #[test]
    fn lists_templates_with_namespaces() {
        let dir = fixture::project();
        fixture::write(
            dir.path(),
            "config/packages/twig.yaml",
            "twig:\n    default_path: '%kernel.project_dir%/templates'\n    paths:\n        '%kernel.project_dir%/emails': email\n",
        );
        fixture::write(dir.path(), "emails/welcome.html.twig", "Hi");
        let t = templates(dir.path(), "templates");
        let names: Vec<&str> = t.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "@email/welcome.html.twig",
                "base.html.twig",
                "order/index.html.twig",
                "order/show.html.twig"
            ]
        );
        assert_eq!(t[3].path, "templates/order/show.html.twig");
    }

    #[test]
    fn project_twig_extensions() {
        let dir = fixture::project();
        let php = crate::index_php_files(dir.path(), "src");
        let ext = project_extensions(&php);
        let price = ext.iter().find(|s| s.name == "price").unwrap();
        assert_eq!(price.kind, TwigSymbolKind::Filter);
        assert_eq!(price.provider, "App\\Twig\\AppExtension");
        assert_eq!(price.location.as_ref().unwrap().line, 13);
        assert!(ext
            .iter()
            .any(|s| s.name == "cart_count" && s.kind == TwigSymbolKind::Function));
    }

    #[test]
    fn controllers_rendering_templates() {
        let dir = fixture::project();
        let php = crate::index_php_files(dir.path(), "src");
        let refs = render_refs(&php);
        let show = refs
            .iter()
            .find(|r| r.template == "order/show.html.twig")
            .unwrap();
        assert_eq!(show.controller, "App\\Controller\\OrderController::show");
        assert_eq!(show.location.line, 22);
    }

    #[test]
    fn builtins_include_symfony_bridge() {
        let all = builtin_symbols();
        assert!(all
            .iter()
            .any(|s| s.name == "path" && s.kind == TwigSymbolKind::Function));
        assert!(all
            .iter()
            .any(|s| s.name == "trans" && s.kind == TwigSymbolKind::Filter));
        assert!(all
            .iter()
            .any(|s| s.name == "form_theme" && s.kind == TwigSymbolKind::Tag));
    }
}
