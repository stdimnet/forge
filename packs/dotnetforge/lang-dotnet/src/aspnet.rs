//! ASP.NET Core endpoints for the Endpoints tool window, like Rider's: minimal APIs
//! (`app.MapGet("/books/{id}", …)`, `MapPost`, `MapMethods`, route groups from
//! `MapGroup`), MVC / Web API controllers (`[Route]`, `[HttpGet]`, `[controller]` and
//! `[action]` tokens, `[Area]`), Blazor pages (`@page` in `.razor`) and Razor Pages
//! (`.cshtml` under `Pages/`). Also the `.http` request of "Generate HTTP Request" and the
//! base URL from the launch profiles.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::csharp::{
    argument_ranges, arguments, attribute_list, matching, tokenize, Attribute, Tok, Token,
    MODIFIERS,
};
use crate::launch::LaunchProfile;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EndpointParam {
    pub name: String,
    /// `path`, `query`, `body`, `header` or `form`.
    pub kind: String,
    #[serde(default)]
    pub type_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Endpoint {
    /// `GET`, `POST`, …, or `ANY` (`Map`, controller actions without a verb).
    pub method: String,
    /// The route template (`/api/books/{id:int}`).
    pub path: String,
    /// `BooksController.Get`, a method group, or `lambda`.
    pub handler: String,
    /// `minimal`, `controller`, `blazor` or `razorPages`.
    pub framework: String,
    pub file: String,
    /// 0-based line.
    pub line: u32,
    /// `WithName("…")` / `Name = "…"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<EndpointParam>,
    /// `RequireAuthorization()` / `[Authorize]`.
    #[serde(default)]
    pub authorized: bool,
    /// The project file (filled in by the caller).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
}

const VERBS: &[(&str, &str)] = &[
    ("MapGet", "GET"),
    ("MapPost", "POST"),
    ("MapPut", "PUT"),
    ("MapDelete", "DELETE"),
    ("MapPatch", "PATCH"),
    ("Map", "ANY"),
    ("MapMethods", ""),
    ("MapHealthChecks", "GET"),
];

/// `a` + `b` as one route (`/api` + `{id}` → `/api/{id}`).
pub fn join_route(a: &str, b: &str) -> String {
    let b = b.trim();
    if let Some(abs) = b.strip_prefix("~/") {
        return format!("/{abs}");
    }
    let a = a.trim().trim_end_matches('/');
    let b = b.trim_start_matches('/');
    let joined = match (a.is_empty(), b.is_empty()) {
        (true, true) => String::new(),
        (true, false) => b.to_string(),
        (false, true) => a.trim_start_matches('/').to_string(),
        (false, false) => format!("{}/{b}", a.trim_start_matches('/')),
    };
    format!("/{joined}")
}

/// Every endpoint of a source file (`.cs`, `.razor`, `.cshtml`).
pub fn scan_file(path: &str, text: &str) -> Vec<Endpoint> {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".razor") {
        return razor_pages(path, text, "blazor");
    }
    if lower.ends_with(".cshtml") {
        return razor_pages(path, text, "razorPages");
    }
    if !lower.ends_with(".cs") {
        return Vec::new();
    }
    // Cheap filter: most files have neither.
    if !text.contains("Map") && !text.contains("Http") && !text.contains("Route") {
        return Vec::new();
    }
    let tokens = tokenize(text);
    let mut out = minimal_apis(path, &tokens);
    out.extend(controllers(path, &tokens));
    out
}

// ---- Minimal APIs ------------------------------------------------------------------------------

fn simple_type(t: &str) -> bool {
    let t = t.trim_end_matches('?').trim_end_matches("[]");
    matches!(
        t,
        "int"
            | "long"
            | "short"
            | "uint"
            | "ulong"
            | "byte"
            | "string"
            | "bool"
            | "decimal"
            | "double"
            | "float"
            | "Guid"
            | "DateTime"
            | "DateTimeOffset"
            | "DateOnly"
            | "TimeOnly"
            | "TimeSpan"
            | "char"
            | "Int32"
            | "Int64"
            | "String"
            | "Boolean"
    )
}

