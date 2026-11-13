//! Routes: static parsing of `routes/*.php` (verbs, `match`, `resource`, `view`, `redirect`,
//! groups with prefix/name/middleware/controller, included route files) and the JSON
//! output of `php artisan route:list --json`.

use std::collections::HashSet;
use std::path::Path;

use phpforge_lang_php::parser::index_source;
use phpforge_lang_php::FileIndex;
use serde::{Deserialize, Serialize};
use tree_sitter::Node;

use crate::php::{self, call_args, call_name, is_call, text, Value};
use crate::project::{read, Location, Psr4};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Route {
    /// Upper-case HTTP methods, `GET` routes include `HEAD` like `route:list`.
    pub methods: Vec<String>,
    /// URI without leading slash (`orders/{order}`), `/` for the root.
    pub uri: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// `App\Http\Controllers\OrderController@show`, `Closure`, `view: welcome`, ...
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub controller: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub controller_method: Option<String>,
    #[serde(default)]
    pub middleware: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    /// Where the route is declared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<Location>,
    /// The controller method (or class for invokable controllers).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_location: Option<Location>,
}

impl Route {
    pub fn method_label(&self) -> String {
        self.methods.join("|")
    }

    /// URL parameters (`{order}`, `{slug?}`) without braces and `?`.
    pub fn parameters(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut rest = self.uri.as_str();
        while let Some(start) = rest.find('{') {
            let Some(end) = rest[start..].find('}') else {
                break;
            };
            let p = &rest[start + 1..start + end];
            out.push(
                p.trim_end_matches('?')
                    .split(':')
                    .next()
                    .unwrap_or(p)
                    .to_string(),
            );
            rest = &rest[start + end + 1..];
        }
        out
    }
}

/// Attributes inherited from enclosing groups.
#[derive(Debug, Clone, Default)]
struct Group {
    prefix: String,
    name: String,
    middleware: Vec<String>,
    controller: Option<String>,
    namespace: Option<String>,
    domain: Option<String>,
}

impl Group {
    fn merge(&self, attrs: &Attrs) -> Group {
        let mut g = self.clone();
        if let Some(p) = &attrs.prefix {
            g.prefix = join_uri(&g.prefix, p);
        }
        if let Some(n) = &attrs.name {
            g.name.push_str(n);
        }
        g.middleware.extend(attrs.middleware.iter().cloned());
        if attrs.controller.is_some() {
            g.controller.clone_from(&attrs.controller);
        }
        if let Some(ns) = &attrs.namespace {
            g.namespace = Some(match &g.namespace {
                Some(outer) if !ns.starts_with('\\') => format!("{outer}\\{ns}"),
                _ => ns.trim_start_matches('\\').to_string(),
            });
        }
        if attrs.domain.is_some() {
            g.domain.clone_from(&attrs.domain);
        }
        g
    }
}

#[derive(Debug, Clone, Default)]
struct Attrs {
    prefix: Option<String>,
    name: Option<String>,
    middleware: Vec<String>,
    controller: Option<String>,
    namespace: Option<String>,
    domain: Option<String>,
}

fn join_uri(a: &str, b: &str) -> String {
    let a = a.trim_matches('/');
    let b = b.trim_matches('/');
    match (a.is_empty(), b.is_empty()) {
        (true, true) => String::new(),
        (true, false) => b.to_string(),
        (false, true) => a.to_string(),
        (false, false) => format!("{a}/{b}"),
    }
}

fn final_uri(uri: String) -> String {
    if uri.is_empty() {
        "/".into()
    } else {
        uri
    }
}

const VERBS: &[&str] = &["get", "post", "put", "patch", "delete", "options", "any"];

struct Parser<'a> {
    root: &'a Path,
    visited: HashSet<String>,
    out: Vec<Route>,
}

