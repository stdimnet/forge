//! Inspection profiles, as in PHPStorm: "Default" (IDE level), "Project Default" (stored in
//! the project) and copies of either. A profile only stores what differs from the
//! inspections' defaults: whether an inspection is enabled, its severity, scope-specific
//! settings (the first scope containing a file wins, "Everywhere else" is the inspection's
//! own setting) and option values.

use std::collections::BTreeMap;

use forge_scopes::{Scope, ScopeContext, ScopeFilter, ScopeSet};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::registry::{Registry, TreeGroup};
use crate::severity::{self, SeverityDef};
use crate::spec::InspectionSpec;

pub const DEFAULT: &str = "Default";
pub const PROJECT_DEFAULT: &str = "Project Default";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProfileLevel {
    Ide,
    Project,
}

/// Settings of one inspection in a scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopeSettings {
    /// A named scope or a predefined scope's label (`Tests`, `Project Test Files`).
    pub scope: String,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,
}

/// What a profile changes for one inspection.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ToolSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub scopes: Vec<ScopeSettings>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub options: BTreeMap<String, Value>,
}

impl ToolSettings {
    fn is_empty(&self) -> bool {
        self.enabled.is_none()
            && self.severity.is_none()
            && self.scopes.is_empty()
            && self.options.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub name: String,
    pub level: ProfileLevel,
    #[serde(default)]
    pub tools: BTreeMap<String, ToolSettings>,
}

/// An inspection's effective settings for a file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Effective {
    /// `None`: keep the level the problem reports (external tools).
    pub severity: Option<String>,
    pub options: BTreeMap<String, Value>,
}

impl Profile {
    pub fn new(name: &str, level: ProfileLevel) -> Self {
        Profile {
            name: name.into(),
            level,
            tools: BTreeMap::new(),
        }
    }

    pub fn tool(&self, id: &str) -> Option<&ToolSettings> {
        self.tools.get(id)
    }

    /// Enabled everywhere else (not counting scopes).
    pub fn is_enabled(&self, spec: &InspectionSpec) -> bool {
        self.tool(&spec.id)
            .and_then(|t| t.enabled)
            .unwrap_or(spec.enabled)
    }

    /// Enabled in any scope or everywhere else.
    pub fn is_enabled_anywhere(&self, spec: &InspectionSpec) -> bool {
        self.is_enabled(spec)
            || self
                .tool(&spec.id)
                .is_some_and(|t| t.scopes.iter().any(|s| s.enabled))
    }

    /// The severity everywhere else; `None` keeps what an external tool reports.
    pub fn severity(&self, spec: &InspectionSpec) -> Option<String> {
        match self.tool(&spec.id).and_then(|t| t.severity.clone()) {
            Some(s) => Some(severity::normalize(&s)),
            None if spec.external => None,
            None => Some(spec.severity.clone()),
        }
    }

    /// The option values: the profile's, else the defaults.
    pub fn options(&self, spec: &InspectionSpec) -> BTreeMap<String, Value> {
        let set = self.tool(&spec.id).map(|t| &t.options);
        spec.options
            .iter()
            .map(|o| {
                let v = set
                    .and_then(|s| s.get(&o.id))
                    .and_then(|v| o.coerce(v))
                    .unwrap_or_else(|| o.default.clone());
                (o.id.clone(), v)
            })
            .collect()
    }

    fn tool_mut(&mut self, id: &str) -> &mut ToolSettings {
        self.tools.entry(id.to_string()).or_default()
    }

    /// Drop what equals the defaults (and settings of options the inspection lacks).
    pub fn compact(&mut self, registry: &Registry) {
        for (id, t) in self.tools.iter_mut() {
            let Some(spec) = registry.get(id) else {
                continue;
            };
            if t.enabled == Some(spec.enabled) {
                t.enabled = None;
            }
            if let Some(s) = &t.severity {
                let s = severity::normalize(s);
                t.severity = (spec.external || s != spec.severity).then_some(s);
            }
            t.options = t
                .options
                .iter()
                .filter_map(|(k, v)| {
                    let o = spec.options.iter().find(|o| &o.id == k)?;
                    let c = o.coerce(v)?;
                    (c != o.default).then(|| (k.clone(), c))
                })
                .collect();
            for s in &mut t.scopes {
                s.severity = s.severity.as_deref().map(severity::normalize);
            }
        }
        self.tools.retain(|_, t| !t.is_empty());
    }

