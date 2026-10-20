//! The service container: services and parameters from `config/services*.yaml`
//! (including `resource:` directories registered with autowiring) and from the compiled
//! container XML Symfony dumps to `var/cache/<env>/` in debug mode.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use phpforge_lang_php::SymbolKind;
use quick_xml::events::Event;
use quick_xml::{Reader, XmlVersion};
use serde::{Deserialize, Serialize};
use yaml_rust2::{Yaml, YamlLoader};

use crate::{rel_path, Config, Location, PhpFile};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ServiceSource {
    /// Declared explicitly in a YAML file.
    Yaml,
    /// Registered through a `resource:` directory (autowiring).
    Resource,
    /// Only known from the compiled container (bundles, framework).
    Container,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Service {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class: Option<String>,
    /// Target id when this is an alias.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
    pub public: bool,
    pub autowire: bool,
    pub autoconfigure: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    pub source: ServiceSource,
    /// Where it is declared (YAML line or class declaration).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<Location>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Parameter {
    pub name: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<Location>,
}

/// The container XML for the configured environment, relative to the root.
pub fn find_container_xml(root: &Path, config: &Config) -> Option<String> {
    if !config.container_xml.is_empty() {
        return root
            .join(&config.container_xml)
            .is_file()
            .then(|| config.container_xml.clone());
    }
    let mut dirs = vec![root.join("var/cache").join(&config.environment)];
    dirs.push(root.join("var/cache/dev"));
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut found: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                name.ends_with("Container.xml")
            })
            .collect();
        found.sort();
        if let Some(p) = found.first() {
            return Some(rel_path(root, p));
        }
    }
    None
}

fn yaml_str(y: &Yaml) -> Option<String> {
    match y {
        Yaml::String(s) | Yaml::Real(s) => Some(s.clone()),
        Yaml::Integer(i) => Some(i.to_string()),
        Yaml::Boolean(b) => Some(b.to_string()),
        Yaml::Null => Some("null".into()),
        _ => None,
    }
}

/// Short single-line rendering of a YAML value (`['en', 'de']`).
pub(crate) fn yaml_display(y: &Yaml) -> String {
    match y {
        Yaml::Array(a) => format!(
            "[{}]",
            a.iter().map(yaml_display).collect::<Vec<_>>().join(", ")
        ),
        Yaml::Hash(h) => format!(
            "{{{}}}",
            h.iter()
                .map(|(k, v)| format!("{}: {}", yaml_display(k), yaml_display(v)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Yaml::String(s) => {
            if s.parse::<f64>().is_ok() || s.is_empty() {
                format!("'{s}'")
            } else {
                s.clone()
            }
        }
        other => yaml_str(other).unwrap_or_default(),
    }
}

fn get<'a>(y: &'a Yaml, key: &str) -> Option<&'a Yaml> {
    match y {
        Yaml::Hash(h) => h.get(&Yaml::String(key.to_string())),
        _ => None,
    }
}

fn get_bool(y: &Yaml, key: &str) -> Option<bool> {
    match get(y, key)? {
        Yaml::Boolean(b) => Some(*b),
        _ => None,
    }
}

/// 0-based line of a mapping key at a given indentation (`  app.mailer:`), searching from
/// the line after `after`.
pub(crate) fn key_line(text: &str, key: &str, indent: Option<usize>, after: usize) -> Option<u32> {
    let variants = [
        format!("{key}:"),
        format!("'{key}':"),
        format!("\"{key}\":"),
    ];
    for (n, line) in text.lines().enumerate().skip(after) {
        let trimmed = line.trim_start();
        let ind = line.len() - trimmed.len();
        if indent.is_some_and(|i| i != ind) {
            continue;
        }
        if variants.iter().any(|v| {
            trimmed.starts_with(v.as_str())
                && trimmed[v.len()..]
                    .chars()
                    .next()
                    .is_none_or(|c| c.is_whitespace())
        }) {
            return Some(n as u32);
        }
    }
    None
}

/// Indentation used below a top-level key (usually 4).
fn child_indent(text: &str, top: &str) -> usize {
    let mut in_section = false;
    for line in text.lines() {
        if line.starts_with(&format!("{top}:")) {
            in_section = true;
            continue;
        }
        if in_section {
            let t = line.trim_start();
            if t.is_empty() || t.starts_with('#') {
                continue;
            }
            return line.len() - t.len();
        }
    }
    4
}

/// Service configuration files: `config/services.yaml`, `config/services_<env>.yaml` and
/// `config/services/*.yaml`.
fn service_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for name in ["services.yaml", "services.yml"] {
        let p = root.join("config").join(name);
        if p.is_file() {
            out.push(p);
        }
    }
    for dir in [root.join("config/services"), root.join("config/packages")] {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            let mut v: Vec<PathBuf> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    matches!(
                        p.extension().and_then(|e| e.to_str()),
                        Some("yaml" | "yml")
                    )
                })
                .collect();
            v.sort();
            out.extend(v);
        }
    }
    out
}