/// Parse all route files of the project statically.
pub fn parse_routes(root: &Path, psr4: &Psr4) -> Vec<Route> {
    let mut p = Parser {
        root,
        visited: HashSet::new(),
        out: Vec::new(),
    };
    let api_prefix = api_prefix(root);
    let web = Group {
        middleware: vec!["web".into()],
        ..Group::default()
    };
    let api = Group {
        prefix: api_prefix,
        middleware: vec!["api".into()],
        ..Group::default()
    };
    p.file("routes/web.php", &web);
    p.file("routes/api.php", &api);
    // Remaining route files that were not included from web.php / api.php.
    for rel in crate::project::files_under(root, "routes", ".php") {
        let name = rel.rsplit('/').next().unwrap_or(&rel);
        if !matches!(name, "console.php" | "channels.php") {
            p.file(&rel, &web);
        }
    }
    for r in &mut p.out {
        if let Some(c) = &r.controller {
            r.action_location = action_location(root, psr4, c, r.controller_method.as_deref());
        }
    }
    p.out
}

/// `api` unless `bootstrap/app.php` configures another `apiPrefix`.
fn api_prefix(root: &Path) -> String {
    let Some(src) = read(root, "bootstrap/app.php") else {
        return "api".into();
    };
    if let Some(i) = src.find("apiPrefix") {
        let rest = &src[i..];
        if let Some(q) = rest.find(['\'', '"']) {
            let quote = &rest[q..q + 1];
            if let Some(end) = rest[q + 1..].find(quote) {
                return rest[q + 1..q + 1 + end].trim_matches('/').to_string();
            }
        }
    }
    "api".into()
}

/// Location of a controller method (or the class for invokable controllers).
pub fn action_location(
    root: &Path,
    psr4: &Psr4,
    class: &str,
    method: Option<&str>,
) -> Option<Location> {
    let rel = psr4.class_path(root, class)?;
    let src = read(root, &rel)?;
    let file = index_source(&rel, &src);
    let cls = file.classes.first()?;
    let method = method.unwrap_or("__invoke");
    match cls.method(method) {
        Some(m) => Some(Location::at(&rel, &src, m.name_range.start)),
        None => Some(Location::at(&rel, &src, cls.name_range.start)),
    }
}

