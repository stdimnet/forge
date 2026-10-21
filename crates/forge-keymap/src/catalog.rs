//! PHPStorm's default keymaps as data: "Windows/Linux" (`$default`) and "macOS".
//!
//! Action ids are PHPStorm's own and double as Forge command ids, so a feature implements
//! e.g. "Go to Class" simply by `registerCommand({ id: "GotoClass", ... })`. Actions marked
//! `native` (clipboard, caret movement, typing) are handled by the browser/CodeMirror and are
//! listed only so they show up in Find Action and the keymap settings.
//!
//! The rows live in `data/phpstorm.json` (one action per line: id, title, group, Windows/Linux
//! and macOS shortcuts separated by `|`, optional context and native flag).

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::shortcut::Platform;

/// One of the two built-in keymaps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KeymapId {
    #[serde(rename = "$default")]
    Default,
    #[serde(rename = "macOS")]
    MacOs,
}

impl KeymapId {
    pub const ALL: [KeymapId; 2] = [KeymapId::Default, KeymapId::MacOs];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "$default",
            Self::MacOs => "macOS",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "$default" => Some(Self::Default),
            "macOS" => Some(Self::MacOs),
            _ => None,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Default => "PHPStorm (Windows/Linux)",
            Self::MacOs => "PHPStorm (macOS)",
        }
    }

    /// Platform whose key names the keymap uses (macOS keymap → Cmd/⌘ symbols).
    pub fn platform(self) -> Platform {
        match self {
            Self::Default => Platform::Other,
            Self::MacOs => Platform::Mac,
        }
    }

    /// The keymap PHPStorm picks on `platform`.
    pub fn for_platform(platform: Platform) -> Self {
        match platform {
            Platform::Mac => Self::MacOs,
            Platform::Other => Self::Default,
        }
    }
}

/// Where an action may fire: "editor" actions only while a code editor has focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActionContext {
    #[default]
    Any,
    Editor,
    Toolwindow,
}

/// An action as listed in Find Action and the keymap settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionDef {
    pub id: String,
    pub title: String,
    /// Tree path in the keymap settings, e.g. "Main Menu/Navigate".
    pub group: String,
    #[serde(default)]
    pub context: ActionContext,
    #[serde(default)]
    pub native: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Top-level groups of the keymap tree, in display order.
pub const ACTION_GROUP_ORDER: [&str; 5] = [
    "Main Menu",
    "Editor Actions",
    "Debugger Actions",
    "Other",
    "Plugins",
];

/// Ids that Forge features already register under another name, plus common alternate
/// spellings. The UI follows these when the PHPStorm id itself has no command.
pub const ACTION_ALIASES: [(&str, &[&str]); 23] = [
    ("SearchEverywhere", &["search.everywhere"]),
    ("ShowSettings", &["settings.open"]),
    ("Run", &["run.run"]),
    ("Debug", &["run.debug"]),
    ("editRunConfigurations", &["run.editConfigurations"]),
    ("Git.Branches", &["git.branches"]),
    ("CheckinProject", &["git.commit"]),
    ("Vcs.Push", &["git.push"]),
    ("Vcs.UpdateProject", &["git.update"]),
    ("Git.Fetch", &["git.fetch"]),
    ("Git.CreateNewBranch", &["git.newBranch"]),
    ("Git.Pull", &["git.update"]),
    ("Git.CreateNewTag", &["Git.Tag"]),
    ("Compare.SameVersion", &["git.showDiff"]),
    // Ctrl+D in the Project view and the Commit window shows the diff of the selected file.
    ("CompareTwoFiles", &["git.showDiff"]),
    ("Vcs.Show.Log", &["git.log"]),
    ("Annotate", &["git.annotate"]),
    ("Generate", &["php.generate"]),
    ("OverrideMethods", &["php.overrideMethods"]),
    ("ImplementMethods", &["php.implementMethods"]),
    ("OpenFile", &["workspace.open"]),
    ("CloseProject", &["workspace.close"]),
    ("IntroduceVariable", &["ExtractVariable"]),
];

