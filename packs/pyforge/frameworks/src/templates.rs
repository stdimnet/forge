//! Templates of Django, Flask and FastAPI projects: where they are, which names the code
//! and the templates use, and the editor features built on that.
//!
//! - Folders: Django's `TEMPLATES` `DIRS` and each app's `templates/` (`jinja2/` for the
//!   Jinja2 backend), Flask's `templates/` next to the app and blueprint
//!   `template_folder=`s, FastAPI's `Jinja2Templates(directory=...)`, then any other
//!   `templates/` folder.
//! - References: `render(request, "x.html")`, `render_to_string`, `get_template`,
//!   `select_template`, `TemplateResponse`, `render_template`, `template_name = "x.html"`;
//!   `{% extends %}`, `{% include %}`, `{% import %}`, `{% from %}`; URL names in
//!   `reverse()` / `redirect()` / `{% url 'name' %}` and Flask endpoints in `url_for()`.
//! - Go to Declaration, completion of template and URL names, gutter marks (template
//!   references, routed views, the views that render a template) and the unresolved
//!   template / URL name inspections.

use std::collections::BTreeSet;

use regex_lite::Regex;
use serde::{Deserialize, Serialize};

use crate::modules::{join, parent};
use crate::source::{self, Kind, LineIndex};
use crate::{flask, Location, Project, WebProject};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateRoot {
    pub dir: String,
    /// `django`, `flask`, `fastapi` or `""` (a `templates/` folder found by name).
    pub framework: String,
    /// What configured it: `DIRS`, `app shop`, `blueprint admin`, ….
    pub origin: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Template {
    /// The name code uses: `shop/product_detail.html`.
    pub name: String,
    pub path: String,
}

/// Template file extensions.
const TEMPLATE_EXTENSIONS: [&str; 8] = [
    "html", "htm", "txt", "xml", "jinja", "jinja2", "j2", "djhtml",
];

fn is_template_file(path: &str) -> bool {
    path.rsplit_once('.')
        .is_some_and(|(_, ext)| TEMPLATE_EXTENSIONS.contains(&ext))
}

fn has_files_below(project: &Project, dir: &str) -> bool {
    let prefix = format!("{dir}/");
    project.files().any(|(p, _)| p.starts_with(&prefix))
}

/// The template folders, in lookup order.
pub fn roots(project: &Project, info: &WebProject) -> Vec<TemplateRoot> {
    let mut out: Vec<TemplateRoot> = Vec::new();
    let mut add = |dir: String, framework: &str, origin: String| {
        if !out.iter().any(|r| r.dir == dir) {
            out.push(TemplateRoot {
                dir,
                framework: framework.into(),
                origin,
            });
        }
    };
    if let Some(d) = &info.django {
        for dir in &d.template_dirs {
            add(dir.clone(), "django", "DIRS".into());
        }
        if d.app_dirs {
            let folder = if d.jinja2 { "jinja2" } else { "templates" };
            for app in &d.apps {
                if let Some(dir) = &app.dir {
                    let t = join(dir, folder);
                    if has_files_below(project, &t) {
                        add(t, "django", format!("app {}", app.label));
                    }
                }
            }
        }
    }
    if info.flask.is_some() {
        for (dir, _) in flask::template_folders(project) {
            add(dir, "flask", "Flask".into());
        }
    }
    if info.fastapi.is_some() || info.flask.is_some() {
        for (path, _) in project.python() {
            for (_, line, open) in project.assigned_calls(path, &["Jinja2Templates"]) {
                let dir = source::call_args(&line.tokens, open).and_then(|(args, _)| {
                    source::keyword(&args, "directory")
                        .or_else(|| source::positional(&args, 0))
                        .and_then(|a| a.string())
                });
                if let Some(dir) = dir {
                    add(
                        dir.trim_start_matches("./")
                            .trim_end_matches('/')
                            .to_string(),
                        "fastapi",
                        "Jinja2Templates".into(),
                    );
                }
            }
        }
    }
    if !info.frameworks.is_empty() {
        let mut found: BTreeSet<String> = BTreeSet::new();
        for (p, _) in project.files() {
            if !is_template_file(p)
                || p.starts_with(".venv/")
                || p.contains("/site-packages/")
                || p.starts_with("node_modules/")
            {
                continue;
            }
            if let Some(i) = p
                .find("templates/")
                .filter(|&i| i == 0 || p.as_bytes()[i - 1] == b'/')
            {
                found.insert(p[..i + "templates".len()].to_string());
            }
        }
        for dir in found {
            add(dir, "", "templates folder".into());
        }
    }
    out
}

/// Every template below the folders; a name found in an earlier folder hides later ones.
pub fn list(project: &Project, roots: &[TemplateRoot]) -> Vec<Template> {
    let mut out: Vec<Template> = Vec::new();
    let mut names = BTreeSet::new();
    for r in roots {
        let prefix = format!("{}/", r.dir);
        for (p, _) in project.files() {
            let Some(name) = p.strip_prefix(&prefix) else {
                continue;
            };
            if is_template_file(p) && names.insert(name.to_string()) {
                out.push(Template {
                    name: name.to_string(),
                    path: p.to_string(),
                });
            }
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// The files a template name resolves to, the one Django / Jinja2 picks first.
pub fn resolve(project: &Project, name: &str) -> Vec<String> {
    let name = name.trim_start_matches('/');
    project
        .info
        .template_roots
        .iter()
        .map(|r| join(&r.dir, name))
        .filter(|p| project.has(p))
        .collect()
}

// ---- References -------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RefKind {
    Template,
    /// A Django URL name (`shop:cart`).
    Url,
    /// A Flask endpoint (`admin.index`).
    Endpoint,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    pub kind: RefKind,
    pub value: String,
    /// Byte range of the string's content.
    pub start: usize,
    pub end: usize,
}

const TEMPLATE_CALLS: [&str; 7] = [
    "render",
    "render_to_string",
    "get_template",
    "select_template",
    "TemplateResponse",
    "render_template",
    "SimpleTemplateResponse",
];
const URL_CALLS: [&str; 4] = ["reverse", "reverse_lazy", "redirect", "resolve_url"];

fn looks_like_url_name(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | ':' | '.'))
}

/// References in Python source.
pub fn python_refs(text: &str) -> Vec<Reference> {
    let mut out = Vec::new();
    let push = |out: &mut Vec<Reference>, kind, t: &source::Token| {
        out.push(Reference {
            kind,
            value: t.text.clone(),
            start: t.inner.start,
            end: t.inner.end,
        });
    };
    for l in source::lines(text) {
        let t = &l.tokens;
        // template_name = "x.html" (class attributes of generic views)
        if let Some(("template_name", at)) = source::assignment(&l) {
            if let Some(s) = t.get(at).filter(|s| s.is_plain_str()) {
                push(&mut out, RefKind::Template, s);
            }
        }
        for (i, tok) in t.iter().enumerate() {
            if tok.kind != Kind::Name || !t.get(i + 1).is_some_and(|x| x.is_op("(")) {
                continue;
            }
            let Some((args, _)) = source::call_args(t, i + 1) else {
                continue;
            };
            let name = tok.text.as_str();
            if let Some(a) = source::keyword(&args, "template_name") {
                for s in a.tokens.iter().filter(|s| s.is_plain_str()) {
                    push(&mut out, RefKind::Template, s);
                }
            }
            if TEMPLATE_CALLS.contains(&name) {
                let arg = args
                    .iter()
                    .filter(|a| a.keyword.is_none() || a.keyword == Some("name"))
                    .find(|a| {
                        a.string_token().is_some()
                            || (name == "select_template"
                                && a.tokens.first().is_some_and(|x| x.is_op("[")))
                    });
                if let Some(a) = arg {
                    for s in a.tokens.iter().filter(|s| s.is_plain_str()) {
                        push(&mut out, RefKind::Template, s);
                    }
                }
            } else if URL_CALLS.contains(&name) || name == "url_for" {
                let kind = if name == "url_for" {
                    RefKind::Endpoint
                } else {
                    RefKind::Url
                };
                if let Some(s) = source::positional(&args, 0).and_then(|a| a.string_token()) {
                    if looks_like_url_name(&s.text)
                        && !(name == "redirect" && !s.text.contains(':') && s.text.contains('.'))
                    {
                        push(&mut out, kind, s);
                    }
                }
            }
        }
    }
    out
}

fn tag_regex() -> Regex {
    Regex::new(r#"\{%-?\s*(extends|include|import|from|embed|url)\s+(?:"([^"]*)"|'([^']*)')"#)
        .expect("regex")
}

fn url_for_regex() -> Regex {
    Regex::new(r#"url_for\(\s*(?:"([^"]*)"|'([^']*)')"#).expect("regex")
}

/// References in a template.
pub fn template_refs(text: &str) -> Vec<Reference> {
    let mut out = Vec::new();
    for c in tag_regex().captures_iter(text) {
        let m = c.get(2).or_else(|| c.get(3)).expect("group");
        let kind = if &c[1] == "url" {
            RefKind::Url
        } else {
            RefKind::Template
        };
        out.push(Reference {
            kind,
            value: m.as_str().to_string(),
            start: m.start(),
            end: m.end(),
        });
    }
    for c in url_for_regex().captures_iter(text) {
        let m = c.get(1).or_else(|| c.get(2)).expect("group");
        out.push(Reference {
            kind: RefKind::Endpoint,
            value: m.as_str().to_string(),
            start: m.start(),
            end: m.end(),
        });
    }
    out.sort_by_key(|r| r.start);
    out
}

pub fn is_python(path: &str) -> bool {
    path.ends_with(".py")
}

fn refs_of(path: &str, text: &str) -> Vec<Reference> {
    if is_python(path) {
        python_refs(text)
    } else if is_template_file(path) {
        template_refs(text)
    } else {
        Vec::new()
    }
}

/// Where a URL name or endpoint is defined.
fn url_targets(project: &Project, kind: RefKind, value: &str) -> Vec<Location> {
    let framework = if kind == RefKind::Url {
        "django"
    } else {
        "flask"
    };
    project
        .info
        .routes
        .iter()
        .filter(|r| r.framework == framework)
        .filter(|r| match &r.name {
            Some(n) => {
                n == value
                    || (kind == RefKind::Endpoint && value.starts_with('.') && n.ends_with(value))
            }
            None => false,
        })
        .map(|r| r.location.clone())
        .collect()
}

fn targets(project: &Project, r: &Reference) -> Vec<Location> {
    match r.kind {
        RefKind::Template => resolve(project, &r.value)
            .into_iter()
            .map(|path| Location {
                path,
                line: 0,
                character: 0,
            })
            .collect(),
        kind => url_targets(project, kind, &r.value),
    }
}

/// Go to Declaration at UTF-16 `offset` of the editor's `text`.
pub fn goto(project: &Project, path: &str, text: &str, offset: usize) -> Vec<Location> {
    let byte = source::byte_offset(text, offset);
    refs_of(path, text)
        .iter()
        .find(|r| r.start <= byte && byte <= r.end)
        .map(|r| targets(project, r))
        .unwrap_or_default()
}

// ---- Completion -------------------------------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionItem {
    pub label: String,
    /// `template` or `url`.
    pub kind: String,
    pub detail: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Completions {
    /// UTF-16 range of the string content that the item replaces.
    pub from: usize,
    pub to: usize,
    pub items: Vec<CompletionItem>,
}

/// What a string before the caret is: (kind, UTF-16 start of its content).
fn completion_context(path: &str, before: &str) -> Option<(RefKind, usize)> {
    let python = [
        (
            RefKind::Template,
            r#"(?:\b(?:render|render_to_string|get_template|select_template|TemplateResponse|SimpleTemplateResponse|render_template)\s*\([^"']*|\btemplate_name\s*=\s*\[?\s*)["']([^"']*)$"#,
        ),
        (
            RefKind::Url,
            r#"\b(?:reverse|reverse_lazy|redirect|resolve_url)\s*\(\s*["']([^"']*)$"#,
        ),
        (RefKind::Endpoint, r#"\burl_for\s*\(\s*["']([^"']*)$"#),
    ];
    let template = [
        (
            RefKind::Template,
            r#"\{%-?\s*(?:extends|include|import|from|embed)\s+["']([^"']*)$"#,
        ),
        (RefKind::Url, r#"\{%-?\s*url\s+["']([^"']*)$"#),
        (RefKind::Endpoint, r#"\burl_for\s*\(\s*["']([^"']*)$"#),
    ];
    let patterns: &[(RefKind, &str)] = if is_python(path) { &python } else { &template };
    for (kind, p) in patterns {
        if let Some(c) = Regex::new(p).expect("regex").captures(before) {
            let m = c.get(1).expect("group");
            return Some((*kind, m.start()));
        }
    }
    None
}

/// Template names, URL names or endpoints for the string at UTF-16 `offset`.
pub fn complete(project: &Project, path: &str, text: &str, offset: usize) -> Option<Completions> {
    let byte = source::byte_offset(text, offset);
    let line_start = text[..byte].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let before = &text[line_start..byte];
    let (kind, start) = completion_context(path, before)?;
    let rest = &text[byte..];
    let word_end = rest.find(['"', '\'', '\n']).unwrap_or(rest.len());
    let from = source::utf16_offset(text, line_start + start);
    let to = source::utf16_offset(text, byte + word_end);
    let items: Vec<CompletionItem> = match kind {
        RefKind::Template => {
            let own = project
                .info
                .templates
                .iter()
                .find(|t| t.path == path)
                .map(|t| t.name.clone());
            project
                .info
                .templates
                .iter()
                .filter(|t| Some(&t.name) != own.as_ref())
                .map(|t| CompletionItem {
                    label: t.name.clone(),
                    kind: "template".into(),
                    detail: parent(&t.path).to_string(),
                })
                .collect()
        }
        _ => {
            let framework = if kind == RefKind::Url {
                "django"
            } else {
                "flask"
            };
            let mut seen = BTreeSet::new();
            project
                .info
                .routes
                .iter()
                .filter(|r| r.framework == framework)
                .filter_map(|r| r.name.as_ref().map(|n| (n, r)))
                .filter(|(n, _)| seen.insert(n.to_string()))
                .map(|(n, r)| CompletionItem {
                    label: n.clone(),
                    kind: "url".into(),
                    detail: r.path.clone(),
                })
                .collect()
        }
    };
    (!items.is_empty()).then_some(Completions { from, to, items })
}

// ---- Gutter marks and inspections --------------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GutterMark {
    pub line: u32,
    /// `template` (a reference to a template), `route` (a routed view or endpoint),
    /// `view` (the views that render this template), `task` (a Celery task).
    pub kind: String,
    pub tooltip: String,
    pub targets: Vec<Location>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    pub line: u32,
    /// UTF-16 columns.
    pub start: u32,
    pub end: u32,
    pub message: String,
    pub severity: String,
    pub code: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileMarks {
    pub gutter: Vec<GutterMark>,
    pub problems: Vec<Problem>,
}

/// The Python references to `template` in the project (for the template's own gutter).
fn renderers(project: &Project, template: &Template) -> Vec<Location> {
    let mut out = Vec::new();
    for (path, text) in project.files() {
        if !is_python(path) && !is_template_file(path) {
            continue;
        }
        if !text.contains(template.name.as_str()) {
            continue;
        }
        let index = LineIndex::new(text);
        for r in refs_of(path, text) {
            if r.kind == RefKind::Template
                && r.value.trim_start_matches('/') == template.name
                && path != template.path
            {
                let (line, character) = index.position(r.start);
                out.push(Location {
                    path: path.to_string(),
                    line,
                    character,
                });
            }
        }
    }
    out
}

/// Gutter marks and problems of a file whose editor text is `text`.
pub fn marks(project: &Project, path: &str, text: &str) -> FileMarks {
    let mut out = FileMarks::default();
    let index = LineIndex::new(text);
    let has_roots = !project.info.template_roots.is_empty();
    let django_routes = project.info.routes.iter().any(|r| r.framework == "django");
    let flask_routes = project.info.routes.iter().any(|r| r.framework == "flask");
    for r in refs_of(path, text) {
        let (line, start) = index.position(r.start);
        let (_, end) = index.position(r.end);
        let found = targets(project, &r);
        match r.kind {
            RefKind::Template if !found.is_empty() => out.gutter.push(GutterMark {
                line,
                kind: "template".into(),
                tooltip: format!("Go to template '{}'", r.value),
                targets: found,
            }),
            RefKind::Template if has_roots && !r.value.is_empty() && !r.value.contains("{{") => {
                out.problems.push(Problem {
                    line,
                    start,
                    end,
                    message: format!("Template file '{}' not found", r.value),
                    severity: "warning".into(),
                    code: "unresolved-template".into(),
                })
            }
            RefKind::Url | RefKind::Endpoint if found.is_empty() => {
                let known = if r.kind == RefKind::Url {
                    django_routes
                } else {
                    flask_routes
                };
                // `.index` is relative to the current blueprint; `static` is built in.
                if known
                    && !(r.kind == RefKind::Endpoint
                        && (r.value.starts_with('.') || r.value == "static"))
                {
                    out.problems.push(Problem {
                        line,
                        start,
                        end,
                        message: format!("Cannot resolve URL name '{}'", r.value),
                        severity: "warning".into(),
                        code: "unresolved-url".into(),
                    });
                }
            }
            _ => {}
        }
    }
    if is_python(path) {
        // Routed views: the URL pattern or decorator they are served by.
        for route in &project.info.routes {
            let Some(h) = route.handler_location.as_ref().filter(|h| h.path == path) else {
                continue;
            };
            let methods = if route.methods.is_empty() {
                String::new()
            } else {
                format!("{} ", route.methods.join(", "))
            };
            if let Some(m) = out
                .gutter
                .iter_mut()
                .find(|m| m.kind == "route" && m.line == h.line)
            {
                m.tooltip.push_str(&format!("\n{methods}{}", route.path));
                m.targets.push(route.location.clone());
                continue;
            }
            out.gutter.push(GutterMark {
                line: h.line,
                kind: "route".into(),
                tooltip: format!("{methods}{}", route.path),
                targets: vec![route.location.clone()],
            });
        }
        for task in project
            .info
            .tasks
            .iter()
            .filter(|t| t.location.path == path)
        {
            out.gutter.push(GutterMark {
                line: task.location.line,
                kind: "task".into(),
                tooltip: format!("Celery task '{}'", task.name),
                targets: vec![task.location.clone()],
            });
        }
    } else if let Some(t) = project.info.templates.iter().find(|t| t.path == path) {
        let from = renderers(project, t);
        if !from.is_empty() {
            out.gutter.push(GutterMark {
                line: 0,
                kind: "view".into(),
                tooltip: format!(
                    "Rendered by {} place{}",
                    from.len(),
                    if from.len() == 1 { "" } else { "s" }
                ),
                targets: from,
            });
        }
    }
    out.gutter.sort_by_key(|m| m.line);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::project;
    use crate::Project;

    fn shop() -> Project {
        project(&[
            ("requirements.txt", "django>=5\n"),
            ("manage.py", "os.environ.setdefault('DJANGO_SETTINGS_MODULE', 'mysite.settings')\n"),
            ("mysite/__init__.py", ""),
            ("mysite/settings.py", include_str!("../tests/fixtures/django/settings.py")),
            ("mysite/urls.py", include_str!("../tests/fixtures/django/urls.py")),
            ("shop/__init__.py", ""),
            ("shop/urls.py", include_str!("../tests/fixtures/django/shop_urls.py")),
            ("shop/views.py", include_str!("../tests/fixtures/django/views.py")),
            ("templates/base.html", "<html>{% block content %}{% endblock %}</html>\n"),
            ("templates/home.html", "{% extends \"base.html\" %}\n{% block content %}<a href=\"{% url 'shop:cart' %}\">Cart</a>{% endblock %}\n"),
            ("shop/templates/shop/product_detail.html", "{% extends 'base.html' %}\n{% include 'shop/missing.html' %}\n<a href=\"{% url 'shop:nope' %}\"></a>\n"),
            ("shop/templates/shop/product_list.html", "{% extends 'base.html' %}\n"),
            ("shop/templates/shop/cart.html", ""),
        ])
    }

    #[test]
    fn roots_and_names() {
        let p = shop();
        let roots: Vec<(&str, &str)> = p
            .info
            .template_roots
            .iter()
            .map(|r| (r.dir.as_str(), r.origin.as_str()))
            .collect();
        assert_eq!(
            roots,
            [("templates", "DIRS"), ("shop/templates", "app shop")]
        );
        let names: Vec<&str> = p.info.templates.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "base.html",
                "home.html",
                "shop/cart.html",
                "shop/product_detail.html",
                "shop/product_list.html"
            ]
        );
        assert_eq!(
            resolve(&p, "shop/cart.html"),
            ["shop/templates/shop/cart.html"]
        );
    }

    #[test]
    fn references() {
        let refs = python_refs(include_str!("../tests/fixtures/django/views.py"));
        let v: Vec<(RefKind, &str)> = refs.iter().map(|r| (r.kind, r.value.as_str())).collect();
        assert_eq!(
            v,
            [
                (RefKind::Template, "home.html"),
                (RefKind::Template, "shop/product_list.html"),
                (RefKind::Template, "shop/product_detail.html"),
                (RefKind::Url, "shop:product-list"),
                (RefKind::Template, "shop/cart.html"),
                (RefKind::Template, "shop/archive.html"),
            ]
        );
        let t = template_refs("{% extends 'a.html' %}{%- include \"b.html\" -%}{% url 'x:y' 1 %}{{ url_for('admin.index') }}");
        let v: Vec<(RefKind, &str)> = t.iter().map(|r| (r.kind, r.value.as_str())).collect();
        assert_eq!(
            v,
            [
                (RefKind::Template, "a.html"),
                (RefKind::Template, "b.html"),
                (RefKind::Url, "x:y"),
                (RefKind::Endpoint, "admin.index")
            ]
        );
    }

    #[test]
    fn goto_templates_and_urls() {
        let p = shop();
        let views = p.file("shop/views.py").unwrap();
        let at = source::utf16_offset(views, views.find("shop/product_detail.html").unwrap() + 3);
        assert_eq!(
            goto(&p, "shop/views.py", views, at)[0].path,
            "shop/templates/shop/product_detail.html"
        );
        let home = p.file("templates/home.html").unwrap();
        let at = source::utf16_offset(home, home.find("base.html").unwrap());
        assert_eq!(
            goto(&p, "templates/home.html", home, at)[0].path,
            "templates/base.html"
        );
        let at = source::utf16_offset(home, home.find("shop:cart").unwrap() + 2);
        let t = goto(&p, "templates/home.html", home, at);
        assert_eq!((t[0].path.as_str(), t[0].line), ("shop/urls.py", 9));
    }

    #[test]
    fn completion() {
        let p = shop();
        let text = "def v(request):\n    return render(request, \"shop/\")\n";
        let at = text.find("shop/\"").unwrap() + 5;
        let c = complete(&p, "shop/views.py", text, at).unwrap();
        assert_eq!(c.from, text.find("shop/\"").unwrap());
        assert_eq!(c.to, at);
        assert!(c
            .items
            .iter()
            .any(|i| i.label == "shop/cart.html" && i.kind == "template"));
        let text = "{% url 'sh";
        let c = complete(&p, "templates/x.html", text, text.len()).unwrap();
        assert_eq!(c.from, 8);
        let labels: Vec<&str> = c.items.iter().map(|i| i.label.as_str()).collect();
        assert!(labels.contains(&"shop:product-detail"));
        assert!(labels.contains(&"home"));
        assert!(complete(&p, "templates/x.html", "{% if x %}", 9).is_none());
    }

    #[test]
    fn gutter_and_problems() {
        let p = shop();
        let views = p.file("shop/views.py").unwrap();
        let m = marks(&p, "shop/views.py", views);
        let kinds: Vec<(u32, &str)> = m.gutter.iter().map(|g| (g.line, g.kind.as_str())).collect();
        assert!(kinds.contains(&(7, "route")));
        assert!(kinds.contains(&(8, "template")));
        assert_eq!(m.problems.len(), 1, "{:?}", m.problems);
        assert_eq!(
            m.problems[0].message,
            "Template file 'shop/archive.html' not found"
        );
        assert_eq!(m.problems[0].code, "unresolved-template");
        let detail = p.file("shop/templates/shop/product_detail.html").unwrap();
        let m = marks(&p, "shop/templates/shop/product_detail.html", detail);
        let msgs: Vec<&str> = m.problems.iter().map(|x| x.message.as_str()).collect();
        assert_eq!(
            msgs,
            [
                "Template file 'shop/missing.html' not found",
                "Cannot resolve URL name 'shop:nope'"
            ]
        );
        let view = m.gutter.iter().find(|g| g.kind == "view").unwrap();
        assert_eq!(
            (view.line, view.targets[0].path.as_str()),
            (0, "shop/views.py")
        );
    }
}
