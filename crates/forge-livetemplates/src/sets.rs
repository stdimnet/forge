//! Built-in template groups and the user's changes to them.
//!
//! Built-in groups live in `data/*.toml`. The user's changes are settings:
//! `editor.liveTemplates.custom` holds new and edited templates (tables of strings: `group`,
//! `abbreviation`, `description`, `text`, `contexts` as a comma-separated list, `variables` as
//! JSON, `builtin` with the id of the built-in template it replaces, `removed = "true"` for
//! deleted built-ins and `key`, a stable id of a user template), `editor.liveTemplates.disabled` the ids (`group/abbreviation`) of
//! unchecked templates. Framework groups (Laravel, Blade, Symfony) apply only in projects that
//! use the framework.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::context::describe;

/// A template variable as in "Edit Template Variables".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Variable {
    pub name: String,
    #[serde(default)]
    pub expression: String,
    #[serde(default)]
    pub default_value: String,
    /// `false` is PHPStorm's "Skip if defined": no tab stop when the expression has a value.
    #[serde(default = "yes")]
    pub always_stop_at: bool,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Template {
    /// `group/abbreviation`.
    pub id: String,
    pub group: String,
    pub abbreviation: String,
    pub description: String,
    pub text: String,
    pub variables: Vec<Variable>,
    pub contexts: Vec<String>,
    /// "Applicable in" summary, e.g. "PHP: statement".
    pub applicable: String,
    pub enabled: bool,
    /// Id of the built-in template this is (or replaces); `None` for user templates.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub builtin: Option<String>,
    /// A built-in template the user changed.
    pub modified: bool,
    /// Stable key of a user template (its settings entry), kept while it is renamed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub name: String,
    /// Framework the group belongs to (`laravel`, `symfony`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub framework: Option<String>,
    /// Whether the group applies in the open project.
    pub active: bool,
    pub builtin: bool,
    pub templates: Vec<Template>,
}

/// The `editor.liveTemplates.*` settings and the open project's frameworks.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateSettings {
    #[serde(default)]
    pub custom: Vec<BTreeMap<String, String>>,
    #[serde(default)]
    pub disabled: Vec<String>,
    /// Frameworks the open project uses (`laravel`, `symfony`).
    #[serde(default)]
    pub frameworks: Vec<String>,
}

#[derive(Deserialize)]
struct SetFile {
    group: SetHeader,
    #[serde(default)]
    template: Vec<SetTemplate>,
}

#[derive(Deserialize)]
struct SetHeader {
    name: String,
    framework: Option<String>,
}

#[derive(Deserialize)]
struct SetTemplate {
    abbreviation: String,
    description: String,
    text: String,
    contexts: Vec<String>,
    #[serde(default)]
    variables: Vec<Variable>,
}

const SET_FILES: [&str; 7] = [
    include_str!("../data/php.toml"),
    include_str!("../data/javascript.toml"),
    include_str!("../data/lua.toml"),
    include_str!("../data/twig.toml"),
    include_str!("../data/laravel.toml"),
    include_str!("../data/blade.toml"),
    include_str!("../data/symfony.toml"),
];

pub(crate) fn template_id(group: &str, abbreviation: &str) -> String {
    format!("{group}/{abbreviation}")
}

/// The built-in groups as shipped.
pub fn builtin_groups() -> &'static [Group] {
    static GROUPS: OnceLock<Vec<Group>> = OnceLock::new();
    GROUPS.get_or_init(|| {
        SET_FILES
            .iter()
            .map(|src| {
                let file: SetFile = toml::from_str(src).expect("built-in live template set");
                Group {
                    framework: file.group.framework,
                    active: true,
                    builtin: true,
                    templates: file
                        .template
                        .into_iter()
                        .map(|t| {
                            let id = template_id(&file.group.name, &t.abbreviation);
                            Template {
                                applicable: describe(&t.contexts),
                                builtin: Some(id.clone()),
                                id,
                                group: file.group.name.clone(),
                                abbreviation: t.abbreviation,
                                description: t.description,
                                text: t.text,
                                variables: t.variables,
                                contexts: t.contexts,
                                enabled: true,
                                modified: false,
                                key: None,
                            }
                        })
                        .collect(),
                    name: file.group.name,
                }
            })
            .collect()
    })
}

