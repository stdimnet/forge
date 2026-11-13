//! Bookmarks (PHPStorm's Bookmarks tool window, `F11`, `Ctrl+F11`, `Alt+2`).
//!
//! - [`Bookmarks`] holds named bookmark lists; one of them is the default list that `F11`
//!   adds to. A bookmark marks a line of a project-relative file and may carry a mnemonic
//!   (`0`–`9`, `A`–`Z`, unique across all lists) and a description.
//! - Every change is an [`Op`] applied with [`Bookmarks::apply`], so the desktop app (state
//!   in memory, saved to Forge's state dir) and browser mode (WebAssembly) share the logic.
//! - Line positions follow edits: the editor reports each document change as [`LineEdit`]s
//!   in old-document coordinates and [`Op::ApplyEdits`] moves the bookmarks below them.
//!   Renamed and deleted files are followed with [`Op::RenamePath`] / [`Op::RemovePath`].
//!
//! The crate has no I/O; the state is serialized as JSON by the caller.

use serde::{Deserialize, Serialize};

/// Name of the list that exists in every project (PHPStorm names it after the project; Forge
/// uses a fixed name so it survives renames).
pub const DEFAULT_LIST: &str = "Bookmarks";

/// One line bookmark.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bookmark {
    pub id: u64,
    /// Project-relative path with `/` separators.
    pub path: String,
    /// 0-based line.
    pub line: usize,
    /// `0`–`9` or `A`–`Z`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mnemonic: Option<char>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

/// A named group of bookmarks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BookmarkList {
    pub name: String,
    pub bookmarks: Vec<Bookmark>,
}

/// All bookmarks of a project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bookmarks {
    pub lists: Vec<BookmarkList>,
    /// The list `F11` adds to.
    pub default_list: String,
    #[serde(default)]
    pub next_id: u64,
}

impl Default for Bookmarks {
    fn default() -> Self {
        Bookmarks {
            lists: vec![BookmarkList {
                name: DEFAULT_LIST.into(),
                bookmarks: Vec::new(),
            }],
            default_list: DEFAULT_LIST.into(),
            next_id: 1,
        }
    }
}

/// One document change in line terms, in the coordinates of the document before the change:
/// the change starts on `line` (at its first column when `at_line_start`), removes `removed`
/// line breaks and inserts `added` line breaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineEdit {
    pub line: usize,
    pub removed: usize,
    pub added: usize,
    #[serde(default)]
    pub at_line_start: bool,
}

/// A change to the bookmarks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Op {
    /// `F11`: add a bookmark to the default list, or remove the one on this line.
    Toggle {
        path: String,
        line: usize,
    },
    /// `Ctrl+F11` / `Ctrl+Shift+<digit>`: give the line's bookmark (created when missing) a
    /// mnemonic, taking it away from any other bookmark; `None` removes the mnemonic. Setting
    /// the mnemonic the line already has removes the bookmark (a toggle, like PHPStorm).
    SetMnemonic {
        path: String,
        line: usize,
        mnemonic: Option<char>,
    },
    Remove {
        id: u64,
    },
    /// Remove every bookmark of a list (or of all lists).
    Clear {
        list: Option<String>,
    },
    Describe {
        id: u64,
        description: String,
    },
    /// Move a bookmark to another list (created when missing).
    MoveToList {
        id: u64,
        list: String,
    },
    AddList {
        name: String,
    },
    RenameList {
        from: String,
        to: String,
    },
    /// Remove a list with its bookmarks; the last list cannot be removed.
    RemoveList {
        name: String,
    },
    SetDefaultList {
        name: String,
    },
    /// Edits of one file, in the order the editor reports them (ascending `line`).
    ApplyEdits {
        path: String,
        edits: Vec<LineEdit>,
    },
    /// A file or folder was renamed or moved.
    RenamePath {
        from: String,
        to: String,
    },
    /// A file or folder was deleted.
    RemovePath {
        path: String,
    },
}

/// A valid mnemonic: an ASCII digit or letter (stored upper case).
pub fn normalize_mnemonic(c: char) -> Option<char> {
    c.is_ascii_alphanumeric().then(|| c.to_ascii_uppercase())
}

fn under(path: &str, dir: &str) -> bool {
    path == dir || path.strip_prefix(dir).is_some_and(|r| r.starts_with('/'))
}

