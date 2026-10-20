//! Routes from `#[Route]` attributes, legacy `@Route` annotations and YAML route files
//! (`config/routes.yaml`, `config/routes/*.yaml`), including import prefixes.

use std::path::{Path, PathBuf};

use phpforge_lang_php::docblock::AnnotationArg;
use phpforge_lang_php::{Attribute, ClassLike, Member};
use serde::{Deserialize, Serialize};
use yaml_rust2::{Yaml, YamlLoader};

use crate::container::key_line;
use crate::{rel_path, Location, PhpFile};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RouteSource {
    Attribute,
    Annotation,
    Yaml,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Route {
    pub name: String,
    pub path: String,
    /// HTTP methods; empty means any.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub methods: Vec<String>,
    /// `App\Controller\OrderController::show`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub controller: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    pub source: RouteSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<Location>,
}

/// Value of a PHP or annotation literal: `'x'`, `"x"` -> `x`.
pub(crate) fn unquote(v: &str) -> String {
    let v = v.trim();
    for q in ['"', '\''] {
        if v.len() >= 2 && v.starts_with(q) && v.ends_with(q) {
            return v[1..v.len() - 1].replace("\\\\", "\\");
        }
    }
    v.to_string()
}

/// Items of an array literal: `['GET', 'POST']`, `{"GET","HEAD"}` or a single `'GET'`.
pub(crate) fn list_items(v: &str) -> Vec<String> {
    let v = v.trim();
    let inner = v
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .or_else(|| v.strip_prefix('{').and_then(|s| s.strip_suffix('}')))
        .or_else(|| v.strip_prefix("array(").and_then(|s| s.strip_suffix(')')));
    match inner {
        Some(inner) => inner
            .split(',')
            .map(unquote)
            .filter(|s| !s.is_empty())
            .collect(),
        None => unquote(v)
            .split('|')
            .map(str::to_string)
            .filter(|s| !s.is_empty())
            .collect(),
    }
}

fn arg<'a>(args: &'a [AnnotationArg], name: &str, position: Option<usize>) -> Option<&'a str> {
    if let Some(a) = args.iter().find(|a| a.name.as_deref() == Some(name)) {
        return Some(&a.value);
    }
    let pos = position?;
    args.iter()
        .filter(|a| a.name.is_none())
        .nth(pos)
        .map(|a| a.value.as_str())
}

/// Route definition from attribute or annotation arguments (path, name, methods, host).
struct RouteArgs {
    path: Option<String>,
    name: Option<String>,
    methods: Vec<String>,
    host: Option<String>,
}

fn route_args(args: &[AnnotationArg]) -> RouteArgs {
    let path = arg(args, "path", Some(0))
        .or_else(|| arg(args, "value", None))
        .map(|p| {
            // Localized paths: show the first one.
            if p.trim_start().starts_with('[') || p.trim_start().starts_with('{') {
                p.split("=>")
                    .nth(1)
                    .or_else(|| p.split(['=', ':']).nth(1))
                    .map(|s| unquote(s.trim_end_matches([']', '}', ',', ' '])))
                    .unwrap_or_default()
            } else {
                unquote(p)
            }
        });
    RouteArgs {
        path,
        name: arg(args, "name", None).map(unquote),
        methods: arg(args, "methods", None)
            .map(list_items)
            .unwrap_or_default()
            .into_iter()
            .map(|m| m.to_uppercase())
            .collect(),
        host: arg(args, "host", None).map(unquote),
    }
}

fn is_route_attr(a: &Attribute) -> bool {
    a.fqn.ends_with("Routing\\Attribute\\Route")
        || a.fqn.ends_with("Routing\\Annotation\\Route")
        || a.name == "Route"
}

/// Symfony's default route name: `app_order_index` for `App\Controller\OrderController::index`.
pub fn default_route_name(class: &str, method: &str) -> String {
    let raw = format!("{}_{}", class.replace('\\', "_"), method).to_lowercase();
    let mut name = raw.replace("bundle_", "_").replace("controller_", "_");
    while name.contains("__") {
        name = name.replace("__", "_");
    }
    name
}

fn join_path(prefix: &str, path: &str) -> String {
    if prefix.is_empty() {
        return if path.is_empty() {
            "/".into()
        } else {
            path.into()
        };
    }
    let joined = format!("{}{}", prefix.trim_end_matches('/'), path);
    if joined.is_empty() {
        "/".into()
    } else {
        joined
    }
}