    /// Apply an edit from the Settings page.
    pub fn edit(&mut self, registry: &Registry, op: &ToolOp) {
        match op {
            ToolOp::SetEnabled { ids, enabled } => {
                for id in ids {
                    self.tool_mut(id).enabled = Some(*enabled);
                }
            }
            ToolOp::SetSeverity { ids, severity } => {
                for id in ids {
                    self.tool_mut(id).severity = Some(severity::normalize(severity));
                }
            }
            ToolOp::SetScopes { id, scopes } => {
                self.tool_mut(id).scopes = scopes.clone();
            }
            ToolOp::SetOption { id, option, value } => {
                self.tool_mut(id)
                    .options
                    .insert(option.clone(), value.clone());
            }
            ToolOp::Reset { ids } => {
                for id in ids {
                    self.tools.remove(id);
                }
            }
        }
        self.compact(registry);
    }

    /// Whether the inspection differs from its defaults (shown in blue in the tree).
    pub fn is_modified(&self, id: &str) -> bool {
        self.tool(id).is_some_and(|t| !t.is_empty())
    }
}

/// An edit of a profile on the Settings page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "op")]
pub enum ToolOp {
    SetEnabled {
        ids: Vec<String>,
        enabled: bool,
    },
    SetSeverity {
        ids: Vec<String>,
        severity: String,
    },
    SetScopes {
        id: String,
        scopes: Vec<ScopeSettings>,
    },
    SetOption {
        id: String,
        option: String,
        value: Value,
    },
    Reset {
        ids: Vec<String>,
    },
}

/// The profiles and the one the project (or the IDE without a project) uses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfilesState {
    pub profiles: Vec<Profile>,
    /// The selected profile's name.
    pub selected: String,
    pub has_project: bool,
    /// Custom severities (Severities Editor).
    #[serde(default)]
    pub severities: Vec<SeverityDef>,
}

/// A change to the list of profiles (the gear menu of the Inspections page).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "op")]
pub enum ProfileOp {
    /// Duplicate `from` as `name` on `level` ("Duplicate…", "Copy to Project", "Copy to IDE").
    Copy {
        from: String,
        name: String,
        level: ProfileLevel,
    },
    Rename {
        from: String,
        name: String,
    },
    /// Default and Project Default cannot be deleted.
    Delete {
        name: String,
    },
    /// Back to the defaults ("Restore Defaults").
    Reset {
        name: String,
    },
    Select {
        name: String,
    },
    /// An imported profile (renamed when the name is taken).
    Add {
        profile: Profile,
    },
    Edit {
        name: String,
        edit: ToolOp,
    },
    SetSeverities {
        severities: Vec<SeverityDef>,
    },
}

impl ProfilesState {
    /// Default (and Project Default with a project) only.
    pub fn fresh(has_project: bool) -> Self {
        let mut profiles = vec![Profile::new(DEFAULT, ProfileLevel::Ide)];
        if has_project {
            profiles.push(Profile::new(PROJECT_DEFAULT, ProfileLevel::Project));
        }
        ProfilesState {
            selected: if has_project {
                PROJECT_DEFAULT
            } else {
                DEFAULT
            }
            .into(),
            profiles,
            has_project,
            severities: Vec::new(),
        }
    }