/// Route parameter names of a template (`{id:int}`, `{**slug}`, `{page?}`).
pub fn route_params(path: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = path;
    while let Some(s) = rest.find('{') {
        let Some(e) = rest[s..].find('}') else { break };
        let inner = &rest[s + 1..s + e];
        if !inner.starts_with('{') {
            let name = inner
                .trim_start_matches('*')
                .split([':', '=', '?'])
                .next()
                .unwrap_or("")
                .trim();
            if !name.is_empty() && !out.iter().any(|x| x == name) {
                out.push(name.to_string());
            }
        }
        rest = &rest[s + e + 1..];
    }
    out
}

/// Parameters of a handler's parameter list (lambda or action method).
fn parameters(tokens: &[Token], open: usize, route: &str, method: &str) -> Vec<EndpointParam> {
    let in_route = route_params(route);
    let mut out = Vec::new();
    for arg in arguments(tokens, open) {
        let mut i = 0;
        let mut source = None;
        while i < arg.len() && arg[i].is('[') {
            let end = matching(arg, i);
            let (attrs, _) = attribute_list(arg, i);
            for a in attrs {
                source = match a.name.as_str() {
                    "FromRoute" => Some("path"),
                    "FromQuery" => Some("query"),
                    "FromBody" => Some("body"),
                    "FromHeader" => Some("header"),
                    "FromForm" => Some("form"),
                    "FromServices" | "FromKeyedServices" => Some("service"),
                    "AsParameters" => Some("service"),
                    _ => source,
                };
            }
            i = end;
        }
        let rest: Vec<&Token> = arg[i..]
            .iter()
            .take_while(|t| !t.is('='))
            .filter(|t| !t.is_ident("this") && !t.is_ident("ref") && !t.is_ident("out"))
            .collect();
        let Some(name) = rest.last().and_then(|t| t.ident()) else {
            continue;
        };
        if rest.len() < 2 {
            continue;
        }
        let type_tokens: Vec<Token> = rest[..rest.len() - 1]
            .iter()
            .map(|t| (*t).clone())
            .collect();
        let type_name = crate::csharp::text(&type_tokens);
        let kind = match source {
            Some(s) => s,
            None if in_route.iter().any(|r| r.eq_ignore_ascii_case(name)) => "path",
            None if simple_type(&type_name) => "query",
            None if type_name.ends_with("CancellationToken")
                || type_name.ends_with("HttpContext")
                || type_name.ends_with("HttpRequest")
                || type_name.ends_with("HttpResponse")
                || type_name.ends_with("ClaimsPrincipal")
                || type_name.ends_with("IFormFile")
                || type_name.starts_with("ILogger") =>
            {
                "service"
            }
            // Complex types are the body of POST / PUT / PATCH, services otherwise.
            None if matches!(method, "POST" | "PUT" | "PATCH") && !type_name.starts_with('I') => {
                if out.iter().any(|p: &EndpointParam| p.kind == "body") {
                    "service"
                } else {
                    "body"
                }
            }
            None => "service",
        };
        if kind == "service" {
            continue;
        }
        out.push(EndpointParam {
            name: name.to_string(),
            kind: kind.into(),
            type_name,
        });
    }
    out
}

/// The handler of a `MapGet(route, handler)` call and its parameter list (for lambdas).
fn handler(arg: &[Token], offset: usize) -> (String, Option<usize>) {
    let mut i = 0;
    while i < arg.len() && (arg[i].is_ident("async") || arg[i].is('[')) {
        i = if arg[i].is('[') {
            matching(arg, i)
        } else {
            i + 1
        };
    }
    if i < arg.len() && arg[i].is('(') {
        let close = matching(arg, i);
        if close < arg.len() && arg[close].is('=') {
            return ("lambda".into(), Some(offset + i));
        }
    }
    if arg.len() > i + 1 && arg[i].ident().is_some() && arg[i + 1].is('=') {
        // `id => …`
        return ("lambda".into(), None);
    }
    if arg.iter().all(|t| t.ident().is_some() || t.is('.')) {
        let name: String = arg
            .iter()
            .map(|t| match &t.tok {
                Tok::Ident(s) => s.clone(),
                _ => ".".into(),
            })
            .collect();
        return (name, None);
    }
    ("lambda".into(), None)
}

