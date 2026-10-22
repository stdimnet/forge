//! Related Symbol (Navigate | Related Symbol..., Ctrl+Alt+Home): the files that belong to
//! the current one, like PHPStorm's "Go to Related".
//!
//! - PHP classes and their PHPUnit / Pest tests (`OrderTest.php` ↔ `Order.php`).
//! - Laravel: a controller's views (`view('orders.show')`, `View::make`, `Inertia::render`)
//!   and routes (`routes/*.php`); a Blade view's controllers and `Route::view` routes; a
//!   model's migrations, factory, policy and seeder (and back).
//! - Symfony: a controller's Twig templates (`render()`, `#[Template]`); a template's
//!   controllers.
//! - JavaScript / TypeScript / Vue / Svelte: a component's or module's tests
//!   (`*.test.ts`, `*.spec.ts`, `__tests__/`), stories and style sheets (and back).
//!
//! Pure: the caller hands over the project's file list and a reader, so the backend runs it
//! next to the files (local or the remote agent) and browser mode runs it as WebAssembly.

use serde::{Deserialize, Serialize};

/// The file to find related items for.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelatedRequest {
    /// Workspace-relative, `/`-separated.
    pub path: String,
    /// The editor's text of the file.
    pub text: String,
}

/// How an item relates to the file; decides its group in the chooser.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RelatedKind {
    Test,
    TestSubject,
    Controller,
    View,
    Template,
    Route,
    Model,
    Migration,
    Factory,
    Policy,
    Seeder,
    Component,
    Story,
    Style,
}

impl RelatedKind {
    /// The group title shown next to the items.
    pub fn label(self) -> &'static str {
        match self {
            Self::Test => "Tests",
            Self::TestSubject => "Test Subject",
            Self::Controller => "Controllers",
            Self::View => "Views",
            Self::Template => "Templates",
            Self::Route => "Routes",
            Self::Model => "Model",
            Self::Migration => "Migrations",
            Self::Factory => "Factory",
            Self::Policy => "Policy",
            Self::Seeder => "Seeder",
            Self::Component => "Component",
            Self::Story => "Stories",
            Self::Style => "Styles",
        }
    }
}

/// One related item; the chooser opens `path` at `line`/`column` (0-based, UTF-16).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelatedItem {
    pub kind: RelatedKind,
    /// [`RelatedKind::label`].
    pub group: String,
    /// `orders.show`, `GET /orders/{order}`, `OrderController::show()`, `Button.test.ts`.
    pub name: String,
    pub path: String,
    pub line: u32,
    pub column: u32,
}

fn item(kind: RelatedKind, name: impl Into<String>, path: &str, line: u32) -> RelatedItem {
    RelatedItem {
        kind,
        group: kind.label().to_string(),
        name: name.into(),
        path: path.to_string(),
        line,
        column: 0,
    }
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn dir_of(path: &str) -> &str {
    path.rfind('/').map_or("", |i| &path[..i])
}

fn is_test_dir(path: &str) -> bool {
    dir_of(path)
        .split('/')
        .any(|s| matches!(s, "tests" | "test" | "__tests__" | "spec" | "specs"))
}

fn line_of(text: &str, byte: usize) -> u32 {
    text[..byte.min(text.len())].matches('\n').count() as u32
}

/// `OrderItem` → `order_item`.
pub fn snake(name: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = name.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        if c.is_uppercase() {
            let prev_lower =
                i > 0 && (chars[i - 1].is_lowercase() || chars[i - 1].is_ascii_digit());
            let next_lower = chars.get(i + 1).is_some_and(|n| n.is_lowercase());
            if i > 0 && (prev_lower || (next_lower && chars[i - 1].is_uppercase())) {
                out.push('_');
            }
            out.extend(c.to_lowercase());
        } else {
            out.push(*c);
        }
    }
    out
}

