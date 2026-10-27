//! PyForge web frameworks (PyCharm Professional's Django, Flask, FastAPI and Celery
//! support), read statically from the project's sources: nothing of the project runs.
//!
//! - [`source`]: a forgiving Python scanner (logical lines, strings, calls, decorators).
//! - [`modules`]: module names, files and imports, to follow `views.index` to its `def`.
//! - [`detect`]: which frameworks a project uses (requirements, pyproject.toml, Pipfile,
//!   `manage.py`, imports) and their entry points (settings module, `FLASK_APP`, the
//!   uvicorn app, the Celery app).
//! - [`django`]: settings (`INSTALLED_APPS`, `ROOT_URLCONF`, `TEMPLATES`), URL patterns
//!   (`path` / `re_path` / `include` with namespaces), models and fields, `manage.py`
//!   commands.
//! - [`flask`], [`fastapi`]: routes of `@app.route`, blueprints, `@app.get` and
//!   `APIRouter` with their prefixes.
//! - [`celery`]: the Celery app and its tasks.
//! - [`templates`]: template folders, references (`render`, `render_template`,
//!   `{% extends %}`, `{% include %}`, `{% url %}`) with Go to Declaration, completion,
//!   gutter marks and the unresolved-template inspection.
//! - [`run`]: the command lines of the Django Server, manage.py, Flask Server, FastAPI and
//!   Celery run configurations.
//!
//! Everything is pure (files in, data out) and also builds to WebAssembly
//! (`crates/forge-wasm/src/python_web.rs`).

pub mod celery;
pub mod detect;
pub mod django;
pub mod fastapi;
pub mod flask;
pub mod modules;
pub mod run;
pub mod source;
pub mod templates;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::modules::Imported;
use crate::source::{Kind, Line, LineIndex};

/// A position in a project file (0-based line, UTF-16 column).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Location {
    pub path: String,
    pub line: u32,
    pub character: u32,
}

/// Settings that override what is detected (Settings | Languages & Frameworks).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Options {
    /// `django.settingsModule`: `DJANGO_SETTINGS_MODULE`.
    pub settings_module: String,
    /// `django.manageScript`: the project's `manage.py`.
    pub manage_script: String,
    /// `flask.app`: `FLASK_APP`.
    pub flask_app: String,
    /// `fastapi.app`: the uvicorn application (`module:attribute`).
    pub fastapi_app: String,
    /// `celery.app`: the `-A` application.
    pub celery_app: String,
}

/// One HTTP endpoint: a Django URL pattern, a Flask route or a FastAPI path operation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Route {
    /// `django`, `flask` or `fastapi`.
    pub framework: String,
    /// HTTP methods; empty for any method (Django), `WS` for FastAPI websockets.
    pub methods: Vec<String>,
    /// The full path with the framework's parameters: `/shop/products/<int:pk>/`,
    /// `/orders/{order_id}`.
    pub path: String,
    /// The URL name (`shop:product-detail`) or endpoint (`admin.index`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The view as written, qualified when it resolves: `shop.views.product_detail`.
    pub handler: String,
    /// Where the pattern or decorator is.
    pub location: Location,
    /// The view function or class.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handler_location: Option<Location>,
    /// The path with sample values for its parameters (`/orders/1`), for the HTTP client.
    pub sample_path: String,
}

/// What the Endpoints and Django Structure tool windows show.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebProject {
    /// `django`, `flask`, `fastapi`, `celery`: the frameworks the project uses.
    pub frameworks: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub django: Option<django::Django>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flask: Option<detect::AppInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fastapi: Option<detect::AppInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub celery: Option<detect::AppInfo>,
    pub routes: Vec<Route>,
    pub models: Vec<django::Model>,
    pub tasks: Vec<celery::Task>,
    pub template_roots: Vec<templates::TemplateRoot>,
    pub templates: Vec<templates::Template>,
}