/// The line of a bookmark at `line` after one edit.
pub fn map_line(line: usize, edit: &LineEdit) -> usize {
    let LineEdit {
        line: start,
        removed,
        added,
        at_line_start,
    } = *edit;
    if line < start || (line == start && !at_line_start) {
        // Above the change, or on the line the change starts in the middle of.
        line
    } else if line > start + removed || (line == start && at_line_start && removed == 0) {
        // Below the change (or pushed down by text inserted at the start of its line).
        line + added - removed
    } else if line == start + removed {
        // On the last line the change touches: the rest of it ends up after the inserted text.
        start + added
    } else {
        // The line was removed: keep the bookmark where the change happened.
        start + added.min(line - start)
    }
}

impl Bookmarks {
    /// Every bookmark with the name of its list.
    pub fn all(&self) -> impl Iterator<Item = (&str, &Bookmark)> {
        self.lists
            .iter()
            .flat_map(|l| l.bookmarks.iter().map(move |b| (l.name.as_str(), b)))
    }

    pub fn find(&self, path: &str, line: usize) -> Option<&Bookmark> {
        self.all()
            .map(|(_, b)| b)
            .find(|b| b.path == path && b.line == line)
    }

    pub fn by_mnemonic(&self, mnemonic: char) -> Option<&Bookmark> {
        let m = normalize_mnemonic(mnemonic)?;
        self.all().map(|(_, b)| b).find(|b| b.mnemonic == Some(m))
    }

    fn bookmarks_mut(&mut self) -> impl Iterator<Item = &mut Bookmark> {
        self.lists.iter_mut().flat_map(|l| l.bookmarks.iter_mut())
    }

    fn list_mut(&mut self, name: &str) -> &mut BookmarkList {
        if let Some(i) = self.lists.iter().position(|l| l.name == name) {
            return &mut self.lists[i];
        }
        self.lists.push(BookmarkList {
            name: name.into(),
            bookmarks: Vec::new(),
        });
        self.lists.last_mut().expect("just pushed")
    }

    fn take(&mut self, id: u64) -> Option<Bookmark> {
        for list in &mut self.lists {
            if let Some(i) = list.bookmarks.iter().position(|b| b.id == id) {
                return Some(list.bookmarks.remove(i));
            }
        }
        None
    }

    fn add(&mut self, path: &str, line: usize, mnemonic: Option<char>) {
        let id = self.next_id.max(1);
        self.next_id = id + 1;
        let default = self.default_list.clone();
        self.list_mut(&default).bookmarks.push(Bookmark {
            id,
            path: path.into(),
            line,
            mnemonic,
            description: String::new(),
        });
    }

    /// Apply one change. Invalid changes (unknown ids, bad names) are ignored.
    pub fn apply(&mut self, op: Op) {
        match op {
            Op::Toggle { path, line } => match self.find(&path, line).map(|b| b.id) {
                Some(id) => {
                    self.take(id);
                }
                None => self.add(&path, line, None),
            },
            Op::SetMnemonic {
                path,
                line,
                mnemonic,
            } => {
                let mnemonic = mnemonic.and_then(normalize_mnemonic);
                let existing = self.find(&path, line).cloned();
                if let (Some(b), Some(m)) = (&existing, mnemonic) {
                    if b.mnemonic == Some(m) {
                        self.take(b.id);
                        return;
                    }
                }
                if let Some(m) = mnemonic {
                    for b in self.bookmarks_mut() {
                        if b.mnemonic == Some(m) {
                            b.mnemonic = None;
                        }
                    }
                }
                match existing {
                    Some(b) => {
                        if let Some(t) = self.bookmarks_mut().find(|x| x.id == b.id) {
                            t.mnemonic = mnemonic;
                        }
                    }
                    None => self.add(&path, line, mnemonic),
                }
            }
            Op::Remove { id } => {
                self.take(id);
            }
            Op::Clear { list } => {
                for l in &mut self.lists {
                    if list.as_ref().is_none_or(|n| *n == l.name) {
                        l.bookmarks.clear();
                    }
                }
            }
            Op::Describe { id, description } => {
                if let Some(b) = self.bookmarks_mut().find(|b| b.id == id) {
                    b.description = description.trim().to_string();
                }
            }
            Op::MoveToList { id, list } => {
                let list = list.trim().to_string();
                if list.is_empty() {
                    return;
                }
                if let Some(b) = self.take(id) {
                    self.list_mut(&list).bookmarks.push(b);
                }
            }
            Op::AddList { name } => {
                let name = name.trim();
                if !name.is_empty() {
                    self.list_mut(name);
                }
            }
            Op::RenameList { from, to } => {
                let to = to.trim().to_string();
                if to.is_empty() || self.lists.iter().any(|l| l.name == to) {
                    return;
                }
                if let Some(l) = self.lists.iter_mut().find(|l| l.name == from) {
                    l.name = to.clone();
                    if self.default_list == from {
                        self.default_list = to;
                    }
                }
            }
            Op::RemoveList { name } => {
                if self.lists.len() <= 1 {
                    return;
                }
                self.lists.retain(|l| l.name != name);
                if self.default_list == name {
                    self.default_list = self.lists[0].name.clone();
                }
            }
            Op::SetDefaultList { name } => {
                if self.lists.iter().any(|l| l.name == name) {
                    self.default_list = name;
                }
            }
            Op::ApplyEdits { path, edits } => self.apply_edits(&path, &edits),
            Op::RenamePath { from, to } => {
                for b in self.bookmarks_mut() {
                    if under(&b.path, &from) {
                        b.path = format!("{to}{}", &b.path[from.len()..]);
                    }
                }
            }
            Op::RemovePath { path } => {
                for l in &mut self.lists {
                    l.bookmarks.retain(|b| !under(&b.path, &path));
                }
            }
        }
    }