/// `order_item` → `order_items`, `category` → `categories` (Laravel's table names).
pub fn plural(word: &str) -> String {
    if let Some(stem) = word.strip_suffix('y') {
        if !stem.ends_with(['a', 'e', 'i', 'o', 'u']) {
            return format!("{stem}ies");
        }
    }
    if word.ends_with('s') || word.ends_with('x') || word.ends_with("ch") || word.ends_with("sh") {
        return format!("{word}es");
    }
    format!("{word}s")
}

/// The quoted first argument after `at` (`'orders.show'` → `orders.show`).
fn quoted_arg(text: &str, at: usize) -> Option<&str> {
    let rest = text[at..].trim_start();
    let q = rest.chars().next().filter(|c| *c == '\'' || *c == '"')?;
    let end = rest[1..].find(q)?;
    Some(&rest[1..1 + end])
}

/// Every occurrence of `needle` in `text` that does not continue an identifier.
fn calls<'a>(text: &'a str, needle: &'a str) -> impl Iterator<Item = usize> + 'a {
    let bytes = text.as_bytes();
    text.match_indices(needle).filter_map(move |(at, _)| {
        let starts_ident = needle
            .as_bytes()
            .first()
            .is_some_and(|b| b.is_ascii_alphanumeric());
        let glued = at > 0 && (bytes[at - 1].is_ascii_alphanumeric() || bytes[at - 1] == b'_');
        (!(starts_ident && glued)).then_some(at)
    })
}

/// A view or template reference in PHP code.
struct ViewRef<'a> {
    name: &'a str,
    at: usize,
}

/// Laravel view names: `view('x')`, `View::make('x')`, `->view('x')`, `Route::view('/', 'x')`.
fn laravel_view_refs(text: &str) -> Vec<ViewRef<'_>> {
    let mut out = Vec::new();
    for needle in ["view(", "View::make(", "->view(", "->markdown("] {
        for at in calls(text, needle) {
            // `Route::view('/uri', 'name')`: the second argument.
            if needle == "view(" && text[..at].ends_with("Route::") {
                let start = at + needle.len();
                let Some(first) = quoted_arg(text, start) else {
                    continue;
                };
                let after = text[start..]
                    .find(first)
                    .map(|i| start + i + first.len() + 1);
                let Some(comma) = after.and_then(|a| text[a..].find(',').map(|i| a + i + 1)) else {
                    continue;
                };
                if let Some(name) = quoted_arg(text, comma) {
                    out.push(ViewRef { name, at });
                }
                continue;
            }
            if needle == "view(" && text[..at].ends_with("->") {
                continue; // handled by `->view(`
            }
            if let Some(name) = quoted_arg(text, at + needle.len()) {
                out.push(ViewRef { name, at });
            }
        }
    }
    out.sort_by_key(|r| r.at);
    out
}

/// Inertia pages: `Inertia::render('Orders/Show')`, `inertia('Orders/Show')`.
fn inertia_refs(text: &str) -> Vec<ViewRef<'_>> {
    let mut out = Vec::new();
    for needle in ["Inertia::render(", "inertia("] {
        for at in calls(text, needle) {
            if let Some(name) = quoted_arg(text, at + needle.len()) {
                out.push(ViewRef { name, at });
            }
        }
    }
    out
}

/// Twig templates: `->render('x.html.twig')`, `renderView`, `stream`, `#[Template('x')]`.
fn twig_refs(text: &str) -> Vec<ViewRef<'_>> {
    let mut out = Vec::new();
    for needle in [
        "->render(",
        "->renderView(",
        "->renderBlock(",
        "->renderBlockView(",
        "->stream(",
        "#[Template(",
    ] {
        for at in calls(text, needle) {
            let mut from = at + needle.len();
            if needle == "#[Template(" {
                let rest = text[from..].trim_start();
                if let Some(r) = rest.strip_prefix("template:") {
                    from = text.len() - r.len();
                }
            }
            if let Some(name) = quoted_arg(text, from).filter(|n| n.ends_with(".twig")) {
                out.push(ViewRef { name, at });
            }
        }
    }
    out.sort_by_key(|r| r.at);
    out
}