/// The project's Python, template and dependency files with their analysis.
#[derive(Debug, Clone, Default)]
pub struct Project {
    files: BTreeMap<String, String>,
    py: BTreeMap<String, Vec<Line>>,
    roots: Vec<String>,
    pub options: Options,
    pub info: WebProject,
}

/// File extensions the analysis reads.
pub const EXTENSIONS: &[&str] = &[
    "py", "html", "htm", "txt", "jinja", "jinja2", "j2", "djhtml", "xml", "toml", "cfg", "ini",
];

/// File names without those extensions that the analysis reads.
pub const FILE_NAMES: &[&str] = &["Pipfile"];

impl Project {
    /// Analyse `files` (workspace-relative path → text).
    pub fn new(files: BTreeMap<String, String>, options: Options) -> Self {
        let py = files
            .iter()
            .filter(|(p, _)| p.ends_with(".py"))
            .map(|(p, t)| (p.clone(), source::lines(t)))
            .collect();
        let paths: Vec<&str> = files.keys().map(String::as_str).collect();
        let roots = modules::source_roots(&paths);
        let mut p = Project {
            files,
            py,
            roots,
            options,
            info: WebProject::default(),
        };
        p.info = p.analyse();
        p
    }

    fn analyse(&self) -> WebProject {
        let mut info = WebProject {
            frameworks: detect::frameworks(self),
            ..Default::default()
        };
        if info.frameworks.iter().any(|f| f == "django") {
            info.django = django::analyse(self);
        }
        if info.frameworks.iter().any(|f| f == "flask") {
            info.flask = Some(detect::flask_app(self));
        }
        if info.frameworks.iter().any(|f| f == "fastapi") {
            info.fastapi = Some(detect::fastapi_app(self));
        }
        if info.frameworks.iter().any(|f| f == "celery") {
            info.celery = Some(detect::celery_app(self));
            info.tasks = celery::tasks(self);
        }
        if let Some(d) = &info.django {
            info.routes.extend(django::routes(self, d));
            info.models = django::models(self);
        }
        if info.flask.is_some() {
            info.routes.extend(flask::routes(self));
        }
        if info.fastapi.is_some() {
            info.routes.extend(fastapi::routes(self));
        }
        info.template_roots = templates::roots(self, &info);
        info.templates = templates::list(self, &info.template_roots);
        info
    }

    pub fn files(&self) -> impl Iterator<Item = (&str, &str)> {
        self.files.iter().map(|(p, t)| (p.as_str(), t.as_str()))
    }

    pub fn file(&self, path: &str) -> Option<&str> {
        self.files.get(path).map(String::as_str)
    }

    pub fn has(&self, path: &str) -> bool {
        self.files.contains_key(path)
    }

    /// Parsed Python files.
    pub fn python(&self) -> impl Iterator<Item = (&str, &[Line])> {
        self.py.iter().map(|(p, l)| (p.as_str(), l.as_slice()))
    }

    pub fn lines(&self, path: &str) -> Option<&[Line]> {
        self.py.get(path).map(Vec::as_slice)
    }

    pub fn roots(&self) -> &[String] {
        &self.roots
    }

    pub fn module_of(&self, path: &str) -> String {
        modules::module_of(path, &self.roots)
    }

    pub fn module_file(&self, module: &str) -> Option<String> {
        modules::file_of(module, &self.roots, |p| self.files.contains_key(p))
    }

    pub fn imports(&self, path: &str) -> BTreeMap<String, Imported> {
        let Some(lines) = self.lines(path) else {
            return BTreeMap::new();
        };
        modules::imports(lines, &self.module_of(path), path.ends_with("__init__.py"))
    }

    /// The location of byte `offset` in `path`.
    pub fn locate(&self, path: &str, offset: usize) -> Location {
        let text = self.file(path).unwrap_or("");
        let (line, character) = LineIndex::new(text).position(offset);
        Location {
            path: path.to_string(),
            line,
            character,
        }
    }