/// Aliases of one action id.
pub fn aliases_of(id: &str) -> &'static [&'static str] {
    ACTION_ALIASES
        .iter()
        .find(|(a, _)| *a == id)
        .map_or(&[], |(_, list)| list)
}

#[derive(Deserialize)]
struct Row {
    #[serde(flatten)]
    def: ActionDef,
    win: String,
    mac: String,
}

/// The built-in PHPStorm catalog.
pub struct Catalog {
    pub actions: Vec<ActionDef>,
    /// Default shortcuts per action, in catalog order.
    pub windows: Vec<(String, Vec<String>)>,
    pub mac: Vec<(String, Vec<String>)>,
}

impl Catalog {
    pub fn defaults(&self, km: KeymapId) -> &[(String, Vec<String>)] {
        match km {
            KeymapId::Default => &self.windows,
            KeymapId::MacOs => &self.mac,
        }
    }

    pub fn get(&self, id: &str) -> Option<&ActionDef> {
        self.actions.iter().find(|a| a.id == id)
    }
}

/// `"A | B"` → `["A", "B"]`. "Fn" is not a key the UI can see; such alternatives are dropped.
fn split_shortcuts(s: &str) -> Vec<String> {
    s.split('|')
        .map(str::trim)
        .filter(|x| !x.is_empty() && !x.contains("Fn+"))
        .map(str::to_string)
        .collect()
}