/// `resources/views/orders/show.blade.php` → `orders.show`.
fn blade_name(path: &str) -> Option<String> {
    let rest = path.strip_prefix("resources/views/")?;
    let name = rest
        .strip_suffix(".blade.php")
        .or_else(|| rest.strip_suffix(".php"))?;
    Some(name.replace('/', "."))
}

fn blade_path(files: &[String], name: &str) -> Option<String> {
    if name.contains("::") {
        return None; // package views
    }
    let base = format!("resources/views/{}", name.replace('.', "/"));
    [".blade.php", ".php"]
        .iter()
        .map(|ext| format!("{base}{ext}"))
        .find(|p| files.iter().any(|f| f == p))
}

/// `templates/orders/show.html.twig` → `orders/show.html.twig`.
fn twig_name(path: &str) -> Option<&str> {
    path.strip_prefix("templates/")
        .filter(|n| n.ends_with(".twig"))
}

/// `name()` of the function around `at`, for `OrderController::show()`.
fn function_before(text: &str, at: usize) -> Option<&str> {
    let before = &text[..at];
    let i = before.rfind("function ")?;
    let rest = &before[i + "function ".len()..];
    let end = rest
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(rest.len());
    (end > 0).then(|| &rest[..end])
}

fn stem_of(path: &str) -> &str {
    let name = file_name(path);
    name.split('.').next().unwrap_or(name)
}

/// `Route::get('/orders/{order}', ...)` → `GET /orders/{order}`.
fn route_label(line: &str) -> String {
    let trimmed = line.trim();
    if let Some(i) = trimmed.find("Route::") {
        let rest = &trimmed[i + "Route::".len()..];
        let verb_end = rest
            .find(|c: char| !c.is_alphanumeric())
            .unwrap_or(rest.len());
        let verb = &rest[..verb_end];
        if let Some(open) = rest[verb_end..].find('(') {
            if let Some(uri) = quoted_arg(rest, verb_end + open + 1) {
                let uri = format!("/{}", uri.trim_start_matches('/'));
                return match verb {
                    "get" | "post" | "put" | "patch" | "delete" | "options" | "any" => {
                        format!("{} {uri}", verb.to_uppercase())
                    }
                    other => format!("{other} {uri}"),
                };
            }
        }
    }
    let mut s: String = trimmed.chars().take(80).collect();
    if trimmed.chars().count() > 80 {
        s.push('…');
    }
    s
}

struct Ctx<'a> {
    files: &'a [String],
    read: &'a dyn Fn(&str) -> Option<String>,
    out: Vec<RelatedItem>,
}

impl Ctx<'_> {
    fn push(&mut self, it: RelatedItem) {
        if !self
            .out
            .iter()
            .any(|o| o.path == it.path && o.line == it.line && o.kind == it.kind)
        {
            self.out.push(it);
        }
    }

    fn files_where(&self, f: impl Fn(&str) -> bool) -> Vec<String> {
        self.files.iter().filter(|p| f(p)).cloned().collect()
    }
}

/// The related items of `req.path`, grouped in a stable order.
pub fn related(
    req: &RelatedRequest,
    files: &[String],
    read: &dyn Fn(&str) -> Option<String>,
) -> Vec<RelatedItem> {
    let mut cx = Ctx {
        files,
        read,
        out: Vec::new(),
    };
    let path = req.path.as_str();
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".blade.php") {
        blade_view(&mut cx, path);
    } else if lower.ends_with(".php") {
        php_file(&mut cx, path, &req.text);
    } else if twig_name(path).is_some() {
        twig_template(&mut cx, path);
    } else {
        web_file(&mut cx, path);
    }
    // Groups in a fixed order, items in discovery order inside a group.
    let mut out = cx.out;
    out.retain(|i| i.path != path);
    out.sort_by_key(|i| i.kind);
    out
}