    /// `def` / `class` `names[0]` (then `names[1]` inside it, …) in a module file.
    pub fn find_definition(&self, path: &str, names: &[&str]) -> Option<Location> {
        let lines = self.lines(path)?;
        let mut indent: Option<usize> = None;
        let mut from = 0;
        let mut found = None;
        for name in names {
            found = None;
            for (i, l) in lines.iter().enumerate().skip(from) {
                if let Some(min) = indent {
                    if l.indent <= min {
                        break;
                    }
                }
                if let Some((_, n)) = source::definition(l) {
                    if n.text == *name && indent.is_none_or(|m| l.indent > m) {
                        found = Some((i, l.indent, n.start));
                        break;
                    }
                }
            }
            let (i, ind, _) = found?;
            indent = Some(ind);
            from = i + 1;
        }
        found.map(|(_, _, offset)| self.locate(path, offset))
    }

    /// Where a dotted reference written in `path` (`views.index`, `ProductList.as_view`,
    /// `shop.views.detail`) is defined, and its qualified name.
    pub fn resolve(&self, path: &str, dotted: &str) -> (String, Option<Location>) {
        let mut parts: Vec<&str> = dotted.split('.').collect();
        if parts.last() == Some(&"as_view") {
            parts.pop();
        }
        if parts.is_empty() {
            return (dotted.to_string(), None);
        }
        let imports = self.imports(path);
        let (module, rest): (String, Vec<&str>) = match imports.get(parts[0]) {
            Some(Imported {
                module,
                attr: Some(a),
            }) => {
                let sub = format!("{module}.{a}");
                if self.module_file(&sub).is_some() {
                    (sub, parts[1..].to_vec())
                } else {
                    let mut rest = vec![a.as_str()];
                    rest.extend(&parts[1..]);
                    (module.clone(), rest)
                }
            }
            Some(Imported { module, attr: None }) => {
                // `import shop.views` binds `shop`; walk down to the longest module.
                let mut m = module.clone();
                let mut i = 1;
                while i < parts.len() && self.module_file(&format!("{m}.{}", parts[i])).is_some() {
                    m = format!("{m}.{}", parts[i]);
                    i += 1;
                }
                (m, parts[i..].to_vec())
            }
            None => (self.module_of(path), parts.clone()),
        };
        if self.module_file(&module).is_none() {
            // An installed package: keep the reference as written.
            return (dotted.to_string(), None);
        }
        let qualified = if rest.is_empty() {
            module.clone()
        } else {
            format!("{module}.{}", rest.join("."))
        };
        let location = self.module_file(&module).and_then(|file| {
            if rest.is_empty() {
                Some(Location {
                    path: file,
                    line: 0,
                    character: 0,
                })
            } else {
                self.find_definition(&file, &rest)
            }
        });
        (qualified, location)
    }

    /// The module-level variable a dotted reference written in `path` names: `bp`,
    /// `admin.bp`, `routers.orders.router` → (its file, its name).
    pub fn resolve_variable(&self, path: &str, dotted: &str) -> Option<(String, String)> {
        let parts: Vec<&str> = dotted.split('.').collect();
        let (var, head) = parts.split_last()?;
        if head.is_empty() {
            return match self.imports(path).get(*var) {
                Some(Imported {
                    module,
                    attr: Some(a),
                }) => {
                    if self.module_file(&format!("{module}.{a}")).is_some() {
                        None
                    } else {
                        self.module_file(module).map(|f| (f, a.clone()))
                    }
                }
                Some(Imported { attr: None, .. }) => None,
                None => Some((path.to_string(), var.to_string())),
            };
        }
        let imports = self.imports(path);
        let mut module = match imports.get(head[0]) {
            Some(Imported {
                module,
                attr: Some(a),
            }) => format!("{module}.{a}"),
            Some(Imported { module, attr: None }) => module.clone(),
            None => head[0].to_string(),
        };
        for p in &head[1..] {
            module = format!("{module}.{p}");
        }
        self.module_file(&module).map(|f| (f, var.to_string()))
    }

