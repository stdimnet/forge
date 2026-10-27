//! Django, read from the sources (PyCharm's Django support without running Django):
//!
//! - the settings module: `django.settingsModule`, else `DJANGO_SETTINGS_MODULE` from
//!   `manage.py` (`os.environ.setdefault(...)`), else a `settings.py` next to `urls.py`;
//! - settings: `INSTALLED_APPS`, `ROOT_URLCONF` and `TEMPLATES` (`DIRS` relative to
//!   `BASE_DIR`, `APP_DIRS`, the Jinja2 backend), following `from .base import *`;
//! - URL patterns from `ROOT_URLCONF` down through `include()` with namespaces
//!   (`app_name`, `namespace=`) and route prefixes; views resolved to their `def` / `class`;
//! - models (`class X(models.Model)`) with their fields;
//! - `manage.py` commands: Django's built-in ones, the project's `management/commands`, and
//!   the output of `manage.py help --commands` when Forge may run it.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::modules::{join, parent};
use crate::source::{self, Kind, Line, Token};
use crate::{detect, sample_path, Location, Project, Route};

/// What the Django Structure tool window and the run configurations need.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Django {
    /// `manage.py`, workspace-relative.
    pub manage: String,
    /// The folder of `manage.py` (`""` for the project root): the working directory.
    pub root: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings_module: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings_file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_urlconf: Option<String>,
    /// `INSTALLED_APPS`, project apps first.
    pub apps: Vec<App>,
    /// `TEMPLATES[...]["DIRS"]`, workspace-relative.
    pub template_dirs: Vec<String>,
    /// `APP_DIRS`: templates are looked up in each app's `templates/` folder.
    pub app_dirs: bool,
    /// The Jinja2 backend is configured (app folders are `jinja2/`).
    pub jinja2: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct App {
    /// `shop` (the last part of the package).
    pub label: String,
    /// The entry of `INSTALLED_APPS` (`shop.apps.ShopConfig`).
    pub entry: String,
    /// Its package folder in the project; `None` for installed packages (`django.contrib.admin`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dir: Option<String>,
}

/// A model and its fields.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Model {
    /// The app label (the package of `models.py`).
    pub app: String,
    pub name: String,
    pub bases: Vec<String>,
    pub location: Location,
    pub fields: Vec<Field>,
    /// `class Meta: abstract = True`.
    pub is_abstract: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    pub name: String,
    /// `CharField`, `ForeignKey`, …
    pub kind: String,
    /// The related model of relations (`Category`, `"auth.User"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    pub location: Location,
}

/// A `manage.py` command for Tools | Run manage.py Task….
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManageCommand {
    pub name: String,
    /// `django`, an app label (`[shop]` in `help --commands`), or `project`.
    pub app: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<Location>,
}

/// Django's own commands with PyCharm's descriptions.
pub const BUILTIN_COMMANDS: &[(&str, &str, &str)] = &[
    ("changepassword", "auth", "Change a user's password for django.contrib.auth."),
    ("check", "core", "Checks the entire Django project for potential problems."),
    ("clearsessions", "sessions", "Clean out expired sessions from the database."),
    ("collectstatic", "staticfiles", "Collect static files in a single location."),
    ("compilemessages", "core", "Compiles .po files to .mo files for use with builtin gettext support."),
    ("createcachetable", "core", "Creates the tables needed to use the SQL cache backend."),
    ("createsuperuser", "auth", "Used to create a superuser."),
    ("dbshell", "core", "Runs the command-line client for specified database."),
    ("diffsettings", "core", "Displays differences between the current settings.py and Django's default settings."),
    ("dumpdata", "core", "Output the contents of the database as a fixture of the given format."),
    ("findstatic", "staticfiles", "Finds the absolute paths for the given static file(s)."),
    ("flush", "core", "Removes ALL DATA from the database, including data added during migrations."),
    ("inspectdb", "core", "Introspects the database tables in the given database and outputs a Django model module."),
    ("loaddata", "core", "Installs the named fixture(s) in the database."),
    ("makemessages", "core", "Runs over the entire source tree and pulls out all strings marked for translation."),
    ("makemigrations", "core", "Creates new migration(s) for apps."),
    ("migrate", "core", "Updates database schema. Manages both apps with migrations and those without."),
    ("optimizemigration", "core", "Optimizes the operations for the named migration."),
    ("remove_stale_contenttypes", "contenttypes", "Deletes stale content types in the database."),
    ("runserver", "core", "Starts a lightweight web server for development."),
    ("sendtestemail", "core", "Sends a test email to the email addresses specified as arguments."),
    ("shell", "core", "Runs a Python interactive interpreter."),
    ("showmigrations", "core", "Shows all available migrations for the current project."),
    ("sqlflush", "core", "Returns a list of the SQL statements required to return all tables to their initial state."),
    ("sqlmigrate", "core", "Prints the SQL statements for the named migration."),
    ("sqlsequencereset", "core", "Prints the SQL statements for resetting sequences for the given app name(s)."),
    ("squashmigrations", "core", "Squashes an existing set of migrations into a single new one."),
    ("startapp", "core", "Creates a Django app directory structure for the given app name."),
    ("startproject", "core", "Creates a Django project directory structure for the given project name."),
    ("test", "core", "Discover and run tests in the specified modules or the current directory."),
    ("testserver", "core", "Runs a development server with data from the given fixture(s)."),
];

