//! The inspections of the enabled plugins and the framework's own ones, the Settings tree
//! (grouped by language / group, with search) and which inspection a tool's problem
//! belongs to.

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::severity;
use crate::spec::{InspectionSpec, OptionKind, OptionSpec, ToolMapping};

/// The id of the duplicate code inspection.
pub const DUPLICATED_CODE: &str = "DuplicatedCode";

/// The inspections the framework itself implements.
pub fn builtin_specs() -> Vec<InspectionSpec> {
    let opt = |id: &str, label: &str, kind: OptionKind, default: serde_json::Value| OptionSpec {
        id: id.into(),
        label: label.into(),
        kind,
        default,
        choices: Vec::new(),
        min: None,
        max: None,
    };
    vec![InspectionSpec {
        id: DUPLICATED_CODE.into(),
        name: "Duplicated code fragment".into(),
        group: "General".into(),
        language: String::new(),
        severity: severity::WEAK_WARNING.into(),
        enabled: true,
        description: "Reports fragments of code that appear more than once in the inspected \
                      scope. Tokens are compared, so formatting and comments do not matter; \
                      with \"Anonymize identifiers\" fragments that only differ in names of \
                      variables, functions and fields count as duplicates too."
            .into(),
        options: vec![
            OptionSpec {
                min: Some(10),
                max: Some(10_000),
                ..opt(
                    "minTokens",
                    "Minimum fragment size (tokens)",
                    OptionKind::Int,
                    json!(60),
                )
            },
            opt(
                "anonymizeIdentifiers",
                "Anonymize identifiers (variables, functions, fields)",
                OptionKind::Bool,
                json!(true),
            ),
            opt(
                "anonymizeLiterals",
                "Anonymize literals (strings, numbers)",
                OptionKind::Bool,
                json!(false),
            ),
        ],
        cleanup: false,
        external: false,
        batch: true,
    }]
}

/// The inspections Forge knows and the mappings of external tools' problems.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Registry {
    pub inspections: Vec<InspectionSpec>,
    pub mappings: Vec<ToolMapping>,
}

/// A group of the Settings tree.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeGroup {
    pub name: String,
    /// The `/`-separated group path.
    pub path: String,
    pub groups: Vec<TreeGroup>,
    pub inspections: Vec<TreeLeaf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeLeaf {
    pub id: String,
    pub name: String,
}

impl TreeGroup {
    /// Every inspection id below this group.
    pub fn all_ids(&self) -> Vec<String> {
        let mut out: Vec<String> = self.inspections.iter().map(|l| l.id.clone()).collect();
        for g in &self.groups {
            out.extend(g.all_ids());
        }
        out
    }

    fn sort(&mut self) {
        self.groups
            .sort_by_key(|g| (g.name != "General", g.name.to_lowercase()));
        self.inspections.sort_by_key(|l| l.name.to_lowercase());
        for g in &mut self.groups {
            g.sort();
        }
    }
}

impl Registry {
    /// The framework's inspections followed by the contributed ones; a later spec with the
    /// same id is dropped.
    pub fn new(
        contributed: impl IntoIterator<Item = InspectionSpec>,
        mappings: impl IntoIterator<Item = ToolMapping>,
    ) -> Self {
        let mut inspections: Vec<InspectionSpec> = Vec::new();
        for s in builtin_specs().into_iter().chain(contributed) {
            if s.id.trim().is_empty() || inspections.iter().any(|x| x.id == s.id) {
                continue;
            }
            inspections.push(InspectionSpec {
                severity: severity::normalize(&s.severity),
                ..s
            });
        }
        Registry {
            inspections,
            mappings: mappings.into_iter().collect(),
        }
    }

    pub fn get(&self, id: &str) -> Option<&InspectionSpec> {
        self.inspections.iter().find(|s| s.id == id)
    }

    /// The inspection a problem belongs to: its `code` when that is an inspection id, else
    /// the most specific mapping of its `source`.
    pub fn inspection_for(&self, source: &str, code: Option<&str>) -> Option<&str> {
        if let Some(c) = code.and_then(|c| self.get(c)) {
            return Some(&c.id);
        }
        self.mappings
            .iter()
            .filter_map(|m| m.matches(source, code).map(|score| (score, m)))
            .max_by_key(|(score, _)| *score)
            .map(|(_, m)| m.inspection.as_str())
            .filter(|id| self.get(id).is_some())
    }

    /// Whether `spec` matches a search text (name, id, group or description, every word).
    fn matches(spec: &InspectionSpec, query: &str) -> bool {
        let hay = format!(
            "{} {} {} {}",
            spec.name, spec.id, spec.group, spec.description
        )
        .to_lowercase();
        query
            .to_lowercase()
            .split_whitespace()
            .all(|w| hay.contains(w))
    }

