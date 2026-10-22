//! The main menu (PHPStorm's New UI hamburger menu and the classic menu bar) as data.
//!
//! The built-in structure follows PHPStorm: File, Edit, View, Navigate, Code, Refactor, Run,
//! Tools, Git (VCS without a repository), Window and Help, with the same submenus, separators
//! and mnemonics. Entries refer to PHPStorm action ids, so labels and shortcuts come from the
//! keymap. Plugins add entries with [`MenuContribution`]s (from their manifest or the UI's
//! `registerMainMenuEntries`), and the UI fills dynamic lists (recent projects, tool windows).
//!
//! [`resolve_main_menu`] turns all of that into the menu the UI renders: labels with their
//! mnemonic, shortcuts, enabled and checked states, tidy separators and no empty submenus.
//! Actions nobody implements stay visible but disabled, like in JetBrains IDEs.

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};

/// One entry of a menu definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum MenuEntry {
    /// An action by PHPStorm id (or any command id).
    Action {
        id: String,
        /// Label instead of the action title; `_` marks the mnemonic ("Save _All").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
        /// Mnemonic letter within the action title, when no label is given.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mnemonic: Option<char>,
        /// Shown with a check box (View | Appearance toggles).
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        checkable: bool,
    },
    /// A submenu. Groups with the same label (mnemonic markers ignored) are merged.
    Group {
        label: String,
        #[serde(default)]
        items: Vec<MenuEntry>,
    },
    Separator,
    /// Items the UI supplies when the menu opens (`recentProjects`, `toolWindows`, …).
    Dynamic {
        source: String,
    },
}

/// Entries a plugin adds to the main menu.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MenuContribution {
    /// Submenu labels from the top, separated by `/`: "Tools", "View/Tool Windows". Missing
    /// submenus are created at the end of their parent, with the label as written here
    /// ("Tools/_Laravel" creates "Laravel" with the mnemonic L).
    pub path: String,
    pub items: Vec<MenuEntry>,
    /// Insert before the entry with this action id or submenu label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    /// Insert after the entry with this action id or submenu label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
}

/// An item of a dynamic list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DynamicItem {
    /// Handed back to the UI when the item is picked.
    pub key: String,
    pub label: String,
    /// Action whose shortcut is shown (and that runs when the UI has no handler).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked: Option<bool>,
}

fn yes() -> bool {
    true
}

/// What the UI sends when the main menu opens.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MainMenuRequest {
    /// Contributions of active plugins and UI features, applied in order.
    pub contributions: Vec<MenuContribution>,
    /// Items of the dynamic lists by source.
    pub dynamic: BTreeMap<String, Vec<DynamicItem>>,
    /// Checkable actions that are currently on.
    pub checked: Vec<String>,
    /// The project is under Git: the VCS menu becomes the Git menu.
    pub git: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MenuItemKind {
    Action,
    Submenu,
    Separator,
}

/// A menu item as the UI renders it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MenuItem {
    pub kind: MenuItemKind,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
    /// Character index of the mnemonic in `label`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mnemonic: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    /// Key of a dynamic item.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shortcut: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub checkable: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub checked: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<MenuItem>,
}

impl MenuItem {
    fn separator() -> Self {
        Self {
            kind: MenuItemKind::Separator,
            label: String::new(),
            mnemonic: None,
            action: None,
            key: None,
            shortcut: None,
            detail: None,
            enabled: true,
            checkable: false,
            checked: false,
            items: Vec::new(),
        }
    }

    fn is_separator(&self) -> bool {
        self.kind == MenuItemKind::Separator
    }
}

/// The resolved main menu: one submenu per top-level menu.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MainMenu {
    pub menus: Vec<MenuItem>,
}

/// What resolution needs to know about actions.
pub trait ActionLookup {
    fn title(&self, id: &str) -> Option<String>;
    /// First shortcut, formatted for display.
    fn shortcut(&self, id: &str) -> Option<String>;
    /// Some command implements the action (directly or through an alias).
    fn is_available(&self, id: &str) -> bool;
}

/// `"Save _All"` → `("Save All", Some(5))`; `__` is a literal underscore.
pub fn parse_mnemonic(text: &str) -> (String, Option<usize>) {
    let mut out = String::with_capacity(text.len());
    let mut mnemonic = None;
    let mut chars = text.chars().peekable();
    let mut index = 0;
    while let Some(c) = chars.next() {
        if c == '_' {
            match chars.peek() {
                Some('_') => {
                    chars.next();
                }
                Some(_) if mnemonic.is_none() => {
                    mnemonic = Some(index);
                    continue;
                }
                _ => {}
            }
        }
        out.push(c);
        index += 1;
    }
    (out, mnemonic)
}

/// Index of the mnemonic letter `m` in `label`: preferably at the start of a word.
fn find_mnemonic(label: &str, m: char) -> Option<usize> {
    let m = m.to_ascii_lowercase();
    let chars: Vec<char> = label.chars().collect();
    let matches = |i: usize| chars[i].to_ascii_lowercase() == m;
    (0..chars.len())
        .find(|&i| matches(i) && (i == 0 || !chars[i - 1].is_alphanumeric()))
        .or_else(|| (0..chars.len()).find(|&i| matches(i)))
}

/// Label without the mnemonic marker, used to match submenus by path.
fn plain(label: &str) -> String {
    parse_mnemonic(label).0
}

/// Menus show "..." like JetBrains IDEs, the keymap titles use "…".
fn menu_text(s: &str) -> String {
    s.replace('…', "...")
}

// ---- Building