/// Settings modules read for a project.
const MAX_SETTINGS_FILES: usize = 6;

pub fn analyse(project: &Project) -> Option<Django> {
    let manage = detect::manage_scripts(project).into_iter().next()?;
    let root = parent(&manage).to_string();
    let settings_module = settings_module(project, &manage);
    let settings_file = settings_module
        .as_deref()
        .and_then(|m| project.module_file(m));
    let mut d = Django {
        manage,
        root,
        settings_module,
        settings_file: settings_file.clone(),
        app_dirs: true,
        ..Default::default()
    };
    if let Some(file) = &settings_file {
        read_settings(project, file, &mut d);
    }
    Some(d)
}

/// `DJANGO_SETTINGS_MODULE`: the setting, `manage.py`, or a `settings.py` next to `urls.py`.
fn settings_module(project: &Project, manage: &str) -> Option<String> {
    if !project.options.settings_module.trim().is_empty() {
        return Some(project.options.settings_module.trim().to_string());
    }
    if let Some(lines) = project.lines(manage) {
        for l in lines {
            let t = &l.tokens;
            for (i, tok) in t.iter().enumerate() {
                if tok.is_plain_str() && tok.text == "DJANGO_SETTINGS_MODULE" {
                    // os.environ.setdefault("DJANGO_SETTINGS_MODULE", "x") or os.environ[...] = "x"
                    if let Some(value) = t[i + 1..].iter().find(|x| x.is_plain_str()) {
                        return Some(value.text.clone());
                    }
                }
            }
        }
    }
    let mut candidates: Vec<&str> = project
        .files()
        .map(|(p, _)| p)
        .filter(|p| p.ends_with("/settings.py") || p.ends_with("/settings/__init__.py"))
        .collect();
    candidates.sort_by_key(|p| {
        (
            !project.has(&join(parent(p.trim_end_matches("/__init__.py")), "urls.py")),
            p.matches('/').count(),
        )
    });
    candidates.first().map(|p| project.module_of(p))
}