    fn apply_edits(&mut self, path: &str, edits: &[LineEdit]) {
        if edits.is_empty() {
            return;
        }
        // Edits are in old-document coordinates: apply the last one first so the earlier
        // ones still refer to unchanged lines.
        let mut sorted = edits.to_vec();
        sorted.sort_by_key(|e| e.line);
        for b in self.bookmarks_mut().filter(|b| b.path == path) {
            b.line = sorted.iter().rev().fold(b.line, map_line);
        }
        // Two bookmarks that collapsed onto one line: keep the one with a mnemonic.
        let mut seen: Vec<(usize, u64, bool)> = Vec::new();
        for (_, b) in self.all().filter(|(_, b)| b.path == path) {
            seen.push((b.line, b.id, b.mnemonic.is_some()));
        }
        seen.sort_by_key(|&(line, id, has)| (line, !has, id));
        let mut drop = Vec::new();
        for pair in seen.windows(2) {
            if pair[0].0 == pair[1].0 {
                drop.push(pair[1].1);
            }
        }
        for id in drop {
            self.take(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toggle(b: &mut Bookmarks, path: &str, line: usize) {
        b.apply(Op::Toggle {
            path: path.into(),
            line,
        });
    }

    fn lines(b: &Bookmarks, path: &str) -> Vec<usize> {
        let mut v: Vec<_> = b
            .all()
            .filter(|(_, x)| x.path == path)
            .map(|(_, x)| x.line)
            .collect();
        v.sort();
        v
    }

    fn edit(line: usize, removed: usize, added: usize, at_line_start: bool) -> Op {
        Op::ApplyEdits {
            path: "a.php".into(),
            edits: vec![LineEdit {
                line,
                removed,
                added,
                at_line_start,
            }],
        }
    }

    #[test]
    fn toggle_adds_and_removes() {
        let mut b = Bookmarks::default();
        toggle(&mut b, "a.php", 3);
        toggle(&mut b, "a.php", 5);
        assert_eq!(lines(&b, "a.php"), vec![3, 5]);
        toggle(&mut b, "a.php", 3);
        assert_eq!(lines(&b, "a.php"), vec![5]);
        assert_eq!(b.lists[0].bookmarks[0].id, 2);
    }

    #[test]
    fn lines_follow_inserted_and_removed_lines() {
        let mut b = Bookmarks::default();
        for l in [2, 5, 9] {
            toggle(&mut b, "a.php", l);
        }
        toggle(&mut b, "other.php", 9);
        // Two lines typed at the end of line 0: everything below moves down.
        b.apply(edit(0, 0, 2, false));
        assert_eq!(lines(&b, "a.php"), vec![4, 7, 11]);
        assert_eq!(lines(&b, "other.php"), vec![9]);
        // Enter in the middle of a bookmarked line keeps the bookmark on it.
        b.apply(edit(4, 0, 1, false));
        assert_eq!(lines(&b, "a.php"), vec![4, 8, 12]);
        // Enter at the start of a bookmarked line pushes it down.
        b.apply(edit(8, 0, 1, true));
        assert_eq!(lines(&b, "a.php"), vec![4, 9, 13]);
        // Deleting lines 5..=8 (joining line 5 with line 9) moves the bookmark of line 9 up.
        b.apply(edit(5, 4, 0, false));
        assert_eq!(lines(&b, "a.php"), vec![4, 5, 9]);
    }

    #[test]
    fn deleted_lines_collapse_and_keep_the_mnemonic() {
        let mut b = Bookmarks::default();
        toggle(&mut b, "a.php", 3);
        b.apply(Op::SetMnemonic {
            path: "a.php".into(),
            line: 4,
            mnemonic: Some('a'),
        });
        // Lines 2..=5 replaced by one line: both bookmarks land on line 2.
        b.apply(edit(2, 3, 0, false));
        let all: Vec<_> = b.all().map(|(_, x)| (x.line, x.mnemonic)).collect();
        assert_eq!(all, vec![(2, Some('A'))]);
    }

    #[test]
    fn several_edits_use_old_coordinates() {
        let mut b = Bookmarks::default();
        toggle(&mut b, "a.php", 10);
        toggle(&mut b, "a.php", 20);
        b.apply(Op::ApplyEdits {
            path: "a.php".into(),
            edits: vec![
                LineEdit {
                    line: 1,
                    removed: 0,
                    added: 3,
                    at_line_start: false,
                },
                LineEdit {
                    line: 15,
                    removed: 2,
                    added: 0,
                    at_line_start: false,
                },
            ],
        });
        assert_eq!(lines(&b, "a.php"), vec![13, 21]);
    }

    #[test]
    fn mnemonics_are_unique_and_toggle() {
        let mut b = Bookmarks::default();
        let set = |b: &mut Bookmarks, line, m| {
            b.apply(Op::SetMnemonic {
                path: "a.php".into(),
                line,
                mnemonic: Some(m),
            })
        };
        set(&mut b, 1, '1');
        set(&mut b, 2, '1');
        assert_eq!(b.by_mnemonic('1').unwrap().line, 2);
        assert_eq!(b.find("a.php", 1).unwrap().mnemonic, None);
        set(&mut b, 2, '1');
        assert!(b.find("a.php", 2).is_none());
        set(&mut b, 3, '!');
        assert_eq!(b.find("a.php", 3).unwrap().mnemonic, None);
        assert_eq!(b.by_mnemonic('z'), None);
    }

    #[test]
    fn lists_descriptions_and_paths() {
        let mut b = Bookmarks::default();
        toggle(&mut b, "app/Models/User.php", 1);
        toggle(&mut b, "app/Http/Kernel.php", 7);
        b.apply(Op::AddList {
            name: "Review".into(),
        });
        b.apply(Op::MoveToList {
            id: 2,
            list: "Review".into(),
        });
        b.apply(Op::Describe {
            id: 2,
            description: " middleware ".into(),
        });
        assert_eq!(b.lists[1].bookmarks[0].description, "middleware");
        b.apply(Op::RenameList {
            from: "Review".into(),
            to: "Later".into(),
        });
        b.apply(Op::SetDefaultList {
            name: "Later".into(),
        });
        toggle(&mut b, "README.md", 0);
        assert_eq!(b.lists[1].bookmarks.len(), 2);

        b.apply(Op::RenamePath {
            from: "app/Models".into(),
            to: "src/Models".into(),
        });
        assert!(b.find("src/Models/User.php", 1).is_some());
        b.apply(Op::RemovePath { path: "app".into() });
        assert!(b.find("app/Http/Kernel.php", 7).is_none());

        b.apply(Op::RemoveList {
            name: "Later".into(),
        });
        assert_eq!(b.default_list, DEFAULT_LIST);
        b.apply(Op::RemoveList {
            name: DEFAULT_LIST.into(),
        });
        assert_eq!(b.lists.len(), 1);
        b.apply(Op::Clear { list: None });
        assert_eq!(b.all().count(), 0);
    }

    #[test]
    fn ops_and_state_serialize_camel_case() {
        let op: Op = serde_json::from_str(
            r#"{"op":"applyEdits","path":"a.php","edits":[{"line":1,"removed":0,"added":1,"atLineStart":true}]}"#,
        )
        .unwrap();
        assert!(matches!(op, Op::ApplyEdits { .. }));
        let op: Op =
            serde_json::from_str(r#"{"op":"setMnemonic","path":"a","line":0,"mnemonic":"b"}"#)
                .unwrap();
        let mut b = Bookmarks::default();
        b.apply(op);
        let json = serde_json::to_string(&b).unwrap();
        assert!(json.contains(r#""defaultList":"Bookmarks""#), "{json}");
        assert!(json.contains(r#""mnemonic":"B""#), "{json}");
        let back: Bookmarks = serde_json::from_str(&json).unwrap();
        assert_eq!(back, b);
    }
}