fn minimal_apis(path: &str, tokens: &[Token]) -> Vec<Endpoint> {
    let mut out = Vec::new();
    // Route group variables → their prefix.
    let mut groups: BTreeMap<String, String> = BTreeMap::new();
    let mut i = 0;
    // The assignment target of the current statement (`var api = …`).
    let mut target: Option<String> = None;
    // The current member-access chain: its route prefix and whether it ends in a group.
    let mut prefix = String::new();
    let mut chain_group = false;
    // Endpoints of the current chain (for `.WithName`, `.RequireAuthorization`).
    let mut chain: Vec<usize> = Vec::new();
    while i < tokens.len() {
        let t = &tokens[i];
        if t.is(';') || t.is('{') || t.is('}') {
            if let Some(name) = target.take() {
                if chain_group {
                    groups.insert(name, prefix.clone());
                }
            }
            prefix.clear();
            chain_group = false;
            chain.clear();
            i += 1;
            continue;
        }
        if let Some(id) = t.ident() {
            let after_dot = i > 0 && tokens[i - 1].is('.');
            if !after_dot {
                // `var api = …` / `api = …`.
                if tokens.get(i + 1).is_some_and(|n| n.is('='))
                    && !tokens.get(i + 2).is_some_and(|n| n.is('=') || n.is('>'))
                {
                    target = Some(id.to_string());
                    i += 2;
                    continue;
                }
                prefix = groups.get(id).cloned().unwrap_or_default();
                chain_group = groups.contains_key(id);
                chain.clear();
                i += 1;
                continue;
            }
            let call = tokens.get(i + 1).is_some_and(|n| n.is('('))
                || tokens.get(i + 1).is_some_and(|n| n.is('<'));
            if !call {
                i += 1;
                continue;
            }
            let mut open = i + 1;
            if tokens[open].is('<') {
                open = matching(tokens, open);
                if !tokens.get(open).is_some_and(|t| t.is('(')) {
                    i += 1;
                    continue;
                }
            }
            let ranges = argument_ranges(tokens, open);
            let args: Vec<&[Token]> = ranges.iter().map(|&(a, b)| &tokens[a..b]).collect();
            let route = args.first().and_then(|a| match a {
                [s] => s.string().map(str::to_string),
                _ => None,
            });
            let end = matching(tokens, open);
            match id {
                "MapGroup" => {
                    if let Some(r) = route {
                        prefix = join_route(&prefix, &r);
                        chain_group = true;
                    }
                    chain.clear();
                }
                "WithName" | "WithDisplayName" => {
                    if let Some(n) = route {
                        for &e in &chain {
                            let ep: &mut Endpoint = &mut out[e];
                            ep.name.get_or_insert(n.clone());
                        }
                    }
                }
                "RequireAuthorization" => {
                    for &e in &chain {
                        let ep: &mut Endpoint = &mut out[e];
                        ep.authorized = true;
                    }
                }
                _ => {
                    if let (Some(verb), Some(r)) =
                        (VERBS.iter().find(|(m, _)| *m == id).map(|(_, v)| *v), route)
                    {
                        let full = join_route(&prefix, &r);
                        let methods: Vec<String> = if verb.is_empty() {
                            args.get(1)
                                .map(|a| {
                                    a.iter()
                                        .filter_map(|t| t.string().map(|s| s.to_ascii_uppercase()))
                                        .collect()
                                })
                                .unwrap_or_default()
                        } else {
                            vec![verb.to_string()]
                        };
                        let handler_at = if verb.is_empty() { 2 } else { 1 };
                        let (name, lambda) = match ranges.get(handler_at) {
                            Some(&(a, b)) if id != "MapHealthChecks" => handler(&tokens[a..b], a),
                            _ if id == "MapHealthChecks" => ("Health checks".into(), None),
                            _ => ("lambda".into(), None),
                        };
                        chain.clear();
                        for m in methods {
                            let params = lambda
                                .map(|o| parameters(tokens, o, &full, &m))
                                .unwrap_or_else(|| {
                                    route_params(&full)
                                        .into_iter()
                                        .map(|n| EndpointParam {
                                            name: n,
                                            kind: "path".into(),
                                            type_name: String::new(),
                                        })
                                        .collect()
                                });
                            chain.push(out.len());
                            out.push(Endpoint {
                                method: m,
                                path: full.clone(),
                                handler: name.clone(),
                                framework: "minimal".into(),
                                file: path.to_string(),
                                line: tokens[i].line,
                                name: None,
                                params,
                                authorized: false,
                                project: None,
                            });
                        }
                        chain_group = false;
                    }
                }
            }
            i = end;
            continue;
        }
        i += 1;
    }
    out
}