    pub fn get(&self, name: &str) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.name == name)
    }

    /// The selected profile (Default when the selection is gone).
    pub fn selected(&self) -> &Profile {
        self.get(&self.selected)
            .or_else(|| self.get(PROJECT_DEFAULT))
            .or_else(|| self.get(DEFAULT))
            .or(self.profiles.first())
            .expect("profiles are never empty")
    }

    /// A name not used yet: `name`, `name (2)`, ...
    pub fn unique_name(&self, name: &str) -> String {
        let base = name.trim();
        let base = if base.is_empty() { "Profile" } else { base };
        if self.get(base).is_none() {
            return base.into();
        }
        (2..)
            .map(|i| format!("{base} ({i})"))
            .find(|n| self.get(n).is_none())
            .expect("some name is free")
    }

    fn check_name(&self, name: &str) -> Result<String, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("The profile name is empty".into());
        }
        if self.get(name).is_some() {
            return Err(format!("A profile named '{name}' already exists"));
        }
        Ok(name.into())
    }

    /// Apply a change; errors are shown to the user.
    pub fn apply(&mut self, registry: &Registry, op: ProfileOp) -> Result<(), String> {
        match op {
            ProfileOp::Copy { from, name, level } => {
                let name = self.check_name(&name)?;
                if level == ProfileLevel::Project && !self.has_project {
                    return Err("No project is open".into());
                }
                let src = self.get(&from).ok_or("No such profile")?;
                let copy = Profile {
                    name: name.clone(),
                    level,
                    tools: src.tools.clone(),
                };
                self.profiles.push(copy);
                self.selected = name;
            }
            ProfileOp::Rename { from, name } => {
                if from == DEFAULT || from == PROJECT_DEFAULT {
                    return Err(format!("'{from}' cannot be renamed"));
                }
                let name = if name.trim() == from {
                    return Ok(());
                } else {
                    self.check_name(&name)?
                };
                let p = self
                    .profiles
                    .iter_mut()
                    .find(|p| p.name == from)
                    .ok_or("No such profile")?;
                p.name = name.clone();
                if self.selected == from {
                    self.selected = name;
                }
            }
            ProfileOp::Delete { name } => {
                if name == DEFAULT || name == PROJECT_DEFAULT {
                    return Err(format!("'{name}' cannot be deleted"));
                }
                self.profiles.retain(|p| p.name != name);
                if self.selected == name {
                    self.selected = self.selected().name.clone();
                }
            }
            ProfileOp::Reset { name } => {
                let p = self
                    .profiles
                    .iter_mut()
                    .find(|p| p.name == name)
                    .ok_or("No such profile")?;
                p.tools.clear();
            }
            ProfileOp::Select { name } => {
                if self.get(&name).is_none() {
                    return Err("No such profile".into());
                }
                self.selected = name;
            }
            ProfileOp::Add { mut profile } => {
                if profile.level == ProfileLevel::Project && !self.has_project {
                    profile.level = ProfileLevel::Ide;
                }
                profile.name = self.unique_name(&profile.name);
                profile.compact(registry);
                self.selected = profile.name.clone();
                self.profiles.push(profile);
            }
            ProfileOp::Edit { name, edit } => {
                let p = self
                    .profiles
                    .iter_mut()
                    .find(|p| p.name == name)
                    .ok_or("No such profile")?;
                p.edit(registry, &edit);
            }
            ProfileOp::SetSeverities { severities } => {
                self.severities = severity::all(&severities)
                    .into_iter()
                    .filter(|s| s.custom)
                    .collect();
            }
        }
        Ok(())
    }
}

/// Whether all, some or none of a group's inspections are enabled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Check {
    All,
    Some,
    None,
}

/// A row of the Settings tree with the profile's state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewGroup {
    pub name: String,
    pub path: String,
    pub check: Check,
    pub modified: bool,
    /// Every inspection id below (for the group checkbox).
    pub ids: Vec<String>,
    pub groups: Vec<ViewGroup>,
    pub inspections: Vec<ViewLeaf>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewLeaf {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    /// The severity everywhere else (`None`: as the tool reports).
    pub severity: Option<String>,
    pub modified: bool,
    /// Has scope-specific settings.
    pub scoped: bool,
}

/// The Settings tree of `profile`, filtered by `query`.
pub fn view(registry: &Registry, profile: &Profile, query: &str) -> ViewGroup {
    fn build(registry: &Registry, profile: &Profile, g: &TreeGroup) -> ViewGroup {
        let inspections: Vec<ViewLeaf> = g
            .inspections
            .iter()
            .filter_map(|l| registry.get(&l.id))
            .map(|s| ViewLeaf {
                id: s.id.clone(),
                name: s.name.clone(),
                enabled: profile.is_enabled_anywhere(s),
                severity: profile.severity(s),
                modified: profile.is_modified(&s.id),
                scoped: profile.tool(&s.id).is_some_and(|t| !t.scopes.is_empty()),
            })
            .collect();
        let groups: Vec<ViewGroup> = g
            .groups
            .iter()
            .map(|c| build(registry, profile, c))
            .collect();
        let ids = g.all_ids();
        let on = ids
            .iter()
            .filter_map(|id| registry.get(id))
            .filter(|s| profile.is_enabled_anywhere(s))
            .count();
        ViewGroup {
            name: g.name.clone(),
            path: g.path.clone(),
            check: if on == 0 {
                Check::None
            } else if on == ids.len() {
                Check::All
            } else {
                Check::Some
            },
            modified: ids.iter().any(|id| profile.is_modified(id)),
            ids,
            groups,
            inspections,
        }
    }
    build(registry, profile, &registry.tree(query))
}

/// A profile ready to answer what an inspection does for a file.
#[derive(Debug, Clone)]
pub struct ResolvedProfile {
    pub registry: Registry,
    pub profile: Profile,
    pub severities: Vec<SeverityDef>,
    scopes: Vec<(String, Option<Scope>)>,
}