/// Read the settings that matter from a settings file and the modules it star-imports.
fn read_settings(project: &Project, file: &str, d: &mut Django) {
    let mut queue = vec![file.to_string()];
    let mut seen = BTreeSet::new();
    let mut base_dir: Option<String> = None;
    while let Some(path) = queue.pop() {
        if !seen.insert(path.clone()) || seen.len() > MAX_SETTINGS_FILES {
            continue;
        }
        let Some(lines) = project.lines(&path) else {
            continue;
        };
        let imports_base = project.module_of(&path);
        for l in lines {
            let t = &l.tokens;
            // from .base import *
            if t.first().is_some_and(|x| x.is_name("from"))
                && t.last().is_some_and(|x| x.is_op("*"))
            {
                let map = crate::modules::imports(
                    &[Line {
                        indent: 0,
                        tokens: replace_star(t),
                    }],
                    &imports_base,
                    path.ends_with("__init__.py"),
                );
                if let Some(imp) = map.get("__star__") {
                    if let Some(f) = project.module_file(&imp.module) {
                        queue.push(f);
                    }
                }
                continue;
            }
            let Some((name, at)) = source::assignment(l) else {
                continue;
            };
            let value = &t[at..];
            let append = t[1].is_op("+=");
            match name {
                "BASE_DIR" => base_dir = Some(base_dir_of(&path, value)),
                "ROOT_URLCONF" => {
                    if let Some(s) = value.first().filter(|x| x.is_plain_str()) {
                        d.root_urlconf = Some(s.text.clone());
                    }
                }
                "INSTALLED_APPS" => {
                    if !append {
                        d.apps.clear();
                    }
                    for s in value.iter().filter(|x| x.is_plain_str()) {
                        d.apps.push(app(project, &s.text));
                    }
                }
                "TEMPLATES" => {
                    let base = base_dir.clone().unwrap_or_else(|| d.root.clone());
                    read_templates(value, &base, d);
                }
                _ => {}
            }
        }
    }
    // Project apps first, in INSTALLED_APPS order.
    d.apps.sort_by_key(|a| a.dir.is_none());
}

/// `from .base import *` with `*` replaced by a name the import scanner keeps.
fn replace_star(t: &[Token]) -> Vec<Token> {
    let mut v = t.to_vec();
    if let Some(last) = v.last_mut() {
        last.kind = Kind::Name;
        last.text = "__star__".into();
    }
    v
}

/// `Path(__file__).resolve().parent.parent` or `os.path.dirname(os.path.dirname(...))`:
/// the folder that many levels above the settings file.
fn base_dir_of(settings: &str, value: &[Token]) -> String {
    let levels = value
        .iter()
        .filter(|t| t.is_name("parent") || t.is_name("dirname"))
        .count()
        + value.iter().filter(|t| t.is_name("parents")).count();
    let mut dir = settings.to_string();
    for _ in 0..levels.max(1) {
        dir = parent(&dir).to_string();
    }
    dir
}

fn app(project: &Project, entry: &str) -> App {
    let mut module = entry.to_string();
    // `shop.apps.ShopConfig` → `shop`
    if let Some(i) = module.find(".apps.") {
        module.truncate(i);
    } else if module
        .rsplit('.')
        .next()
        .is_some_and(|l| l.ends_with("Config"))
    {
        module = module
            .rsplit_once('.')
            .map(|(m, _)| m.to_string())
            .unwrap_or(module);
    }
    let dir = project.module_file(&module).map(|f| {
        f.trim_end_matches("/__init__.py")
            .trim_end_matches(".py")
            .to_string()
    });
    App {
        label: module.rsplit('.').next().unwrap_or(&module).to_string(),
        entry: entry.to_string(),
        dir,
    }
}

fn read_templates(value: &[Token], base: &str, d: &mut Django) {
    let Some(open) = value.iter().position(|t| t.is_op("[")) else {
        return;
    };
    for backend in source::list_items(value, open) {
        let key = |name: &str| {
            backend
                .iter()
                .position(|t| t.is_plain_str() && t.text == name)
                .filter(|i| backend.get(i + 1).is_some_and(|c| c.is_op(":")))
                .map(|i| i + 2)
        };
        if let Some(i) = key("BACKEND") {
            if backend.get(i).is_some_and(|t| t.text.contains("jinja2")) {
                d.jinja2 = true;
            }
        }
        if let Some(i) = key("APP_DIRS") {
            d.app_dirs = !backend.get(i).is_some_and(|t| t.is_name("False"));
        }
        if let Some(i) = key("DIRS") {
            if backend.get(i).is_some_and(|t| t.is_op("[")) {
                for item in source::list_items(backend, i) {
                    let parts: Vec<&str> = item
                        .iter()
                        .filter(|t| t.is_plain_str())
                        .map(|t| t.text.as_str())
                        .collect();
                    if parts.is_empty() {
                        continue;
                    }
                    let rel = parts.join("/");
                    let dir = if item.iter().any(|t| t.is_name("BASE_DIR")) || !rel.starts_with('/')
                    {
                        join(base, rel.trim_matches('/'))
                    } else {
                        rel
                    };
                    if !d.template_dirs.contains(&dir) {
                        d.template_dirs.push(dir);
                    }
                }
            }
        }
    }
}