impl Parser<'_> {
    fn file(&mut self, rel: &str, group: &Group) {
        if !self.visited.insert(rel.to_string()) {
            return;
        }
        let Some(src) = read(self.root, rel) else {
            return;
        };
        let Some(tree) = php::parse(&src) else { return };
        let index = index_source(rel, &src);
        let ctx = FileCtx {
            rel: rel.to_string(),
            src: &src,
            index: &index,
        };
        self.statements(tree.root_node(), &ctx, group);
    }

    fn statements(&mut self, node: Node, ctx: &FileCtx, group: &Group) {
        let mut cursor = node.walk();
        for stmt in node.named_children(&mut cursor) {
            match stmt.kind() {
                "expression_statement" => {
                    if let Some(expr) = stmt.named_child(0) {
                        self.expression(expr, ctx, group);
                    }
                }
                "namespace_definition" | "compound_statement" => {
                    if let Some(body) = stmt.child_by_field_name("body") {
                        self.statements(body, ctx, group);
                    } else {
                        self.statements(stmt, ctx, group);
                    }
                }
                _ => {}
            }
        }
    }

    fn expression(&mut self, expr: Node, ctx: &FileCtx, group: &Group) {
        if matches!(
            expr.kind(),
            "require_expression"
                | "require_once_expression"
                | "include_expression"
                | "include_once_expression"
        ) {
            if let Some(file) = included_file(expr, ctx) {
                self.file(&file, group);
            }
            return;
        }
        if !is_call(expr) {
            return;
        }
        let chain = flatten_chain(expr, ctx.src);
        let Some((first, rest)) = chain.split_first() else {
            return;
        };
        if first.scope.as_deref().map(|s| s.trim_start_matches('\\')) != Some("Route")
            && first.scope.as_deref() != Some("Illuminate\\Support\\Facades\\Route")
        {
            return;
        }
        let mut attrs = Attrs::default();
        let mut routes: Vec<Route> = Vec::new();
        let mut resource: Option<ResourceSpec> = None;
        for link in std::iter::once(first).chain(rest.iter()) {
            let args = call_args(link.node);
            let s0 = args.first().and_then(|a| php::string_value(*a, ctx.src));
            let in_route = !routes.is_empty() || resource.is_some();
            match link.name.as_str() {
                v if VERBS.contains(&v) && !in_route => {
                    let methods = match v {
                        "any" => vec!["GET", "HEAD", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"]
                            .into_iter()
                            .map(String::from)
                            .collect(),
                        "get" => vec!["GET".into(), "HEAD".into()],
                        other => vec![other.to_uppercase()],
                    };
                    let g = group.merge(&attrs);
                    routes.push(self.route(
                        methods,
                        s0.unwrap_or_default(),
                        args.get(1).copied(),
                        &g,
                        link.node,
                        ctx,
                    ));
                }
                "match" if !in_route => {
                    let methods: Vec<String> = args
                        .first()
                        .map(|a| php::value(*a, ctx.src).strings())
                        .unwrap_or_default()
                        .into_iter()
                        .map(|m| m.to_uppercase())
                        .flat_map(|m| {
                            if m == "GET" {
                                vec![m, "HEAD".into()]
                            } else {
                                vec![m]
                            }
                        })
                        .collect();
                    let uri = args
                        .get(1)
                        .and_then(|a| php::string_value(*a, ctx.src))
                        .unwrap_or_default();
                    let g = group.merge(&attrs);
                    routes.push(self.route(methods, uri, args.get(2).copied(), &g, link.node, ctx));
                }
                "view" if !in_route => {
                    let g = group.merge(&attrs);
                    let view = args
                        .get(1)
                        .and_then(|a| php::string_value(*a, ctx.src))
                        .unwrap_or_default();
                    let mut r = self.route(
                        vec!["GET".into(), "HEAD".into()],
                        s0.unwrap_or_default(),
                        None,
                        &g,
                        link.node,
                        ctx,
                    );
                    r.action = format!("view: {view}");
                    routes.push(r);
                }
                "redirect" | "permanentRedirect" if !in_route => {
                    let g = group.merge(&attrs);
                    let to = args
                        .get(1)
                        .and_then(|a| php::string_value(*a, ctx.src))
                        .unwrap_or_default();
                    let methods = ["GET", "HEAD", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"]
                        .map(String::from)
                        .to_vec();
                    let mut r =
                        self.route(methods, s0.unwrap_or_default(), None, &g, link.node, ctx);
                    r.action = format!("redirect: {to}");
                    routes.push(r);
                }
                "fallback" if !in_route => {
                    let g = group.merge(&attrs);
                    let mut r = self.route(
                        vec!["GET".into(), "HEAD".into()],
                        "{fallbackPlaceholder}".into(),
                        args.first().copied(),
                        &g,
                        link.node,
                        ctx,
                    );
                    r.uri = final_uri(join_uri(&g.prefix, "{fallbackPlaceholder}"));
                    routes.push(r);
                }
                "resource" | "apiResource" | "singleton" | "apiSingleton" if !in_route => {
                    let controller = args
                        .get(1)
                        .and_then(|a| class_arg(*a, ctx, &group.merge(&attrs)));
                    resource = Some(ResourceSpec {
                        kind: link.name.clone(),
                        name: s0.unwrap_or_default(),
                        controller,
                        only: None,
                        except: Vec::new(),
                        names: Vec::new(),
                        name_prefix: None,
                        middleware: Vec::new(),
                        node_start: link.node.start_byte(),
                    });
                }
                "resources" | "apiResources" if !in_route => {
                    let g = group.merge(&attrs);
                    if let Some(map) = args.first() {
                        if let Value::Array(entries) = php::value(*map, ctx.src) {
                            for e in entries {
                                let (Some(name), Value::ClassRef(c)) = (e.key.clone(), &e.value)
                                else {
                                    continue;
                                };
                                let spec = ResourceSpec {
                                    kind: if link.name == "resources" {
                                        "resource".into()
                                    } else {
                                        "apiResource".into()
                                    },
                                    name,
                                    controller: Some(resolve_class(c, ctx, &g)),
                                    only: None,
                                    except: Vec::new(),
                                    names: Vec::new(),
                                    name_prefix: None,
                                    middleware: Vec::new(),
                                    node_start: e.offset,
                                };
                                self.expand_resource(&spec, &g, ctx);
                            }
                        }
                    }
                }
                "group" => {
                    let g = group.merge(&attrs);
                    // Route::group(['prefix' => ...], fn) or ->group(fn) / ->group(base_path(...)).
                    let mut body_arg = args.first().copied();
                    let mut g = g;
                    if let (Some(a), Some(b)) = (args.first(), args.get(1)) {
                        if a.kind() == "array_creation_expression" {
                            g = g.merge(&array_attrs(&php::value(*a, ctx.src), ctx, &g));
                            body_arg = Some(*b);
                        }
                    }
                    if let Some(body) = body_arg {
                        self.group_body(body, ctx, &g);
                    }
                    return;
                }
                "prefix" if !in_route => attrs.prefix = s0,
                "name" | "as" => {
                    if let Some(r) = routes.last_mut() {
                        let n = s0.unwrap_or_default();
                        r.name = Some(format!("{}{}", r.name.take().unwrap_or_default(), n));
                    } else if let Some(res) = &mut resource {
                        res.name_prefix = s0;
                    } else if let Some(n) = s0 {
                        attrs.name = Some(format!("{}{n}", attrs.name.take().unwrap_or_default()));
                    }
                }
                "middleware" => {
                    let list: Vec<String> = args
                        .iter()
                        .flat_map(|a| php::value(*a, ctx.src).strings())
                        .collect();
                    if let Some(r) = routes.last_mut() {
                        r.middleware.extend(list);
                    } else if let Some(res) = &mut resource {
                        res.middleware.extend(list);
                    } else {
                        attrs.middleware.extend(list);
                    }
                }
                "controller" if !in_route => {
                    attrs.controller = args
                        .first()
                        .and_then(|a| class_arg(*a, ctx, &group.merge(&attrs)));
                }
                "namespace" if !in_route => attrs.namespace = s0,
                "domain" if !in_route => attrs.domain = s0,
                "only" => {
                    if let Some(res) = &mut resource {
                        res.only = Some(
                            args.iter()
                                .flat_map(|a| php::value(*a, ctx.src).strings())
                                .collect(),
                        );
                    }
                }
                "except" => {
                    if let Some(res) = &mut resource {
                        res.except = args
                            .iter()
                            .flat_map(|a| php::value(*a, ctx.src).strings())
                            .collect();
                    }
                }
                "names" => {
                    if let (Some(res), Some(a)) = (&mut resource, args.first()) {
                        match php::value(*a, ctx.src) {
                            Value::Str(prefix) => res.name_prefix = Some(format!("{prefix}.")),
                            Value::Array(entries) => {
                                res.names = entries
                                    .into_iter()
                                    .filter_map(|e| Some((e.key?, e.value.as_str()?.to_string())))
                                    .collect();
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
        if let Some(spec) = resource {
            let g = group.merge(&attrs);
            self.expand_resource(&spec, &g, ctx);
        }
        self.out.extend(routes);
    }

    fn group_body(&mut self, body: Node, ctx: &FileCtx, g: &Group) {
        match body.kind() {
            "anonymous_function" => {
                if let Some(b) = body.child_by_field_name("body") {
                    self.statements(b, ctx, g);
                }
            }
            "arrow_function" => {
                if let Some(b) = body.child_by_field_name("body") {
                    self.expression(b, ctx, g);
                }
            }
            _ => {
                // base_path('routes/admin.php'), __DIR__.'/admin.php' or a plain string.
                if let Some(file) = included_file(body, ctx) {
                    self.file(&file, g);
                }
            }
        }
    }

    fn route(
        &self,
        methods: Vec<String>,
        uri: String,
        action: Option<Node>,
        g: &Group,
        node: Node,
        ctx: &FileCtx,
    ) -> Route {
        let (action_label, controller, method) = match action {
            Some(a) => describe_action(a, ctx, g),
            None => match &g.controller {
                Some(c) => (format!("{c}@__invoke"), Some(c.clone()), None),
                None => ("Closure".into(), None, None),
            },
        };
        Route {
            methods,
            uri: final_uri(join_uri(&g.prefix, &uri)),
            name: (!g.name.is_empty()).then(|| g.name.clone()),
            action: action_label,
            controller,
            controller_method: method,
            middleware: g.middleware.clone(),
            domain: g.domain.clone(),
            source: Some(Location::at(&ctx.rel, ctx.src, node.start_byte())),
            action_location: None,
        }
    }

    fn expand_resource(&mut self, spec: &ResourceSpec, g: &Group, ctx: &FileCtx) {
        let api = spec.kind.starts_with("api");
        let singleton = spec.kind.ends_with("ingleton");
        let all: &[(&str, &str, &str)] = if singleton {
            &[
                ("show", "GET|HEAD", ""),
                ("edit", "GET|HEAD", "/edit"),
                ("update", "PUT|PATCH", ""),
            ]
        } else {
            &[
                ("index", "GET|HEAD", ""),
                ("create", "GET|HEAD", "/create"),
                ("store", "POST", ""),
                ("show", "GET|HEAD", "/{}"),
                ("edit", "GET|HEAD", "/{}/edit"),
                ("update", "PUT|PATCH", "/{}"),
                ("destroy", "DELETE", "/{}"),
            ]
        };
        // `photos.comments` → photos/{photo}/comments
        let segments: Vec<&str> = spec.name.split('.').filter(|s| !s.is_empty()).collect();
        let mut base = String::new();
        for (i, seg) in segments.iter().enumerate() {
            base = join_uri(&base, seg);
            if i + 1 < segments.len() {
                base = format!("{base}/{{{}}}", singular(seg));
            }
        }
        let param = segments.last().map(|s| singular(s)).unwrap_or_default();
        let source = Location::at(&ctx.rel, ctx.src, spec.node_start);
        for (action, methods, suffix) in all {
            if api && matches!(*action, "create" | "edit") {
                continue;
            }
            if let Some(only) = &spec.only {
                if !only.iter().any(|o| o == action) {
                    continue;
                }
            }
            if spec.except.iter().any(|e| e == action) {
                continue;
            }
            let uri = format!("{base}{}", suffix.replace("{}", &format!("{{{param}}}")));
            let route_name = spec
                .names
                .iter()
                .find(|(k, _)| k == action)
                .map(|(_, v)| v.clone())
                .unwrap_or_else(|| {
                    format!(
                        "{}{}.{action}",
                        spec.name_prefix.clone().unwrap_or_default(),
                        spec.name
                    )
                });
            let mut middleware = g.middleware.clone();
            middleware.extend(spec.middleware.iter().cloned());
            self.out.push(Route {
                methods: methods.split('|').map(String::from).collect(),
                uri: final_uri(join_uri(&g.prefix, &uri)),
                name: Some(format!("{}{route_name}", g.name)),
                action: match &spec.controller {
                    Some(c) => format!("{c}@{action}"),
                    None => (*action).to_string(),
                },
                controller: spec.controller.clone(),
                controller_method: Some((*action).to_string()),
                middleware,
                domain: g.domain.clone(),
                source: Some(source.clone()),
                action_location: None,
            });
        }
    }
}

struct ResourceSpec {
    kind: String,
    name: String,
    controller: Option<String>,
    only: Option<Vec<String>>,
    except: Vec<String>,
    names: Vec<(String, String)>,
    name_prefix: Option<String>,
    middleware: Vec<String>,
    node_start: usize,
}

struct FileCtx<'a> {
    rel: String,
    src: &'a str,
    index: &'a FileIndex,
}

/// `photos` → `photo`, `categories` → `category` (Laravel uses the singular as parameter).
pub fn singular(word: &str) -> String {
    let w = word.replace('-', "_");
    const UNCOUNTABLE: &[&str] = &[
        "news",
        "series",
        "species",
        "data",
        "media",
        "feedback",
        "information",
        "equipment",
    ];
    if UNCOUNTABLE.iter().any(|u| w.ends_with(u)) {
        return w;
    }
    if let Some(stem) = w.strip_suffix("ies") {
        format!("{stem}y")
    } else if w.ends_with("sses")
        || w.ends_with("shes")
        || w.ends_with("ches")
        || w.ends_with("xes")
    {
        w[..w.len() - 2].to_string()
    } else if let Some(stem) = w.strip_suffix('s') {
        if stem.ends_with(['s', 'u', 'i']) {
            w
        } else {
            stem.to_string()
        }
    } else {
        w
    }
}

struct Link<'t> {
    name: String,
    scope: Option<String>,
    node: Node<'t>,
}

/// `Route::a(...)->b(...)->c(...)` → [a, b, c] (the first carries the scope `Route`).
fn flatten_chain<'t>(expr: Node<'t>, src: &str) -> Vec<Link<'t>> {
    let mut links = Vec::new();
    let mut cur = Some(expr);
    while let Some(node) = cur {
        if !is_call(node) {
            break;
        }
        let name = call_name(node, src).unwrap_or_default().to_string();
        match node.kind() {
            "scoped_call_expression" => {
                let scope = node
                    .child_by_field_name("scope")
                    .map(|s| text(s, src).to_string());
                links.push(Link { name, scope, node });
                cur = None;
            }
            "member_call_expression" | "nullsafe_member_call_expression" => {
                links.push(Link {
                    name,
                    scope: None,
                    node,
                });
                cur = node.child_by_field_name("object");
            }
            _ => {
                links.push(Link {
                    name,
                    scope: None,
                    node,
                });
                cur = None;
            }
        }
    }
    links.reverse();
    links
}

fn resolve_class(name: &str, ctx: &FileCtx, g: &Group) -> String {
    let name = name.trim();
    if let Some(ns) = &g.namespace {
        if !name.starts_with('\\')
            && !ctx
                .index
                .uses
                .iter()
                .any(|u| u.local_name() == name.split('\\').next().unwrap_or(name))
        {
            return format!("{ns}\\{name}");
        }
    }
    ctx.index.resolve_class(name, 0)
}

fn class_arg(node: Node, ctx: &FileCtx, g: &Group) -> Option<String> {
    match php::value(node, ctx.src) {
        Value::ClassRef(c) => Some(resolve_class(&c, ctx, g)),
        Value::Str(s) => Some(resolve_class(&s, ctx, g)),
        _ => None,
    }
}

/// `(label, controller, method)` of a route action.
fn describe_action(
    node: Node,
    ctx: &FileCtx,
    g: &Group,
) -> (String, Option<String>, Option<String>) {
    match php::value(node, ctx.src) {
        Value::Array(entries) => {
            let class = entries.first().and_then(|e| match &e.value {
                Value::ClassRef(c) => Some(resolve_class(c, ctx, g)),
                Value::Str(s) => Some(resolve_class(s, ctx, g)),
                _ => None,
            });
            let method = entries
                .get(1)
                .and_then(|e| e.value.as_str().map(String::from));
            match class {
                Some(c) => {
                    let m = method.clone().unwrap_or_else(|| "__invoke".into());
                    (format!("{c}@{m}"), Some(c), Some(m))
                }
                None => (php::text(node, ctx.src).to_string(), None, None),
            }
        }
        Value::ClassRef(c) => {
            let c = resolve_class(&c, ctx, g);
            (format!("{c}@__invoke"), Some(c), Some("__invoke".into()))
        }
        Value::Str(s) => {
            if let Some((class, method)) = s.split_once('@') {
                let c = resolve_class(class, ctx, g);
                (format!("{c}@{method}"), Some(c), Some(method.to_string()))
            } else if let Some(c) = &g.controller {
                // Route::controller(X::class)->group(fn () => Route::get('/x', 'show'))
                (format!("{c}@{s}"), Some(c.clone()), Some(s))
            } else {
                let c = resolve_class(&s, ctx, g);
                (format!("{c}@__invoke"), Some(c), Some("__invoke".into()))
            }
        }
        _ => ("Closure".into(), None, None),
    }
}

/// Group attributes from an array: `['prefix' => 'admin', 'as' => 'admin.', 'middleware' => 'auth']`.
fn array_attrs(v: &Value, ctx: &FileCtx, g: &Group) -> Attrs {
    let e = v.entries();
    let get = |k: &str| php::Entry::get(e, k);
    Attrs {
        prefix: get("prefix").and_then(|v| v.as_str().map(String::from)),
        name: get("as").and_then(|v| v.as_str().map(String::from)),
        middleware: get("middleware").map(|v| v.strings()).unwrap_or_default(),
        controller: get("controller").and_then(|v| match v {
            Value::ClassRef(c) => Some(resolve_class(c, ctx, g)),
            _ => None,
        }),
        namespace: get("namespace").and_then(|v| v.as_str().map(String::from)),
        domain: get("domain").and_then(|v| v.as_str().map(String::from)),
    }
}

/// The route file referenced by `require __DIR__.'/auth.php'`, `base_path('routes/x.php')`
/// or a plain path string.
fn included_file(node: Node, ctx: &FileCtx) -> Option<String> {
    let mut found: Option<String> = None;
    let mut base_path_call = false;
    php::walk(node, &mut |n| {
        if n.kind() == "function_call_expression" && call_name(n, ctx.src) == Some("base_path") {
            base_path_call = true;
        }
        if found.is_none() {
            if let Some(s) = php::string_value(n, ctx.src) {
                if s.ends_with(".php") {
                    found = Some(s);
                }
            }
        }
        true
    });
    let s = found?;
    let dir = ctx.rel.rsplit_once('/').map_or("", |(d, _)| d);
    let rel = if base_path_call || s.starts_with("routes/") {
        s.trim_start_matches('/').to_string()
    } else {
        format!("{dir}/{}", s.trim_start_matches('/'))
    };
    Some(normalize(&rel))
}

fn normalize(path: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            s => out.push(s),
        }
    }
    out.join("/")
}

/// Routes from `php artisan route:list --json`. Source locations are taken from the
/// statically parsed routes when method and URI match.
pub fn parse_route_list_json(
    json: &str,
    root: &Path,
    psr4: &Psr4,
    statics: &[Route],
) -> Option<Vec<Route>> {
    #[derive(Deserialize)]
    struct Raw {
        #[serde(default)]
        domain: Option<String>,
        method: String,
        uri: String,
        #[serde(default)]
        name: Option<String>,
        action: String,
        #[serde(default)]
        middleware: Vec<String>,
    }
    let raw: Vec<Raw> = serde_json::from_str(json.trim()).ok()?;
    let routes = raw
        .into_iter()
        .map(|r| {
            let methods: Vec<String> = r.method.split('|').map(String::from).collect();
            let (controller, method) = match r.action.split_once('@') {
                Some((c, m)) => (Some(c.to_string()), Some(m.to_string())),
                None if r.action.contains('\\') => {
                    (Some(r.action.clone()), Some("__invoke".to_string()))
                }
                None => (None, None),
            };
            let uri = final_uri(r.uri.trim_matches('/').to_string());
            let source = statics
                .iter()
                .find(|s| s.uri == uri && s.methods == methods)
                .and_then(|s| s.source.clone());
            let action_location = controller
                .as_deref()
                .and_then(|c| action_location(root, psr4, c, method.as_deref()));
            Route {
                methods,
                uri,
                name: r.name.filter(|n| !n.is_empty()),
                action: r.action,
                controller,
                controller_method: method,
                middleware: r.middleware,
                domain: r.domain.filter(|d| !d.is_empty()),
                source,
                action_location,
            }
        })
        .collect();
    Some(routes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn singularizes_resource_names() {
        assert_eq!(singular("photos"), "photo");
        assert_eq!(singular("categories"), "category");
        assert_eq!(singular("addresses"), "address");
        assert_eq!(singular("news"), "news");
        assert_eq!(singular("status"), "status");
        assert_eq!(singular("blog-posts"), "blog_post");
    }

    #[test]
    fn route_parameters() {
        let r = Route {
            methods: vec!["GET".into()],
            uri: "users/{user}/posts/{post:slug}/{page?}".into(),
            name: None,
            action: "Closure".into(),
            controller: None,
            controller_method: None,
            middleware: vec![],
            domain: None,
            source: None,
            action_location: None,
        };
        assert_eq!(r.parameters(), vec!["user", "post", "page"]);
    }
}
