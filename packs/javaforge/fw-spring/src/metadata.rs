//! Spring Boot's configuration metadata (`META-INF/spring-configuration-metadata.json` and
//! `additional-spring-configuration-metadata.json`): properties with their types,
//! descriptions, defaults and deprecations, groups, value hints, plus the constants of the
//! enum types the properties use. Keys are stored in their canonical (kebab-case) form, so
//! lookups follow Spring's relaxed binding.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::canonical;
use crate::Location;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Deprecation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replacement: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Property {
    pub name: String,
    #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_value: Option<Value>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub deprecated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deprecation: Option<Deprecation>,
    /// Where the property is declared in the project (`@ConfigurationProperties` fields).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<Location>,
    /// The jar (or `project`) the metadata came from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
}

impl Property {
    pub fn is_deprecated(&self) -> bool {
        self.deprecated || self.deprecation.is_some()
    }

    /// The type without packages: `Map<String, LogLevel>`.
    pub fn short_type(&self) -> String {
        self.type_name
            .as_deref()
            .map(short_type)
            .unwrap_or_default()
    }

    pub fn is_boolean(&self) -> bool {
        matches!(
            self.type_name.as_deref(),
            Some("java.lang.Boolean" | "boolean" | "kotlin.Boolean" | "Boolean")
        )
    }

    /// `Map<…>` / `Properties`: any key below the property is valid.
    pub fn is_map(&self) -> bool {
        self.type_name.as_deref().is_some_and(|t| {
            let raw = t.split('<').next().unwrap_or(t);
            raw.ends_with("Map") || raw == "java.util.Properties"
        })
    }

    /// A list or array of objects: indexed keys below it are valid.
    pub fn is_collection(&self) -> bool {
        self.type_name.as_deref().is_some_and(|t| {
            let raw = t.split('<').next().unwrap_or(t);
            raw.ends_with("List")
                || raw.ends_with("Set")
                || raw.ends_with("Collection")
                || t.ends_with("[]")
        })
    }

    /// The type argument that holds the values (`Map<String, X>` → X, `List<X>` → X).
    pub fn element_type(&self) -> Option<String> {
        let t = self.type_name.as_deref()?;
        let inner = t.split_once('<')?.1.strip_suffix('>')?;
        let parts = crate::scan::split_top(inner);
        parts.last().map(|s| s.trim().to_string())
    }

    pub fn default_text(&self) -> Option<String> {
        match self.default_value.as_ref()? {
            Value::String(s) => Some(s.clone()),
            Value::Array(a) => Some(
                a.iter()
                    .map(|v| {
                        v.as_str()
                            .map(str::to_string)
                            .unwrap_or_else(|| v.to_string())
                    })
                    .collect::<Vec<_>>()
                    .join(","),
            ),
            Value::Null => None,
            other => Some(other.to_string()),
        }
    }
}