// ---- Controllers --------------------------------------------------------------------------------

const HTTP_ATTRIBUTES: &[(&str, &str)] = &[
    ("HttpGet", "GET"),
    ("HttpPost", "POST"),
    ("HttpPut", "PUT"),
    ("HttpDelete", "DELETE"),
    ("HttpPatch", "PATCH"),
    ("HttpHead", "HEAD"),
    ("HttpOptions", "OPTIONS"),
];

fn replace_tokens(route: &str, controller: &str, action: &str, area: &str) -> String {
    let mut out = String::new();
    let mut rest = route;
    while let Some(s) = rest.find('[') {
        out.push_str(&rest[..s]);
        let Some(e) = rest[s..].find(']') else {
            out.push_str(&rest[s..]);
            rest = "";
            break;
        };
        let token = &rest[s + 1..s + e];
        out.push_str(match token.to_ascii_lowercase().as_str() {
            "controller" => controller,
            "action" => action,
            "area" => area,
            _ => token,
        });
        rest = &rest[s + e + 1..];
    }
    out.push_str(rest);
    out
}

struct ControllerScope {
    name: String,
    routes: Vec<String>,
    area: String,
    authorized: bool,
    depth: i32,
}

fn controllers(path: &str, tokens: &[Token]) -> Vec<Endpoint> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut pending: Vec<Attribute> = Vec::new();
    let mut scope: Option<ControllerScope> = None;
    let mut i = 0;
    while i < tokens.len() {
        let t = &tokens[i];
        if t.is('[') {
            let (attrs, end) = attribute_list(tokens, i);
            pending.extend(attrs);
            i = end;
            continue;
        }
        if t.is('{') {
            depth += 1;
            i += 1;
            continue;
        }
        if t.is('}') {
            depth -= 1;
            if scope.as_ref().is_some_and(|s| depth < s.depth) {
                scope = None;
            }
            pending.clear();
            i += 1;
            continue;
        }
        if t.is(';') {
            pending.clear();
            i += 1;
            continue;
        }
        if t.is_ident("class") {
            let Some(name) = tokens.get(i + 1).and_then(Token::ident) else {
                i += 1;
                continue;
            };
            // Base types up to the body.
            let mut j = i + 2;
            let mut bases = Vec::new();
            while j < tokens.len() && !tokens[j].is('{') && !tokens[j].is(';') {
                if let Some(b) = tokens[j].ident() {
                    bases.push(b.to_string());
                }
                j += 1;
            }
            let attr = |n: &str| pending.iter().any(|a| a.name == n);
            let is_controller = !attr("NonController")
                && (attr("ApiController")
                    || attr("Controller")
                    || bases
                        .iter()
                        .any(|b| b == "Controller" || b == "ControllerBase")
                    || name.ends_with("Controller") && !bases.is_empty());
            if is_controller {
                let routes = pending
                    .iter()
                    .filter(|a| a.name == "Route")
                    .filter_map(|a| a.strings.first().cloned())
                    .collect();
                let area = pending
                    .iter()
                    .find(|a| a.name == "Area")
                    .and_then(|a| a.strings.first().cloned())
                    .unwrap_or_default();
                scope = Some(ControllerScope {
                    name: name.to_string(),
                    routes,
                    area,
                    authorized: attr("Authorize"),
                    depth: depth + 1,
                });
            }
            pending.clear();
            i = j;
            continue;
        }
        if let (Some(s), Some(id)) = (scope.as_ref(), t.ident()) {
            // A method declaration at class level: `… Name(` after modifiers and a type.
            if depth == s.depth
                && tokens.get(i + 1).is_some_and(|n| n.is('('))
                && i > 0
                && !tokens[i - 1].is('.')
                && !tokens[i - 1].is('=')
                && !MODIFIERS.contains(&id)
            {
                let verbs: Vec<(&str, Option<String>, Option<String>)> = pending
                    .iter()
                    .filter_map(|a| {
                        let verb = HTTP_ATTRIBUTES.iter().find(|(n, _)| *n == a.name)?.1;
                        let name = a
                            .named
                            .iter()
                            .find(|(k, _)| k == "Name")
                            .map(|(_, v)| v.clone());
                        Some((verb, a.strings.first().cloned(), name))
                    })
                    .chain(
                        pending
                            .iter()
                            .filter(|a| a.name == "AcceptVerbs")
                            .flat_map(|a| {
                                a.strings
                                    .iter()
                                    .map(|v| (leak_verb(v), None, None))
                                    .collect::<Vec<_>>()
                            }),
                    )
                    .collect();
                let method_routes: Vec<String> = pending
                    .iter()
                    .filter(|a| a.name == "Route")
                    .filter_map(|a| a.strings.first().cloned())
                    .collect();
                let public = (0..i)
                    .rev()
                    .take_while(|&k| {
                        !tokens[k].is(';')
                            && !tokens[k].is('}')
                            && !tokens[k].is('{')
                            && !tokens[k].is(']')
                    })
                    .any(|k| tokens[k].is_ident("public"));
                let skip = pending.iter().any(|a| a.name == "NonAction") || !public;
                if !skip && (!verbs.is_empty() || !method_routes.is_empty()) {
                    let controller = s.name.strip_suffix("Controller").unwrap_or(&s.name);
                    let class_routes = if s.routes.is_empty() {
                        vec![String::new()]
                    } else {
                        s.routes.clone()
                    };
                    let verbs = if verbs.is_empty() {
                        vec![("ANY", None, None)]
                    } else {
                        verbs
                    };
                    let authorized = s.authorized || pending.iter().any(|a| a.name == "Authorize");
                    let mut templates: Vec<(String, Option<String>, &str)> = Vec::new();
                    for (verb, tpl, name) in &verbs {
                        match tpl {
                            Some(t) => templates.push((t.clone(), name.clone(), verb)),
                            None if method_routes.is_empty() => {
                                templates.push((String::new(), name.clone(), verb))
                            }
                            None => {
                                for r in &method_routes {
                                    templates.push((r.clone(), name.clone(), verb));
                                }
                            }
                        }
                    }
                    for (tpl, name, verb) in templates {
                        for base in &class_routes {
                            let route = if tpl.starts_with('/') || tpl.starts_with("~/") {
                                join_route("", &tpl)
                            } else {
                                join_route(base, &tpl)
                            };
                            let route = replace_tokens(&route, controller, id, &s.area);
                            let params = parameters(tokens, i + 1, &route, verb);
                            out.push(Endpoint {
                                method: verb.to_string(),
                                path: route,
                                handler: format!("{}.{id}", s.name),
                                framework: "controller".into(),
                                file: path.to_string(),
                                line: t.line,
                                name: name.clone(),
                                params,
                                authorized,
                                project: None,
                            });
                        }
                    }
                }
                pending.clear();
                i = matching(tokens, i + 1);
                continue;
            }
        }
        i += 1;
    }
    out
}