fn insert_contribution(menu: &mut Vec<MenuEntry>, c: &MenuContribution) {
    let mut level = menu;
    for part in c.path.split('/').map(str::trim).filter(|p| !p.is_empty()) {
        let name = plain(part);
        let pos = level
            .iter()
            .position(|e| matches!(e, MenuEntry::Group { label, .. } if plain(label) == name));
        let pos = pos.unwrap_or_else(|| {
            level.push(MenuEntry::Group {
                label: part.to_string(),
                items: Vec::new(),
            });
            level.len() - 1
        });
        let MenuEntry::Group { items, .. } = &mut level[pos] else {
            unreachable!()
        };
        level = items;
    }
    let anchor_of = |e: &MenuEntry, name: &str| match e {
        MenuEntry::Action { id, .. } => id == name,
        MenuEntry::Group { label, .. } => plain(label) == name,
        _ => false,
    };
    let at = if let Some(name) = &c.before {
        level.iter().position(|e| anchor_of(e, name))
    } else if let Some(name) = &c.after {
        level.iter().position(|e| anchor_of(e, name)).map(|i| i + 1)
    } else {
        None
    };
    let mut at = at.unwrap_or(level.len());
    for item in &c.items {
        merge_entry(level, &mut at, item.clone());
    }
}

/// Insert `entry` at `*at`; a group whose label exists in `level` is merged into it instead.
fn merge_entry(level: &mut Vec<MenuEntry>, at: &mut usize, entry: MenuEntry) {
    if let MenuEntry::Group { label, items } = &entry {
        let name = plain(label);
        if let Some(MenuEntry::Group {
            items: existing, ..
        }) = level
            .iter_mut()
            .find(|e| matches!(e, MenuEntry::Group { label, .. } if plain(label) == name))
        {
            let mut end = existing.len();
            for item in items {
                merge_entry(existing, &mut end, item.clone());
            }
            return;
        }
    }
    level.insert(*at, entry);
    *at += 1;
}

/// The built-in menu plus `contributions`.
pub fn main_menu_entries(git: bool, contributions: &[MenuContribution]) -> Vec<MenuEntry> {
    let mut menu = builtin_main_menu(git);
    for c in contributions {
        insert_contribution(&mut menu, c);
    }
    menu
}

// ---- Resolving

struct Resolver<'a> {
    lookup: &'a dyn ActionLookup,
    checked: HashSet<&'a str>,
    dynamic: &'a BTreeMap<String, Vec<DynamicItem>>,
}

impl Resolver<'_> {
    fn list(&self, entries: &[MenuEntry]) -> Vec<MenuItem> {
        let mut out: Vec<MenuItem> = Vec::new();
        for e in entries {
            match e {
                MenuEntry::Separator => {
                    if out.last().is_some_and(|l| !l.is_separator()) {
                        out.push(MenuItem::separator());
                    }
                }
                MenuEntry::Action {
                    id,
                    label,
                    mnemonic,
                    checkable,
                } => out.push(self.action(id, label.as_deref(), *mnemonic, *checkable)),
                MenuEntry::Group { label, items } => {
                    let items = self.list(items);
                    if items.is_empty() {
                        continue;
                    }
                    let (label, mnemonic) = parse_mnemonic(&menu_text(label));
                    out.push(MenuItem {
                        kind: MenuItemKind::Submenu,
                        label,
                        mnemonic,
                        enabled: true,
                        items,
                        ..MenuItem::separator()
                    });
                }
                MenuEntry::Dynamic { source } => {
                    for d in self.dynamic.get(source).into_iter().flatten() {
                        out.push(MenuItem {
                            kind: MenuItemKind::Action,
                            label: menu_text(&d.label),
                            action: d.action.clone(),
                            key: Some(d.key.clone()),
                            shortcut: d.action.as_deref().and_then(|a| self.lookup.shortcut(a)),
                            detail: d.detail.clone(),
                            enabled: d.enabled,
                            checkable: d.checked.is_some(),
                            checked: d.checked.unwrap_or(false),
                            ..MenuItem::separator()
                        });
                    }
                }
            }
        }
        if out.last().is_some_and(MenuItem::is_separator) {
            out.pop();
        }
        out
    }

    fn action(
        &self,
        id: &str,
        label: Option<&str>,
        mnemonic: Option<char>,
        checkable: bool,
    ) -> MenuItem {
        let (label, index) = match label {
            Some(l) => parse_mnemonic(&menu_text(l)),
            None => {
                let title = menu_text(&self.lookup.title(id).unwrap_or_else(|| id.to_string()));
                let index = mnemonic.and_then(|m| find_mnemonic(&title, m));
                (title, index)
            }
        };
        MenuItem {
            kind: MenuItemKind::Action,
            label,
            mnemonic: index,
            action: Some(id.to_string()),
            shortcut: self.lookup.shortcut(id),
            enabled: self.lookup.is_available(id),
            checkable,
            checked: checkable && self.checked.contains(id),
            ..MenuItem::separator()
        }
    }
}

/// Resolve the main menu for the UI.
pub fn resolve_main_menu(request: &MainMenuRequest, lookup: &dyn ActionLookup) -> MainMenu {
    let entries = main_menu_entries(request.git, &request.contributions);
    let resolver = Resolver {
        lookup,
        checked: request.checked.iter().map(String::as_str).collect(),
        dynamic: &request.dynamic,
    };
    MainMenu {
        menus: resolver
            .list(&entries)
            .into_iter()
            .filter(|m| m.kind == MenuItemKind::Submenu)
            .collect(),
    }
}

// ---- The built-in structure (PHPStorm New UI, Windows/Linux)

fn a(id: &str) -> MenuEntry {
    MenuEntry::Action {
        id: id.into(),
        label: None,
        mnemonic: None,
        checkable: false,
    }
}

/// Action with a mnemonic letter of its title.
fn m(id: &str, mnemonic: char) -> MenuEntry {
    MenuEntry::Action {
        id: id.into(),
        label: None,
        mnemonic: Some(mnemonic),
        checkable: false,
    }
}

/// Action with its own label (`_` marks the mnemonic).
fn l(id: &str, label: &str) -> MenuEntry {
    MenuEntry::Action {
        id: id.into(),
        label: Some(label.into()),
        mnemonic: None,
        checkable: false,
    }
}