// ---- URL patterns -------------------------------------------------------------------------

const MAX_INCLUDE_DEPTH: usize = 8;

struct UrlScan<'a> {
    project: &'a Project,
    out: Vec<Route>,
    seen: BTreeSet<(String, String)>,
}

/// URL patterns of the project, from `ROOT_URLCONF` (or every `urls.py` when it is unknown).
pub fn routes(project: &Project, d: &Django) -> Vec<Route> {
    let mut scan = UrlScan {
        project,
        out: Vec::new(),
        seen: BTreeSet::new(),
    };
    let root = d
        .root_urlconf
        .as_deref()
        .and_then(|m| project.module_file(m));
    match root {
        Some(file) => scan.file(&file, "/", None, 0),
        None => {
            let mut files: Vec<&str> = project
                .files()
                .map(|(p, _)| p)
                .filter(|p| p.ends_with("urls.py"))
                .collect();
            files.sort_by_key(|p| p.matches('/').count());
            for f in files {
                scan.file(f, "/", None, 0);
            }
        }
    }
    scan.out
}

/// `prefix` + `route` for display: `/shop/` + `products/<int:pk>/`; regex anchors dropped.
fn join_route(prefix: &str, route: &str) -> String {
    let r = route
        .trim_start_matches('^')
        .trim_end_matches('$')
        .trim_start_matches('/');
    format!(
        "{}{}",
        if prefix.ends_with('/') {
            prefix.to_string()
        } else {
            format!("{prefix}/")
        },
        r
    )
}