fn php_file(cx: &mut Ctx, path: &str, text: &str) {
    let name = stem_of(path);
    // Tests and the tested class.
    if let Some(subject) = name.strip_suffix("Test").filter(|s| !s.is_empty()) {
        let wanted = format!("{subject}.php");
        for f in cx.files_where(|f| file_name(f) == wanted && !is_test_dir(f)) {
            cx.push(item(RelatedKind::TestSubject, file_name(&f), &f, 0));
        }
    } else {
        let wanted = format!("{name}Test.php");
        for f in cx.files_where(|f| file_name(f) == wanted) {
            cx.push(item(RelatedKind::Test, file_name(&f), &f, 0));
        }
    }
    // Views and templates the code renders.
    for r in laravel_view_refs(text) {
        if let Some(p) = blade_path(cx.files, r.name) {
            cx.push(item(RelatedKind::View, r.name, &p, 0));
        }
    }
    for r in inertia_refs(text) {
        let base = format!("resources/js/Pages/{}", r.name);
        for ext in ["vue", "tsx", "jsx", "svelte", "ts", "js"] {
            let p = format!("{base}.{ext}");
            if cx.files.iter().any(|f| *f == p) {
                cx.push(item(RelatedKind::View, r.name, &p, 0));
                break;
            }
        }
    }
    for r in twig_refs(text) {
        let p = format!("templates/{}", r.name);
        if cx.files.iter().any(|f| *f == p) {
            cx.push(item(RelatedKind::Template, r.name, &p, 0));
        }
    }
    // Routes of a controller.
    if name.ends_with("Controller") {
        let needles = [format!("{name}::class"), format!("{name}@")];
        for f in cx.files_where(|f| f.starts_with("routes/") && f.ends_with(".php")) {
            let Some(src) = (cx.read)(&f) else { continue };
            for (i, line) in src.lines().enumerate() {
                if needles.iter().any(|n| line.contains(n.as_str())) && line.contains("Route::") {
                    cx.push(item(RelatedKind::Route, route_label(line), &f, i as u32));
                }
            }
        }
    }
    laravel_model(cx, path, name, text);
}

/// Model ↔ migrations, factory, policy and seeder.
fn laravel_model(cx: &mut Ctx, path: &str, name: &str, text: &str) {
    let is_model = path.starts_with("app/Models/")
        || ["extends Model", "extends Authenticatable", "extends Pivot"]
            .iter()
            .any(|e| text.contains(e));
    if is_model && path.starts_with("app/") {
        let table = table_of(name, text);
        for f in cx.files_where(|f| f.starts_with("database/migrations/") && migrates(f, &table)) {
            cx.push(item(RelatedKind::Migration, file_name(&f), &f, 0));
        }
        for (kind, file) in [
            (
                RelatedKind::Factory,
                format!("database/factories/{name}Factory.php"),
            ),
            (
                RelatedKind::Policy,
                format!("app/Policies/{name}Policy.php"),
            ),
            (
                RelatedKind::Seeder,
                format!("database/seeders/{name}Seeder.php"),
            ),
        ] {
            if cx.files.iter().any(|f| *f == file) {
                cx.push(item(kind, file_name(&file), &file, 0));
            }
        }
        return;
    }
    // Back to the model.
    let model = if path.starts_with("database/factories/") {
        name.strip_suffix("Factory")
    } else if path.starts_with("app/Policies/") {
        name.strip_suffix("Policy")
    } else if path.starts_with("database/seeders/") {
        name.strip_suffix("Seeder")
    } else {
        None
    };
    let models = cx.files_where(|f| {
        f.starts_with("app/") && f.ends_with(".php") && !f.starts_with("app/Http/")
    });
    if let Some(model) = model.filter(|m| !m.is_empty()) {
        if let Some(f) = models
            .iter()
            .find(|f| stem_of(f) == model && f.starts_with("app/Models/"))
            .or_else(|| models.iter().find(|f| stem_of(f) == model))
        {
            cx.push(item(RelatedKind::Model, file_name(f), f, 0));
        }
        return;
    }
    if path.starts_with("database/migrations/") {
        let tables = migration_tables(file_name(path), text);
        for f in models.iter().filter(|f| f.starts_with("app/Models/")) {
            let src = (cx.read)(f).unwrap_or_default();
            if tables.contains(&table_of(stem_of(f), &src)) {
                cx.push(item(RelatedKind::Model, file_name(f), f, 0));
            }
        }
    }
}