impl ResolvedProfile {
    /// `scopes` and `context` evaluate the scope names the profile uses.
    pub fn new(
        registry: Registry,
        profile: Profile,
        severities: Vec<SeverityDef>,
        scopes: &ScopeSet,
        context: &ScopeContext,
    ) -> Self {
        let mut compiled: Vec<(String, Option<Scope>)> = Vec::new();
        for t in profile.tools.values() {
            for s in &t.scopes {
                if compiled.iter().any(|(n, _)| n == &s.scope) {
                    continue;
                }
                let scope = scopes.resolve(&s.scope).and_then(|spec| {
                    Scope::new(&ScopeFilter {
                        scope: spec,
                        context: context.clone(),
                    })
                    .ok()
                });
                compiled.push((s.scope.clone(), scope));
            }
        }
        ResolvedProfile {
            registry,
            profile,
            severities,
            scopes: compiled,
        }
    }

    fn in_scope(&self, name: &str, path: &str) -> bool {
        self.scopes
            .iter()
            .find(|(n, _)| n == name)
            .and_then(|(_, s)| s.as_ref())
            .is_some_and(|s| s.contains(path))
    }

    /// What inspection `id` does for `path`: `None` when it is off there. Inspections the
    /// registry does not know are on, keeping the problem's severity.
    pub fn effective(&self, id: &str, path: &str) -> Option<Effective> {
        let Some(spec) = self.registry.get(id) else {
            return Some(Effective {
                severity: None,
                options: BTreeMap::new(),
            });
        };
        let mut enabled = self.profile.is_enabled(spec);
        let mut level = self.profile.severity(spec);
        if let Some(t) = self.profile.tool(id) {
            if let Some(s) = t.scopes.iter().find(|s| self.in_scope(&s.scope, path)) {
                enabled = s.enabled;
                if let Some(sev) = &s.severity {
                    level = Some(severity::normalize(sev));
                }
            }
        }
        enabled.then(|| Effective {
            severity: level,
            options: self.profile.options(spec),
        })
    }

