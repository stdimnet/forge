//! Where profiles live:
//!
//! - IDE profiles (Default and its copies): the user settings, `inspections.profiles` (a
//!   list of `{ name, level, tools }`); custom severities: `inspections.severities`.
//! - Project profiles: PHPStorm XML files in `.forge/inspectionProfiles/`
//!   (`Project_Default.xml`, written once it differs from the defaults); further project
//!   profiles are listed in the project setting `inspections.projectProfiles`.
//! - The selected profile: `inspections.profile` in the project settings (the user settings
//!   without a project).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::io;
use crate::profile::{Profile, ProfileLevel, ProfilesState, DEFAULT, PROJECT_DEFAULT};
use crate::registry::Registry;
use crate::severity::SeverityDef;

pub const PROFILES_KEY: &str = "inspections.profiles";
pub const SEVERITIES_KEY: &str = "inspections.severities";
pub const SELECTED_KEY: &str = "inspections.profile";
pub const PROJECT_PROFILES_KEY: &str = "inspections.projectProfiles";
pub const PROFILES_DIR: &str = ".forge/inspectionProfiles";

/// A settings layer: flat dotted keys.
pub type Layer = BTreeMap<String, Value>;

/// The project-relative file of a project profile.
pub fn profile_path(name: &str) -> String {
    format!("{PROFILES_DIR}/{}", io::file_name(name))
}

/// The profiles from the settings layers and the project's profile files (`read` returns a
/// project-relative file's text).
pub fn load(
    user: &Layer,
    project: Option<&Layer>,
    read: &dyn Fn(&str) -> Option<String>,
    registry: &Registry,
) -> ProfilesState {
    let mut state = ProfilesState::fresh(project.is_some());
    let stored: Vec<Profile> = user
        .get(PROFILES_KEY)
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    for mut p in stored {
        p.level = ProfileLevel::Ide;
        p.compact(registry);
        if p.name == DEFAULT {
            state.profiles[0] = p;
        } else if !p.name.trim().is_empty() && state.get(&p.name).is_none() {
            state.profiles.push(p);
        }
    }
    state.severities = user
        .get(SEVERITIES_KEY)
        .and_then(|v| serde_json::from_value::<Vec<SeverityDef>>(v.clone()).ok())
        .unwrap_or_default();
    if let Some(project) = project {
        let mut names = vec![PROJECT_DEFAULT.to_string()];
        names.extend(
            project
                .get(PROJECT_PROFILES_KEY)
                .and_then(|v| serde_json::from_value::<Vec<String>>(v.clone()).ok())
                .unwrap_or_default(),
        );
        for name in names {
            let loaded = read(&profile_path(&name)).and_then(|t| io::from_xml(&t, registry).ok());
            let mut p = loaded.unwrap_or_else(|| Profile::new(&name, ProfileLevel::Project));
            p.name = name.clone();
            p.level = ProfileLevel::Project;
            if name == PROJECT_DEFAULT {
                if let Some(slot) = state
                    .profiles
                    .iter_mut()
                    .find(|x| x.name == PROJECT_DEFAULT)
                {
                    *slot = p;
                }
            } else if state.get(&name).is_none() {
                state.profiles.push(p);
            }
        }
    }
    let selected = project
        .and_then(|p| p.get(SELECTED_KEY))
        .or_else(|| user.get(SELECTED_KEY))
        .and_then(|v| v.as_str())
        .filter(|n| state.get(n).is_some());
    if let Some(n) = selected {
        state.selected = n.into();
    }
    state
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WriteScope {
    User,
    Project,
}

/// A settings change; `value: None` removes the key.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingWrite {
    pub key: String,
    pub value: Option<Value>,
    pub scope: WriteScope,
}

/// A project file change; `text: None` deletes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileWrite {
    pub path: String,
    pub text: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavePlan {
    pub settings: Vec<SettingWrite>,
    pub files: Vec<FileWrite>,
}