/// Checkable action with its own label.
fn check(id: &str, label: &str) -> MenuEntry {
    MenuEntry::Action {
        id: id.into(),
        label: Some(label.into()),
        mnemonic: None,
        checkable: true,
    }
}

fn g(label: &str, items: Vec<MenuEntry>) -> MenuEntry {
    MenuEntry::Group {
        label: label.into(),
        items,
    }
}

fn dynamic(source: &str) -> MenuEntry {
    MenuEntry::Dynamic {
        source: source.into(),
    }
}

const SEP: MenuEntry = MenuEntry::Separator;

/// Dynamic sources of the built-in menu.
pub const DYNAMIC_SOURCES: [&str; 2] = ["recentProjects", "toolWindows"];

/// PHPStorm's main menu. `git` picks the Git menu over the VCS menu.
pub fn builtin_main_menu(git: bool) -> Vec<MenuEntry> {
    vec![
        file_menu(),
        edit_menu(),
        view_menu(),
        navigate_menu(),
        code_menu(),
        refactor_menu(),
        run_menu(),
        tools_menu(),
        if git { git_menu() } else { vcs_menu() },
        window_menu(),
        help_menu(),
    ]
}

fn file_menu() -> MenuEntry {
    g(
        "_File",
        vec![
            g(
                "_New",
                vec![
                    l("NewProject", "_Project..."),
                    l("Vcs.Checkout", "Project from _Version Control..."),
                    SEP,
                    l("NewFile", "_File"),
                    l("NewScratchFile", "_Scratch File"),
                    l("NewDir", "_Directory"),
                ],
            ),
            l("OpenFile", "_Open..."),
            g(
                "_Recent Projects",
                vec![
                    dynamic("recentProjects"),
                    SEP,
                    l("ManageRecentProjects", "_Manage Projects..."),
                ],
            ),
            l("CloseProject", "Clos_e Project"),
            l("RenameProject", "Rename Pro_ject..."),
            SEP,
            l("ShowSettings", "_Settings..."),
            l("ShowProjectStructureSettings", "Project Str_ucture..."),
            g(
                "File Propert_ies",
                vec![
                    l("ChangeFileEncodingAction", "File _Encoding"),
                    l("RemoveBom", "Remove _BOM"),
                    l("AssociateWithFileType", "Associate with File _Type..."),
                    g(
                        "_Line Separators",
                        vec![
                            l("ConvertToWindowsLineSeparators", "CRLF - _Windows (\\r\\n)"),
                            l("ConvertToUnixLineSeparators", "LF - _Unix and macOS (\\n)"),
                            l("ConvertToMacLineSeparators", "CR - Classic _Mac OS (\\r)"),
                        ],
                    ),
                    l("ToggleReadOnlyAttribute", "Make File Read-_Only"),
                    l("ShowFilePath", "File _Path"),
                ],
            ),
            g(
                "Local _History",
                vec![
                    l("LocalHistory.ShowHistory", "_Show History"),
                    l("LocalHistory.PutLabel", "_Put Label..."),
                ],
            ),
            SEP,
            l("SaveAll", "Save _All"),
            l("Synchronize", "Reloa_d All from Disk"),
            l("InvalidateCaches", "In_validate Caches..."),
            SEP,
            g(
                "_Manage IDE Settings",
                vec![
                    l("ImportSettings", "_Import Settings..."),
                    l("ExportSettings", "_Export Settings..."),
                    l("RestoreDefaultSettings", "_Restore Default Settings..."),
                ],
            ),
            g(
                "Ne_w Projects Setup",
                vec![l(
                    "TemplateProjectProperties",
                    "_Settings for New Projects...",
                )],
            ),
            l("SaveFileAsTemplate", "Save File as _Template..."),
            SEP,
            g("E_xport", vec![l("ExportToHTML", "Export to _HTML...")]),
            l("Print", "_Print..."),
            SEP,
            check("TogglePowerSave", "Power Save _Mode"),
            SEP,
            l("Exit", "E_xit"),
        ],
    )
}

fn edit_menu() -> MenuEntry {
    g(
        "_Edit",
        vec![
            m("$Undo", 'u'),
            m("$Redo", 'r'),
            SEP,
            l("$Cut", "Cu_t"),
            m("$Copy", 'c'),
            g(
                "Cop_y Path/Reference",
                vec![
                    l("CopyPaths", "_Absolute Path"),
                    l("CopyFileName", "File _Name"),
                    l("CopyContentRootPath", "Path From _Content Root"),
                    SEP,
                    l("CopyReference", "_Reference"),
                ],
            ),
            m("$Paste", 'p'),
            g(
                "Paste _Special",
                vec![
                    l("PasteMultiple", "Paste from _History..."),
                    l("PasteSimple", "Paste as Plain _Text"),
                ],
            ),
            m("$Delete", 'd'),
            SEP,
            g(
                "_Find",
                vec![
                    m("Find", 'f'),
                    m("Replace", 'r'),
                    l("FindNext", "Find _Next / Move to Next Occurrence"),
                    l(
                        "FindPrevious",
                        "Find Pre_vious / Move to Previous Occurrence",
                    ),
                    m("FindWordAtCaret", 'w'),
                    m("SelectAllOccurrences", 'a'),
                    m("SelectNextOccurrence", 's'),
                    m("UnselectPreviousOccurrence", 'u'),
                    SEP,
                    l("FindInPath", "Find in _Files..."),
                    l("ReplaceInPath", "R_eplace in Files..."),
                    SEP,
                    l(
                        "StructuralSearchPlugin.StructuralSearchAction",
                        "Search Structurall_y...",
                    ),
                    l(
                        "StructuralSearchPlugin.StructuralReplaceAction",
                        "Replace Structurall_y...",
                    ),
                ],
            ),
            g(
                "Find _Usages",
                vec![
                    m("FindUsages", 'u'),
                    m("ShowSettingsAndFindUsages", 's'),
                    l("ShowUsages", "Show Usages"),
                    m("FindUsagesInFile", 'i'),
                    m("HighlightUsagesInFile", 'h'),
                ],
            ),
            SEP,
            l("$SelectAll", "Select _All"),
            l("EditorSelectWord", "E_xtend Selection"),
            l("EditorUnSelectWord", "Shrin_k Selection"),
            check("EditorToggleColumnMode", "Colu_mn Selection Mode"),
            SEP,
            m("EditorJoinLines", 'j'),
            l("EditorDuplicate", "D_uplicate Line or Selection"),
            m("EditorSortLines", 'o'),
            l("EditorReverseLines", "Re_verse Lines"),
            l("FillParagraph", "Fill Paragrap_h"),
            l("EditorToggleCase", "Toggle _Case"),
            SEP,
            g(
                "Macros",
                vec![
                    l("StartStopMacroRecording", "Start Macro _Recording"),
                    l("PlaybackLastMacro", "_Play Back Last Macro"),
                    l("PlaySavedMacrosAction", "Play _Saved Macros..."),
                    l("EditMacros", "_Edit Macros"),
                ],
            ),
        ],
    )
}