/// Class-level `#[Route]` (prefix) and the routes of one action.
fn class_routes(file: &PhpFile, class: &ClassLike, out: &mut Vec<Route>) {
    let class_attr = class.attributes.iter().find(|a| is_route_attr(a));
    let class_ann = class
        .doc
        .as_ref()
        .and_then(|d| d.annotations.iter().find(|a| a.name == "Route"));
    let prefix = class_attr
        .map(|a| route_args(&a.args))
        .or_else(|| class_ann.map(|a| route_args(&a.args)));
    let (path_prefix, name_prefix, class_methods) = match &prefix {
        Some(p) => (
            p.path.clone().unwrap_or_default(),
            p.name.clone().unwrap_or_default(),
            p.methods.clone(),
        ),
        None => (String::new(), String::new(), Vec::new()),
    };
    let mut add = |m: &Member, args: RouteArgs, source: RouteSource| {
        let name = match args.name {
            Some(n) => format!("{name_prefix}{n}"),
            None => default_route_name(&class.fqn, &m.name),
        };
        let methods = if args.methods.is_empty() {
            class_methods.clone()
        } else {
            args.methods
        };
        out.push(Route {
            name,
            path: join_path(&path_prefix, &args.path.unwrap_or_default()),
            methods,
            controller: Some(format!("{}::{}", class.fqn, m.name)),
            host: args.host,
            source,
            location: Some(Location::new(
                &file.path,
                m.name_range.line,
                m.name_range.column,
            )),
        });
    };
    for m in class.methods() {
        for a in m.attributes.iter().filter(|a| is_route_attr(a)) {
            add(m, route_args(&a.args), RouteSource::Attribute);
        }
        if let Some(doc) = &m.doc {
            for a in doc.annotations.iter().filter(|a| a.name == "Route") {
                add(m, route_args(&a.args), RouteSource::Annotation);
            }
        }
    }
    // Invokable controller with the route on the class.
    if let (Some(p), Some(invoke)) = (&prefix, class.method("__invoke")) {
        if !invoke.attributes.iter().any(is_route_attr) && p.name.is_some() {
            out.push(Route {
                name: p.name.clone().unwrap_or_default(),
                path: join_path("", &path_prefix),
                methods: class_methods.clone(),
                controller: Some(class.fqn.clone()),
                host: p.host.clone(),
                source: if class_attr.is_some() {
                    RouteSource::Attribute
                } else {
                    RouteSource::Annotation
                },
                location: Some(Location::new(
                    &file.path,
                    class.name_range.line,
                    class.name_range.column,
                )),
            });
        }
    }
}

/// Routes declared on controller classes.
pub fn from_php(php: &[PhpFile]) -> Vec<Route> {
    let mut out = Vec::new();
    for f in php {
        for c in &f.index.classes {
            class_routes(f, c, &mut out);
        }
    }
    out
}

fn yget<'a>(y: &'a Yaml, key: &str) -> Option<&'a Yaml> {
    match y {
        Yaml::Hash(h) => h.get(&Yaml::String(key.to_string())),
        _ => None,
    }
}

fn ystr(y: &Yaml) -> Option<String> {
    match y {
        Yaml::String(s) => Some(s.clone()),
        Yaml::Integer(i) => Some(i.to_string()),
        _ => None,
    }
}

fn route_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for name in ["routes.yaml", "routes.yml"] {
        let p = root.join("config").join(name);
        if p.is_file() {
            out.push(p);
        }
    }
    if let Ok(entries) = std::fs::read_dir(root.join("config/routes")) {
        let mut v: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| matches!(p.extension().and_then(|e| e.to_str()), Some("yaml" | "yml")))
            .collect();
        v.sort();
        out.extend(v);
    }
    out
}