impl UrlScan<'_> {
    fn file(&mut self, path: &str, prefix: &str, namespace: Option<String>, depth: usize) {
        if depth > MAX_INCLUDE_DEPTH || !self.seen.insert((path.to_string(), prefix.to_string())) {
            return;
        }
        let Some(lines) = self.project.lines(path) else {
            return;
        };
        let app_name = lines.iter().find_map(|l| match source::assignment(l) {
            Some(("app_name", at)) => l
                .tokens
                .get(at)
                .filter(|t| t.is_plain_str())
                .map(|t| t.text.clone()),
            _ => None,
        });
        let namespace = namespace.or(app_name);
        for l in lines {
            let Some(("urlpatterns", at)) = source::assignment(l) else {
                continue;
            };
            self.patterns(path, &l.tokens[at..], prefix, &namespace, depth);
        }
    }

    /// A list of patterns (`[path(...), ...]`, `i18n_patterns(...)`, `[...] + [...]`).
    fn patterns(
        &mut self,
        path: &str,
        value: &[Token],
        prefix: &str,
        ns: &Option<String>,
        depth: usize,
    ) {
        let mut i = 0;
        while i < value.len() {
            let t = &value[i];
            if t.is_op("[") {
                let items = source::list_items(value, i);
                for item in &items {
                    self.pattern(path, item, prefix, ns, depth);
                }
                i = skip_group(value, i);
                continue;
            }
            if t.is_name("i18n_patterns") && value.get(i + 1).is_some_and(|x| x.is_op("(")) {
                if let Some((args, close)) = source::call_args(value, i + 1) {
                    for a in args.iter().filter(|a| a.keyword.is_none()) {
                        self.pattern(path, a.tokens, prefix, ns, depth);
                    }
                    i = close + 1;
                    continue;
                }
            }
            i += 1;
        }
    }

    fn pattern(
        &mut self,
        path: &str,
        item: &[Token],
        prefix: &str,
        ns: &Option<String>,
        depth: usize,
    ) {
        let Some(open) = item.iter().position(|t| t.is_op("(")) else {
            return;
        };
        let Some((callee, _)) = source::dotted_before(item, open) else {
            return;
        };
        let last = callee.rsplit('.').next().unwrap_or(&callee);
        if !matches!(last, "path" | "re_path" | "url") {
            return;
        }
        let Some((args, _)) = source::call_args(item, open) else {
            return;
        };
        let Some(route_arg) =
            source::positional(&args, 0).or_else(|| source::keyword(&args, "route"))
        else {
            return;
        };
        let Some(route) = route_arg.string() else {
            return;
        };
        let full = join_route(prefix, &route);
        let Some(view) = source::positional(&args, 1).or_else(|| source::keyword(&args, "view"))
        else {
            return;
        };
        if let Some((callee, inner)) = view
            .call()
            .filter(|(c, _)| c.rsplit('.').next() == Some("include"))
        {
            let _ = callee;
            self.include(path, &inner, &full, ns, depth);
            return;
        }
        let written = view_name(view.tokens);
        let (handler, handler_location) = match &written {
            Some(w) => {
                let (q, loc) = self.project.resolve(path, w);
                (q, loc)
            }
            None => (tokens_text(view.tokens), None),
        };
        let name = source::keyword(&args, "name")
            .and_then(|a| a.string())
            .map(|n| match ns {
                Some(ns) => format!("{ns}:{n}"),
                None => n,
            });
        let at = route_arg.tokens.first().map(|t| t.start).unwrap_or(0);
        self.out.push(Route {
            framework: "django".into(),
            methods: Vec::new(),
            sample_path: sample_path(&full),
            path: full,
            name,
            handler,
            location: self.project.locate(path, at),
            handler_location,
        });
    }

    /// `include("shop.urls")`, `include(("shop.urls", "shop"), namespace="shop")`,
    /// `include([path(...), ...])`.
    fn include(
        &mut self,
        path: &str,
        args: &[source::Arg<'_>],
        prefix: &str,
        ns: &Option<String>,
        depth: usize,
    ) {
        let Some(first) = source::positional(args, 0) else {
            return;
        };
        let explicit = source::keyword(args, "namespace").and_then(|a| a.string());
        // `("shop.urls", "shop")` or `([path(...)], "api")`: the target and its namespace.
        let tuple = first.tokens.first().is_some_and(|t| t.is_op("("))
            && first.tokens.last().is_some_and(|t| t.is_op(")"));
        let (target, tuple_ns): (&[Token], Option<String>) = if tuple {
            let items = source::list_items(first.tokens, 0);
            let ns = items.get(1).and_then(|i| match i {
                [t] if t.is_plain_str() => Some(t.text.clone()),
                _ => None,
            });
            (items.first().copied().unwrap_or(&[]), ns)
        } else {
            (first.tokens, None)
        };
        let nested = |own: Option<String>| -> Option<String> {
            match (ns, own) {
                (Some(outer), Some(inner)) => Some(format!("{outer}:{inner}")),
                (Some(outer), None) => Some(outer.clone()),
                (None, inner) => inner,
            }
        };
        let own = explicit.or(tuple_ns);
        if let [t] = target {
            if !t.is_plain_str() {
                return;
            }
            if let Some(file) = self.project.module_file(&t.text) {
                // The included module's `app_name` applies when no namespace is given.
                let own = own.or_else(|| {
                    self.project.lines(&file).and_then(|lines| {
                        lines.iter().find_map(|l| match source::assignment(l) {
                            Some(("app_name", at)) => l
                                .tokens
                                .get(at)
                                .filter(|t| t.is_plain_str())
                                .map(|t| t.text.clone()),
                            _ => None,
                        })
                    })
                });
                self.file_with(&file, prefix, nested(own), depth + 1);
            }
            return;
        }
        if target.first().is_some_and(|t| t.is_op("[")) {
            self.patterns(path, target, prefix, &nested(own), depth + 1);
        }
    }

    fn file_with(&mut self, path: &str, prefix: &str, ns: Option<String>, depth: usize) {
        if depth > MAX_INCLUDE_DEPTH || !self.seen.insert((path.to_string(), prefix.to_string())) {
            return;
        }
        let Some(lines) = self.project.lines(path) else {
            return;
        };
        for l in lines {
            if let Some(("urlpatterns", at)) = source::assignment(l) {
                self.patterns(path, &l.tokens[at..], prefix, &ns, depth);
            }
        }
    }
}

