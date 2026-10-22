//! Changelists, like PHPStorm's: named groups of local changes in the Commit tool window. One
//! list is *active*; new changes go there. Whole files belong to a list, and single change
//! hunks of a file ([`crate::hunks`]) can be moved to another list (partial changelists).
//!
//! The state is Forge's own (git knows nothing about changelists) and is stored per project in
//! Forge's data directory as `changelists.json`, never in the project.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The default changelist's id and name ("Changes" in PHPStorm).
pub const DEFAULT_ID: &str = "default";
pub const DEFAULT_NAME: &str = "Changes";

/// A changelist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Changelist {
    pub id: String,
    pub name: String,
    /// The "Comment" of the New / Edit Changelist dialog; becomes the commit message draft.
    #[serde(default)]
    pub comment: String,
}

/// All changelists of a project and which changes they hold.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Changelists {
    pub lists: Vec<Changelist>,
    pub active: String,
    /// Changed file (workspace-relative) → list id.
    #[serde(default)]
    pub files: BTreeMap<String, String>,
    /// Hunks moved away from their file's list: path → hunk id → list id.
    #[serde(default)]
    pub hunks: BTreeMap<String, BTreeMap<String, String>>,
}

impl Default for Changelists {
    fn default() -> Self {
        Self {
            lists: vec![Changelist {
                id: DEFAULT_ID.into(),
                name: DEFAULT_NAME.into(),
                comment: String::new(),
            }],
            active: DEFAULT_ID.into(),
            files: BTreeMap::new(),
            hunks: BTreeMap::new(),
        }
    }
}

/// A file as listed in one changelist: the whole file, or only some of its hunks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangelistFile {
    pub path: String,
    /// The hunk ids of the file that belong to this list, `None` for the whole file.
    pub hunks: Option<Vec<String>>,
}

/// One list with its files, for the Commit tool window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangelistView {
    pub id: String,
    pub name: String,
    pub comment: String,
    pub active: bool,
    pub files: Vec<ChangelistFile>,
}

fn err(msg: impl Into<String>) -> String {
    msg.into()
}