fn leak_verb(v: &str) -> &'static str {
    match v.to_ascii_uppercase().as_str() {
        "GET" => "GET",
        "POST" => "POST",
        "PUT" => "PUT",
        "DELETE" => "DELETE",
        "PATCH" => "PATCH",
        "HEAD" => "HEAD",
        "OPTIONS" => "OPTIONS",
        _ => "ANY",
    }
}

// ---- Blazor and Razor Pages -----------------------------------------------------------------------

/// `@page "/books/{id:int}"` directives (Blazor: every one is a route; Razor Pages: the
/// route follows the file's path below `Pages/`).
fn razor_pages(path: &str, text: &str, framework: &str) -> Vec<Endpoint> {
    let mut out = Vec::new();
    let name = crate::paths::file_name(path);
    let stem = name.split('.').next().unwrap_or(name).to_string();
    for (n, line) in text.lines().enumerate() {
        let l = line.trim();
        let Some(rest) = l.strip_prefix("@page") else {
            continue;
        };
        if rest.chars().next().is_some_and(|c| c.is_alphanumeric()) {
            continue;
        }
        let template = rest
            .trim()
            .trim_start_matches('"')
            .split('"')
            .next()
            .unwrap_or("")
            .trim()
            .to_string();
        let route = if framework == "blazor" {
            if template.is_empty() {
                continue;
            }
            join_route("", &template)
        } else {
            // Razor Pages: `Pages/Books/Index.cshtml` → `/Books`, plus the template.
            let norm = path.replace('\\', "/");
            let Some(rel) = norm
                .split_once("/Pages/")
                .map(|(_, r)| r.to_string())
                .or_else(|| norm.strip_prefix("Pages/").map(str::to_string))
            else {
                continue;
            };
            let rel = rel.trim_end_matches(".cshtml");
            let rel = rel
                .strip_suffix("Index")
                .unwrap_or(rel)
                .trim_end_matches('/');
            if template.starts_with('/') {
                join_route("", &template)
            } else {
                join_route(&join_route("", rel), &template)
            }
        };
        out.push(Endpoint {
            method: "GET".into(),
            params: route_params(&route)
                .into_iter()
                .map(|p| EndpointParam {
                    name: p,
                    kind: "path".into(),
                    type_name: String::new(),
                })
                .collect(),
            path: route,
            handler: stem.clone(),
            framework: framework.into(),
            file: path.to_string(),
            line: n as u32,
            name: None,
            authorized: text.contains("[Authorize]") || text.contains("@attribute [Authorize"),
            project: None,
        });
        if framework != "blazor" {
            break;
        }
    }
    out
}