/// What to write so that `load` returns `new`; `old` is what was loaded (for the project
/// files of deleted or renamed profiles).
pub fn plan_save(old: &ProfilesState, new: &ProfilesState, registry: &Registry) -> SavePlan {
    let mut plan = SavePlan::default();
    let ide: Vec<Value> = new
        .profiles
        .iter()
        .filter(|p| p.level == ProfileLevel::Ide)
        .filter(|p| p.name != DEFAULT || !p.tools.is_empty())
        .map(|p| {
            let mut p = p.clone();
            p.compact(registry);
            serde_json::to_value(p).unwrap_or(Value::Null)
        })
        .collect();
    plan.settings.push(SettingWrite {
        key: PROFILES_KEY.into(),
        value: (!ide.is_empty()).then_some(Value::Array(ide)),
        scope: WriteScope::User,
    });
    plan.settings.push(SettingWrite {
        key: SEVERITIES_KEY.into(),
        value: (!new.severities.is_empty())
            .then(|| serde_json::to_value(&new.severities).unwrap_or(Value::Null)),
        scope: WriteScope::User,
    });
    plan.settings.push(SettingWrite {
        key: SELECTED_KEY.into(),
        value: Some(Value::String(new.selected().name.clone())),
        scope: if new.has_project {
            WriteScope::Project
        } else {
            WriteScope::User
        },
    });
    if !new.has_project {
        return plan;
    }
    let project: Vec<&Profile> = new
        .profiles
        .iter()
        .filter(|p| p.level == ProfileLevel::Project)
        .collect();
    let others: Vec<Value> = project
        .iter()
        .filter(|p| p.name != PROJECT_DEFAULT)
        .map(|p| Value::String(p.name.clone()))
        .collect();
    plan.settings.push(SettingWrite {
        key: PROJECT_PROFILES_KEY.into(),
        value: (!others.is_empty()).then_some(Value::Array(others)),
        scope: WriteScope::Project,
    });
    let mut written: Vec<String> = Vec::new();
    for p in &project {
        let path = profile_path(&p.name);
        let mut p = (*p).clone();
        p.compact(registry);
        let unchanged_default = p.name == PROJECT_DEFAULT && p.tools.is_empty();
        let before = old
            .get(&p.name)
            .filter(|o| o.level == ProfileLevel::Project);
        if unchanged_default {
            if before.is_some_and(|b| !b.tools.is_empty()) {
                plan.files.push(FileWrite {
                    path: path.clone(),
                    text: None,
                });
            }
        } else if before != Some(&p) {
            plan.files.push(FileWrite {
                path: path.clone(),
                text: Some(io::to_xml(&p, registry)),
            });
        }
        written.push(path);
    }
    for o in old
        .profiles
        .iter()
        .filter(|p| p.level == ProfileLevel::Project)
    {
        let path = profile_path(&o.name);
        if !written.contains(&path) {
            plan.files.push(FileWrite { path, text: None });
        }
    }
    plan
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::{ProfileOp, ToolOp};
    use crate::spec::InspectionSpec;

    fn registry() -> Registry {
        Registry::new(
            vec![InspectionSpec {
                id: "PhpUnusedAssignment".into(),
                name: "Unused assignment".into(),
                group: "PHP/Data flow".into(),
                language: "php".into(),
                severity: "WARNING".into(),
                enabled: true,
                description: String::new(),
                options: vec![],
                cleanup: false,
                external: false,
                batch: true,
            }],
            vec![],
        )
    }

    /// Apply a plan to in-memory layers and files.
    fn apply(
        plan: &SavePlan,
        user: &mut Layer,
        project: &mut Layer,
        files: &mut BTreeMap<String, String>,
    ) {
        for w in &plan.settings {
            let layer = match w.scope {
                WriteScope::User => &mut *user,
                WriteScope::Project => &mut *project,
            };
            match &w.value {
                Some(v) => {
                    layer.insert(w.key.clone(), v.clone());
                }
                None => {
                    layer.remove(&w.key);
                }
            }
        }
        for f in &plan.files {
            match &f.text {
                Some(t) => {
                    files.insert(f.path.clone(), t.clone());
                }
                None => {
                    files.remove(&f.path);
                }
            }
        }
    }

    #[test]
    fn save_and_load_round_trip() {
        let r = registry();
        let (mut user, mut project, mut files) = (Layer::new(), Layer::new(), BTreeMap::new());
        let read = |files: &BTreeMap<String, String>| {
            let files = files.clone();
            move |p: &str| files.get(p).cloned()
        };
        let old = load(&user, Some(&project), &read(&files), &r);
        assert_eq!(old.selected, PROJECT_DEFAULT);
        let mut new = old.clone();
        let off = ToolOp::SetEnabled {
            ids: vec!["PhpUnusedAssignment".into()],
            enabled: false,
        };
        new.apply(
            &r,
            ProfileOp::Edit {
                name: PROJECT_DEFAULT.into(),
                edit: off.clone(),
            },
        )
        .unwrap();
        new.apply(
            &r,
            ProfileOp::Copy {
                from: DEFAULT.into(),
                name: "Strict".into(),
                level: ProfileLevel::Ide,
            },
        )
        .unwrap();
        new.apply(
            &r,
            ProfileOp::Edit {
                name: "Strict".into(),
                edit: ToolOp::SetSeverity {
                    ids: vec!["PhpUnusedAssignment".into()],
                    severity: "ERROR".into(),
                },
            },
        )
        .unwrap();
        new.apply(
            &r,
            ProfileOp::Copy {
                from: PROJECT_DEFAULT.into(),
                name: "CI".into(),
                level: ProfileLevel::Project,
            },
        )
        .unwrap();
        let plan = plan_save(&old, &new, &r);
        apply(&plan, &mut user, &mut project, &mut files);
        assert!(files.contains_key(".forge/inspectionProfiles/Project_Default.xml"));
        assert!(files.contains_key(".forge/inspectionProfiles/CI.xml"));
        let loaded = load(&user, Some(&project), &read(&files), &r);
        assert_eq!(loaded, new);

        // Rename CI and restore Project Default: old files go.
        let mut next = loaded.clone();
        next.apply(
            &r,
            ProfileOp::Rename {
                from: "CI".into(),
                name: "Build".into(),
            },
        )
        .unwrap();
        next.apply(
            &r,
            ProfileOp::Reset {
                name: PROJECT_DEFAULT.into(),
            },
        )
        .unwrap();
        let plan = plan_save(&loaded, &next, &r);
        apply(&plan, &mut user, &mut project, &mut files);
        let names: Vec<&String> = files.keys().collect();
        assert_eq!(names, [".forge/inspectionProfiles/Build.xml"]);
        assert_eq!(load(&user, Some(&project), &read(&files), &r), next);

        // Without a project only the user settings change.
        let ide = load(&user, None, &|_| None, &r);
        assert_eq!(ide.profiles.len(), 2);
        assert!(plan_save(&ide, &ide, &r).files.is_empty());
    }
}