impl Changelists {
    /// Restore a stored state, repairing what does not fit (missing default list, unknown
    /// active list, assignments to deleted lists).
    pub fn from_json(text: &str) -> Self {
        let mut cl: Self = serde_json::from_str(text).unwrap_or_default();
        cl.repair();
        cl
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    fn repair(&mut self) {
        self.lists.retain(|l| !l.id.is_empty());
        if self.lists.is_empty() {
            *self = Self::default();
            return;
        }
        if !self.has(&self.active) {
            self.active = self.lists[0].id.clone();
        }
        let ids: Vec<String> = self.lists.iter().map(|l| l.id.clone()).collect();
        let active = self.active.clone();
        for list in self.files.values_mut() {
            if !ids.contains(list) {
                *list = active.clone();
            }
        }
        for hunks in self.hunks.values_mut() {
            hunks.retain(|_, list| ids.contains(list));
        }
        self.hunks.retain(|_, h| !h.is_empty());
    }

    fn has(&self, id: &str) -> bool {
        self.lists.iter().any(|l| l.id == id)
    }

    fn by_name(&self, name: &str) -> Option<&Changelist> {
        self.lists
            .iter()
            .find(|l| l.name.trim().eq_ignore_ascii_case(name.trim()))
    }

    /// The list a file belongs to (the active list for files not seen yet).
    pub fn list_of(&self, path: &str) -> &str {
        self.files
            .get(path)
            .map(String::as_str)
            .unwrap_or(&self.active)
    }

    /// Follow the repository status: new changes go to the active list, files without
    /// changes leave their list. Returns whether anything changed.
    pub fn sync(&mut self, changed: &[String]) -> bool {
        let before = (self.files.len(), self.hunks.len());
        let mut added = false;
        self.files.retain(|p, _| changed.contains(p));
        self.hunks.retain(|p, _| changed.contains(p));
        for p in changed {
            if !self.files.contains_key(p) {
                self.files.insert(p.clone(), self.active.clone());
                added = true;
            }
        }
        added || before != (self.files.len(), self.hunks.len())
    }

    /// Drop moved hunks that no longer exist and fold a file whose hunks all ended up in one
    /// list back into a whole-file assignment. `hunks` are the file's current hunk ids.
    pub fn normalize_file(&mut self, path: &str, hunks: &[String]) -> bool {
        let Some(moved) = self.hunks.get_mut(path) else {
            return false;
        };
        let before = moved.len();
        moved.retain(|id, _| hunks.contains(id));
        let file_list = self
            .files
            .get(path)
            .cloned()
            .unwrap_or_else(|| self.active.clone());
        moved.retain(|_, list| *list != file_list);
        let mut changed = moved.len() != before;
        if !moved.is_empty() && moved.len() == hunks.len() {
            let first = moved.values().next().cloned().unwrap_or_default();
            if moved.values().all(|l| *l == first) {
                self.files.insert(path.to_string(), first);
                moved.clear();
                changed = true;
            }
        }
        if moved.is_empty() {
            self.hunks.remove(path);
        }
        changed
    }

    /// New Changelist…: returns the new list's id.
    pub fn create(
        &mut self,
        name: &str,
        comment: &str,
        make_active: bool,
    ) -> Result<String, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err(err("Changelist name must not be empty"));
        }
        if self.by_name(name).is_some() {
            return Err(err(format!("A changelist named '{name}' already exists")));
        }
        let mut n = self.lists.len();
        let id = loop {
            let id = format!("cl{n}");
            if !self.has(&id) {
                break id;
            }
            n += 1;
        };
        self.lists.push(Changelist {
            id: id.clone(),
            name: name.into(),
            comment: comment.trim_end().into(),
        });
        if make_active {
            self.active = id.clone();
        }
        Ok(id)
    }

    /// The list named `name`, created when missing (Apply Patch and Unshelve with
    /// "Create changelists automatically").
    pub fn ensure(&mut self, name: &str) -> Result<String, String> {
        match self.by_name(name) {
            Some(l) => Ok(l.id.clone()),
            None => self.create(name, "", false),
        }
    }

    /// Edit Changelist…: rename, change the comment, make active.
    pub fn edit(
        &mut self,
        id: &str,
        name: &str,
        comment: &str,
        make_active: bool,
    ) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err(err("Changelist name must not be empty"));
        }
        if self.by_name(name).is_some_and(|l| l.id != id) {
            return Err(err(format!("A changelist named '{name}' already exists")));
        }
        let list = self
            .lists
            .iter_mut()
            .find(|l| l.id == id)
            .ok_or_else(|| err("No such changelist"))?;
        list.name = name.into();
        list.comment = comment.trim_end().into();
        if make_active {
            self.active = id.into();
        }
        Ok(())
    }

    /// Set Active Changelist.
    pub fn set_active(&mut self, id: &str) -> Result<(), String> {
        if !self.has(id) {
            return Err(err("No such changelist"));
        }
        self.active = id.into();
        Ok(())
    }

    /// Delete Changelist: its changes move to the active list. The active list cannot be
    /// deleted (PHPStorm asks to activate another one first).
    pub fn delete(&mut self, id: &str) -> Result<(), String> {
        if id == self.active {
            return Err(err("The active changelist cannot be deleted"));
        }
        if !self.has(id) {
            return Err(err("No such changelist"));
        }
        self.lists.retain(|l| l.id != id);
        let active = self.active.clone();
        for list in self.files.values_mut() {
            if list == id {
                *list = active.clone();
            }
        }
        for hunks in self.hunks.values_mut() {
            for list in hunks.values_mut() {
                if list == id {
                    *list = active.clone();
                }
            }
        }
        Ok(())
    }

    /// Move to Another Changelist (F6) for whole files.
    pub fn move_files(&mut self, paths: &[String], target: &str) -> Result<(), String> {
        if !self.has(target) {
            return Err(err("No such changelist"));
        }
        for p in paths {
            self.files.insert(p.clone(), target.into());
            self.hunks.remove(p);
        }
        Ok(())
    }

    /// Move single hunks of a file to another list (from the gutter or the commit diff).
    /// `all` are the file's current hunk ids.
    pub fn move_hunks(
        &mut self,
        path: &str,
        ids: &[String],
        target: &str,
        all: &[String],
    ) -> Result<(), String> {
        if !self.has(target) {
            return Err(err("No such changelist"));
        }
        self.files
            .entry(path.into())
            .or_insert_with(|| self.active.clone());
        let moved = self.hunks.entry(path.into()).or_default();
        for id in ids {
            moved.insert(id.clone(), target.into());
        }
        self.normalize_file(path, all);
        Ok(())
    }

    /// The lists with their files. `changed` are the files with local changes, in display
    /// order; `hunks_of` gives the current hunk ids of files with moved hunks.
    pub fn view(
        &self,
        changed: &[String],
        hunks_of: &dyn Fn(&str) -> Vec<String>,
    ) -> Vec<ChangelistView> {
        let mut out: Vec<ChangelistView> = self
            .lists
            .iter()
            .map(|l| ChangelistView {
                id: l.id.clone(),
                name: l.name.clone(),
                comment: l.comment.clone(),
                active: l.id == self.active,
                files: Vec::new(),
            })
            .collect();
        for path in changed {
            let file_list = self.list_of(path).to_string();
            let push = |out: &mut Vec<ChangelistView>, list: &str, hunks: Option<Vec<String>>| {
                if let Some(v) = out.iter_mut().find(|v| v.id == list) {
                    v.files.push(ChangelistFile {
                        path: path.clone(),
                        hunks,
                    });
                }
            };
            match self.hunks.get(path).filter(|m| !m.is_empty()) {
                None => push(&mut out, &file_list, None),
                Some(moved) => {
                    let mut per_list: BTreeMap<String, Vec<String>> = BTreeMap::new();
                    for id in hunks_of(path) {
                        let list = moved.get(&id).cloned().unwrap_or_else(|| file_list.clone());
                        per_list.entry(list).or_default().push(id);
                    }
                    // Keep the list order of the view.
                    for v in out.iter_mut() {
                        if let Some(ids) = per_list.remove(&v.id) {
                            v.files.push(ChangelistFile {
                                path: path.clone(),
                                hunks: Some(ids),
                            });
                        }
                    }
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(p: &[&str]) -> Vec<String> {
        p.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn new_changes_go_to_the_active_list() {
        let mut cl = Changelists::default();
        assert!(cl.sync(&paths(&["a.php"])));
        let feature = cl.create("Feature", "Add feature", true).unwrap();
        assert!(cl.sync(&paths(&["a.php", "b.php"])));
        assert_eq!(cl.list_of("a.php"), DEFAULT_ID);
        assert_eq!(cl.list_of("b.php"), feature);
        assert!(!cl.sync(&paths(&["a.php", "b.php"])));
        // Committed or rolled back: the file leaves its list.
        assert!(cl.sync(&paths(&["b.php"])));
        assert!(!cl.files.contains_key("a.php"));
    }

    #[test]
    fn creates_edits_and_deletes_lists() {
        let mut cl = Changelists::default();
        assert!(cl.create(" ", "", false).is_err());
        let id = cl.create("Docs", "", false).unwrap();
        assert!(cl.create("docs", "", false).is_err());
        assert!(cl.edit(&id, "Changes", "", false).is_err());
        cl.edit(&id, "Documentation", "Update the guide", true)
            .unwrap();
        assert_eq!(cl.active, id);
        assert!(cl.delete(&id).is_err(), "the active list stays");
        cl.set_active(DEFAULT_ID).unwrap();
        cl.sync(&paths(&["x"]));
        cl.move_files(&paths(&["x"]), &id).unwrap();
        cl.delete(&id).unwrap();
        assert_eq!(cl.list_of("x"), DEFAULT_ID);
        assert_eq!(cl.ensure("Patch").unwrap(), cl.ensure("patch").unwrap());
    }

    #[test]
    fn hunks_can_live_in_another_list() {
        let mut cl = Changelists::default();
        let other = cl.create("Other", "", false).unwrap();
        cl.sync(&paths(&["f.php", "g.php"]));
        let all = paths(&["h1", "h2", "h3"]);
        cl.move_hunks("f.php", &paths(&["h2"]), &other, &all)
            .unwrap();
        let hunks = |p: &str| {
            if p == "f.php" {
                all.clone()
            } else {
                Vec::new()
            }
        };
        let view = cl.view(&paths(&["f.php", "g.php"]), &hunks);
        assert_eq!(view[0].files.len(), 2);
        assert_eq!(view[0].files[0].hunks, Some(paths(&["h1", "h3"])));
        assert_eq!(view[0].files[1].hunks, None);
        assert_eq!(view[1].files[0].hunks, Some(paths(&["h2"])));

        // Moving the remaining hunks folds the file into the other list.
        cl.move_hunks("f.php", &paths(&["h1", "h3"]), &other, &all)
            .unwrap();
        assert_eq!(cl.list_of("f.php"), other);
        assert!(cl.hunks.is_empty());

        // A hunk that no longer exists is forgotten.
        cl.move_hunks("g.php", &paths(&["x"]), &other, &paths(&["x", "y"]))
            .unwrap();
        assert!(cl.normalize_file("g.php", &paths(&["y"])));
        assert!(!cl.hunks.contains_key("g.php"));
    }

    #[test]
    fn repairs_stored_state() {
        let cl = Changelists::from_json(
            r#"{"lists":[{"id":"a","name":"A"}],"active":"gone","files":{"x":"zzz"},"hunks":{"y":{"h":"zzz"}}}"#,
        );
        assert_eq!(cl.active, "a");
        assert_eq!(cl.files["x"], "a");
        assert!(cl.hunks.is_empty());
        assert_eq!(Changelists::from_json("not json"), Changelists::default());
        let back = Changelists::from_json(&cl.to_json());
        assert_eq!(back, cl);
    }
}