/// The index after the bracket group that opens at `open`.
fn skip_group(t: &[Token], open: usize) -> usize {
    let mut depth = 0;
    for (i, x) in t.iter().enumerate().skip(open) {
        if x.kind != Kind::Op {
            continue;
        }
        match x.text.as_str() {
            "(" | "[" | "{" => depth += 1,
            ")" | "]" | "}" => {
                depth -= 1;
                if depth == 0 {
                    return i + 1;
                }
            }
            _ => {}
        }
    }
    t.len()
}

/// `views.index`, `views.Detail.as_view(...)` → the dotted view without the call.
fn view_name(t: &[Token]) -> Option<String> {
    let end = t.iter().position(|x| x.is_op("(")).unwrap_or(t.len());
    source::dotted_exact(&t[..end])
}

fn tokens_text(t: &[Token]) -> String {
    t.iter()
        .map(|x| x.text.as_str())
        .collect::<Vec<_>>()
        .join("")
}

// ---- Models -------------------------------------------------------------------------------

const RELATIONS: [&str; 4] = [
    "ForeignKey",
    "OneToOneField",
    "ManyToManyField",
    "GenericForeignKey",
];

fn is_models_file(path: &str) -> bool {
    path.ends_with("/models.py") || path == "models.py" || path.contains("/models/")
}

/// The models of every `models.py` (and `models/` package).
pub fn models(project: &Project) -> Vec<Model> {
    let mut out = Vec::new();
    for (path, lines) in project.python() {
        if !is_models_file(path) || path.contains("/migrations/") {
            continue;
        }
        let app = {
            let dir = path.split("/models").next().unwrap_or("");
            dir.rsplit('/').next().unwrap_or(dir).to_string()
        };
        let mut known: BTreeSet<String> = BTreeSet::new();
        for (i, l) in lines.iter().enumerate() {
            let Some(("class", name)) = source::definition(l) else {
                continue;
            };
            if l.indent > 0 {
                continue;
            }
            let bases = class_bases(&l.tokens);
            let is_model = bases.iter().any(|b| {
                b == "models.Model"
                    || b.ends_with("Model")
                    || known.contains(b.rsplit('.').next().unwrap_or(b))
            });
            if !is_model {
                continue;
            }
            known.insert(name.text.clone());
            let body: Vec<&Line> = lines[i + 1..]
                .iter()
                .take_while(|b| b.indent > l.indent)
                .collect();
            let body_indent = body.first().map(|b| b.indent).unwrap_or(0);
            let mut fields = Vec::new();
            let mut is_abstract = false;
            let mut in_meta = false;
            for b in &body {
                if b.indent == body_indent {
                    in_meta =
                        matches!(source::definition(b), Some(("class", n)) if n.text == "Meta");
                }
                if in_meta && b.indent > body_indent {
                    if matches!(source::assignment(b), Some(("abstract", at)) if b.tokens.get(at).is_some_and(|t| t.is_name("True")))
                    {
                        is_abstract = true;
                    }
                    continue;
                }
                if b.indent != body_indent {
                    continue;
                }
                if let Some(f) = field(project, path, b) {
                    fields.push(f);
                }
            }
            out.push(Model {
                app: app.clone(),
                name: name.text.clone(),
                bases,
                location: project.locate(path, name.start),
                fields,
                is_abstract,
            });
        }
    }
    out
}

fn class_bases(t: &[Token]) -> Vec<String> {
    let Some(open) = t.iter().position(|x| x.is_op("(")) else {
        return Vec::new();
    };
    source::call_args(t, open)
        .map(|(args, _)| {
            args.iter()
                .filter(|a| a.keyword.is_none())
                .filter_map(|a| a.dotted())
                .collect()
        })
        .unwrap_or_default()
}

fn field(project: &Project, path: &str, line: &Line) -> Option<Field> {
    let (name, at) = source::assignment(line)?;
    let t = &line.tokens;
    let open = t[at..].iter().position(|x| x.is_op("("))? + at;
    let callee = source::dotted_exact(&t[at..open])?;
    let kind = callee.rsplit('.').next().unwrap_or(&callee).to_string();
    if !(kind.ends_with("Field") || RELATIONS.contains(&kind.as_str())) {
        return None;
    }
    let target = if RELATIONS.contains(&kind.as_str()) {
        source::call_args(t, open).and_then(|(args, _)| {
            let a = source::positional(&args, 0).or_else(|| source::keyword(&args, "to"))?;
            a.string().or_else(|| a.dotted())
        })
    } else {
        None
    };
    Some(Field {
        name: name.to_string(),
        kind,
        target,
        location: project.locate(path, t[0].start),
    })
}