fn view_menu() -> MenuEntry {
    g(
        "_View",
        vec![
            g("_Tool Windows", vec![dynamic("toolWindows")]),
            g(
                "_Appearance",
                vec![
                    l("TogglePresentationMode", "Enter _Presentation Mode"),
                    l("ToggleDistractionFreeMode", "Enter _Distraction-Free Mode"),
                    l("ToggleFullScreen", "Enter F_ull Screen"),
                    l("ToggleZenMode", "Enter _Zen Mode"),
                    SEP,
                    check("ToggleCompactMode", "Co_mpact Mode"),
                    SEP,
                    check(
                        "ToggleMainMenuInSeparateToolbar",
                        "Show Main Menu in _Separate Toolbar",
                    ),
                    check("ViewToolButtons", "Tool _Window Bars"),
                    check("ViewStatusBar", "Status _Bar"),
                    g(
                        "_Navigation Bar",
                        vec![
                            check("NavBarLocationTop", "_Top"),
                            check("NavBarLocationBottom", "_In Status Bar"),
                            check("NavBarLocationHide", "_Don't Show"),
                        ],
                    ),
                    check("ShowMembersInNavigationBar", "Me_mbers in Navigation Bar"),
                ],
            ),
            l("QuickChangeScheme", "Quick Switch _Scheme..."),
            SEP,
            l("QuickImplementations", "Quick _Definition"),
            l("QuickJavaDoc", "_Quick Documentation"),
            l("ExternalJavaDoc", "E_xternal Documentation"),
            l("ParameterInfo", "Parameter _Info"),
            l("ExpressionTypeInfo", "T_ype Info"),
            l("EditorContextInfo", "_Context Info"),
            SEP,
            l("ShowErrorDescription", "_Error Description"),
            SEP,
            l("RecentFiles", "Recent F_iles"),
            l("RecentChangedFiles", "Recently Changed Fi_les"),
            l("RecentLocations", "Recent L_ocations"),
            SEP,
            l("CompareTwoFiles", "Compare _With..."),
            l("CompareClipboardWithSelection", "Compare with Clip_board"),
            SEP,
            g(
                "Active Edi_tor",
                vec![
                    check("EditorToggleUseSoftWraps", "Soft-_Wrap"),
                    check("EditorToggleShowWhitespaces", "Show W_hitespaces"),
                    check("EditorToggleShowLineNumbers", "Show _Line Numbers"),
                    check("EditorToggleShowIndentLines", "Show _Indent Guides"),
                    check("EditorToggleShowBreadcrumbs", "Show _Breadcrumbs"),
                ],
            ),
        ],
    )
}

fn navigate_menu() -> MenuEntry {
    g(
        "_Navigate",
        vec![
            m("Back", 'b'),
            m("Forward", 'f'),
            l("JumpToLastChange", "_Last Edit Location"),
            l("JumpToNextChange", "Ne_xt Edit Location"),
            SEP,
            g(
                "Boo_kmarks",
                vec![
                    m("ToggleBookmark", 't'),
                    l(
                        "ToggleBookmarkWithMnemonic",
                        "Toggle Bookmark with _Mnemonic",
                    ),
                    SEP,
                    l("ShowBookmarks", "Show _All Bookmarks"),
                ],
            ),
            SEP,
            l("SearchEverywhere", "Search _Everywhere"),
            l("GotoClass", "_Class..."),
            l("GotoFile", "F_ile..."),
            l("GotoSymbol", "_Symbol..."),
            l("GotoAction", "_Action..."),
            l("GotoLine", "Line/Col_umn..."),
            SEP,
            l("SelectIn", "Select I_n..."),
            l("ShowNavBar", "Navigation _Bar"),
            SEP,
            l("GotoDeclaration", "_Declaration or Usages"),
            l("GotoImplementation", "I_mplementation(s)"),
            l("GotoTypeDeclaration", "_Type Declaration"),
            l("GotoSuperMethod", "S_uper Method"),
            l("GotoTest", "Te_st"),
            l("GotoRelated", "Related Symb_ol..."),
            SEP,
            l("FileStructurePopup", "File Stru_cture"),
            l("TypeHierarchy", "Type _Hierarchy"),
            l("MethodHierarchy", "M_ethod Hierarchy"),
            l("CallHierarchy", "Call Hie_rarchy"),
            SEP,
            l("GotoNextError", "Next _Highlighted Error"),
            l("GotoPreviousError", "_Previous Highlighted Error"),
            SEP,
            l("MethodDown", "Ne_xt Method"),
            l("MethodUp", "Pre_vious Method"),
            SEP,
            l("VcsShowNextChangeMarker", "Next Change"),
            l("VcsShowPrevChangeMarker", "Previous Change"),
        ],
    )
}

