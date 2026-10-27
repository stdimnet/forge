//! Editor assistance in `application.properties` / `application.yml`: completion of keys
//! (YAML keys are inserted as nested mappings, like IntelliJ IDEA) and values (booleans,
//! enum constants, hints, profiles) with documentation, the "unknown / deprecated property"
//! and "invalid value" inspections, hover documentation and Go to Declaration from a key
//! to its `@ConfigurationProperties` class. Offsets crossing this API are UTF-16.

use serde::Serialize;

use crate::config::{self, canonical, Format, Place};
use crate::metadata::{Metadata, Property};
use crate::{byte_offset, utf16_offset, Location};

/// What the assistance knows about the project besides the metadata.
#[derive(Debug, Clone, Copy)]
pub struct Context<'a> {
    pub metadata: &'a Metadata,
    /// Profiles of the project (file names, `@Profile`, `on-profile`).
    pub profiles: &'a [String],
}

/// A completion item in the shape of the JSON Schema completion (`forge-jsonschema`), so
/// the editor renders both the same way.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub label: String,
    /// `property` or `value`.
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Markdown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub documentation: Option<String>,
    /// LSP snippet syntax.
    pub insert: String,
    pub required: bool,
    pub deprecated: bool,
    pub retrigger: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Completion {
    pub from: usize,
    pub replace_from: usize,
    pub replace_to: usize,
    pub items: Vec<Item>,
    /// More items match than were returned: ask again while typing.
    pub incomplete: bool,
}

const MAX_ITEMS: usize = 400;

/// Keys whose values are profile names.
const PROFILE_KEYS: &[&str] = &[
    "spring.profiles.active",
    "spring.profiles.include",
    "spring.profiles.default",
    "spring.config.activate.on-profile",
];

fn snippet_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('$', "\\$").replace('}', "\\}")
}

/// Markdown documentation of a property.
pub fn documentation(p: &Property) -> String {
    let mut out = format!("**{}**", p.name);
    let mut facts = Vec::new();
    if let Some(t) = &p.type_name {
        facts.push(format!("`{}`", p.short_type().replace('`', "")));
        let _ = t;
    }
    if let Some(d) = p.default_text() {
        facts.push(format!("default `{d}`"));
    }
    if !facts.is_empty() {
        out.push_str("\n\n");
        out.push_str(&facts.join(" · "));
    }
    if let Some(d) = &p.description {
        out.push_str("\n\n");
        out.push_str(d);
    }
    if p.is_deprecated() {
        out.push_str("\n\n**Deprecated**");
        if let Some(dep) = &p.deprecation {
            if let Some(r) = &dep.reason {
                out.push_str(": ");
                out.push_str(r);
            }
            if let Some(r) = &dep.replacement {
                out.push_str(&format!(" Use `{r}` instead."));
            }
        }
    }
    if let Some(src) = &p.source_type {
        out.push_str(&format!("\n\nDeclared in `{src}`"));
    } else if let Some(o) = &p.origin {
        out.push_str(&format!("\n\nFrom `{o}`"));
    }
    out
}

/// Fuzzy rank of `name` for `typed` (both canonical): `None` when it does not match.
fn rank(name: &str, typed: &str) -> Option<u32> {
    if typed.is_empty() {
        return Some(1);
    }
    if name.starts_with(typed) {
        return Some(0);
    }
    if name.contains(typed) {
        return Some(1);
    }
    let mut chars = name.chars();
    typed
        .chars()
        .all(|t| chars.any(|c| c == t))
        .then_some(2)
}

fn indent_unit(text: &str) -> usize {
    text.lines()
        .map(|l| l.len() - l.trim_start_matches(' ').len())
        .filter(|n| *n > 0 && *n <= 8)
        .min()
        .unwrap_or(2)
}

fn has_values(ctx: &Context, key: &str) -> bool {
    PROFILE_KEYS.contains(&canonical(key).as_str()) || !ctx.metadata.values_of(key).is_empty()
}