    /// The Settings tree: groups by their path, inspections by name; with a `query` only
    /// matching inspections (and their groups).
    pub fn tree(&self, query: &str) -> TreeGroup {
        let mut root = TreeGroup::default();
        for spec in &self.inspections {
            if !query.trim().is_empty() && !Self::matches(spec, query) {
                continue;
            }
            let mut node = &mut root;
            let mut path = String::new();
            for part in spec
                .group
                .split('/')
                .map(str::trim)
                .filter(|p| !p.is_empty())
            {
                if !path.is_empty() {
                    path.push('/');
                }
                path.push_str(part);
                let i = match node.groups.iter().position(|g| g.name == part) {
                    Some(i) => i,
                    None => {
                        node.groups.push(TreeGroup {
                            name: part.into(),
                            path: path.clone(),
                            ..TreeGroup::default()
                        });
                        node.groups.len() - 1
                    }
                };
                node = &mut node.groups[i];
            }
            node.inspections.push(TreeLeaf {
                id: spec.id.clone(),
                name: spec.name.clone(),
            });
        }
        root.sort();
        root
    }

    /// Run Inspection by Name: inspections whose name (or id) matches `query`, best first.
    pub fn find_by_name(&self, query: &str, limit: usize) -> Vec<&InspectionSpec> {
        let q = query.trim().to_lowercase();
        let mut hits: Vec<(u8, &InspectionSpec)> = self
            .inspections
            .iter()
            .filter_map(|s| {
                let name = s.name.to_lowercase();
                let id = s.id.to_lowercase();
                let rank = if q.is_empty() {
                    3
                } else if name.starts_with(&q) || id.starts_with(&q) {
                    0
                } else if name.contains(&q) || id.contains(&q) {
                    1
                } else if Self::matches(s, &q) || subsequence(&q, &name) {
                    2
                } else {
                    return None;
                };
                Some((rank, s))
            })
            .collect();
        hits.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.name.cmp(&b.1.name)));
        hits.into_iter().take(limit).map(|(_, s)| s).collect()
    }
}

/// `needle`'s characters appear in `hay` in order (`unas` matches `unused assignment`).
fn subsequence(needle: &str, hay: &str) -> bool {
    let mut it = hay.chars();
    needle
        .chars()
        .filter(|c| !c.is_whitespace())
        .all(|c| it.any(|h| h == c))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(id: &str, group: &str) -> InspectionSpec {
        InspectionSpec {
            id: id.into(),
            name: format!("{id} name"),
            group: group.into(),
            language: String::new(),
            severity: "weak_warning".into(),
            enabled: true,
            description: String::new(),
            options: vec![],
            cleanup: false,
            external: false,
            batch: true,
        }
    }

    fn registry() -> Registry {
        Registry::new(
            vec![
                spec("PhpUnusedAssignment", "PHP/Data flow"),
                spec("PhpDeprecation", "PHP/General"),
                spec("RsClippy", "Rust"),
                spec("PhpDeprecation", "Other"),
            ],
            vec![
                ToolMapping {
                    source: "clippy".into(),
                    code: None,
                    inspection: "RsClippy".into(),
                },
                ToolMapping {
                    source: "lsp".into(),
                    code: None,
                    inspection: "Missing".into(),
                },
            ],
        )
    }

    #[test]
    fn duplicate_ids_keep_the_first_and_levels_normalize() {
        let r = registry();
        assert_eq!(r.inspections.len(), 4);
        assert_eq!(r.get("PhpDeprecation").unwrap().group, "PHP/General");
        assert_eq!(r.get("RsClippy").unwrap().severity, "WEAK WARNING");
    }

    #[test]
    fn problems_map_to_inspections() {
        let r = registry();
        assert_eq!(
            r.inspection_for("php-inspections", Some("PhpDeprecation")),
            Some("PhpDeprecation")
        );
        assert_eq!(
            r.inspection_for("clippy", Some("needless_return")),
            Some("RsClippy")
        );
        // A mapping onto an unknown inspection is ignored.
        assert_eq!(r.inspection_for("lsp", Some("P1013")), None);
        assert_eq!(r.inspection_for("other", None), None);
    }

    #[test]
    fn tree_groups_by_language_and_filters() {
        let r = registry();
        let t = r.tree("");
        let names: Vec<&str> = t.groups.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(names, ["General", "PHP", "Rust"]);
        let php = &t.groups[1];
        assert!(php.groups.iter().any(|g| g.path == "PHP/Data flow"));
        assert_eq!(php.all_ids().len(), 2);
        let found = r.tree("unusedassign");
        assert_eq!(found.all_ids(), ["PhpUnusedAssignment"]);
        assert!(r.tree("nothing-like-this").groups.is_empty());
    }

    #[test]
    fn find_by_name_ranks_prefixes_first() {
        let r = registry();
        let hits: Vec<&str> = r
            .find_by_name("php", 10)
            .iter()
            .map(|s| s.id.as_str())
            .collect();
        assert_eq!(hits, ["PhpDeprecation", "PhpUnusedAssignment"]);
        let fuzzy: Vec<&str> = r
            .find_by_name("dupl", 10)
            .iter()
            .map(|s| s.id.as_str())
            .collect();
        assert_eq!(fuzzy, [DUPLICATED_CODE]);
    }
}