    /// Whether the inspection runs anywhere (Inspect Code skips the others).
    pub fn enabled_anywhere(&self, id: &str) -> bool {
        match self.registry.get(id) {
            Some(s) => self.profile.is_enabled_anywhere(s),
            None => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::{OptionKind, OptionSpec};
    use forge_scopes::NamedScope;
    use serde_json::json;

    fn registry() -> Registry {
        let base = InspectionSpec {
            id: "PhpUnusedAssignment".into(),
            name: "Unused assignment".into(),
            group: "PHP/Data flow".into(),
            language: "php".into(),
            severity: "WARNING".into(),
            enabled: true,
            description: String::new(),
            options: vec![OptionSpec {
                id: "reportParams".into(),
                label: "Report parameters".into(),
                kind: OptionKind::Bool,
                default: json!(false),
                choices: vec![],
                min: None,
                max: None,
            }],
            cleanup: false,
            external: false,
            batch: true,
        };
        let off = InspectionSpec {
            id: "PhpConstantCondition".into(),
            name: "Constant condition".into(),
            enabled: false,
            options: vec![],
            ..base.clone()
        };
        let tool = InspectionSpec {
            id: "RsClippy".into(),
            name: "Clippy".into(),
            group: "Rust".into(),
            external: true,
            options: vec![],
            ..base.clone()
        };
        Registry::new(vec![base, off, tool], vec![])
    }

    #[test]
    fn edits_store_only_differences() {
        let r = registry();
        let mut p = Profile::new("Default", ProfileLevel::Ide);
        p.edit(
            &r,
            &ToolOp::SetEnabled {
                ids: vec!["PhpUnusedAssignment".into()],
                enabled: true,
            },
        );
        assert!(p.tools.is_empty(), "same as the default");
        p.edit(
            &r,
            &ToolOp::SetSeverity {
                ids: vec!["PhpUnusedAssignment".into()],
                severity: "weak warning".into(),
            },
        );
        assert_eq!(
            p.tools["PhpUnusedAssignment"].severity.as_deref(),
            Some("WEAK WARNING")
        );
        p.edit(
            &r,
            &ToolOp::SetOption {
                id: "PhpUnusedAssignment".into(),
                option: "reportParams".into(),
                value: json!(false),
            },
        );
        assert!(p.tools["PhpUnusedAssignment"].options.is_empty());
        p.edit(
            &r,
            &ToolOp::SetOption {
                id: "PhpUnusedAssignment".into(),
                option: "reportParams".into(),
                value: json!("true"),
            },
        );
        assert_eq!(
            p.tools["PhpUnusedAssignment"].options["reportParams"],
            json!(true)
        );
        let spec = r.get("PhpUnusedAssignment").unwrap();
        assert_eq!(p.options(spec)["reportParams"], json!(true));
        p.edit(
            &r,
            &ToolOp::Reset {
                ids: vec!["PhpUnusedAssignment".into()],
            },
        );
        assert!(p.tools.is_empty());
    }

    #[test]
    fn scope_settings_win_for_files_in_the_scope() {
        let r = registry();
        let mut p = Profile::new("P", ProfileLevel::Project);
        p.edit(
            &r,
            &ToolOp::SetScopes {
                id: "PhpUnusedAssignment".into(),
                scopes: vec![
                    ScopeSettings {
                        scope: "Legacy".into(),
                        enabled: false,
                        severity: None,
                    },
                    ScopeSettings {
                        scope: "Tests".into(),
                        enabled: true,
                        severity: Some("WEAK WARNING".into()),
                    },
                ],
            },
        );
        let scopes = ScopeSet {
            shared: vec![
                NamedScope {
                    name: "Legacy".into(),
                    pattern: "file:legacy//*".into(),
                },
                NamedScope {
                    name: "Tests".into(),
                    pattern: "file:tests//*".into(),
                },
            ],
            local: vec![],
        };
        let res = ResolvedProfile::new(r, p, vec![], &scopes, &ScopeContext::default());
        assert!(res
            .effective("PhpUnusedAssignment", "legacy/a.php")
            .is_none());
        assert_eq!(
            res.effective("PhpUnusedAssignment", "tests/ATest.php")
                .unwrap()
                .severity
                .as_deref(),
            Some("WEAK WARNING")
        );
        assert_eq!(
            res.effective("PhpUnusedAssignment", "src/A.php")
                .unwrap()
                .severity
                .as_deref(),
            Some("WARNING")
        );
        assert!(
            res.effective("PhpConstantCondition", "src/A.php").is_none(),
            "off by default"
        );
        assert_eq!(
            res.effective("RsClippy", "src/main.rs").unwrap().severity,
            None,
            "external keeps its level"
        );
        assert!(res.effective("Unknown", "x").is_some());
    }

    #[test]
    fn profile_operations() {
        let r = registry();
        let mut s = ProfilesState::fresh(true);
        assert_eq!(s.selected().name, PROJECT_DEFAULT);
        s.apply(
            &r,
            ProfileOp::Copy {
                from: "Default".into(),
                name: "Strict".into(),
                level: ProfileLevel::Ide,
            },
        )
        .unwrap();
        assert_eq!(s.selected, "Strict");
        assert!(s
            .apply(
                &r,
                ProfileOp::Copy {
                    from: "Default".into(),
                    name: "Strict".into(),
                    level: ProfileLevel::Ide
                }
            )
            .is_err());
        s.apply(
            &r,
            ProfileOp::Rename {
                from: "Strict".into(),
                name: "CI".into(),
            },
        )
        .unwrap();
        assert_eq!(s.selected, "CI");
        assert!(s
            .apply(
                &r,
                ProfileOp::Rename {
                    from: "Default".into(),
                    name: "X".into()
                }
            )
            .is_err());
        s.apply(
            &r,
            ProfileOp::Edit {
                name: "CI".into(),
                edit: ToolOp::SetEnabled {
                    ids: vec!["PhpConstantCondition".into()],
                    enabled: true,
                },
            },
        )
        .unwrap();
        assert!(s.get("CI").unwrap().is_modified("PhpConstantCondition"));
        s.apply(
            &r,
            ProfileOp::Add {
                profile: Profile::new("CI", ProfileLevel::Project),
            },
        )
        .unwrap();
        assert_eq!(s.selected, "CI (2)");
        s.apply(
            &r,
            ProfileOp::Delete {
                name: "CI (2)".into(),
            },
        )
        .unwrap();
        assert_eq!(s.selected, PROJECT_DEFAULT);
        assert!(s
            .apply(
                &r,
                ProfileOp::Delete {
                    name: DEFAULT.into()
                }
            )
            .is_err());
    }

    #[test]
    fn view_reports_group_state() {
        let r = registry();
        let p = Profile::new("Default", ProfileLevel::Ide);
        let v = view(&r, &p, "");
        let php = v.groups.iter().find(|g| g.name == "PHP").unwrap();
        assert_eq!(php.check, Check::Some);
        assert!(!php.modified);
        let rust = v.groups.iter().find(|g| g.name == "Rust").unwrap();
        assert_eq!(rust.inspections[0].severity, None);
    }
}
