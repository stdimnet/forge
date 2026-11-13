//! PHPStorm's default keymaps as data: "Windows/Linux" (`$default`) and "macOS".
//!
//! Action ids are PHPStorm's own and double as Forge command ids, so a feature implements
//! e.g. "Go to Class" simply by `registerCommand({ id: "GotoClass", ... })`. Actions marked
//! `native` (clipboard, caret movement, typing) are handled by the browser/CodeMirror and are
//! listed only so they show up in Find Action and the keymap settings.
//!
//! The rows live in `data/phpstorm.json` (one action per line: id, title, group, Windows/Linux
//! and macOS shortcuts separated by `|`, optional context and native flag).
//!
//! The other bundled keymaps (VS Code, Eclipse, Sublime Text, Emacs) are derived from one of
//! the two PHPStorm keymaps, like in PHPStorm: `data/keymaps.json` lists only the actions whose
//! shortcuts differ from the parent (an empty string removes all of them).

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::shortcut::Platform;

/// One of the two PHPStorm keymaps; every other keymap derives from one of them, which also
/// decides the platform its shortcuts are written for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum KeymapId {
    #[serde(rename = "$default")]
    #[default]
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

/// Where an action may fire: "editor" actions only while a code editor has focus, "diff"
/// actions only inside a diff or merge window, "notebook" actions in a Jupyter notebook
/// (command mode and cell editors) and "notebook-command" actions only in its command mode
/// (plain keys such as A, B, D D).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActionContext {
    #[default]
    Any,
    Editor,
    Toolwindow,
    Diff,
    Notebook,
    #[serde(rename = "notebook-command")]
    NotebookCommand,
}

impl ActionContext {
    /// Bindings of more specific contexts win when several match.
    pub fn specificity(self) -> u8 {
        match self {
            ActionContext::Any => 0,
            ActionContext::Editor | ActionContext::Toolwindow => 1,
            ActionContext::Diff | ActionContext::Notebook | ActionContext::NotebookCommand => 2,
        }
    }
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
    /// The English title when `title` is a translation; Find Action matches both.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_en: Option<String>,
}

impl ActionDef {
    /// Catalog key of the action title: `action.<id>`.
    pub fn title_key(id: &str) -> String {
        format!("action.{id}")
    }

    /// Show the title in the language of `l10n`, keeping the English one in `title_en`.
    pub fn localize(&mut self, l10n: &forge_i18n::Localizer) {
        if let Some(t) = l10n.translate(&Self::title_key(&self.id)) {
            if t != self.title {
                self.title_en = Some(std::mem::replace(&mut self.title, t));
            }
        }
    }
}

/// The English source catalog of the PHPStorm action titles (`locales/en/actions.json`).
pub fn action_messages() -> std::collections::BTreeMap<String, String> {
    catalog()
        .actions
        .iter()
        .map(|a| (ActionDef::title_key(&a.id), a.title.clone()))
        .collect()
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
pub const ACTION_ALIASES: [(&str, &[&str]); 33] = [
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
    ("Vcs.Show.Log", &["git.log"]),
    ("Annotate", &["git.annotate"]),
    ("Generate", &["php.generate"]),
    ("OverrideMethods", &["php.overrideMethods"]),
    ("ImplementMethods", &["php.implementMethods"]),
    ("OpenFile", &["workspace.open"]),
    ("CloseProject", &["workspace.close"]),
    ("IntroduceVariable", &["ExtractVariable"]),
    // Code With Me: the action ids of the plugin bundled with PHPStorm 2026.1 (`cwm-plugin`)
    // for the commands Forge registered first under `CodeWithMe.*`.
    ("GetJoinLinkAction", &["CodeWithMe.StartSession"]),
    ("CopyJoinLinkAction", &["CodeWithMe.CopySessionLink"]),
    ("ShutdownCodeWithMe", &["CodeWithMe.EndSession"]),
    ("ThinClientDisconnect", &["CodeWithMe.LeaveSession"]),
    ("ShowPermissionsAction", &["CodeWithMe.Permissions"]),
    ("SingleUserFollowAction", &["CodeWithMe.FollowParticipant"]),
    (
        "SingleJumpToCursorAction",
        &["CodeWithMe.JumpToParticipant"],
    ),
    ("CurrentLeadUnfollowAction", &["CodeWithMe.StopFollowing"]),
    (
        "ForceOthersToFollowAction",
        &["CodeWithMe.MakeEveryoneFollow"],
    ),
    ("CWMHostShowPopupAction", &["CodeWithMe.ShowToolWindow"]),
    ("CWMHostOpenSettingsAction", &["CodeWithMe.Settings"]),
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

/// A bundled keymap derived from a PHPStorm keymap ("VS Code", "Eclipse (macOS)", …).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundledKeymap {
    /// PHPStorm's keymap name ("VSCode", "Eclipse (Mac OS X)").
    pub id: String,
    pub title: String,
    pub parent: KeymapId,
    /// Actions whose shortcuts replace the parent's, in data order.
    pub changes: Vec<(String, Vec<String>)>,
}

#[derive(Deserialize)]
struct BundledRow {
    id: String,
    title: String,
    parent: KeymapId,
    shortcuts: serde_json::Map<String, serde_json::Value>,
}

/// The derived bundled keymaps, in display order.
pub fn bundled_keymaps() -> &'static [BundledKeymap] {
    static KEYMAPS: OnceLock<Vec<BundledKeymap>> = OnceLock::new();
    KEYMAPS.get_or_init(|| {
        let rows: Vec<BundledRow> = serde_json::from_str(include_str!("../data/keymaps.json"))
            .expect("data/keymaps.json is valid");
        rows.into_iter()
            .map(|r| BundledKeymap {
                id: r.id,
                title: r.title,
                parent: r.parent,
                changes: r
                    .shortcuts
                    .into_iter()
                    .map(|(id, v)| (id, split_shortcuts(v.as_str().unwrap_or_default())))
                    .collect(),
            })
            .collect()
    })
}