/// `protected $table = 'x'`, else the plural snake case of the class name.
fn table_of(name: &str, text: &str) -> String {
    if let Some(i) = text.find("$table") {
        let rest = &text[i + "$table".len()..];
        if let Some(eq) = rest.find('=').filter(|e| rest[..*e].trim().is_empty()) {
            if let Some(t) = quoted_arg(rest, eq + 1) {
                return t.to_string();
            }
        }
    }
    plural(&snake(name))
}

fn migrates(file: &str, table: &str) -> bool {
    let n = file_name(file);
    ["create", "to", "from", "in", "on"]
        .iter()
        .any(|w| n.contains(&format!("_{w}_{table}_table")))
}

/// Tables a migration creates or changes (`Schema::create('orders'`, the file name).
fn migration_tables(file: &str, text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for needle in ["Schema::create(", "Schema::table("] {
        for at in calls(text, needle) {
            if let Some(t) = quoted_arg(text, at + needle.len()) {
                out.push(t.to_string());
            }
        }
    }
    if let Some(rest) = file.strip_suffix("_table.php") {
        for w in ["_create_", "_to_", "_from_", "_in_", "_on_"] {
            if let Some(i) = rest.rfind(w) {
                out.push(rest[i + w.len()..].to_string());
            }
        }
    }
    out
}

/// A Blade view: the controllers (and other classes) that render it and `Route::view`s.
fn blade_view(cx: &mut Ctx, path: &str) {
    let Some(name) = blade_name(path) else { return };
    for f in cx.files_where(|f| {
        f.ends_with(".php")
            && !f.ends_with(".blade.php")
            && (f.starts_with("app/") || f.starts_with("routes/"))
    }) {
        let Some(src) = (cx.read)(&f) else { continue };
        for r in laravel_view_refs(&src)
            .into_iter()
            .filter(|r| r.name == name)
        {
            let line = line_of(&src, r.at);
            if f.starts_with("routes/") {
                let text = src.lines().nth(line as usize).unwrap_or("");
                cx.push(item(RelatedKind::Route, route_label(text), &f, line));
            } else {
                let label = match function_before(&src, r.at) {
                    Some(func) => format!("{}::{func}()", stem_of(&f)),
                    None => stem_of(&f).to_string(),
                };
                cx.push(item(RelatedKind::Controller, label, &f, line));
            }
        }
    }
}

/// A Twig template: the controller actions that render it.
fn twig_template(cx: &mut Ctx, path: &str) {
    let Some(name) = twig_name(path) else { return };
    for f in cx.files_where(|f| f.starts_with("src/") && f.ends_with(".php")) {
        let Some(src) = (cx.read)(&f) else { continue };
        for r in twig_refs(&src).into_iter().filter(|r| r.name == name) {
            let label = match function_before(&src, r.at) {
                Some(func) => format!("{}::{func}()", stem_of(&f)),
                None => stem_of(&f).to_string(),
            };
            cx.push(item(
                RelatedKind::Controller,
                label,
                &f,
                line_of(&src, r.at),
            ));
        }
    }
}

const CODE_EXTS: &[&str] = &[
    "js", "jsx", "ts", "tsx", "mjs", "cjs", "mts", "cts", "vue", "svelte",
];
const STYLE_EXTS: &[&str] = &["css", "scss", "sass", "less", "styl", "pcss"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    Source,
    Test,
    Story,
    Style,
}