fn code_menu() -> MenuEntry {
    g(
        "_Code",
        vec![
            l("ShowIntentionActions", "Show Context _Actions"),
            SEP,
            g(
                "Code _Completion",
                vec![
                    m("CodeCompletion", 'b'),
                    m("SmartTypeCompletion", 't'),
                    m("HippieCompletion", 'c'),
                    l("HippieBackwardCompletion", "Cyclic Expand Word (_Backward)"),
                ],
            ),
            SEP,
            l("OverrideMethods", "_Override Methods..."),
            l("ImplementMethods", "_Implement Methods..."),
            l("Generate", "_Generate..."),
            SEP,
            l("SurroundWith", "Su_rround With..."),
            l("Unwrap", "_Unwrap/Remove..."),
            SEP,
            g(
                "_Folding",
                vec![
                    m("ExpandRegion", 'e'),
                    m("CollapseRegion", 'c'),
                    l("ExpandRegionRecursively", "Expand _Recursively"),
                    l("CollapseRegionRecursively", "Collapse Rec_ursively"),
                    l("ExpandAllRegions", "Expand _All"),
                    l("CollapseAllRegions", "Collapse A_ll"),
                    SEP,
                    l("CollapseDocComments", "Collapse _Doc Comments"),
                    SEP,
                    l("CollapseSelection", "Fold _Selection / Remove Region"),
                ],
            ),
            SEP,
            l("InsertLiveTemplate", "Insert _Live Template..."),
            l(
                "SurroundWithLiveTemplate",
                "Surround with Li_ve Template...",
            ),
            l("SaveAsTemplate", "Save as Live _Template..."),
            SEP,
            l("CommentByLineComment", "Comment with Line Co_mment"),
            l("CommentByBlockComment", "Comment with _Block Comment"),
            SEP,
            l("ReformatCode", "Reformat Cod_e"),
            l("ShowReformatFileDialog", "Reformat File..."),
            l("AutoIndentLines", "Auto-Indent Li_nes"),
            l("OptimizeImports", "O_ptimize Imports"),
            a("RearrangeCode"),
            SEP,
            a("MoveStatementDown"),
            a("MoveStatementUp"),
            a("MoveElementLeft"),
            a("MoveElementRight"),
            a("MoveLineDown"),
            a("MoveLineUp"),
            SEP,
            l("InspectCode", "In_spect Code..."),
            l("CodeInspection.OnEditor", "Code Cleanup..."),
            l("RunInspection", "Run Inspection by _Name..."),
            SEP,
            l(
                "AnalyzeStacktraceOnError",
                "Analyze Stack _Trace or Thread Dump...",
            ),
        ],
    )
}

fn refactor_menu() -> MenuEntry {
    g(
        "_Refactor",
        vec![
            l("Refactorings.QuickListPopupAction", "Refactor _This..."),
            SEP,
            l("RenameElement", "_Rename..."),
            l("RenameFile", "Rename _File..."),
            l("ChangeSignature", "_Change Signature..."),
            SEP,
            g(
                "_Extract/Introduce",
                vec![
                    l("IntroduceVariable", "_Variable..."),
                    l("IntroduceConstant", "_Constant..."),
                    l("IntroduceField", "_Field..."),
                    l("IntroduceParameter", "_Parameter..."),
                    SEP,
                    l("ExtractMethod", "_Method..."),
                    l("ExtractInterface", "_Interface..."),
                    l("ExtractSuperclass", "_Superclass..."),
                ],
            ),
            l("Inline", "_Inline..."),
            SEP,
            l("Move", "_Move..."),
            l("CopyElement", "C_opy File..."),
            l("SafeDelete", "_Safe Delete..."),
            SEP,
            l("MembersPullUp", "Pull Members _Up..."),
            l("MemberPushDown", "Push Members _Down..."),
        ],
    )
}

fn run_menu() -> MenuEntry {
    g(
        "R_un",
        vec![
            l("Run", "_Run"),
            l("Debug", "_Debug"),
            l("ChooseRunConfiguration", "R_un..."),
            l("ChooseDebugConfiguration", "D_ebug..."),
            l("XDebugger.AttachToProcess", "Attac_h to Process..."),
            l("editRunConfigurations", "Edit Con_figurations..."),
            SEP,
            l("Stop", "_Stop"),
            SEP,
            g(
                "Debugging _Actions",
                vec![
                    l("StepOver", "Step _Over"),
                    l("StepInto", "Step _Into"),
                    l("ForceStepInto", "_Force Step Into"),
                    l("SmartStepInto", "S_mart Step Into"),
                    l("StepOut", "Step O_ut"),
                    l("RunToCursor", "Run to _Cursor"),
                    l("ForceRunToCursor", "Force Run to Cu_rsor"),
                    SEP,
                    l("Pause", "_Pause Program"),
                    l("Resume", "R_esume Program"),
                    SEP,
                    l("EvaluateExpression", "E_valuate Expression..."),
                    l("QuickEvaluateExpression", "_Quick Evaluate Expression"),
                    l("ShowExecutionPoint", "Show E_xecution Point"),
                ],
            ),
            SEP,
            g(
                "Toggle _Breakpoint",
                vec![
                    l("ToggleLineBreakpoint", "_Line Breakpoint"),
                    l(
                        "ToggleTemporaryLineBreakpoint",
                        "_Temporary Line Breakpoint",
                    ),
                ],
            ),
            l("ViewBreakpoints", "View Brea_kpoints..."),
            check("XDebugger.MuteBreakpoints", "_Mute Breakpoints"),
        ],
    )
}

fn tools_menu() -> MenuEntry {
    g(
        "_Tools",
        vec![
            l("RunAnything", "Run _Anything..."),
            l("SaveAsTemplate", "Save as Live _Template..."),
            l("SaveFileAsTemplate", "Save File as Te_mplate..."),
            SEP,
            l("Terminal.NewTab", "Open in _Terminal"),
            l("IdeScriptingConsole", "_IDE Scripting Console"),
            // Plugin submenus (Laravel, Symfony, Docker, …) follow this separator.
            SEP,
        ],
    )
}