/// A `editor.liveTemplates.custom` entry.
struct Custom<'a> {
    map: &'a BTreeMap<String, String>,
}

impl Custom<'_> {
    fn get(&self, key: &str) -> &str {
        self.map.get(key).map_or("", |s| s.as_str())
    }
    fn group(&self) -> &str {
        match self.get("group").trim() {
            "" => "user",
            g => g,
        }
    }
    fn contexts(&self) -> Vec<String> {
        self.get("contexts")
            .split(',')
            .map(str::trim)
            .filter(|c| !c.is_empty())
            .map(String::from)
            .collect()
    }
    fn variables(&self) -> Vec<Variable> {
        serde_json::from_str(self.get("variables")).unwrap_or_default()
    }
}

/// Built-in groups with the user's templates, changes, removals and disabled templates
/// applied; groups and templates sorted by name like PHPStorm's tree.
pub fn resolve(settings: &TemplateSettings) -> Vec<Group> {
    let mut groups: Vec<Group> = builtin_groups().to_vec();
    for map in &settings.custom {
        let c = Custom { map };
        let abbreviation = c.get("abbreviation").trim().to_string();
        let target = match c.get("builtin") {
            "" => template_id(c.group(), &abbreviation),
            b => b.to_string(),
        };
        // Take out the template this entry replaces (built-in or an earlier entry).
        let mut previous = None;
        for g in groups.iter_mut() {
            if let Some(pos) = g
                .templates
                .iter()
                .position(|t| t.builtin.as_deref() == Some(&target) || t.id == target)
            {
                previous = Some(g.templates.remove(pos));
                break;
            }
        }
        // A new template may not have an abbreviation yet; it shows but never expands.
        if c.get("removed") == "true" {
            continue;
        }
        let builtin = previous.as_ref().and_then(|p| p.builtin.clone());
        let contexts = c.contexts();
        let template = Template {
            id: template_id(c.group(), &abbreviation),
            group: c.group().to_string(),
            abbreviation,
            description: c.get("description").to_string(),
            text: c.get("text").to_string(),
            variables: c.variables(),
            applicable: describe(&contexts),
            contexts,
            enabled: true,
            modified: builtin.is_some(),
            builtin,
            key: Some(c.get("key").to_string()).filter(|k| !k.is_empty()),
        };
        match groups.iter_mut().find(|g| g.name == template.group) {
            Some(g) => g.templates.push(template),
            None => groups.push(Group {
                name: template.group.clone(),
                framework: None,
                active: true,
                builtin: false,
                templates: vec![template],
            }),
        }
    }
    for g in groups.iter_mut() {
        g.active = g
            .framework
            .as_ref()
            .is_none_or(|f| settings.frameworks.contains(f));
        for t in g.templates.iter_mut() {
            t.enabled = !settings.disabled.contains(&t.id);
        }
        g.templates.sort_by_key(|a| a.abbreviation.to_lowercase());
    }
    groups.retain(|g| g.builtin || !g.templates.is_empty());
    groups.sort_by_key(|a| a.name.to_lowercase());
    groups
}