/// Expand `{a,b}` braces in an exclude pattern.
fn expand_braces(pattern: &str) -> Vec<String> {
    if let (Some(open), Some(close)) = (pattern.find('{'), pattern.find('}')) {
        if open < close {
            let head = &pattern[..open];
            let tail = &pattern[close + 1..];
            return pattern[open + 1..close]
                .split(',')
                .flat_map(|alt| expand_braces(&format!("{head}{}{tail}", alt.trim())))
                .collect();
        }
    }
    vec![pattern.to_string()]
}

/// Normalize `a/./b/../c` into `a/c` (paths relative to the root).
fn normalize(path: &str) -> String {
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

fn is_excluded(path: &str, excludes: &[String]) -> bool {
    excludes.iter().any(|e| {
        let e = e.trim_end_matches('/');
        if let Some(prefix) = e.strip_suffix("*") {
            path.starts_with(prefix)
        } else {
            path == e || path.starts_with(&format!("{e}/"))
        }
    })
}

/// Services and parameters declared in the YAML configuration.
pub fn from_yaml(root: &Path, php: &[PhpFile]) -> (Vec<Service>, Vec<Parameter>) {
    let mut services: Vec<Service> = Vec::new();
    let mut parameters: Vec<Parameter> = Vec::new();
    for file in service_files(root) {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        let Ok(docs) = YamlLoader::load_from_str(&text) else {
            continue;
        };
        let Some(doc) = docs.first() else { continue };
        let rel = rel_path(root, &file);
        let base_dir = Path::new(&rel)
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();

        if let Some(Yaml::Hash(params)) = get(doc, "parameters") {
            let indent = child_indent(&text, "parameters");
            for (k, v) in params {
                let Some(name) = yaml_str(k) else { continue };
                let line = key_line(&text, &name, Some(indent), 0);
                parameters.push(Parameter {
                    value: yaml_display(v),
                    location: line.map(|l| Location::new(&rel, l, indent as u32)),
                    name,
                });
            }
        }

        let Some(Yaml::Hash(defs)) = get(doc, "services") else {
            continue;
        };
        let indent = child_indent(&text, "services");
        let services_line = key_line(&text, "services", Some(0), 0).unwrap_or(0) as usize;
        let defaults = get(&Yaml::Hash(defs.clone()), "_defaults").cloned();
        let default_autowire = defaults
            .as_ref()
            .and_then(|d| get_bool(d, "autowire"))
            .unwrap_or(false);
        let default_autoconfigure = defaults
            .as_ref()
            .and_then(|d| get_bool(d, "autoconfigure"))
            .unwrap_or(false);
        let default_public = defaults
            .as_ref()
            .and_then(|d| get_bool(d, "public"))
            .unwrap_or(false);

        for (k, v) in defs {
            let Some(id) = yaml_str(k) else { continue };
            if id == "_defaults" || id == "_instanceof" {
                continue;
            }
            let line = key_line(&text, &id, Some(indent), services_line);
            let location = line.map(|l| Location::new(&rel, l, indent as u32));

            // Namespace prefix with a resource directory: one service per class.
            if let Some(resource) = get(v, "resource").and_then(yaml_str) {
                let prefix = id.trim_end_matches('\\');
                let dir = normalize(&format!("{base_dir}/{resource}"));
                let mut excludes: Vec<String> = Vec::new();
                match get(v, "exclude") {
                    Some(Yaml::Array(a)) => {
                        for e in a.iter().filter_map(yaml_str) {
                            excludes.extend(expand_braces(&e));
                        }
                    }
                    Some(e) => {
                        if let Some(e) = yaml_str(e) {
                            excludes.extend(expand_braces(&e));
                        }
                    }
                    None => {}
                }
                let excludes: Vec<String> = excludes
                    .iter()
                    .map(|e| normalize(&format!("{base_dir}/{e}")))
                    .collect();
                let autowire = get_bool(v, "autowire").unwrap_or(default_autowire);
                let autoconfigure = get_bool(v, "autoconfigure").unwrap_or(default_autoconfigure);
                for f in php {
                    if !f.path.starts_with(&format!("{dir}/")) && dir != f.path {
                        continue;
                    }
                    if is_excluded(&f.path, &excludes) {
                        continue;
                    }
                    for c in &f.index.classes {
                        if c.kind != SymbolKind::Class || c.modifiers.is_abstract {
                            continue;
                        }
                        if !c.fqn.starts_with(&format!("{prefix}\\")) {
                            continue;
                        }
                        if services.iter().any(|s| s.id == c.fqn) {
                            continue;
                        }
                        services.push(Service {
                            id: c.fqn.clone(),
                            class: Some(c.fqn.clone()),
                            alias: None,
                            public: default_public,
                            autowire,
                            autoconfigure,
                            tags: Vec::new(),
                            source: ServiceSource::Resource,
                            location: Some(Location::new(
                                &f.path,
                                c.name_range.line,
                                c.name_range.column,
                            )),
                        });
                    }
                }
                continue;
            }

            // `id: '@target'` short alias, or `alias: target`.
            let alias = match v {
                Yaml::String(s) if s.starts_with('@') => Some(s[1..].to_string()),
                _ => get(v, "alias").and_then(yaml_str),
            };
            let class = get(v, "class")
                .and_then(yaml_str)
                .or_else(|| (alias.is_none() && id.contains('\\')).then(|| id.clone()));
            let tags = match get(v, "tags") {
                Some(Yaml::Array(a)) => a
                    .iter()
                    .filter_map(|t| yaml_str(t).or_else(|| get(t, "name").and_then(yaml_str)))
                    .collect(),
                _ => Vec::new(),
            };
            let svc = Service {
                id: id.clone(),
                class,
                alias,
                public: get_bool(v, "public").unwrap_or(default_public),
                autowire: get_bool(v, "autowire").unwrap_or(default_autowire),
                autoconfigure: get_bool(v, "autoconfigure").unwrap_or(default_autoconfigure),
                tags,
                source: ServiceSource::Yaml,
                location,
            };
            // An explicit definition refines a resource-registered one.
            if let Some(existing) = services.iter_mut().find(|s| s.id == id) {
                existing.public = svc.public;
                existing.tags = svc.tags;
                if svc.class.is_some() {
                    existing.class = svc.class;
                }
                if svc.alias.is_some() {
                    existing.alias = svc.alias;
                }
            } else {
                services.push(svc);
            }
        }
    }
    (services, parameters)
}

fn attr(e: &quick_xml::events::BytesStart, name: &str) -> Option<String> {
    e.attributes()
        .flatten()
        .find(|a| a.key.as_ref() == name)
        .and_then(|a| a.normalized_value(XmlVersion::Implicit1_0).ok().map(|v| v.into_owned()))
}

/// Services and parameters of a compiled container dump (`*Container.xml`).
pub fn parse_container_xml(xml: &str) -> (Vec<Service>, Vec<Parameter>) {
    let mut reader = Reader::from_str(xml);
    let mut services: Vec<Service> = Vec::new();
    let mut parameters: Vec<Parameter> = Vec::new();
    let mut in_service: Option<usize> = None;
    // Stack of open <parameter> elements: (key, collected text, is collection).
    let mut param_stack: Vec<(Option<String>, String, bool)> = Vec::new();
    loop {
        let event = match reader.read_event() {
            Ok(e) => e,
            Err(_) => break,
        };
        match event {
            Event::Start(ref e) | Event::Empty(ref e) => {
                let empty = matches!(event, Event::Empty(_));
                match e.name().as_ref() {
                    "service" => {
                        let Some(id) = attr(e, "id") else { continue };
                        let svc = Service {
                            class: attr(e, "class"),
                            alias: attr(e, "alias"),
                            public: attr(e, "public").as_deref() == Some("true"),
                            autowire: attr(e, "autowire").as_deref() == Some("true"),
                            autoconfigure: attr(e, "autoconfigure").as_deref() == Some("true"),
                            tags: Vec::new(),
                            source: ServiceSource::Container,
                            location: None,
                            id,
                        };
                        if !param_stack.is_empty() || in_service.is_some() {
                            // Inline service definition inside an argument.
                            continue;
                        }
                        services.push(svc);
                        if !empty {
                            in_service = Some(services.len() - 1);
                        }
                    }
                    "tag" => {
                        if let (Some(i), Some(name)) = (in_service, attr(e, "name")) {
                            if !services[i].tags.contains(&name) {
                                services[i].tags.push(name);
                            }
                        }
                    }
                    "parameter" if in_service.is_none() => {
                        let key = attr(e, "key");
                        let collection = attr(e, "type").as_deref() == Some("collection");
                        if empty {
                            if param_stack.is_empty() {
                                if let Some(k) = key {
                                    parameters.push(Parameter {
                                        name: k,
                                        value: if collection { "[]".into() } else { "null".into() },
                                        location: None,
                                    });
                                }
                            } else if let Some(top) = param_stack.last_mut() {
                                top.1.push_str(if top.1.is_empty() { "null" } else { ", null" });
                            }
                        } else {
                            param_stack.push((key, String::new(), collection));
                        }
                    }
                    _ => {}
                }
            }
            Event::Text(t) => {
                if let Some(top) = param_stack.last_mut() {
                    if !top.2 {
                        let raw: &str = &t;
                        top.1.push_str(
                            &raw.trim()
                                .replace("&lt;", "<")
                                .replace("&gt;", ">")
                                .replace("&quot;", "\"")
                                .replace("&apos;", "'")
                                .replace("&amp;", "&"),
                        );
                    }
                }
            }
            Event::End(ref e) => match e.name().as_ref() {
                "service" => in_service = None,
                "parameter" if in_service.is_none() => {
                    if let Some((key, text, collection)) = param_stack.pop() {
                        let value = if collection { format!("[{text}]") } else { text };
                        if let Some(parent) = param_stack.last_mut() {
                            if !parent.1.is_empty() {
                                parent.1.push_str(", ");
                            }
                            match key {
                                Some(k) if parent.2 => {
                                    parent.1.push_str(&format!("{k}: {value}"))
                                }
                                _ => parent.1.push_str(&value),
                            }
                        } else if let Some(k) = key {
                            parameters.push(Parameter {
                                name: k,
                                value,
                                location: None,
                            });
                        }
                    }
                }
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }
    // Hidden ids (`.abstract.*`, `.service_locator.*`) are not usable from application code.
    services.retain(|s| !s.id.starts_with('.'));
    (services, parameters)
}

/// Add services and parameters only the compiled container knows; refine known ones.
pub fn merge(
    services: &mut Vec<Service>,
    compiled: Vec<Service>,
    parameters: &mut Vec<Parameter>,
    compiled_params: Vec<Parameter>,
) {
    let known: HashMap<String, usize> = services
        .iter()
        .enumerate()
        .map(|(i, s)| (s.id.clone(), i))
        .collect();
    for s in compiled {
        match known.get(&s.id) {
            Some(&i) => {
                let existing = &mut services[i];
                existing.public |= s.public;
                for t in s.tags {
                    if !existing.tags.contains(&t) {
                        existing.tags.push(t);
                    }
                }
                if existing.class.is_none() {
                    existing.class = s.class;
                }
            }
            None => services.push(s),
        }
    }
    for p in compiled_params {
        match parameters.iter_mut().find(|x| x.name == p.name) {
            Some(existing) => {
                if existing.value.contains('%') {
                    existing.value = p.value;
                }
            }
            None => parameters.push(p),
        }
    }
}

/// The `kernel.*` parameters every Symfony application has.
pub fn add_kernel_parameters(parameters: &mut Vec<Parameter>) {
    for (name, value) in [
        ("kernel.project_dir", "%kernel.project_dir%"),
        ("kernel.environment", "dev"),
        ("kernel.debug", "true"),
        ("kernel.cache_dir", "%kernel.project_dir%/var/cache/dev"),
        ("kernel.logs_dir", "%kernel.project_dir%/var/log"),
        ("kernel.build_dir", "%kernel.project_dir%/var/cache/dev"),
        ("kernel.charset", "UTF-8"),
        ("kernel.secret", "%env(APP_SECRET)%"),
        ("kernel.default_locale", "en"),
    ] {
        if !parameters.iter().any(|p| p.name == name) {
            parameters.push(Parameter {
                name: name.into(),
                value: value.into(),
                location: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture;

    #[test]
    fn yaml_services_resources_and_parameters() {
        let dir = fixture::project();
        let php = crate::index_php_files(dir.path(), "src");
        let (services, params) = from_yaml(dir.path(), &php);
        let ids: Vec<&str> = services.iter().map(|s| s.id.as_str()).collect();
        // Resource services, excluding Entity/ and Kernel.php.
        assert!(ids.contains(&"App\\Controller\\OrderController"));
        assert!(ids.contains(&"App\\Twig\\AppExtension"));
        assert!(!ids.contains(&"App\\Entity\\Order"));
        assert!(!ids.contains(&"App\\Kernel"));
        let checkout = services.iter().find(|s| s.id == "App\\Service\\Checkout").unwrap();
        assert!(checkout.autowire);
        assert_eq!(checkout.source, ServiceSource::Resource);

        let mailer = services.iter().find(|s| s.id == "app.mailer").unwrap();
        assert_eq!(mailer.class.as_deref(), Some("App\\Service\\Mailer"));
        assert!(mailer.public);
        assert_eq!(mailer.location.as_ref().unwrap().line, 20);
        let alias = services.iter().find(|s| s.id == "mailer.default").unwrap();
        assert_eq!(alias.alias.as_deref(), Some("app.mailer"));

        let currency = params.iter().find(|p| p.name == "app.currency").unwrap();
        assert_eq!(currency.value, "EUR");
        assert_eq!(currency.location.as_ref().unwrap().line, 1);
        let locales = params.iter().find(|p| p.name == "app.supported_locales").unwrap();
        assert_eq!(locales.value, "[en, de]");
    }

    #[test]
    fn compiled_container_xml() {
        let (services, params) = parse_container_xml(fixture::CONTAINER_XML);
        let router = services.iter().find(|s| s.id == "router").unwrap();
        assert!(router.public);
        assert_eq!(router.tags, vec!["monolog.logger"]);
        assert!(services.iter().all(|s| !s.id.starts_with('.')));
        let alias = services.iter().find(|s| s.id == "Twig\\Environment").unwrap();
        assert_eq!(alias.alias.as_deref(), Some("twig"));
        assert_eq!(
            params.iter().find(|p| p.name == "kernel.environment").unwrap().value,
            "dev"
        );
        assert_eq!(params.iter().find(|p| p.name == "locales").unwrap().value, "[en]");
    }

    #[test]
    fn finds_and_merges_container_xml() {
        let dir = fixture::project();
        fixture::write(
            dir.path(),
            "var/cache/dev/App_KernelDevDebugContainer.xml",
            fixture::CONTAINER_XML,
        );
        let snap = crate::scan(dir.path(), &Config::default());
        assert_eq!(
            snap.info.container_xml.as_deref(),
            Some("var/cache/dev/App_KernelDevDebugContainer.xml")
        );
        assert!(snap.service("router").is_some());
        // Known from YAML: keeps its location.
        assert!(snap.service("App\\Service\\Checkout").unwrap().location.is_some());
    }

    #[test]
    fn braces_and_normalize() {
        assert_eq!(
            expand_braces("../src/{Entity,Tests}/"),
            vec!["../src/Entity/", "../src/Tests/"]
        );
        assert_eq!(normalize("config/../src/./Entity/"), "src/Entity");
    }
}