// ---- manage.py commands -------------------------------------------------------------------

/// Django's commands plus the project's `<app>/management/commands/<name>.py`.
pub fn static_commands(project: &Project) -> Vec<ManageCommand> {
    let mut out: Vec<ManageCommand> = BUILTIN_COMMANDS
        .iter()
        .map(|(name, app, description)| ManageCommand {
            name: name.to_string(),
            app: format!("django.{app}").replace("django.core", "django"),
            description: description.to_string(),
            location: None,
        })
        .collect();
    for (path, lines) in project.python() {
        let Some((before, file)) = path.rsplit_once("/management/commands/") else {
            continue;
        };
        let Some(name) = file
            .strip_suffix(".py")
            .filter(|n| !n.starts_with('_') && !n.contains('/'))
        else {
            continue;
        };
        let app = before.rsplit('/').next().unwrap_or(before).to_string();
        let description = lines
            .iter()
            .find_map(|l| match source::assignment(l) {
                Some(("help", at)) => l
                    .tokens
                    .get(at)
                    .filter(|t| t.is_plain_str())
                    .map(|t| t.text.clone()),
                _ => None,
            })
            .unwrap_or_default();
        out.retain(|c| c.name != name);
        out.push(ManageCommand {
            name: name.to_string(),
            app,
            description,
            location: Some(project.locate(path, 0)),
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Commands from the output of `manage.py help --commands` (one per line), with the
/// descriptions and locations known statically.
pub fn parse_help_commands(output: &str, known: &[ManageCommand]) -> Vec<ManageCommand> {
    let mut out: Vec<ManageCommand> = output
        .lines()
        .map(str::trim)
        .filter(|l| {
            !l.is_empty()
                && l.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        })
        .map(|name| {
            known
                .iter()
                .find(|k| k.name == name)
                .cloned()
                .unwrap_or(ManageCommand {
                    name: name.to_string(),
                    app: String::new(),
                    description: String::new(),
                    location: None,
                })
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out.dedup_by(|a, b| a.name == b.name);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::project;

    fn shop() -> Project {
        project(&[
            ("requirements.txt", "django>=5\n"),
            ("manage.py", "import os, sys\n\ndef main():\n    os.environ.setdefault(\"DJANGO_SETTINGS_MODULE\", \"mysite.settings\")\n"),
            ("mysite/__init__.py", ""),
            ("mysite/settings.py", include_str!("../tests/fixtures/django/settings.py")),
            ("mysite/urls.py", include_str!("../tests/fixtures/django/urls.py")),
            ("shop/__init__.py", ""),
            ("shop/apps.py", "class ShopConfig(AppConfig):\n    name = 'shop'\n"),
            ("shop/urls.py", include_str!("../tests/fixtures/django/shop_urls.py")),
            ("shop/views.py", include_str!("../tests/fixtures/django/views.py")),
            ("shop/models.py", include_str!("../tests/fixtures/django/models.py")),
            ("shop/management/commands/import_products.py", "class Command(BaseCommand):\n    help = 'Import products from a CSV file.'\n"),
            ("shop/management/commands/_utils.py", ""),
        ])
    }

    #[test]
    fn settings() {
        let p = shop();
        let d = p.info.django.clone().unwrap();
        assert_eq!(d.manage, "manage.py");
        assert_eq!(d.settings_module.as_deref(), Some("mysite.settings"));
        assert_eq!(d.settings_file.as_deref(), Some("mysite/settings.py"));
        assert_eq!(d.root_urlconf.as_deref(), Some("mysite.urls"));
        assert_eq!(d.template_dirs, ["templates"]);
        assert!(d.app_dirs);
        assert_eq!(
            d.apps[0],
            App {
                label: "shop".into(),
                entry: "shop.apps.ShopConfig".into(),
                dir: Some("shop".into())
            }
        );
        assert!(d
            .apps
            .iter()
            .any(|a| a.entry == "django.contrib.admin" && a.dir.is_none()));
    }

    #[test]
    fn url_patterns() {
        let p = shop();
        let r: Vec<(String, Option<String>, String)> = p
            .info
            .routes
            .iter()
            .map(|r| (r.path.clone(), r.name.clone(), r.handler.clone()))
            .collect();
        assert_eq!(
            r,
            [
                ("/admin/".into(), None, "admin.site.urls".into()),
                ("/".into(), Some("home".into()), "shop.views.home".into()),
                (
                    "/shop/".into(),
                    Some("shop:product-list".into()),
                    "shop.views.ProductListView".into()
                ),
                (
                    "/shop/products/<int:pk>/".into(),
                    Some("shop:product-detail".into()),
                    "shop.views.product_detail".into()
                ),
                (
                    "/shop/cart/".into(),
                    Some("shop:cart".into()),
                    "shop.views.cart".into()
                ),
                (
                    "/api/v1/health/".into(),
                    Some("api:health".into()),
                    "shop.views.health".into()
                ),
                (
                    "/legacy/(?P<year>[0-9]{4})/".into(),
                    Some("legacy-archive".into()),
                    "shop.views.archive".into()
                ),
            ]
        );
        let detail = &p.info.routes[3];
        assert_eq!(detail.sample_path, "/shop/products/1/");
        assert_eq!(detail.location.path, "shop/urls.py");
        assert_eq!(
            detail.handler_location.as_ref().unwrap().path,
            "shop/views.py"
        );
    }

    #[test]
    fn model_fields() {
        let p = shop();
        let names: Vec<&str> = p.info.models.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, ["TimeStamped", "Category", "Product", "Order"]);
        assert!(p.info.models[0].is_abstract);
        let product = &p.info.models[2];
        assert_eq!(product.app, "shop");
        let fields: Vec<(&str, &str, Option<&str>)> = product
            .fields
            .iter()
            .map(|f| (f.name.as_str(), f.kind.as_str(), f.target.as_deref()))
            .collect();
        assert_eq!(
            fields,
            [
                ("name", "CharField", None),
                ("slug", "SlugField", None),
                ("price", "DecimalField", None),
                ("category", "ForeignKey", Some("Category")),
                ("tags", "ManyToManyField", Some("Tag")),
            ]
        );
    }

    #[test]
    fn commands() {
        let p = shop();
        let all = static_commands(&p);
        let custom = all.iter().find(|c| c.name == "import_products").unwrap();
        assert_eq!(custom.app, "shop");
        assert_eq!(custom.description, "Import products from a CSV file.");
        assert!(all.iter().any(|c| c.name == "migrate" && c.app == "django"));
        assert!(!all.iter().any(|c| c.name == "_utils"));
        let parsed = parse_help_commands("\nmigrate\nimport_products\nrunserver_plus\n", &all);
        assert_eq!(
            parsed.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
            ["import_products", "migrate", "runserver_plus"]
        );
        assert_eq!(parsed[0].app, "shop");
    }

    #[test]
    fn settings_package_with_star_import() {
        let p = project(&[
            ("manage.py", "os.environ.setdefault('DJANGO_SETTINGS_MODULE', 'conf.settings.dev')\n# django\n"),
            ("conf/__init__.py", ""),
            ("conf/settings/__init__.py", ""),
            ("conf/settings/base.py", "from pathlib import Path\nBASE_DIR = Path(__file__).resolve().parent.parent.parent\nROOT_URLCONF = 'conf.urls'\nTEMPLATES = [{'BACKEND': 'django.template.backends.jinja2.Jinja2', 'DIRS': [BASE_DIR / 'jinja'], 'APP_DIRS': False}]\n"),
            ("conf/settings/dev.py", "from .base import *\nDEBUG = True\n"),
        ]);
        let d = p.info.django.clone().unwrap();
        assert_eq!(d.root_urlconf.as_deref(), Some("conf.urls"));
        assert_eq!(d.template_dirs, ["jinja"]);
        assert!(d.jinja2);
        assert!(!d.app_dirs);
    }
}
