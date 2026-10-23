//! The settings groups of File | Manage IDE Settings | Export / Import Settings (PHPStorm's
//! "Select Components to Export"). A group is a set of setting key prefixes of the user
//! settings, or a configuration file next to them. Every key belongs to exactly one group:
//! the longest matching prefix wins, keys no group claims are "Other settings".

use serde::Serialize;

/// One group of the export and import dialogs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsGroup {
    pub id: &'static str,
    pub label: &'static str,
    /// Setting key prefixes (`editor.` or a whole key) of the user settings.
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    pub prefixes: &'static [&'static str],
    /// A file of `<config dir>/forge` exported as a whole.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<&'static str>,
}

const fn keys(
    id: &'static str,
    label: &'static str,
    prefixes: &'static [&'static str],
) -> SettingsGroup {
    SettingsGroup {
        id,
        label,
        prefixes,
        file: None,
    }
}

const fn file(id: &'static str, label: &'static str, file: &'static str) -> SettingsGroup {
    SettingsGroup {
        id,
        label,
        prefixes: &[],
        file: Some(file),
    }
}

/// Id of the group of keys no other group claims.
pub const OTHER: &str = "other";

const GROUPS: &[SettingsGroup] = &[
    keys(
        "appearance",
        "Appearance & Behavior",
        &[
            "appearance.",
            "projectView.",
            "structureView.",
            "system.",
            "notifications.",
            "scopes.",
            "fileColors.",
            "updates.",
        ],
    ),
    keys("keymap", "Keymap", &["keymap."]),
    keys(
        "editor",
        "Editor",
        &[
            "editor.",
            "todo.",
            "spelling.",
            "grammar.",
            "csv.",
            "images.",
            "markdown.",
            "findUsages.",
            "search.",
        ],
    ),
    keys(
        "codeStyle",
        "Code Style",
        &["editor.codeStyle.", "php.codeStyle."],
    ),
    keys(
        "fileTypes",
        "File Types",
        &[
            "fileTypes.",
            "templates.dataLanguages",
            "templates.delimiters",
        ],
    ),
    keys("templates", "File and Code Templates", &["fileTemplates."]),
    keys(
        "liveTemplates",
        "Live Templates",
        &["editor.liveTemplates.", "editor.postfix."],
    ),
    keys("vcs", "Version Control", &["vcs.", "localHistory."]),
    keys(
        "languages",
        "Languages & Frameworks",
        &[
            "php.",
            "lua.",
            "web.",
            "javascript.",
            "laravel.",
            "symfony.",
            "doctrine.",
            "angular.",
            "jsonSchema.",
            "langservers.",
            "commandLineTools.",
        ],
    ),
    keys(
        "tools",
        "Tools",
        &[
            "tools.",
            "terminal.",
            "actionsOnSave.",
            "database.",
            "docker.",
            "kubernetes.",
            "httpClient.",
            "run.",
            "agents.",
            "browserCompanion.",
            "vim.",
        ],
    ),
    keys("plugins", "Plugins", &["plugins."]),
    keys(OTHER, "Other settings", &[]),
    file(
        "newProjects",
        "Settings for New Projects",
        "new-project-settings.toml",
    ),
    file("aiProviders", "AI Providers (without API keys)", "ai.toml"),
    file(
        "dataSources",
        "Data Sources (without passwords)",
        "dataSources.toml",
    ),
    file("ssh", "SSH Configurations", "ssh.toml"),
];

/// All groups, in the order the dialogs list them.
pub fn settings_groups() -> &'static [SettingsGroup] {
    GROUPS
}

pub fn group(id: &str) -> Option<&'static SettingsGroup> {
    GROUPS.iter().find(|g| g.id == id)
}

/// The group a setting key belongs to (the longest matching prefix wins).
pub fn group_of_key(key: &str) -> &'static str {
    GROUPS
        .iter()
        .flat_map(|g| g.prefixes.iter().map(move |p| (g.id, *p)))
        .filter(|(_, p)| {
            if p.ends_with('.') {
                key.starts_with(p)
            } else {
                key == *p || key.starts_with(&format!("{p}."))
            }
        })
        .max_by_key(|(_, p)| p.len())
        .map_or(OTHER, |(id, _)| id)
}

/// A group with the number of values it would export, for the dialogs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupCount {
    pub id: &'static str,
    pub label: &'static str,
    /// Settings in the group, or 1 for a file group whose file exists.
    pub count: usize,
}

/// Count the user `keys` per group; `files` are the names of the existing config files.
/// Groups without anything to export are left out.
pub fn count_groups<'a>(
    keys: impl IntoIterator<Item = &'a str>,
    files: &[&str],
) -> Vec<GroupCount> {
    let keys: Vec<&str> = keys.into_iter().collect();
    GROUPS
        .iter()
        .map(|g| GroupCount {
            id: g.id,
            label: g.label,
            count: match g.file {
                Some(f) => usize::from(files.contains(&f)),
                None => keys.iter().filter(|k| group_of_key(k) == g.id).count(),
            },
        })
        .filter(|g| g.count > 0)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_go_to_the_most_specific_group() {
        assert_eq!(group_of_key("editor.font.size"), "editor");
        assert_eq!(group_of_key("editor.codeStyle.scheme"), "codeStyle");
        assert_eq!(group_of_key("editor.liveTemplates.custom"), "liveTemplates");
        assert_eq!(group_of_key("keymap.overrides.default"), "keymap");
        assert_eq!(group_of_key("templates.delimiters"), "fileTypes");
        assert_eq!(group_of_key("templates.dataLanguagesX"), OTHER);
        assert_eq!(group_of_key("fileTypes.custom"), "fileTypes");
        assert_eq!(group_of_key("php.interpreters"), "languages");
        assert_eq!(group_of_key("plugins.disabled"), "plugins");
        assert_eq!(group_of_key("somethingNew.flag"), OTHER);
    }

    #[test]
    fn counts_skip_empty_groups() {
        let counts = count_groups(
            [
                "editor.font.size",
                "editor.tabs.limit",
                "keymap.active",
                "x.y",
            ],
            &["ssh.toml"],
        );
        let pairs: Vec<_> = counts.iter().map(|g| (g.id, g.count)).collect();
        assert_eq!(
            pairs,
            [("keymap", 1), ("editor", 2), (OTHER, 1), ("ssh", 1)]
        );
        assert!(group("aiProviders").unwrap().file.is_some());
    }
}