    /// Names assigned at module level from a call: `app = Flask(__name__)` → (`app`, line).
    pub fn assigned_calls<'a>(
        &'a self,
        path: &str,
        callees: &[&str],
    ) -> Vec<(&'a str, &'a Line, usize)> {
        let Some(lines) = self.lines(path) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for l in lines {
            let Some((name, at)) = source::assignment(l) else {
                continue;
            };
            let t = &l.tokens;
            let Some(open) = t[at..].iter().position(|x| x.is_op("(")).map(|o| o + at) else {
                continue;
            };
            let Some(callee) = source::dotted_exact(&t[at..open]) else {
                continue;
            };
            let last = callee.rsplit('.').next().unwrap_or(&callee);
            if callees.contains(&last) {
                out.push((name, l, open));
            }
        }
        out
    }
}

/// `true` when any token of a line is the name `n`.
pub(crate) fn mentions(line: &Line, n: &str) -> bool {
    line.tokens
        .iter()
        .any(|t| t.kind == Kind::Name && t.text == n)
}

/// Replace path parameters with sample values: `<int:pk>` → `1`, `{order_id}` → `1`,
/// `<slug>` → `slug`, regex groups → their name.
pub fn sample_path(path: &str) -> String {
    let mut out = String::new();
    let mut chars = path.chars().peekable();
    while let Some(c) = chars.next() {
        let close = match c {
            '<' => '>',
            '{' => '}',
            '(' => ')',
            _ => {
                if !matches!(c, '^' | '$') {
                    out.push(c);
                }
                continue;
            }
        };
        let mut inner = String::new();
        let mut depth = 1;
        for d in chars.by_ref() {
            if d == c {
                depth += 1;
            } else if d == close {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            inner.push(d);
        }
        let (conv, name) = match c {
            '(' => {
                let name = inner
                    .strip_prefix("?P<")
                    .and_then(|r| r.split('>').next())
                    .unwrap_or("value");
                let numeric = inner.contains("[0-9]") || inner.contains("\\d");
                (if numeric { "int" } else { "str" }, name.to_string())
            }
            _ => match inner.split_once(':') {
                Some((conv, name)) if c == '<' => (conv, name.to_string()),
                Some((name, conv)) => (conv, name.to_string()),
                None => ("", inner.clone()),
            },
        };
        let int_like = matches!(conv, "int" | "float")
            || name == "pk"
            || name == "id"
            || name.ends_with("_id")
            || name.ends_with("Id");
        out.push_str(if int_like { "1" } else { &name });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn project(files: &[(&str, &str)]) -> Project {
        Project::new(
            files
                .iter()
                .map(|(p, t)| (p.to_string(), t.to_string()))
                .collect(),
            Options::default(),
        )
    }

    #[test]
    fn resolves_references_through_imports() {
        let p = project(&[
            ("shop/__init__.py", ""),
            ("shop/views.py", "def index(request):\n    pass\n\nclass Detail(View):\n    def get(self):\n        pass\n"),
            ("shop/urls.py", "from . import views\nfrom .views import Detail as D\nimport shop.views\n"),
        ]);
        let (q, loc) = p.resolve("shop/urls.py", "views.index");
        assert_eq!(q, "shop.views.index");
        assert_eq!(loc.unwrap().line, 0);
        let (q, loc) = p.resolve("shop/urls.py", "D.as_view");
        assert_eq!(q, "shop.views.Detail");
        assert_eq!(loc.unwrap().line, 3);
        let (_, loc) = p.resolve("shop/urls.py", "shop.views.Detail.get");
        assert_eq!(loc.unwrap().line, 4);
    }

    #[test]
    fn sample_paths() {
        assert_eq!(
            sample_path("/products/<int:pk>/<slug:slug>/"),
            "/products/1/slug/"
        );
        assert_eq!(sample_path("/orders/{order_id}"), "/orders/1");
        assert_eq!(sample_path("/files/{path:path}"), "/files/path");
        assert_eq!(
            sample_path("/^articles/(?P<year>[0-9]{4})/$"),
            "/articles/1/"
        );
    }
}