/// `Button.test.tsx` → (`Button`, Test); `Button.module.css` → (`Button`, Style).
fn web_role(path: &str) -> Option<(&str, Role)> {
    let name = file_name(path);
    let mut parts = name.split('.');
    let stem = parts.next()?;
    let rest: Vec<&str> = parts.collect();
    let ext = rest.last()?.to_ascii_lowercase();
    let quals = &rest[..rest.len() - 1];
    let role = if STYLE_EXTS.contains(&ext.as_str()) {
        Role::Style
    } else if ext == "mdx" || quals.iter().any(|q| matches!(*q, "stories" | "story")) {
        Role::Story
    } else if !CODE_EXTS.contains(&ext.as_str()) {
        return None;
    } else if quals
        .iter()
        .any(|q| matches!(*q, "test" | "spec" | "e2e" | "cy"))
        || dir_of(path).split('/').any(|s| s == "__tests__")
    {
        Role::Test
    } else {
        Role::Source
    };
    (!stem.is_empty()).then_some((stem, role))
}

/// Components and modules ↔ tests, stories and styles.
fn web_file(cx: &mut Ctx, path: &str) {
    let Some((stem, role)) = web_role(path) else {
        return;
    };
    let dir = dir_of(path);
    // The component's folder when the file sits in `__tests__` or `stories`.
    let home = match file_name(dir) {
        "__tests__" | "stories" | "styles" => dir_of(dir),
        _ => dir,
    };
    let near = |f: &str| {
        let d = dir_of(f);
        d == dir
            || d == home
            || (dir_of(d) == home && matches!(file_name(d), "__tests__" | "stories" | "styles"))
    };
    let findable = stem != "index";
    for f in cx.files_where(|f| f != path) {
        let Some((s, r)) = web_role(&f) else { continue };
        if s != stem || r == role {
            continue;
        }
        let kind = match (role, r) {
            (Role::Source, Role::Test) if near(&f) || (findable && is_test_dir(&f)) => {
                RelatedKind::Test
            }
            (Role::Test, Role::Source) if near(&f) || (findable && !is_test_dir(&f)) => {
                RelatedKind::TestSubject
            }
            (_, Role::Story) if near(&f) => RelatedKind::Story,
            (Role::Source, Role::Style) if near(&f) => RelatedKind::Style,
            (Role::Story | Role::Style, Role::Source) if near(&f) => RelatedKind::Component,
            _ => continue,
        };
        cx.push(item(kind, file_name(&f), &f, 0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn project(files: &[(&str, &str)]) -> (Vec<String>, HashMap<String, String>) {
        let list = files.iter().map(|(p, _)| p.to_string()).collect();
        let map = files
            .iter()
            .map(|(p, t)| (p.to_string(), t.to_string()))
            .collect();
        (list, map)
    }

    fn run(files: &[(&str, &str)], path: &str) -> Vec<(RelatedKind, String, String, u32)> {
        let (list, map) = project(files);
        let text = map.get(path).cloned().unwrap_or_default();
        let read = |p: &str| map.get(p).cloned();
        related(
            &RelatedRequest {
                path: path.into(),
                text,
            },
            &list,
            &read,
        )
        .into_iter()
        .map(|i| (i.kind, i.name, i.path, i.line))
        .collect()
    }

    const CONTROLLER: &str = "<?php\nnamespace App\\Http\\Controllers;\nclass OrderController extends Controller\n{\n    public function index()\n    {\n        return view('orders.index', ['orders' => []]);\n    }\n\n    public function show(Order $order)\n    {\n        return View::make(\"orders.show\");\n    }\n}\n";
    const ROUTES: &str = "<?php\nuse App\\Http\\Controllers\\OrderController;\n\nRoute::get('/orders', [OrderController::class, 'index'])->name('orders.index');\nRoute::get('orders/{order}', [OrderController::class, 'show']);\nRoute::view('/about', 'pages.about');\n";

    fn laravel() -> Vec<(&'static str, &'static str)> {
        vec![
            ("app/Http/Controllers/OrderController.php", CONTROLLER),
            (
                "app/Models/Order.php",
                "<?php\nclass Order extends Model {}\n",
            ),
            (
                "app/Models/OrderItem.php",
                "<?php\nclass OrderItem extends Model { protected $table = 'line_items'; }\n",
            ),
            ("app/Policies/OrderPolicy.php", "<?php\n"),
            ("database/factories/OrderFactory.php", "<?php\n"),
            (
                "database/migrations/2026_01_01_000000_create_orders_table.php",
                "<?php\nSchema::create('orders', function () {});\n",
            ),
            (
                "database/migrations/2026_02_01_000000_add_note_to_orders_table.php",
                "<?php\n",
            ),
            (
                "database/migrations/2026_03_01_000000_create_line_items_table.php",
                "<?php\n",
            ),
            ("resources/views/orders/index.blade.php", "<ul></ul>"),
            ("resources/views/orders/show.blade.php", "<div></div>"),
            ("resources/views/pages/about.blade.php", "<p></p>"),
            ("routes/web.php", ROUTES),
            ("tests/Feature/OrderControllerTest.php", "<?php\n"),
        ]
    }

    #[test]
    fn laravel_controller_views_routes_and_tests() {
        let got = run(&laravel(), "app/Http/Controllers/OrderController.php");
        let names: Vec<(RelatedKind, &str)> = got.iter().map(|g| (g.0, g.1.as_str())).collect();
        assert_eq!(
            names,
            [
                (RelatedKind::Test, "OrderControllerTest.php"),
                (RelatedKind::View, "orders.index"),
                (RelatedKind::View, "orders.show"),
                (RelatedKind::Route, "GET /orders"),
                (RelatedKind::Route, "GET /orders/{order}"),
            ]
        );
        assert_eq!(got[3].2, "routes/web.php");
        assert_eq!(got[3].3, 3);
        // And back from the test.
        let back = run(&laravel(), "tests/Feature/OrderControllerTest.php");
        assert_eq!(back[0].0, RelatedKind::TestSubject);
        assert_eq!(back[0].2, "app/Http/Controllers/OrderController.php");
    }

    #[test]
    fn blade_views_know_their_controllers_and_routes() {
        let got = run(&laravel(), "resources/views/orders/show.blade.php");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].0, RelatedKind::Controller);
        assert_eq!(got[0].1, "OrderController::show()");
        assert_eq!(got[0].3, 11);
        let about = run(&laravel(), "resources/views/pages/about.blade.php");
        assert_eq!(about[0].0, RelatedKind::Route);
        assert_eq!(about[0].1, "view /about");
    }

    #[test]
    fn models_migrations_factories_and_policies() {
        let got = run(&laravel(), "app/Models/Order.php");
        let kinds: Vec<(RelatedKind, &str)> = got.iter().map(|g| (g.0, g.1.as_str())).collect();
        assert_eq!(
            kinds,
            [
                (
                    RelatedKind::Migration,
                    "2026_01_01_000000_create_orders_table.php"
                ),
                (
                    RelatedKind::Migration,
                    "2026_02_01_000000_add_note_to_orders_table.php"
                ),
                (RelatedKind::Factory, "OrderFactory.php"),
                (RelatedKind::Policy, "OrderPolicy.php"),
            ]
        );
        // A custom table name.
        let item = run(&laravel(), "app/Models/OrderItem.php");
        assert_eq!(item[0].1, "2026_03_01_000000_create_line_items_table.php");
        // Back to the model.
        let m = run(
            &laravel(),
            "database/migrations/2026_01_01_000000_create_orders_table.php",
        );
        assert_eq!(
            m,
            [(
                RelatedKind::Model,
                "Order.php".into(),
                "app/Models/Order.php".into(),
                0
            )]
        );
        let f = run(&laravel(), "database/factories/OrderFactory.php");
        assert_eq!(f[0].2, "app/Models/Order.php");
        let p = run(&laravel(), "app/Policies/OrderPolicy.php");
        assert_eq!(p[0].0, RelatedKind::Model);
    }

    #[test]
    fn symfony_controllers_and_twig_templates() {
        let ctrl = "<?php\nclass OrderController extends AbstractController\n{\n    #[Template('order/list.html.twig')]\n    public function list() {}\n\n    public function show(): Response\n    {\n        return $this->render('order/show.html.twig', []);\n    }\n}\n";
        let files = [
            ("src/Controller/OrderController.php", ctrl),
            ("templates/order/show.html.twig", "{{ order.id }}"),
            ("templates/order/list.html.twig", ""),
        ];
        let got = run(&files, "src/Controller/OrderController.php");
        let names: Vec<&str> = got.iter().map(|g| g.1.as_str()).collect();
        assert_eq!(names, ["order/list.html.twig", "order/show.html.twig"]);
        assert!(got.iter().all(|g| g.0 == RelatedKind::Template));
        let back = run(&files, "templates/order/show.html.twig");
        assert_eq!(
            back,
            [(
                RelatedKind::Controller,
                "OrderController::show()".into(),
                "src/Controller/OrderController.php".into(),
                8
            )]
        );
    }

    #[test]
    fn components_tests_stories_and_styles() {
        let files = [
            ("src/components/Button.vue", ""),
            ("src/components/Button.spec.ts", ""),
            ("src/components/Button.stories.ts", ""),
            ("src/components/Button.module.css", ""),
            ("src/components/__tests__/Button.test.ts", ""),
            ("src/utils/format.ts", ""),
            ("tests/unit/format.test.ts", ""),
            ("src/other/Button.tsx", ""),
            ("src/index.ts", ""),
            ("tests/index.test.ts", ""),
        ];
        let got = run(&files, "src/components/Button.vue");
        let kinds: Vec<(RelatedKind, &str)> = got.iter().map(|g| (g.0, g.2.as_str())).collect();
        assert_eq!(
            kinds,
            [
                (RelatedKind::Test, "src/components/Button.spec.ts"),
                (RelatedKind::Test, "src/components/__tests__/Button.test.ts"),
                (RelatedKind::Story, "src/components/Button.stories.ts"),
                (RelatedKind::Style, "src/components/Button.module.css"),
            ]
        );
        let back = run(&files, "src/components/__tests__/Button.test.ts");
        assert_eq!(
            back[0],
            (
                RelatedKind::TestSubject,
                "Button.vue".into(),
                "src/components/Button.vue".into(),
                0
            )
        );
        let style = run(&files, "src/components/Button.module.css");
        assert_eq!(style[0].0, RelatedKind::Component);
        // Tests in a separate folder, found by name.
        assert_eq!(
            run(&files, "src/utils/format.ts")[0].2,
            "tests/unit/format.test.ts"
        );
        assert_eq!(
            run(&files, "tests/unit/format.test.ts")[0].2,
            "src/utils/format.ts"
        );
        // `index` is too common to match across folders.
        assert!(run(&files, "src/index.ts").is_empty());
    }

    #[test]
    fn php_classes_and_their_tests() {
        let files = [
            ("app/Services/Checkout.php", "<?php\nclass Checkout {}\n"),
            ("tests/Unit/CheckoutTest.php", "<?php\n"),
        ];
        assert_eq!(
            run(&files, "app/Services/Checkout.php")[0].2,
            "tests/Unit/CheckoutTest.php"
        );
        assert_eq!(
            run(&files, "tests/Unit/CheckoutTest.php")[0].2,
            "app/Services/Checkout.php"
        );
        assert_eq!(snake("OrderItem"), "order_item");
        assert_eq!(snake("HTMLParser"), "html_parser");
        assert_eq!(plural("category"), "categories");
    }
}