/// Add the routes of YAML files and apply import prefixes (`prefix: /api`) to the attribute
/// routes of the imported controller directories.
pub fn from_yaml_files(root: &Path, routes: &mut Vec<Route>) {
    for file in route_files(root) {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        let Ok(docs) = YamlLoader::load_from_str(&text) else {
            continue;
        };
        let Some(Yaml::Hash(defs)) = docs.first() else {
            continue;
        };
        let rel = rel_path(root, &file);
        let base_dir = Path::new(&rel)
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        for (k, v) in defs {
            let Some(name) = ystr(k) else { continue };
            if name.starts_with("when@") {
                continue;
            }
            if let Some(resource) = yget(v, "resource") {
                let dir = match resource {
                    Yaml::String(s) => Some(s.clone()),
                    other => yget(other, "path").and_then(ystr),
                };
                let prefix = yget(v, "prefix").and_then(ystr).unwrap_or_default();
                let name_prefix = yget(v, "name_prefix").and_then(ystr).unwrap_or_default();
                if let Some(dir) = dir {
                    if prefix.is_empty() && name_prefix.is_empty() {
                        continue;
                    }
                    let dir = crate_normalize(&format!("{base_dir}/{dir}"));
                    for r in routes.iter_mut() {
                        let inside = r
                            .location
                            .as_ref()
                            .is_some_and(|l| l.path.starts_with(&dir));
                        if inside && r.source != RouteSource::Yaml {
                            r.path = join_path(&prefix, &r.path);
                            r.name = format!("{name_prefix}{}", r.name);
                        }
                    }
                }
                continue;
            }
            let Some(path) = yget(v, "path").map(|p| match p {
                Yaml::Hash(h) => h.values().next().and_then(ystr).unwrap_or_default(),
                other => ystr(other).unwrap_or_default(),
            }) else {
                continue;
            };
            let controller = yget(v, "controller").and_then(ystr).or_else(|| {
                yget(v, "defaults")
                    .and_then(|d| yget(d, "_controller"))
                    .and_then(ystr)
            });
            let methods = match yget(v, "methods") {
                Some(Yaml::Array(a)) => a
                    .iter()
                    .filter_map(ystr)
                    .map(|m| m.to_uppercase())
                    .collect(),
                Some(other) => ystr(other)
                    .map(|s| s.split('|').map(|m| m.trim().to_uppercase()).collect())
                    .unwrap_or_default(),
                None => Vec::new(),
            };
            let line = key_line(&text, &name, Some(0), 0);
            routes.push(Route {
                name,
                path,
                methods,
                controller,
                host: yget(v, "host").and_then(ystr),
                source: RouteSource::Yaml,
                location: line.map(|l| Location::new(&rel, l, 0)),
            });
        }
    }
}

fn crate_normalize(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for p in path.split('/') {
        match p {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            _ => parts.push(p),
        }
    }
    parts.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture;

    #[test]
    fn attribute_routes_with_class_prefix() {
        let dir = fixture::project();
        let php = crate::index_php_files(dir.path(), "src");
        let routes = from_php(&php);
        let show = routes.iter().find(|r| r.name == "order_show").unwrap();
        assert_eq!(show.path, "/orders/{id}");
        assert_eq!(show.methods, vec!["GET"]);
        assert_eq!(
            show.controller.as_deref(),
            Some("App\\Controller\\OrderController::show")
        );
        assert_eq!(show.location.as_ref().unwrap().line, 20);
        let index = routes.iter().find(|r| r.name == "order_index").unwrap();
        assert_eq!(index.path, "/orders");
    }

    #[test]
    fn legacy_annotations() {
        let dir = fixture::project();
        let php = crate::index_php_files(dir.path(), "src");
        let routes = from_php(&php);
        let r = routes.iter().find(|r| r.name == "legacy_report").unwrap();
        assert_eq!(r.path, "/legacy/report");
        assert_eq!(r.methods, vec!["GET", "HEAD"]);
        assert_eq!(r.source, RouteSource::Annotation);
    }

    #[test]
    fn yaml_routes() {
        let dir = fixture::project();
        let mut routes = Vec::new();
        from_yaml_files(dir.path(), &mut routes);
        let health = routes.iter().find(|r| r.name == "api_health").unwrap();
        assert_eq!(health.path, "/api/health");
        assert_eq!(health.methods, vec!["GET"]);
        assert_eq!(health.location.as_ref().unwrap().line, 12);
        assert!(routes.iter().any(|r| r.name == "home" && r.path == "/"));
        assert!(!routes.iter().any(|r| r.name == "controllers"));
    }

    #[test]
    fn import_prefix_applies_to_attribute_routes() {
        let dir = fixture::project();
        fixture::write(
            dir.path(),
            "config/routes/api.yaml",
            "api:\n    resource: ../../src/Controller/\n    type: attribute\n    prefix: /api\n    name_prefix: api_\n",
        );
        let php = crate::index_php_files(dir.path(), "src");
        let mut routes = from_php(&php);
        from_yaml_files(dir.path(), &mut routes);
        assert!(routes
            .iter()
            .any(|r| r.name == "api_order_show" && r.path == "/api/orders/{id}"));
    }

    #[test]
    fn default_names() {
        assert_eq!(
            default_route_name("App\\Controller\\OrderController", "index"),
            "app_order_index"
        );
    }

    #[test]
    fn list_items_formats() {
        assert_eq!(list_items("['GET', 'POST']"), vec!["GET", "POST"]);
        assert_eq!(list_items("{\"GET\",\"HEAD\"}"), vec!["GET", "HEAD"]);
        assert_eq!(list_items("'GET|POST'"), vec!["GET", "POST"]);
    }
}