/// The PHPStorm catalog, parsed once from the embedded data.
pub fn catalog() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        let rows: Vec<Row> = serde_json::from_str(include_str!("../data/phpstorm.json"))
            .expect("data/phpstorm.json is valid");
        let mut out = Catalog {
            actions: Vec::with_capacity(rows.len()),
            windows: Vec::with_capacity(rows.len()),
            mac: Vec::with_capacity(rows.len()),
        };
        for row in rows {
            out.windows
                .push((row.def.id.clone(), split_shortcuts(&row.win)));
            out.mac
                .push((row.def.id.clone(), split_shortcuts(&row.mac)));
            out.actions.push(row.def);
        }
        out
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{all_conflicts, build_bindings};
    use crate::shortcut::{canonical_shortcut, try_parse_shortcut};
    use std::collections::HashSet;

    fn sc(km: KeymapId, id: &str) -> Vec<Option<String>> {
        let list = &catalog()
            .defaults(km)
            .iter()
            .find(|(a, _)| a == id)
            .unwrap_or_else(|| panic!("{id} missing"))
            .1;
        list.iter()
            .map(|s| canonical_shortcut(s, km.platform()))
            .collect()
    }

    #[test]
    fn covers_the_full_action_list() {
        let c = catalog();
        assert!(c.actions.len() >= 150);
        let ids: HashSet<_> = c.actions.iter().map(|a| &a.id).collect();
        assert_eq!(ids.len(), c.actions.len());
    }

    #[test]
    fn every_shortcut_parses() {
        for km in KeymapId::ALL {
            for (id, list) in catalog().defaults(km) {
                for s in list {
                    assert!(
                        try_parse_shortcut(s, km.platform()).is_some(),
                        "{id}: {s} ({km:?})"
                    );
                }
            }
        }
    }

    #[test]
    fn drops_fn_alternatives() {
        assert_eq!(
            split_shortcuts("PageUp | Fn+Up"),
            vec!["PageUp".to_string()]
        );
    }

    #[test]
    fn matches_phpstorm_windows_linux_defaults() {
        let expectations = [
            ("SearchEverywhere", "Double Shift"),
            ("GotoAction", "Ctrl+Shift+A"),
            ("GotoClass", "Ctrl+N"),
            ("GotoFile", "Ctrl+Shift+N"),
            ("GotoSymbol", "Ctrl+Alt+Shift+N"),
            ("RecentFiles", "Ctrl+E"),
            ("RecentLocations", "Ctrl+Shift+E"),
            ("Generate", "Alt+Insert"),
            ("ShowIntentionActions", "Alt+Enter"),
            ("SelectNextOccurrence", "Alt+J"),
            ("EditorSelectWord", "Ctrl+W"),
            ("EditorUnSelectWord", "Ctrl+Shift+W"),
            ("RenameElement", "Shift+F6"),
            ("Refactorings.QuickListPopupAction", "Ctrl+Alt+Shift+T"),
            ("CheckinProject", "Ctrl+K"),
            ("Vcs.Push", "Ctrl+Shift+K"),
            ("Vcs.UpdateProject", "Ctrl+T"),
            ("ActivateProjectToolWindow", "Alt+1"),
            ("ActivateTerminalToolWindow", "Alt+F12"),
            ("HideAllWindows", "Ctrl+Shift+F12"),
            ("ShowSettings", "Ctrl+Alt+S"),
            ("NewScratchFile", "Ctrl+Alt+Shift+Insert"),
            ("CloseContent", "Ctrl+F4"),
            ("NextTab", "Alt+Right"),
            ("PreviousTab", "Alt+Left"),
            ("FileStructurePopup", "Ctrl+F12"),
            ("QuickJavaDoc", "Ctrl+Q"),
            ("ParameterInfo", "Ctrl+P"),
            ("EditorCompleteStatement", "Ctrl+Shift+Enter"),
            ("SurroundWith", "Ctrl+Alt+T"),
            ("InsertLiveTemplate", "Ctrl+J"),
            ("EditorDuplicate", "Ctrl+D"),
            ("EditorDeleteLine", "Ctrl+Y"),
            ("ReformatCode", "Ctrl+Alt+L"),
            ("OptimizeImports", "Ctrl+Alt+O"),
            ("ToggleLineBreakpoint", "Ctrl+F8"),
            ("Run", "Shift+F10"),
            ("Debug", "Shift+F9"),
        ];
        for (id, s) in expectations {
            let want = canonical_shortcut(s, Platform::Other);
            assert!(sc(KeymapId::Default, id).contains(&want), "{id}");
        }
    }

    #[test]
    fn matches_phpstorm_macos_defaults() {
        let expectations = [
            ("GotoAction", "Cmd+Shift+A"),
            ("GotoClass", "Cmd+O"),
            ("GotoFile", "Cmd+Shift+O"),
            ("ShowSettings", "Cmd+,"),
            ("EditorSelectWord", "Alt+Up"),
            ("SelectNextOccurrence", "Ctrl+G"),
            ("Generate", "Cmd+N"),
            ("ActivateProjectToolWindow", "Cmd+1"),
            ("CloseContent", "Cmd+W"),
            ("Run", "Ctrl+R"),
            ("Refactorings.QuickListPopupAction", "Ctrl+T"),
        ];
        for (id, s) in expectations {
            let want = canonical_shortcut(s, Platform::Mac);
            assert!(sc(KeymapId::MacOs, id).contains(&want), "{id}");
        }
    }

    #[test]
    fn has_no_unintended_same_context_conflicts() {
        let c = catalog();
        // PHPStorm itself binds these pairs on purpose (different UI contexts in the IDE).
        let known = [
            "Ctrl+Alt+Z", // Rollback / Rollback Lines
            "Alt+Meta+Z",
            "Meta+N", // New… / Generate (editor)
            "Alt+Insert",
            "Ctrl+Shift+Tab",
        ];
        for km in KeymapId::ALL {
            let non_native: Vec<(String, Vec<String>)> = c
                .defaults(km)
                .iter()
                .filter(|(id, _)| !c.get(id).is_some_and(|a| a.native))
                .cloned()
                .collect();
            let bindings = build_bindings(
                &non_native,
                |id| c.get(id).map(|a| a.context).unwrap_or_default(),
                km.platform(),
            );
            let unexpected: Vec<_> = all_conflicts(&bindings)
                .into_iter()
                .filter(|(id, _)| !known.contains(&id.as_str()))
                .collect();
            assert_eq!(unexpected, vec![], "{km:?}");
        }
    }

    #[test]
    fn aliases_point_to_forge_command_ids() {
        assert_eq!(aliases_of("Run"), &["run.run"]);
        assert!(aliases_of("GotoClass").is_empty());
        assert_eq!(aliases_of("OverrideMethods"), &["php.overrideMethods"]);
    }
}