fn git_menu() -> MenuEntry {
    g(
        "_Git",
        vec![
            l("CheckinProject", "_Commit..."),
            l("Vcs.Push", "_Push..."),
            l("Vcs.UpdateProject", "_Update Project..."),
            l("Git.Pull", "Pu_ll..."),
            l("Git.Fetch", "_Fetch"),
            SEP,
            l("Git.Merge", "_Merge..."),
            l("Git.Rebase", "_Rebase..."),
            l("Git.ResolveConflicts", "Re_solve Conflicts..."),
            SEP,
            l("Git.Branches", "_Branches..."),
            l("Git.CreateNewBranch", "_New Branch..."),
            l("Git.CreateNewTag", "New _Tag..."),
            l("Git.Reset", "Reset _HEAD..."),
            SEP,
            l("Vcs.Show.Log", "Show Git _Log"),
            SEP,
            g(
                "Uncommitted Chan_ges",
                vec![
                    l("ChangesView.Shelve", "S_helve Changes..."),
                    l("Vcs.Show.Shelf", "Show S_helf"),
                    l("Git.Stash", "_Stash Changes..."),
                    l("Git.Unstash", "_Unstash Changes..."),
                    l("ChangesView.Rollback", "_Rollback..."),
                    l("Vcs.Show.Local.Changes", "Show _Local Changes"),
                ],
            ),
            g(
                "Current F_ile",
                vec![
                    l("CheckinFiles", "_Commit File..."),
                    l("Git.Add", "_Add"),
                    l("Git.Ignore.File", "Add to .git_ignore"),
                    l("Annotate", "Annotate with Git _Blame"),
                    l("Compare.SameVersion", "Show _Diff"),
                    l("Compare.Selected", "Compare with _Revision..."),
                    l("Git.CompareWithBranch", "Compare with Bra_nch..."),
                    l("Vcs.ShowTabbedFileHistory", "Show _History"),
                    l("Vcs.ShowHistoryForBlock", "Show History for _Selection"),
                    l("Vcs.RollbackChangedLines", "Rollback _Lines"),
                ],
            ),
            SEP,
            l("Git.Configure.Remotes", "Manage _Remotes..."),
            l("Git.Clone", "Cl_one..."),
            SEP,
            l("Vcs.QuickListPopupAction", "_VCS Operations..."),
        ],
    )
}

fn vcs_menu() -> MenuEntry {
    g(
        "VC_S",
        vec![
            l("Vcs.QuickListPopupAction", "_VCS Operations..."),
            SEP,
            l("Start.Use.Vcs", "_Enable Version Control Integration..."),
            l("Git.Init", "_Create Git Repository..."),
            l("Vcs.Checkout", "_Get from Version Control..."),
            SEP,
            g(
                "Local _History",
                vec![
                    l("LocalHistory.ShowHistory", "_Show History"),
                    l("LocalHistory.PutLabel", "_Put Label..."),
                ],
            ),
        ],
    )
}

fn window_menu() -> MenuEntry {
    g(
        "_Window",
        vec![
            g(
                "La_youts",
                vec![
                    l("StoreDefaultLayout", "_Store Current Layout as Default"),
                    l("RestoreDefaultLayout", "_Restore Default Layout"),
                ],
            ),
            SEP,
            g(
                "_Active Tool Window",
                vec![
                    l("HideActiveWindow", "_Hide Active Tool Window"),
                    l("HideSideWindows", "Hide _Side Tool Windows"),
                    l("JumpToLastWindow", "_Jump to Last Tool Window"),
                    l("MaximizeToolWindow", "_Maximize Tool Window"),
                    SEP,
                    l("ResizeToolWindowLeft", "Stretch to _Left"),
                    l("ResizeToolWindowRight", "Stretch to _Right"),
                    l("ResizeToolWindowUp", "Stretch to _Top"),
                    l("ResizeToolWindowDown", "Stretch to _Bottom"),
                    SEP,
                    l("CloseActiveTab", "_Close Active Tab"),
                ],
            ),
            l("HideAllWindows", "Hide All Tool _Windows"),
            g(
                "_Editor Tabs",
                vec![
                    l("NextTab", "Select _Next Tab"),
                    l("PreviousTab", "Select _Previous Tab"),
                    SEP,
                    l("PinActiveEditorTab", "P_in Tab"),
                    l("SplitVertically", "Split _Right"),
                    l("SplitHorizontally", "Split _Down"),
                    l("NextSplitter", "Goto Ne_xt Splitter"),
                    l("PrevSplitter", "Goto Pre_vious Splitter"),
                    SEP,
                    l("CloseContent", "_Close Tab"),
                    l("CloseAllEditorsButActive", "Close _Other Tabs"),
                    l("CloseAllEditors", "Close _All Tabs"),
                    l("CloseAllUnmodifiedEditors", "Close _Unmodified Tabs"),
                    l("ReopenClosedTab", "R_eopen Closed Tab"),
                ],
            ),
            SEP,
            l("NextProjectWindow", "Ne_xt Project Window"),
            l("PreviousProjectWindow", "Pre_vious Project Window"),
        ],
    )
}