/// Enabled templates of the groups that apply in the open project.
pub fn active_templates(settings: &TemplateSettings) -> Vec<Template> {
    resolve(settings)
        .into_iter()
        .filter(|g| g.active)
        .flat_map(|g| g.templates)
        .filter(|t| t.enabled && !t.abbreviation.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn find<'a>(groups: &'a [Group], id: &str) -> Option<&'a Template> {
        groups
            .iter()
            .flat_map(|g| &g.templates)
            .find(|t| t.id == id)
    }

    #[test]
    fn ships_the_builtin_sets() {
        let groups = resolve(&TemplateSettings::default());
        let names: Vec<&str> = groups.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "Blade",
                "JavaScript",
                "Laravel",
                "Lua",
                "PHP",
                "Symfony",
                "Twig"
            ]
        );
        for abbr in [
            "fore", "forek", "pubf", "prif", "prof", "pubsf", "__c", "eco", "thr",
        ] {
            assert!(find(&groups, &format!("PHP/{abbr}")).is_some(), "{abbr}");
        }
        let laravel = groups.iter().find(|g| g.name == "Laravel").unwrap();
        assert_eq!(laravel.framework.as_deref(), Some("laravel"));
        assert!(!laravel.active);
        let fore = find(&groups, "PHP/fore").unwrap();
        assert_eq!(fore.applicable, "PHP: statement");
        // Every built-in template parses and names known contexts.
        let known: Vec<String> = crate::context_tree()
            .iter()
            .flat_map(|n| {
                std::iter::once(n.id.clone()).chain(n.children.iter().map(|c| c.id.clone()))
            })
            .collect();
        for t in groups.iter().flat_map(|g| &g.templates) {
            assert!(
                !t.contexts.is_empty() && t.contexts.iter().all(|c| known.contains(c)),
                "{}",
                t.id
            );
            for v in &t.variables {
                assert!(
                    crate::template::variable_names(&t.text).contains(&v.name),
                    "{} {}",
                    t.id,
                    v.name
                );
                for e in [&v.expression, &v.default_value]
                    .into_iter()
                    .filter(|e| !e.is_empty())
                {
                    assert!(crate::expr::parse(e).is_some(), "{} {e}", t.id);
                }
            }
        }
    }

    #[test]
    fn applies_user_changes() {
        let settings = TemplateSettings {
            custom: vec![
                entry(&[
                    ("group", "PHP"),
                    ("abbreviation", "fe"),
                    ("builtin", "PHP/fore"),
                    ("text", "foreach"),
                    ("contexts", "PHP"),
                ]),
                entry(&[
                    ("builtin", "PHP/eco"),
                    ("group", "PHP"),
                    ("abbreviation", "eco"),
                    ("removed", "true"),
                ]),
                entry(&[
                    ("key", "k1"),
                    ("abbreviation", "hello"),
                    ("text", "hello $NAME$"),
                    ("contexts", "PHP_STATEMENT, HTML"),
                    ("variables", r#"[{"name":"NAME","expression":"user()"}]"#),
                ]),
            ],
            disabled: vec!["PHP/thr".into()],
            frameworks: vec!["laravel".into()],
        };
        let groups = resolve(&settings);
        assert!(find(&groups, "PHP/fore").is_none());
        let fe = find(&groups, "PHP/fe").unwrap();
        assert!(fe.modified);
        assert_eq!(fe.builtin.as_deref(), Some("PHP/fore"));
        assert!(find(&groups, "PHP/eco").is_none());
        let hello = find(&groups, "user/hello").unwrap();
        assert!(!hello.modified && hello.builtin.is_none());
        assert_eq!(hello.key.as_deref(), Some("k1"));
        assert_eq!(hello.contexts, ["PHP_STATEMENT", "HTML"]);
        assert_eq!(hello.variables[0].expression, "user()");
        assert!(hello.variables[0].always_stop_at);
        assert!(!find(&groups, "PHP/thr").unwrap().enabled);
        assert!(groups.iter().find(|g| g.name == "Laravel").unwrap().active);
        assert!(!groups.iter().find(|g| g.name == "Symfony").unwrap().active);
        let active = active_templates(&settings);
        assert!(active.iter().any(|t| t.id == "Laravel/rget"));
        assert!(!active
            .iter()
            .any(|t| t.id == "PHP/thr" || t.group == "Symfony"));
    }
}
