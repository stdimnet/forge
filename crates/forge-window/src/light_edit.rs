//! The LightEdit window (PHPStorm's lightweight editor): files opened from outside a project
//! ("Open With → Forge", `forge -e file`) show as tabs of one detached editor window, without
//! a project, tool windows or indexing. Later files open as more tabs of the same window, and
//! a file that is open already becomes the active tab again.
//!
//! This is the window's tab model; the Tauri shell owns the window and the files. Each tab
//! has an id `light-edit:<path with forward slashes>`, which is the tab path in the editor;
//! only files that are tabs here can be read or written through the LightEdit commands.

use serde::{Deserialize, Serialize};

/// Dialog id of the LightEdit window (its label is `dialog-light-edit`).
pub const LIGHT_EDIT_ID: &str = "light-edit";
/// Prefix of the tab ids (the editor's file system provider for LightEdit files).
pub const TAB_PREFIX: &str = "light-edit:";

/// One file of the LightEdit window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LightEditFile {
    /// Tab id: [`TAB_PREFIX`] plus the path with `/` separators.
    pub id: String,
    /// The absolute path as the OS spells it.
    pub path: String,
    /// The file name, shown on the tab.
    pub name: String,
}

/// A position to show after opening (`forge -e file:12`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reveal {
    pub id: String,
    pub line: Option<u32>,
    pub column: Option<u32>,
}

/// What the window shows: its tabs and the active one.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LightEditSnapshot {
    pub files: Vec<LightEditFile>,
    pub active: Option<String>,
    /// The file the last open request asked for, with its position.
    pub reveal: Option<Reveal>,
}

/// The tab id of an absolute path.
pub fn tab_id(path: &str) -> String {
    format!("{TAB_PREFIX}{}", path.replace('\\', "/"))
}

fn file_name(path: &str) -> String {
    path.rsplit(['/', '\\'])
        .find(|s| !s.is_empty())
        .unwrap_or(path)
        .to_string()
}

/// Same file? Windows and macOS file systems usually ignore case.
fn same_path(a: &str, b: &str, ignore_case: bool) -> bool {
    let (a, b) = (a.replace('\\', "/"), b.replace('\\', "/"));
    if ignore_case {
        a.to_lowercase() == b.to_lowercase()
    } else {
        a == b
    }
}

/// The tabs of the LightEdit window. Serializable, so the browser-mode mock can keep them
/// between calls of the WebAssembly build.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LightEditTabs {
    files: Vec<LightEditFile>,
    active: Option<String>,
    reveal: Option<Reveal>,
    ignore_case: bool,
}

impl LightEditTabs {
    /// Tabs for a file system that ignores case (`true` on Windows and macOS).
    pub fn new(ignore_case: bool) -> Self {
        Self {
            ignore_case,
            ..Self::default()
        }
    }

    /// Open `path` (absolute) as a tab, or activate its tab when it is open already.
    /// Returns the tab id and whether a new tab was added.
    pub fn open(&mut self, path: &str, line: Option<u32>, column: Option<u32>) -> (String, bool) {
        let existing = self
            .files
            .iter()
            .find(|f| same_path(&f.path, path, self.ignore_case))
            .map(|f| f.id.clone());
        let (id, added) = match existing {
            Some(id) => (id, false),
            None => {
                let id = tab_id(path);
                self.files.push(LightEditFile {
                    id: id.clone(),
                    path: path.to_string(),
                    name: file_name(path),
                });
                (id, true)
            }
        };
        self.active = Some(id.clone());
        self.reveal = Some(Reveal {
            id: id.clone(),
            line,
            column,
        });
        (id, added)
    }

    /// Close a tab; the tab to its left (or the new first tab) becomes active.
    pub fn close(&mut self, id: &str) -> bool {
        let Some(index) = self.files.iter().position(|f| f.id == id) else {
            return false;
        };
        self.files.remove(index);
        if self.active.as_deref() == Some(id) {
            self.active = self
                .files
                .get(index.saturating_sub(1))
                .map(|f| f.id.clone());
        }
        if self.reveal.as_ref().is_some_and(|r| r.id == id) {
            self.reveal = None;
        }
        true
    }

    /// Make a tab active (the user switched tabs).
    pub fn activate(&mut self, id: &str) -> bool {
        let known = self.files.iter().any(|f| f.id == id);
        if known {
            self.active = Some(id.to_string());
        }
        known
    }

    /// The path of a tab, `None` for ids that are not tabs of the window.
    pub fn path_of(&self, id: &str) -> Option<&str> {
        self.files
            .iter()
            .find(|f| f.id == id)
            .map(|f| f.path.as_str())
    }

    /// The window closed: no tabs are left.
    pub fn clear(&mut self) {
        self.files.clear();
        self.active = None;
        self.reveal = None;
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    pub fn snapshot(&self) -> LightEditSnapshot {
        LightEditSnapshot {
            files: self.files.clone(),
            active: self.active.clone(),
            reveal: self.reveal.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn later_files_open_as_tabs_of_the_same_window() {
        let mut tabs = LightEditTabs::new(false);
        let (a, added) = tabs.open("/home/me/notes.md", None, None);
        assert!(added);
        assert_eq!(a, "light-edit:/home/me/notes.md");
        let (b, added) = tabs.open("/etc/hosts", Some(3), None);
        assert!(added);
        let snap = tabs.snapshot();
        assert_eq!(snap.files.len(), 2);
        assert_eq!(snap.files[1].name, "hosts");
        assert_eq!(snap.active.as_deref(), Some(b.as_str()));
        assert_eq!(
            snap.reveal,
            Some(Reveal {
                id: b,
                line: Some(3),
                column: None
            })
        );
    }

    #[test]
    fn an_open_file_is_reused_and_activated() {
        let mut tabs = LightEditTabs::new(false);
        let (a, _) = tabs.open("/home/me/notes.md", None, None);
        tabs.open("/etc/hosts", None, None);
        let (again, added) = tabs.open("/home/me/notes.md", Some(7), Some(2));
        assert_eq!(again, a);
        assert!(!added);
        assert_eq!(tabs.snapshot().files.len(), 2);
        assert_eq!(tabs.snapshot().active.as_deref(), Some(a.as_str()));
        // Case matters on Linux, not on Windows / macOS.
        assert!(tabs.open("/home/me/NOTES.md", None, None).1);
        let mut win = LightEditTabs::new(true);
        let (first, _) = win.open(r"C:\Users\me\todo.txt", None, None);
        assert_eq!(first, "light-edit:C:/Users/me/todo.txt");
        assert_eq!(win.open("c:/users/me/TODO.txt", None, None), (first, false));
        assert_eq!(win.snapshot().files[0].name, "todo.txt");
    }

    #[test]
    fn closing_activates_the_neighbour_and_forgets_the_path() {
        let mut tabs = LightEditTabs::new(false);
        let (a, _) = tabs.open("/a.txt", None, None);
        let (b, _) = tabs.open("/b.txt", None, None);
        let (c, _) = tabs.open("/c.txt", None, None);
        assert!(tabs.activate(&b));
        assert!(tabs.close(&b));
        assert_eq!(tabs.snapshot().active.as_deref(), Some(a.as_str()));
        assert_eq!(tabs.path_of(&b), None);
        assert_eq!(tabs.path_of(&c), Some("/c.txt"));
        assert!(!tabs.close(&b));
        assert!(!tabs.activate("light-edit:/nope"));
        tabs.close(&a);
        assert_eq!(tabs.snapshot().active.as_deref(), Some(c.as_str()));
        tabs.clear();
        assert!(tabs.is_empty());
    }
}