fn help_menu() -> MenuEntry {
    g(
        "_Help",
        vec![
            l("GotoAction", "_Find Action..."),
            l("HelpTopics", "_Help"),
            l("Help.KeymapReference", "_Keymap Reference"),
            l("ShowTips", "_Tip of the Day"),
            l("WhatsNewAction", "What's _New in Forge"),
            SEP,
            l("SubmitBug", "Submit a _Bug Report..."),
            l("SubmitFeedback", "Submit F_eedback..."),
            SEP,
            l("ShowLog", "Show _Log in Files"),
            l("CollectZippedLogs", "Collect Logs and _Diagnostic Data"),
            SEP,
            l("EditCustomProperties", "Edit Custom _Properties..."),
            l("EditCustomVmOptions", "Edit Custom _VM Options..."),
            SEP,
            l("CheckForUpdate", "_Check for Updates..."),
            l("About", "_About"),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Titles from the PHPStorm catalog, every listed action implemented.
    struct Fake {
        available: HashSet<&'static str>,
        shortcuts: HashMap<&'static str, &'static str>,
    }

    impl ActionLookup for Fake {
        fn title(&self, id: &str) -> Option<String> {
            crate::catalog::catalog().get(id).map(|a| a.title.clone())
        }
        fn shortcut(&self, id: &str) -> Option<String> {
            self.shortcuts.get(id).map(|s| s.to_string())
        }
        fn is_available(&self, id: &str) -> bool {
            self.available.contains(id)
        }
    }

    fn fake(available: &[&'static str]) -> Fake {
        Fake {
            available: available.iter().copied().collect(),
            shortcuts: HashMap::from([
                ("SaveAll", "Ctrl+S"),
                ("ActivateProjectToolWindow", "Alt+1"),
            ]),
        }
    }

    fn labels(items: &[MenuItem]) -> Vec<String> {
        items
            .iter()
            .map(|i| match i.kind {
                MenuItemKind::Separator => "-".to_string(),
                _ => i.label.clone(),
            })
            .collect()
    }

    fn find<'a>(items: &'a [MenuItem], path: &str) -> &'a MenuItem {
        let mut level = items;
        let mut found = None;
        for part in path.split('/') {
            let item = level
                .iter()
                .find(|i| i.label == part)
                .unwrap_or_else(|| panic!("{part} missing in {:?}", labels(level)));
            level = &item.items;
            found = Some(item);
        }
        found.unwrap()
    }

    #[test]
    fn has_phpstorms_top_level_menus_with_unique_mnemonics() {
        let menu = resolve_main_menu(
            &MainMenuRequest {
                git: true,
                ..Default::default()
            },
            &fake(&[]),
        );
        assert_eq!(
            labels(&menu.menus),
            [
                "File", "Edit", "View", "Navigate", "Code", "Refactor", "Run", "Tools", "Git",
                "Window", "Help"
            ]
        );
        let letters: Vec<char> = menu
            .menus
            .iter()
            .map(|m| {
                m.label
                    .chars()
                    .nth(m.mnemonic.unwrap())
                    .unwrap()
                    .to_ascii_lowercase()
            })
            .collect();
        assert_eq!(
            letters,
            ['f', 'e', 'v', 'n', 'c', 'r', 'u', 't', 'g', 'w', 'h']
        );
        let without_git = resolve_main_menu(&MainMenuRequest::default(), &fake(&[]));
        assert_eq!(without_git.menus[8].label, "VCS");
    }

    #[test]
    fn file_menu_follows_phpstorm() {
        let menu = resolve_main_menu(&MainMenuRequest::default(), &fake(&["SaveAll"]));
        let file = find(&menu.menus, "File");
        assert_eq!(
            labels(&file.items),
            [
                "New",
                "Open...",
                "Recent Projects",
                "Close Project",
                "Rename Project...",
                "-",
                "Settings...",
                "Project Structure...",
                "File Properties",
                "Local History",
                "-",
                "Save All",
                "Reload All from Disk",
                "Invalidate Caches...",
                "-",
                "Manage IDE Settings",
                "New Projects Setup",
                "Save File as Template...",
                "-",
                "Export",
                "Print...",
                "-",
                "Power Save Mode",
                "-",
                "Exit"
            ]
        );
        let save = find(&menu.menus, "File/Save All");
        assert_eq!(
            (save.enabled, save.mnemonic, save.shortcut.as_deref()),
            (true, Some(5), Some("Ctrl+S"))
        );
        // Not implemented: shown, but disabled.
        assert!(!find(&menu.menus, "File/Invalidate Caches...").enabled);
        assert!(
            find(&menu.menus, "File/File Properties/Line Separators")
                .items
                .len()
                == 3
        );
    }

    #[test]
    fn dynamic_lists_fill_their_submenus_and_empty_submenus_disappear() {
        let mut request = MainMenuRequest::default();
        // Without tool windows the submenu is empty and left out; Recent Projects keeps
        // "Manage Projects..." without a dangling separator.
        let menu = resolve_main_menu(&request, &fake(&[]));
        let view = find(&menu.menus, "View");
        assert!(!labels(&view.items).contains(&"Tool Windows".to_string()));
        assert_eq!(
            labels(&find(&menu.menus, "File/Recent Projects").items),
            ["Manage Projects..."]
        );

        request.dynamic.insert(
            "toolWindows".into(),
            vec![DynamicItem {
                key: "project".into(),
                label: "Project".into(),
                action: Some("ActivateProjectToolWindow".into()),
                detail: None,
                enabled: true,
                checked: None,
            }],
        );
        request.dynamic.insert(
            "recentProjects".into(),
            vec![DynamicItem {
                key: "/home/me/shop".into(),
                label: "shop".into(),
                action: None,
                detail: Some("~/shop".into()),
                enabled: true,
                checked: None,
            }],
        );
        let menu = resolve_main_menu(&request, &fake(&[]));
        let project = find(&menu.menus, "View/Tool Windows/Project");
        assert_eq!(project.key.as_deref(), Some("project"));
        assert_eq!(project.shortcut.as_deref(), Some("Alt+1"));
        assert_eq!(
            labels(&find(&menu.menus, "File/Recent Projects").items),
            ["shop", "-", "Manage Projects..."]
        );
    }

    #[test]
    fn checkable_entries_reflect_the_request() {
        let request = MainMenuRequest {
            checked: vec!["ViewToolButtons".into()],
            ..Default::default()
        };
        let menu = resolve_main_menu(&request, &fake(&["ViewToolButtons", "ViewStatusBar"]));
        let bars = find(&menu.menus, "View/Appearance/Tool Window Bars");
        let status = find(&menu.menus, "View/Appearance/Status Bar");
        assert!(bars.checkable && bars.checked && bars.enabled);
        assert!(status.checkable && !status.checked);
    }

    #[test]
    fn contributions_create_merge_and_anchor_entries() {
        let laravel = MenuContribution {
            path: "Tools".into(),
            items: vec![g("Laravel", vec![a("Laravel.RunArtisan")])],
            before: None,
            after: None,
        };
        let more_laravel = MenuContribution {
            path: "Tools/_Laravel".into(),
            items: vec![a("Laravel.ShowRoutes")],
            before: None,
            after: None,
        };
        let debug = MenuContribution {
            path: "Run".into(),
            items: vec![
                SEP,
                l(
                    "PhpListenDebugAction",
                    "Start Listening for PHP Debug Connections",
                ),
            ],
            before: None,
            after: Some("XDebugger.MuteBreakpoints".into()),
        };
        let first = MenuContribution {
            path: "Help".into(),
            items: vec![l("Forge.Docs", "Forge Docs")],
            before: Some("GotoAction".into()),
            after: None,
        };
        let new_menu = MenuContribution {
            path: "Tools/Docker".into(),
            items: vec![l("Docker.PullImage", "Pull Image...")],
            before: None,
            after: None,
        };
        let entries = main_menu_entries(true, &[laravel, more_laravel, debug, first, new_menu]);
        let resolver = Resolver {
            lookup: &fake(&["PhpListenDebugAction"]),
            checked: HashSet::new(),
            dynamic: &BTreeMap::new(),
        };
        let menus = resolver.list(&entries);
        let tools = find(&menus, "Tools");
        assert_eq!(
            labels(&tools.items)
                .iter()
                .rev()
                .take(2)
                .collect::<Vec<_>>(),
            ["Docker", "Laravel"]
        );
        assert_eq!(find(&menus, "Tools/Laravel").items.len(), 2);
        let run = labels(&find(&menus, "Run").items);
        assert_eq!(
            run[run.len() - 2..],
            ["-", "Start Listening for PHP Debug Connections"]
        );
        assert!(find(&menus, "Run/Start Listening for PHP Debug Connections").enabled);
        assert_eq!(labels(&find(&menus, "Help").items)[0], "Forge Docs");
    }

    #[test]
    fn contributions_deserialize_from_manifest_toml_shapes() {
        let c: MenuContribution = serde_json::from_value(serde_json::json!({
            "path": "Tools",
            "after": "RunAnything",
            "items": [
                { "type": "group", "label": "_Symfony", "items": [
                    { "type": "action", "id": "Symfony.RunConsoleCommand", "mnemonic": "r" },
                    { "type": "separator" },
                    { "type": "action", "id": "symfony.refresh", "label": "Re_fresh Index" }
                ]}
            ]
        }))
        .unwrap();
        let entries = main_menu_entries(false, &[c]);
        let MenuEntry::Group { items, .. } = &entries[7] else {
            panic!()
        };
        assert!(matches!(&items[1], MenuEntry::Group { label, .. } if label == "_Symfony"));
    }

    #[test]
    fn the_keymap_service_resolves_titles_shortcuts_and_aliases() {
        use crate::keymap::{CommandInfo, KeymapService};
        use crate::shortcut::Platform;
        let mut service = KeymapService::new(Platform::Other);
        service.set_commands(
            ["git.fetch", "GotoClass"]
                .map(|id| CommandInfo {
                    id: id.into(),
                    title: id.into(),
                    category: None,
                })
                .to_vec(),
        );
        let menu = service.main_menu(&MainMenuRequest {
            git: true,
            ..Default::default()
        });
        // Implemented through the alias `git.fetch`.
        assert!(find(&menu.menus, "Git/Fetch").enabled);
        assert!(!find(&menu.menus, "Git/Merge...").enabled);
        let class = find(&menu.menus, "Navigate/Class...");
        assert_eq!(
            (class.enabled, class.shortcut.as_deref()),
            (true, Some("Ctrl+N"))
        );
        // Titles come from the keymap when the menu gives no label.
        assert_eq!(find(&menu.menus, "Edit/Undo").mnemonic, Some(0));
    }

    #[test]
    fn separators_are_collapsed() {
        let resolver = Resolver {
            lookup: &fake(&[]),
            checked: HashSet::new(),
            dynamic: &BTreeMap::new(),
        };
        let out = resolver.list(&[
            SEP,
            a("SaveAll"),
            SEP,
            SEP,
            g("Empty", vec![]),
            SEP,
            a("Exit"),
            SEP,
        ]);
        assert_eq!(labels(&out), ["Save All", "-", "Exit"]);
    }

    #[test]
    fn mnemonics_are_parsed_and_found() {
        assert_eq!(parse_mnemonic("Save _All"), ("Save All".into(), Some(5)));
        assert_eq!(parse_mnemonic("snake__case"), ("snake_case".into(), None));
        assert_eq!(parse_mnemonic("Plain"), ("Plain".into(), None));
        assert_eq!(find_mnemonic("Find Usages", 'u'), Some(5));
        assert_eq!(find_mnemonic("Reformat Code", 'c'), Some(9));
        assert_eq!(find_mnemonic("Redo", 'x'), None);
    }

    #[test]
    fn every_builtin_action_has_a_label() {
        fn walk(entries: &[MenuEntry]) {
            for e in entries {
                match e {
                    MenuEntry::Action { id, label, .. } => assert!(
                        label.is_some() || crate::catalog::catalog().get(id).is_some(),
                        "{id} has neither a label nor a catalog title"
                    ),
                    MenuEntry::Group { items, .. } => walk(items),
                    _ => {}
                }
            }
        }
        walk(&builtin_main_menu(true));
        walk(&builtin_main_menu(false));
    }
}