// ---- Base URL and HTTP requests ---------------------------------------------------------------------

/// The URL a project listens on: `applicationUrl` of its first `Project` launch profile
/// (the `http` one when there are several URLs), else `http://localhost:5000`.
pub fn base_url(profiles: &[LaunchProfile]) -> String {
    let urls: Vec<&str> = profiles
        .iter()
        .filter(|p| p.command_name.eq_ignore_ascii_case("Project"))
        .flat_map(|p| p.application_url.split(';'))
        .map(str::trim)
        .filter(|u| !u.is_empty())
        .collect();
    urls.iter()
        .find(|u| u.starts_with("http://"))
        .or(urls.first())
        .map(|u| u.trim_end_matches('/').to_string())
        .unwrap_or_else(|| "http://localhost:5000".into())
}

/// An `.http` request for an endpoint (Rider's "Generate HTTP Request"): route parameters
/// become `{{variables}}` declared above the request (constraints dropped), query parameters
/// are listed, bodies get a JSON content type and an empty object.
pub fn http_request(e: &Endpoint, base_url: &str) -> String {
    let mut out = String::new();
    let mut path = String::new();
    let mut vars: Vec<(String, String)> = Vec::new();
    let mut rest = e.path.as_str();
    while let Some(s) = rest.find('{') {
        path.push_str(&rest[..s]);
        let Some(len) = rest[s..].find('}') else {
            path.push_str(&rest[s..]);
            rest = "";
            break;
        };
        let inner = &rest[s + 1..s + len];
        let name = inner
            .trim_start_matches('*')
            .split([':', '=', '?'])
            .next()
            .unwrap_or(inner)
            .trim()
            .to_string();
        let constraint = inner.split(':').nth(1).unwrap_or("");
        let typed = e
            .params
            .iter()
            .find(|p| p.kind == "path" && p.name.eq_ignore_ascii_case(&name))
            .map(|p| p.type_name.to_ascii_lowercase())
            .unwrap_or_default();
        let example = if constraint.starts_with("int")
            || constraint.starts_with("long")
            || typed.starts_with("int")
            || typed.starts_with("long")
        {
            "1"
        } else if constraint.starts_with("guid") || typed.starts_with("guid") {
            "00000000-0000-0000-0000-000000000000"
        } else if constraint.starts_with("bool") || typed.starts_with("bool") {
            "true"
        } else {
            "value"
        };
        path.push_str(&format!("{{{{{name}}}}}"));
        if !vars.iter().any(|(n, _)| *n == name) {
            vars.push((name, example.to_string()));
        }
        rest = &rest[s + len + 1..];
    }
    path.push_str(rest);
    for (v, example) in &vars {
        out.push_str(&format!("@{v} = {example}\n"));
    }
    if !vars.is_empty() {
        out.push('\n');
    }
    out.push_str(&format!(
        "### {}\n",
        e.name.clone().unwrap_or_else(|| if e.handler == "lambda" {
            format!("{} {}", e.method, e.path)
        } else {
            e.handler.clone()
        })
    ));
    let query: Vec<String> = e
        .params
        .iter()
        .filter(|p| p.kind == "query")
        .map(|p| format!("{}=", p.name))
        .collect();
    let method = if e.method == "ANY" { "GET" } else { &e.method };
    out.push_str(&format!(
        "{method} {}{path}",
        base_url.trim_end_matches('/')
    ));
    if !query.is_empty() {
        out.push('?');
        out.push_str(&query.join("&"));
    }
    out.push('\n');
    out.push_str("Accept: application/json\n");
    for h in e.params.iter().filter(|p| p.kind == "header") {
        out.push_str(&format!("{}: \n", h.name));
    }
    if e.params.iter().any(|p| p.kind == "body") {
        out.push_str("Content-Type: application/json\n\n{\n}\n");
    } else if e.params.iter().any(|p| p.kind == "form") {
        out.push_str("Content-Type: application/x-www-form-urlencoded\n\n");
        let form: Vec<String> = e
            .params
            .iter()
            .filter(|p| p.kind == "form")
            .map(|p| format!("{}=", p.name))
            .collect();
        out.push_str(&form.join("&"));
        out.push('\n');
    }
    out
}