/// A derived bundled keymap by id.
pub fn bundled_keymap(id: &str) -> Option<&'static BundledKeymap> {
    bundled_keymaps().iter().find(|k| k.id == id)
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
            ("CloseContent", "Ctrl+F4"),
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
            "F6", // Move (refactoring) / Move to Another Changelist (Commit tool window)
            "F7", // Step Into / Next Difference (diff and merge windows)
            "Shift+F7",
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

    /// Shortcuts of `km` (a derived keymap) with its changes applied to the parent's.
    fn derived(km: &BundledKeymap) -> Vec<(String, Vec<String>)> {
        let mut out: Vec<(String, Vec<String>)> = catalog().defaults(km.parent).to_vec();
        for (id, list) in &km.changes {
            match out.iter_mut().find(|(a, _)| a == id) {
                Some(entry) => entry.1 = list.clone(),
                None => out.push((id.clone(), list.clone())),
            }
        }
        out
    }

    #[test]
    fn bundles_the_other_keymaps_of_phpstorm() {
        let ids: Vec<&str> = bundled_keymaps().iter().map(|k| k.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "VSCode",
                "VSCode OSX",
                "Eclipse",
                "Eclipse (Mac OS X)",
                "Sublime Text",
                "Sublime Text (Mac OS X)",
                "Emacs"
            ]
        );
        assert_eq!(
            bundled_keymap("VSCode OSX").unwrap().parent,
            KeymapId::MacOs
        );
        assert!(bundled_keymap("Nope").is_none());
    }

    #[test]
    fn derived_keymaps_only_change_known_actions_with_valid_shortcuts() {
        let c = catalog();
        for km in bundled_keymaps() {
            for (id, list) in &km.changes {
                assert!(c.get(id).is_some(), "{}: unknown action {id}", km.id);
                for s in list {
                    assert!(
                        try_parse_shortcut(s, km.parent.platform()).is_some(),
                        "{}: {id}: {s}",
                        km.id
                    );
                }
            }
        }
    }

    #[test]
    fn derived_keymaps_use_the_editors_own_shortcuts() {
        let expect = [
            ("VSCode", "GotoAction", "Ctrl+Shift+P"),
            ("VSCode", "GotoFile", "Ctrl+P"),
            ("VSCode", "RenameElement", "F2"),
            ("VSCode", "SelectNextOccurrence", "Ctrl+D"),
            ("VSCode OSX", "GotoFile", "Cmd+P"),
            ("Eclipse", "GotoFile", "Ctrl+Shift+R"),
            ("Eclipse", "ShowIntentionActions", "Ctrl+1"),
            ("Eclipse (Mac OS X)", "EditorDeleteLine", "Cmd+D"),
            ("Sublime Text", "GotoSymbol", "Ctrl+Shift+R"),
            ("Sublime Text (Mac OS X)", "GotoAction", "Cmd+Shift+P"),
            ("Emacs", "SaveAll", "Ctrl+X Ctrl+S"),
            ("Emacs", "GotoAction", "Alt+X"),
        ];
        for (km, id, s) in expect {
            let km = bundled_keymap(km).unwrap();
            let platform = km.parent.platform();
            let list = derived(km)
                .into_iter()
                .find(|(a, _)| a == id)
                .map(|(_, l)| l)
                .unwrap_or_default();
            let want = canonical_shortcut(s, platform);
            assert!(
                list.iter().any(|x| canonical_shortcut(x, platform) == want),
                "{}: {id}",
                km.id
            );
        }
    }

    #[test]
    fn mouse_side_buttons_navigate_back_and_forward_in_every_keymap() {
        for km in [KeymapId::Default, KeymapId::MacOs] {
            let back = canonical_shortcut("Button4 Click", km.platform());
            let forward = canonical_shortcut("Button5 Click", km.platform());
            assert!(sc(km, "Back").contains(&back), "{km:?}");
            assert!(sc(km, "Forward").contains(&forward), "{km:?}");
        }
        for km in bundled_keymaps() {
            let platform = km.parent.platform();
            for (id, button) in [("Back", "Button4 Click"), ("Forward", "Button5 Click")] {
                // A derived keymap that overrides the action keeps the mouse button.
                if let Some((_, list)) = derived(km).into_iter().find(|(a, _)| a == id) {
                    let want = canonical_shortcut(button, platform);
                    assert!(
                        list.iter().any(|x| canonical_shortcut(x, platform) == want),
                        "{}: {id}",
                        km.id
                    );
                }
            }
        }
    }

    #[test]
    fn derived_keymaps_have_no_unintended_same_context_conflicts() {
        let c = catalog();
        // Pairs PHPStorm binds on purpose (different UI contexts), as in the PHPStorm keymaps.
        let known = [
            "Ctrl+Alt+Z",
            "Alt+Meta+Z",
            "Meta+N",
            "Alt+Insert",
            "Ctrl+Shift+Tab",
            "F6",
            "F7",
            "Shift+F7",
        ];
        let mut failures = Vec::new();
        for km in bundled_keymaps() {
            let non_native: Vec<(String, Vec<String>)> = derived(km)
                .into_iter()
                .filter(|(id, _)| !c.get(id).is_some_and(|a| a.native))
                .collect();
            let bindings = build_bindings(
                &non_native,
                |id| c.get(id).map(|a| a.context).unwrap_or_default(),
                km.parent.platform(),
            );
            let unexpected: Vec<_> = all_conflicts(&bindings)
                .into_iter()
                .filter(|(id, _)| !known.contains(&id.as_str()))
                .collect();
            if !unexpected.is_empty() {
                failures.push(format!("{}: {unexpected:?}", km.id));
            }
        }
        assert_eq!(failures, Vec::<String>::new());
    }

    #[test]
    fn aliases_point_to_forge_command_ids() {
        assert_eq!(aliases_of("Run"), &["run.run"]);
        assert!(aliases_of("GotoClass").is_empty());
        assert_eq!(aliases_of("OverrideMethods"), &["php.overrideMethods"]);
    }

    #[test]
    fn code_with_me_actions_follow_phpstorm_ids_to_forge_commands() {
        assert_eq!(
            aliases_of("GetJoinLinkAction"),
            &["CodeWithMe.StartSession"]
        );
        assert_eq!(aliases_of("ShutdownCodeWithMe"), &["CodeWithMe.EndSession"]);
        assert_eq!(
            aliases_of("ThinClientDisconnect"),
            &["CodeWithMe.LeaveSession"]
        );
        assert_eq!(
            aliases_of("ForceOthersToFollowAction"),
            &["CodeWithMe.MakeEveryoneFollow"]
        );
        // Every PHPStorm id maps to one Forge command, and no Forge id is listed twice.
        let cwm: Vec<_> = ACTION_ALIASES
            .iter()
            .flat_map(|(_, list)| list.iter())
            .filter(|a| a.starts_with("CodeWithMe."))
            .collect();
        assert_eq!(cwm.len(), 11);
        let unique: std::collections::HashSet<_> = cwm.iter().collect();
        assert_eq!(unique.len(), cwm.len());
        // Forge's own actions without a PHPStorm counterpart keep their ids.
        assert!(aliases_of("CodeWithMe.JoinSession").is_empty());
    }
}