/// `java.util.Map<java.lang.String, org.x.LogLevel>` → `Map<String, LogLevel>`.
pub fn short_type(t: &str) -> String {
    let mut out = String::new();
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut String| {
        out.push_str(word.rsplit(['.', '$']).next().unwrap_or(word));
        word.clear();
    };
    for c in t.chars() {
        if c.is_alphanumeric() || c == '.' || c == '$' || c == '_' {
            word.push(c);
        } else {
            flush(&mut word, &mut out);
            out.push(c);
            if c == ',' {
                out.push(' ');
            }
        }
    }
    flush(&mut word, &mut out);
    out.replace(",  ", ", ")
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub name: String,
    #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValueHint {
    pub value: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl ValueHint {
    pub fn text(&self) -> String {
        match &self.value {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Provider {
    pub name: String,
    #[serde(default)]
    pub parameters: Value,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hint {
    pub name: String,
    #[serde(default)]
    pub values: Vec<ValueHint>,
    #[serde(default)]
    pub providers: Vec<Provider>,
}

#[derive(Debug, Default, Deserialize)]
struct File {
    #[serde(default)]
    groups: Vec<Group>,
    #[serde(default)]
    properties: Vec<Property>,
    #[serde(default)]
    hints: Vec<Hint>,
}

/// All metadata of a project.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    /// By canonical name.
    pub properties: BTreeMap<String, Property>,
    pub groups: BTreeMap<String, Group>,
    pub hints: BTreeMap<String, Hint>,
    /// Constants of enum types (`org.springframework.boot.logging.LogLevel` → `TRACE`, …).
    pub enums: BTreeMap<String, Vec<String>>,
    /// Where the metadata came from (jar file names, `project`).
    pub sources: Vec<String>,
}

impl Metadata {
    /// Add one metadata file; returns the number of properties it declared.
    pub fn add_json(&mut self, json: &str, origin: &str) -> Result<usize, String> {
        let file: File = serde_json::from_str(json).map_err(|e| format!("{origin}: {e}"))?;
        let n = file.properties.len();
        for g in file.groups {
            self.groups.entry(canonical(&g.name)).or_insert(g);
        }
        for mut p in file.properties {
            p.origin.get_or_insert_with(|| origin.to_string());
            self.add_property(p);
        }
        for h in file.hints {
            let key = canonical(&h.name);
            match self.hints.get_mut(&key) {
                Some(existing) => {
                    existing.values.extend(h.values);
                    existing.providers.extend(h.providers);
                }
                None => {
                    self.hints.insert(key, h);
                }
            }
        }
        if !self.sources.iter().any(|s| s == origin) {
            self.sources.push(origin.to_string());
        }
        Ok(n)
    }

    /// Add a property; a later declaration fills what an earlier one left out
    /// (`additional-spring-configuration-metadata.json` adds descriptions and defaults).
    pub fn add_property(&mut self, p: Property) {
        let key = canonical(&p.name);
        match self.properties.get_mut(&key) {
            Some(existing) => {
                if existing.description.is_none() {
                    existing.description = p.description;
                }
                if existing.default_value.is_none() {
                    existing.default_value = p.default_value;
                }
                if existing.type_name.is_none() {
                    existing.type_name = p.type_name;
                }
                if existing.location.is_none() {
                    existing.location = p.location;
                }
                if existing.deprecation.is_none() {
                    existing.deprecation = p.deprecation;
                }
                existing.deprecated |= p.deprecated;
            }
            None => {
                self.properties.insert(key, p);
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.properties.is_empty()
    }

    /// The property of a key (relaxed binding).
    pub fn get(&self, key: &str) -> Option<&Property> {
        self.properties.get(&canonical(key))
    }

    /// The `Map` / collection property a key lies below (`logging.level.org.acme` →
    /// `logging.level`).
    pub fn container_of(&self, key: &str) -> Option<&Property> {
        let c = canonical(key);
        let mut end = c.len();
        while let Some(dot) = c[..end].rfind('.') {
            if let Some(p) = self.properties.get(&c[..dot]) {
                if p.is_map() || p.is_collection() {
                    return Some(p);
                }
            }
            end = dot;
        }
        None
    }

    /// Known as a property, below a map / collection property, or a path to properties
    /// (a YAML mapping key such as `spring.datasource`).
    pub fn is_known(&self, key: &str) -> bool {
        let c = canonical(key);
        if self.properties.contains_key(&c) || self.container_of(key).is_some() {
            return true;
        }
        let prefix = format!("{c}.");
        self.properties
            .range(prefix.clone()..)
            .next()
            .is_some_and(|(k, _)| k.starts_with(&prefix))
    }

    /// Hint values of a key: its own hint, or the `.values` hint of the map it is in.
    pub fn hint_values(&self, key: &str) -> Vec<&ValueHint> {
        let c = canonical(key);
        if let Some(h) = self.hints.get(&c) {
            return h.values.iter().collect();
        }
        if let Some(h) = self.hints.get(&format!("{c}.values")) {
            return h.values.iter().collect();
        }
        if let Some(map) = self.container_of(key) {
            if let Some(h) = self.hints.get(&format!("{}.values", canonical(&map.name))) {
                return h.values.iter().collect();
            }
        }
        Vec::new()
    }

    /// Enum constants of a type (`$` and `.` for nested types both work).
    pub fn enum_values(&self, type_name: &str) -> Option<&Vec<String>> {
        self.enums
            .get(type_name)
            .or_else(|| self.enums.get(&type_name.replace('$', ".")))
    }

    /// The values a key accepts, with descriptions: booleans, enum constants (in the
    /// lower-case form Spring documents) and hints.
    pub fn values_of(&self, key: &str) -> Vec<(String, Option<String>)> {
        let mut out: Vec<(String, Option<String>)> = self
            .hint_values(key)
            .into_iter()
            .map(|h| (h.text(), h.description.clone()))
            .collect();
        let prop = self.get(key).cloned().or_else(|| {
            // A value of a map property: the map's element type.
            self.container_of(key).map(|m| Property {
                type_name: m.element_type(),
                ..Property::default()
            })
        });
        if let Some(p) = prop {
            if p.is_boolean() || p.type_name.as_deref() == Some("Boolean") {
                for b in ["true", "false"] {
                    if !out.iter().any(|(v, _)| v == b) {
                        out.push((b.to_string(), None));
                    }
                }
            } else if let Some(values) = p.type_name.as_deref().and_then(|t| self.enum_values(t)) {
                for v in values {
                    let text = v.to_ascii_lowercase().replace('_', "-");
                    if !out.iter().any(|(x, _)| x.eq_ignore_ascii_case(&text)) {
                        out.push((text, None));
                    }
                }
            }
        }
        out
    }
}

#[cfg(test)]
pub(crate) const TEST_METADATA: &str = r#"{
  "groups": [
    { "name": "server", "type": "org.springframework.boot.autoconfigure.web.ServerProperties", "sourceType": "org.springframework.boot.autoconfigure.web.ServerProperties" }
  ],
  "properties": [
    { "name": "server.port", "type": "java.lang.Integer", "description": "Server HTTP port.", "sourceType": "org.springframework.boot.autoconfigure.web.ServerProperties", "defaultValue": 8080 },
    { "name": "server.servlet.context-path", "type": "java.lang.String", "description": "Context path of the application." },
    { "name": "spring.jpa.show-sql", "type": "java.lang.Boolean", "description": "Whether to enable logging of SQL statements.", "defaultValue": false },
    { "name": "spring.jpa.hibernate.ddl-auto", "type": "java.lang.String", "description": "DDL mode." },
    { "name": "spring.main.banner-mode", "type": "org.springframework.boot.Banner$Mode", "description": "Mode used to display the banner when the application runs." },
    { "name": "logging.level", "type": "java.util.Map<java.lang.String,org.springframework.boot.logging.LogLevel>", "description": "Log levels severity mapping." },
    { "name": "spring.profiles.active", "type": "java.util.List<java.lang.String>", "description": "Comma-separated list of active profiles." },
    { "name": "server.max-http-header-size", "type": "org.springframework.util.unit.DataSize", "deprecated": true, "deprecation": { "level": "error", "reason": "Moved.", "replacement": "server.max-http-request-header-size" } },
    { "name": "management.endpoints.web.exposure.include", "type": "java.util.Set<java.lang.String>", "description": "Endpoint IDs that should be included or '*' for all." }
  ],
  "hints": [
    { "name": "spring.jpa.hibernate.ddl-auto", "values": [ { "value": "none", "description": "Disable DDL handling." }, { "value": "update", "description": "Update the schema if necessary." }, { "value": "create-drop" } ] },
    { "name": "logging.level.keys", "values": [ { "value": "root", "description": "Root logger used to assign the default logging level." } ] },
    { "name": "logging.level.values", "values": [ { "value": "trace" }, { "value": "debug" }, { "value": "info" } ] }
  ]
}"#;

#[cfg(test)]
pub(crate) use tests::sample;

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn sample() -> Metadata {
        let mut m = Metadata::default();
        m.add_json(TEST_METADATA, "spring-boot-autoconfigure-3.3.2.jar")
            .unwrap();
        m.enums.insert(
            "org.springframework.boot.Banner$Mode".into(),
            vec!["OFF".into(), "CONSOLE".into(), "LOG".into()],
        );
        m
    }

    #[test]
    fn reads_and_merges_metadata() {
        let mut m = sample();
        assert_eq!(m.properties.len(), 9);
        assert_eq!(
            m.get("server.port").unwrap().default_text().as_deref(),
            Some("8080")
        );
        assert_eq!(
            m.get("spring.jpa.showSql").unwrap().name,
            "spring.jpa.show-sql"
        );
        assert!(m
            .get("server.max-http-header-size")
            .unwrap()
            .is_deprecated());
        assert_eq!(
            m.get("logging.level").unwrap().short_type(),
            "Map<String, LogLevel>"
        );
        assert_eq!(
            m.container_of("logging.level.org.acme.Shop").unwrap().name,
            "logging.level"
        );
        assert!(m.is_known("logging.level.org.acme"));
        assert!(m.is_known("spring.jpa"));
        assert!(!m.is_known("spring.jpa.unknown"));
        let added = m
            .add_json(
                r#"{"properties":[{"name":"server.port","description":"ignored","defaultValue":1},{"name":"shop.currency","type":"java.lang.String"}]}"#,
                "project",
            )
            .unwrap();
        assert_eq!(added, 2);
        assert_eq!(
            m.get("server.port").unwrap().description.as_deref(),
            Some("Server HTTP port.")
        );
        assert!(m.get("shop.currency").is_some());
        assert_eq!(m.sources.len(), 2);
        assert!(m.add_json("{", "broken.jar").is_err());
    }

    #[test]
    fn values_from_hints_booleans_and_enums() {
        let m = sample();
        let names = |k: &str| {
            m.values_of(k)
                .into_iter()
                .map(|(v, _)| v)
                .collect::<Vec<_>>()
        };
        assert_eq!(names("spring.jpa.show-sql"), ["true", "false"]);
        assert_eq!(
            names("spring.jpa.hibernate.ddl-auto"),
            ["none", "update", "create-drop"]
        );
        assert_eq!(names("spring.main.banner-mode"), ["off", "console", "log"]);
        assert_eq!(names("logging.level.org.acme"), ["trace", "debug", "info"]);
        assert!(names("server.port").is_empty());
    }
}