/// Endpoints sorted by path, then method.
pub fn sort(endpoints: &mut [Endpoint]) {
    endpoints.sort_by(|a, b| {
        a.path
            .to_ascii_lowercase()
            .cmp(&b.path.to_ascii_lowercase())
            .then_with(|| a.method.cmp(&b.method))
            .then_with(|| a.file.cmp(&b.file))
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::fixture;

    #[test]
    fn minimal_apis_with_groups() {
        let e = scan_file("src/Api/Program.cs", &fixture("aspnet/Program.cs"));
        let rows: Vec<String> = e
            .iter()
            .map(|e| format!("{} {} {}", e.method, e.path, e.handler))
            .collect();
        assert_eq!(
            rows,
            [
                "GET / lambda",
                "GET /api/books lambda",
                "GET /api/books/{id:int} lambda",
                "POST /api/books BookHandlers.Create",
                "DELETE /api/books/{id:int} lambda",
                "GET /api/authors/{authorId}/books lambda",
                "GET /search lambda",
                "HEAD /search lambda",
                "GET /health Health checks",
            ]
        );
        let get = &e[2];
        assert_eq!(get.name.as_deref(), Some("GetBook"));
        assert_eq!(
            get.params[0],
            EndpointParam {
                name: "id".into(),
                kind: "path".into(),
                type_name: "int".into()
            }
        );
        assert!(e[4].authorized);
        let search = &e[6];
        assert_eq!(
            search
                .params
                .iter()
                .map(|p| (p.name.as_str(), p.kind.as_str()))
                .collect::<Vec<_>>(),
            [("q", "query"), ("page", "query")]
        );
        assert_eq!(e[1].line, 14);
    }

    #[test]
    fn controllers_with_tokens() {
        let e = scan_file(
            "src/Api/Controllers/OrdersController.cs",
            &fixture("aspnet/OrdersController.cs"),
        );
        let rows: Vec<String> = e
            .iter()
            .map(|e| format!("{} {} {}", e.method, e.path, e.handler))
            .collect();
        assert_eq!(
            rows,
            [
                "GET /api/Orders OrdersController.List",
                "GET /api/Orders/{id:guid} OrdersController.Get",
                "POST /api/Orders OrdersController.Create",
                "PUT /api/Orders/{id:guid}/status OrdersController.SetStatus",
                "GET /api/Orders/Export OrdersController.Export",
                "GET /admin/stats/Summary OrdersController.Summary",
            ]
        );
        assert_eq!(e[1].name.as_deref(), Some("GetOrder"));
        assert!(e[2].authorized);
        assert_eq!(
            e[2].params,
            [EndpointParam {
                name: "order".into(),
                kind: "body".into(),
                type_name: "NewOrder".into()
            }]
        );
        assert_eq!(e[0].params[0].kind, "query");
    }

    #[test]
    fn blazor_and_razor_pages() {
        let b = scan_file(
            "src/Web/Components/Pages/Counter.razor",
            "@page \"/counter\"\n@page \"/counter/{start:int}\"\n<h1>Counter</h1>",
        );
        assert_eq!(
            b.iter().map(|e| e.path.as_str()).collect::<Vec<_>>(),
            ["/counter", "/counter/{start:int}"]
        );
        assert_eq!(b[0].framework, "blazor");
        assert_eq!(b[1].handler, "Counter");
        let p = scan_file(
            "src/Web/Pages/Books/Index.cshtml",
            "@page\n@model IndexModel\n",
        );
        assert_eq!(p[0].path, "/Books");
        let p = scan_file("src/Web/Pages/Books/Edit.cshtml", "@page \"{id:int}\"\n");
        assert_eq!(p[0].path, "/Books/Edit/{id:int}");
        assert!(scan_file("src/Web/Pages/Shared/_Layout.cshtml", "<html/>").is_empty());
    }

    #[test]
    fn http_requests_and_base_urls() {
        let e = scan_file("src/Api/Program.cs", &fixture("aspnet/Program.cs"));
        let r = http_request(&e[2], "http://localhost:5080");
        assert_eq!(r, "@id = 1\n\n### GetBook\nGET http://localhost:5080/api/books/{{id}}\nAccept: application/json\n");
        let post = http_request(&e[3], "http://localhost:5080/");
        assert!(post.contains("POST http://localhost:5080/api/books\n"));
        assert!(post.ends_with("Accept: application/json\n"));
        let c = scan_file("C.cs", &fixture("aspnet/OrdersController.cs"));
        let create = http_request(&c[2], "http://h");
        assert!(create.starts_with("### OrdersController.Create\nPOST http://h/api/Orders\n"));
        assert!(create.ends_with("Content-Type: application/json\n\n{\n}\n"));
        let search = http_request(&e[6], "http://h");
        assert!(search.contains("GET http://h/search?q=&page=\n"));
        let profiles = crate::launch::parse_launch_settings(&fixture(
            "bookstore/src/Bookstore.Api/Properties/launchSettings.json",
        ))
        .unwrap();
        assert!(base_url(&profiles).starts_with("http://localhost:"));
        assert_eq!(base_url(&[]), "http://localhost:5000");
    }

    #[test]
    fn joins_routes() {
        assert_eq!(join_route("/api/", "/books"), "/api/books");
        assert_eq!(join_route("api", ""), "/api");
        assert_eq!(join_route("api", "~/root"), "/root");
        assert_eq!(join_route("", ""), "/");
        assert_eq!(
            route_params("/a/{id:int}/{**slug}/{p?}"),
            ["id", "slug", "p"]
        );
    }
}