/// Completion at the UTF-16 `offset` of a configuration file.
pub fn complete(path: &str, text: &str, offset: usize, ctx: &Context) -> Option<Completion> {
    let (format, _) = config::config_file(path)?;
    let byte = byte_offset(text, offset);
    let place = config::place_at(format, text, byte)?;
    let u = |b: usize| utf16_offset(text, b);
    match place {
        Place::Key { parent, typed, from, to, indent } => {
            let parent_c = canonical(&parent);
            let typed_c = canonical(&typed);
            let prefix = if parent_c.is_empty() { String::new() } else { format!("{parent_c}.") };
            let mut found: Vec<(u32, &Property, String)> = Vec::new();
            for (name, p) in ctx.metadata.properties.range(prefix.clone()..) {
                if !name.starts_with(&prefix) {
                    break;
                }
                let rest = &name[prefix.len()..];
                if let Some(r) = rank(rest, &typed_c) {
                    found.push((r + u32::from(p.is_deprecated()) * 10, p, rest.to_string()));
                }
            }
            // Map keys with hints (`logging.level.root`).
            let mut extra: Vec<Item> = Vec::new();
            for (name, hint) in &ctx.metadata.hints {
                let Some(map) = name.strip_suffix(".keys") else { continue };
                if !map.starts_with(&prefix) {
                    continue;
                }
                for v in &hint.values {
                    let rest = format!("{}.{}", &map[prefix.len()..], v.text());
                    if rank(&canonical(&rest), &typed_c).is_some() {
                        extra.push(key_item(format, text, &rest, indent, v.description.clone(), None, false, true));
                    }
                }
            }
            found.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.2.cmp(&b.2)));
            let incomplete = found.len() > MAX_ITEMS;
            let mut items: Vec<Item> = found
                .into_iter()
                .take(MAX_ITEMS)
                .map(|(_, p, rest)| {
                    // Keep the user's spelling of the parent; insert the canonical rest.
                    key_item(
                        format,
                        text,
                        &rest,
                        indent,
                        Some(documentation(p)),
                        Some(p.short_type()).filter(|t| !t.is_empty()),
                        p.is_deprecated(),
                        has_values(ctx, &p.name),
                    )
                })
                .collect();
            items.extend(extra);
            Some(Completion {
                from: u(from),
                replace_from: u(from),
                replace_to: u(to),
                items,
                incomplete,
            })
        }
        Place::Value { key, typed, from, to } => {
            let typed_l = typed.to_ascii_lowercase();
            let mut values: Vec<(String, Option<String>)> = if PROFILE_KEYS.contains(&canonical(&key).as_str())
                || canonical(&key).starts_with("spring.profiles.group.")
            {
                ctx.profiles.iter().map(|p| (p.clone(), Some("Profile".to_string()))).collect()
            } else {
                ctx.metadata.values_of(&key)
            };
            // Comma-separated lists complete the current element.
            let (from, typed_l) = match typed.rfind(',') {
                Some(i) => (from + i + 1, typed_l[i + 1..].trim_start().to_string()),
                None => (from, typed_l),
            };
            values.retain(|(v, _)| rank(&v.to_ascii_lowercase(), &typed_l).is_some());
            if values.is_empty() {
                return None;
            }
            let detail = ctx.metadata.get(&key).map(Property::short_type).filter(|t| !t.is_empty());
            let line_end = text[from..].find('\n').map(|i| from + i).unwrap_or(text.len());
            let to = if typed.contains(',') { from + text[from..line_end].find(',').unwrap_or(line_end - from) } else { to };
            Some(Completion {
                from: u(from),
                replace_from: u(from),
                replace_to: u(to.max(from)),
                items: values
                    .into_iter()
                    .map(|(v, desc)| Item {
                        insert: snippet_escape(&v),
                        label: v,
                        kind: "value",
                        detail: detail.clone(),
                        documentation: desc,
                        required: false,
                        deprecated: false,
                        retrigger: false,
                    })
                    .collect(),
                incomplete: false,
            })
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn key_item(
    format: Format,
    text: &str,
    rest: &str,
    indent: usize,
    documentation: Option<String>,
    detail: Option<String>,
    deprecated: bool,
    retrigger: bool,
) -> Item {
    let insert = match format {
        Format::Properties => format!("{}=$0", snippet_escape(rest)),
        Format::Yaml => {
            let unit = indent_unit(text);
            let segments: Vec<&str> = rest.split('.').collect();
            let mut s = String::new();
            for (i, seg) in segments.iter().enumerate() {
                if i > 0 {
                    s.push('\n');
                    s.push_str(&" ".repeat(indent + unit * i));
                }
                s.push_str(&snippet_escape(seg));
                s.push(':');
            }
            s.push_str(" $0");
            s
        }
    };
    Item {
        label: rest.to_string(),
        kind: "property",
        detail,
        documentation,
        insert,
        required: false,
        deprecated,
        retrigger,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    /// UTF-16 range.
    pub from: usize,
    pub to: usize,
    /// `error`, `warning` or `weak`.
    pub severity: &'static str,
    pub message: String,
    /// `unknownProperty`, `deprecatedProperty` or `invalidValue`.
    pub code: &'static str,
    /// The replacement of a deprecated key (the quick fix).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replacement: Option<String>,
}

fn is_placeholder(v: &str) -> bool {
    v.contains("${") || v.contains("#{")
}

/// Spring's `StringToBooleanConverter`.
fn is_boolean_text(v: &str) -> bool {
    matches!(
        v.trim().to_ascii_lowercase().as_str(),
        "true" | "false" | "on" | "off" | "yes" | "no" | "1" | "0" | ""
    )
}

/// Inspections of a configuration file. Unknown keys are only reported when metadata was
/// found (otherwise every key would be unknown).
pub fn inspect(path: &str, text: &str, ctx: &Context) -> Vec<Problem> {
    let Some((format, _)) = config::config_file(path) else {
        return Vec::new();
    };
    let meta = ctx.metadata;
    if meta.is_empty() {
        return Vec::new();
    }
    let u = |b: usize| utf16_offset(text, b);
    let mut out = Vec::new();
    for e in config::parse(format, text) {
        let Some(value) = e.value.as_deref() else {
            continue;
        };
        let key = &e.key;
        match meta.get(key) {
            Some(p) => {
                if p.is_deprecated() {
                    let dep = p.deprecation.clone().unwrap_or_default();
                    let mut message = format!("Deprecated configuration property '{}'", p.name);
                    if let Some(r) = &dep.reason {
                        message.push_str(&format!(": {r}"));
                    }
                    if let Some(r) = &dep.replacement {
                        message.push_str(&format!(" Use '{r}' instead."));
                    }
                    out.push(Problem {
                        from: u(e.key_from),
                        to: u(e.key_to),
                        severity: if dep.level.as_deref() == Some("error") { "error" } else { "warning" },
                        message,
                        code: "deprecatedProperty",
                        replacement: dep.replacement,
                    });
                }
                if is_placeholder(value) || value.is_empty() {
                    continue;
                }
                if p.is_boolean() && !is_boolean_text(value) {
                    out.push(Problem {
                        from: u(e.value_from),
                        to: u(e.value_to),
                        severity: "warning",
                        message: format!("Cannot convert '{value}' to boolean"),
                        code: "invalidValue",
                        replacement: None,
                    });
                } else if let Some(consts) = p.type_name.as_deref().and_then(|t| meta.enum_values(t)) {
                    let norm = |s: &str| s.trim().to_ascii_lowercase().replace('-', "_");
                    if !consts.iter().any(|c| norm(c) == norm(value)) {
                        out.push(Problem {
                            from: u(e.value_from),
                            to: u(e.value_to),
                            severity: "warning",
                            message: format!(
                                "'{value}' is not one of {}",
                                consts.iter().map(|c| c.to_ascii_lowercase().replace('_', "-")).collect::<Vec<_>>().join(", ")
                            ),
                            code: "invalidValue",
                            replacement: None,
                        });
                    }
                }
            }
            None if meta.is_known(key) => {}
            None => out.push(Problem {
                from: u(if format == Format::Yaml { e.key_from } else { e.key_from }),
                to: u(e.key_to.max(e.key_from)),
                severity: "warning",
                message: format!("Cannot resolve configuration property '{key}'"),
                code: "unknownProperty",
                replacement: None,
            }),
        }
    }
    out
}

/// The key under a UTF-16 offset: (dotted key, byte range of its text).
fn key_at(path: &str, text: &str, offset: usize) -> Option<(String, usize, usize)> {
    let (format, _) = config::config_file(path)?;
    let byte = byte_offset(text, offset);
    config::parse(format, text)
        .into_iter()
        .find(|e| e.key_from <= byte && byte <= e.key_to.max(e.key_from))
        .map(|e| (e.key, e.key_from, e.key_to))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hover {
    pub from: usize,
    pub to: usize,
    pub markdown: String,
}

/// Documentation of the key under the cursor.
pub fn hover(path: &str, text: &str, offset: usize, ctx: &Context) -> Option<Hover> {
    let (key, from, to) = key_at(path, text, offset)?;
    let p = ctx.metadata.get(&key).or_else(|| ctx.metadata.container_of(&key))?;
    Some(Hover {
        from: utf16_offset(text, from),
        to: utf16_offset(text, to),
        markdown: documentation(p),
    })
}

/// Go to Declaration on a key: the `@ConfigurationProperties` field (or class) that binds
/// it. `class_location` finds a project class by qualified name.
pub fn goto(
    path: &str,
    text: &str,
    offset: usize,
    ctx: &Context,
    class_location: &dyn Fn(&str) -> Option<Location>,
) -> Vec<Location> {
    let Some((key, _, _)) = key_at(path, text, offset) else {
        return Vec::new();
    };
    let Some(p) = ctx.metadata.get(&key).or_else(|| ctx.metadata.container_of(&key)) else {
        return Vec::new();
    };
    if let Some(l) = &p.location {
        return vec![l.clone()];
    }
    p.source_type
        .as_deref()
        .and_then(|t| class_location(&t.replace('$', ".")))
        .into_iter()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::sample;

    fn ctx_with<'a>(m: &'a Metadata, profiles: &'a [String]) -> Context<'a> {
        Context { metadata: m, profiles }
    }

    fn labels(c: &Completion) -> Vec<&str> {
        c.items.iter().map(|i| i.label.as_str()).collect()
    }

    #[test]
    fn completes_properties_keys_and_values() {
        let m = sample();
        let profiles = vec!["dev".to_string(), "prod".to_string()];
        let ctx = ctx_with(&m, &profiles);
        let text = "server.port=8080\nspring.jpa.sh";
        let c = complete("src/main/resources/application.properties", text, text.len(), &ctx).unwrap();
        assert_eq!(labels(&c)[0], "spring.jpa.show-sql");
        assert_eq!(c.items[0].insert, "spring.jpa.show-sql=$0");
        assert!(c.items[0].retrigger);
        assert!(c.items[0].documentation.as_deref().unwrap().contains("Whether to enable logging of SQL statements."));
        assert_eq!((c.from, c.replace_to), (17, text.len()));

        let text = "spring.jpa.show-sql=";
        let c = complete("application.properties", text, text.len(), &ctx).unwrap();
        assert_eq!(labels(&c), ["true", "false"]);
        let text = "spring.main.banner-mode=o";
        let c = complete("application-dev.properties", text, text.len(), &ctx).unwrap();
        assert_eq!(labels(&c), ["off", "console", "log"].iter().filter(|v| v.contains('o')).copied().collect::<Vec<_>>());
        let text = "spring.profiles.active=dev,p";
        let c = complete("application.properties", text, text.len(), &ctx).unwrap();
        assert_eq!(labels(&c), ["prod"]);
        assert_eq!(c.from, text.len() - 1);
        let text = "logging.level.";
        let c = complete("application.properties", text, text.len(), &ctx).unwrap();
        assert!(labels(&c).contains(&"logging.level.root"));
    }

    #[test]
    fn completes_yaml_keys_as_nested_mappings() {
        let m = sample();
        let ctx = ctx_with(&m, &[]);
        let text = "spring:\n  jpa:\n    sh";
        let c = complete("application.yml", text, text.len(), &ctx).unwrap();
        assert_eq!(labels(&c), ["show-sql"]);
        assert_eq!(c.items[0].insert, "show-sql: $0");
        let text = "server:\n  ";
        let c = complete("application.yml", text, text.len(), &ctx).unwrap();
        let ctx_path = c.items.iter().find(|i| i.label == "servlet.context-path").unwrap();
        assert_eq!(ctx_path.insert, "servlet:\n    context-path: $0");
        let text = "spring:\n  jpa:\n    show-sql: ";
        let c = complete("application.yml", text, text.len(), &ctx).unwrap();
        assert_eq!(labels(&c), ["true", "false"]);
        let text = "spring:\n  jpa:\n    hibernate:\n      ddl-auto: cr";
        let c = complete("application.yml", text, text.len(), &ctx).unwrap();
        assert_eq!(labels(&c), ["create-drop"]);
        assert_eq!(c.items[0].documentation, None);
    }

    #[test]
    fn inspections() {
        let m = sample();
        let ctx = ctx_with(&m, &[]);
        let text = "server:\n  port: 8080\n  prot: 1\nspring:\n  jpa:\n    show-sql: maybe\n  main:\n    banner-mode: loud\nlogging:\n  level:\n    org.acme: debug\nserver.max-http-header-size: 1KB\nplaceholder: ${HOME}\n";
        let problems = inspect("application.yml", text, &ctx);
        let codes: Vec<(&str, &str)> = problems.iter().map(|p| (p.code, &text[p.from..p.to])).collect();
        assert_eq!(
            codes,
            [
                ("unknownProperty", "prot"),
                ("invalidValue", "maybe"),
                ("invalidValue", "loud"),
                ("deprecatedProperty", "server.max-http-header-size"),
                ("unknownProperty", "placeholder"),
            ]
        );
        assert_eq!(problems[3].severity, "error");
        assert_eq!(problems[3].replacement.as_deref(), Some("server.max-http-request-header-size"));
        // Without metadata nothing is reported.
        let empty = Metadata::default();
        assert!(inspect("application.yml", text, &ctx_with(&empty, &[])).is_empty());
        let props = inspect("application.properties", "spring.jpa.showSql=true\nlogging.level.root=info\n", &ctx);
        assert!(props.is_empty(), "{props:?}");
    }

    #[test]
    fn hover_and_goto() {
        let mut m = sample();
        m.add_property(Property {
            name: "shop.orders.limit".into(),
            type_name: Some("java.lang.Integer".into()),
            location: Some(Location { path: "src/main/java/com/acme/ShopProperties.java".into(), line: 12, column: 16 }),
            ..Property::default()
        });
        let ctx = ctx_with(&m, &[]);
        let text = "server.port=1\nshop.orders.limit=5\n";
        let h = hover("application.properties", text, 3, &ctx).unwrap();
        assert!(h.markdown.starts_with("**server.port**"));
        assert_eq!((h.from, h.to), (0, 11));
        let at = text.find("limit").unwrap();
        let found = goto("application.properties", text, at, &ctx, &|_| None);
        assert_eq!(found[0].line, 12);
        let server = goto("application.properties", text, 2, &ctx, &|c| {
            (c == "org.springframework.boot.autoconfigure.web.ServerProperties").then(|| Location {
                path: "ServerProperties.java".into(),
                line: 1,
                column: 0,
            })
        });
        assert_eq!(server.len(), 1);
    }
}
