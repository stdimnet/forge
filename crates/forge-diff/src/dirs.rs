//! Folder diff (PHPStorm's Compare Directories): every file and folder of two trees with
//! whether it is equal, different or exists on one side only.

use std::collections::BTreeMap;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Entries read per side at most; larger trees are compared partially (`truncated`).
pub const MAX_ENTRIES: usize = 50_000;

/// Folders that are never compared (repository metadata).
const SKIPPED_DIRS: &[&str] = &[".git", ".hg", ".svn"];

/// A file or folder of one side, its path relative to the compared folder (`/` separated).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListedEntry {
    pub path: String,
    pub dir: bool,
    #[serde(default)]
    pub size: u64,
    /// Content digest of a file (see [`digest`]).
    #[serde(default)]
    pub digest: u64,
    /// Modification time, seconds since the Unix epoch.
    #[serde(default)]
    pub modified: Option<i64>,
}

impl ListedEntry {
    pub fn file(path: impl Into<String>, bytes: &[u8], modified: Option<i64>) -> Self {
        Self {
            path: path.into(),
            dir: false,
            size: bytes.len() as u64,
            digest: digest(bytes),
            modified,
        }
    }

    pub fn dir(path: impl Into<String>, modified: Option<i64>) -> Self {
        Self {
            path: path.into(),
            dir: true,
            size: 0,
            digest: 0,
            modified,
        }
    }
}

/// The entries of one side.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Listing {
    pub entries: Vec<ListedEntry>,
    /// More than [`MAX_ENTRIES`] entries: the rest was not read.
    #[serde(default)]
    pub truncated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DiffStatus {
    Equal,
    Different,
    LeftOnly,
    RightOnly,
}

/// One row of the folder diff.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirDiffEntry {
    /// Path relative to the compared folders.
    pub path: String,
    pub dir: bool,
    /// Nesting level, 0 for direct children of the compared folders.
    pub depth: usize,
    pub status: DiffStatus,
    pub left_size: Option<u64>,
    pub right_size: Option<u64>,
    pub left_modified: Option<i64>,
    pub right_modified: Option<i64>,
}

/// Result of Compare Directories.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirCompare {
    /// Rows in tree order: folders before files, names sorted, children after their folder.
    pub entries: Vec<DirDiffEntry>,
    pub truncated: bool,
}

/// FNV-1a digest of a file's bytes; equal sizes and digests count as equal content.
pub fn digest(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

#[derive(Default)]
struct Node {
    dir: bool,
    left: Option<ListedEntry>,
    right: Option<ListedEntry>,
    children: BTreeMap<String, Node>,
}

impl Node {
    fn insert(&mut self, entry: &ListedEntry, left: bool) {
        let mut node = self;
        let parts: Vec<&str> = entry.path.split('/').filter(|p| !p.is_empty()).collect();
        for (i, part) in parts.iter().enumerate() {
            node = node.children.entry((*part).to_string()).or_default();
            if i + 1 < parts.len() {
                // A parent missing from the listing still is a folder on this side.
                node.dir = true;
                let side = if left {
                    &mut node.left
                } else {
                    &mut node.right
                };
                if side.is_none() {
                    *side = Some(ListedEntry::dir(parts[..=i].join("/"), None));
                }
            }
        }
        node.dir |= entry.dir;
        if left {
            node.left = Some(entry.clone());
        } else {
            node.right = Some(entry.clone());
        }
    }
}

/// Compare two listings (paths relative to the compared folders).
pub fn compare_listings(left: &Listing, right: &Listing) -> DirCompare {
    let mut root = Node::default();
    for e in &left.entries {
        root.insert(e, true);
    }
    for e in &right.entries {
        root.insert(e, false);
    }
    let mut entries = Vec::new();
    emit(&root, "", 0, &mut entries);
    DirCompare {
        entries,
        truncated: left.truncated || right.truncated,
    }
}

/// Append the rows of `node`'s children; returns whether all of them are equal.
fn emit(node: &Node, prefix: &str, depth: usize, out: &mut Vec<DirDiffEntry>) -> bool {
    let mut all_equal = true;
    // Folders first, then files, each sorted by name (case-insensitively, like the tree).
    let mut kids: Vec<(&String, &Node)> = node.children.iter().collect();
    kids.sort_by(|a, b| {
        b.1.dir
            .cmp(&a.1.dir)
            .then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase()))
            .then_with(|| a.0.cmp(b.0))
    });
    for (name, child) in kids {
        let path = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        let at = out.len();
        out.push(DirDiffEntry {
            path: path.clone(),
            dir: child.dir,
            depth,
            status: DiffStatus::Equal,
            left_size: child.left.as_ref().filter(|e| !e.dir).map(|e| e.size),
            right_size: child.right.as_ref().filter(|e| !e.dir).map(|e| e.size),
            left_modified: child.left.as_ref().and_then(|e| e.modified),
            right_modified: child.right.as_ref().and_then(|e| e.modified),
        });
        let children_equal = emit(child, &path, depth + 1, out);
        let status = match (&child.left, &child.right) {
            (Some(_), None) => DiffStatus::LeftOnly,
            (None, Some(_)) => DiffStatus::RightOnly,
            (Some(l), Some(r)) if l.dir != r.dir => DiffStatus::Different,
            (Some(l), Some(r)) if !l.dir => {
                if l.size == r.size && l.digest == r.digest {
                    DiffStatus::Equal
                } else {
                    DiffStatus::Different
                }
            }
            _ if children_equal => DiffStatus::Equal,
            _ => DiffStatus::Different,
        };
        out[at].status = status;
        all_equal &= status == DiffStatus::Equal;
    }
    all_equal
}

/// Every file and folder below `root` on this machine (repository metadata skipped,
/// symbolic links not followed: a link compares by its target path).
pub fn list_tree(root: &Path, limit: usize) -> io::Result<Listing> {
    let meta = std::fs::metadata(root)?;
    if !meta.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{} is not a folder", root.display()),
        ));
    }
    let mut listing = Listing::default();
    let mut stack = vec![(root.to_path_buf(), String::new())];
    while let Some((dir, rel)) = stack.pop() {
        let read = match std::fs::read_dir(&dir) {
            Ok(r) => r,
            // Unreadable subfolders are compared as empty.
            Err(_) if !rel.is_empty() => continue,
            Err(e) => return Err(e),
        };
        for item in read.flatten() {
            if listing.entries.len() >= limit {
                listing.truncated = true;
                return Ok(listing);
            }
            let name = item.file_name().to_string_lossy().into_owned();
            let path = if rel.is_empty() {
                name.clone()
            } else {
                format!("{rel}/{name}")
            };
            let Ok(meta) = item.path().symlink_metadata() else {
                continue;
            };
            let modified = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64);
            if meta.is_dir() {
                if SKIPPED_DIRS.contains(&name.as_str()) {
                    continue;
                }
                listing.entries.push(ListedEntry::dir(&path, modified));
                stack.push((item.path(), path));
            } else if meta.file_type().is_symlink() {
                let target = std::fs::read_link(item.path())
                    .map(|t| t.to_string_lossy().into_owned())
                    .unwrap_or_default();
                listing
                    .entries
                    .push(ListedEntry::file(path, target.as_bytes(), modified));
            } else {
                let bytes = std::fs::read(item.path()).unwrap_or_default();
                listing
                    .entries
                    .push(ListedEntry::file(path, &bytes, modified));
            }
        }
    }
    Ok(listing)
}

/// Compare two folders on this machine.
pub fn compare_trees(left: &Path, right: &Path) -> io::Result<DirCompare> {
    let l = list_tree(left, MAX_ENTRIES)?;
    let r = list_tree(right, MAX_ENTRIES)?;
    Ok(compare_listings(&l, &r))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, path: &str, text: &str) {
        let p = root.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }

    fn rows(c: &DirCompare) -> Vec<(String, DiffStatus, usize)> {
        c.entries
            .iter()
            .map(|e| (e.path.clone(), e.status, e.depth))
            .collect()
    }

    #[test]
    fn compares_two_fixture_folders() {
        let tmp = tempfile::tempdir().unwrap();
        let l = tmp.path().join("left");
        let r = tmp.path().join("right");
        write(&l, "same.txt", "x\n");
        write(&r, "same.txt", "x\n");
        write(&l, "changed.txt", "a\n");
        write(&r, "changed.txt", "b\n");
        write(&l, "only-left.txt", "l\n");
        write(&r, "only-right.txt", "r\n");
        write(&l, "src/a.php", "<?php\n");
        write(&r, "src/a.php", "<?php\n");
        write(&l, "lib/x.php", "1");
        write(&r, "lib/x.php", "2");
        write(&l, "old/gone.php", "");
        write(&l, ".git/HEAD", "ref: refs/heads/main\n");
        std::fs::create_dir_all(r.join("empty")).unwrap();

        let c = compare_trees(&l, &r).unwrap();
        assert!(!c.truncated);
        assert_eq!(
            rows(&c),
            vec![
                ("empty".into(), DiffStatus::RightOnly, 0),
                ("lib".into(), DiffStatus::Different, 0),
                ("lib/x.php".into(), DiffStatus::Different, 1),
                ("old".into(), DiffStatus::LeftOnly, 0),
                ("old/gone.php".into(), DiffStatus::LeftOnly, 1),
                ("src".into(), DiffStatus::Equal, 0),
                ("src/a.php".into(), DiffStatus::Equal, 1),
                ("changed.txt".into(), DiffStatus::Different, 0),
                ("only-left.txt".into(), DiffStatus::LeftOnly, 0),
                ("only-right.txt".into(), DiffStatus::RightOnly, 0),
                ("same.txt".into(), DiffStatus::Equal, 0),
            ]
        );
        let changed = c.entries.iter().find(|e| e.path == "changed.txt").unwrap();
        assert_eq!((changed.left_size, changed.right_size), (Some(2), Some(2)));
        assert!(changed.left_modified.is_some());
        let only = c
            .entries
            .iter()
            .find(|e| e.path == "only-left.txt")
            .unwrap();
        assert_eq!(only.right_size, None);
    }

    #[test]
    fn a_file_against_a_folder_differs_and_limits_truncate() {
        let left = Listing {
            entries: vec![ListedEntry::file("x", b"1", None)],
            truncated: false,
        };
        let right = Listing {
            entries: vec![
                ListedEntry::dir("x", None),
                ListedEntry::file("x/y", b"", None),
            ],
            truncated: true,
        };
        let c = compare_listings(&left, &right);
        assert!(c.truncated);
        assert_eq!(c.entries[0].status, DiffStatus::Different);
        assert!(c.entries[0].dir);
        assert_eq!(c.entries[1].status, DiffStatus::RightOnly);

        let tmp = tempfile::tempdir().unwrap();
        for i in 0..5 {
            write(tmp.path(), &format!("f{i}"), "");
        }
        let l = list_tree(tmp.path(), 3).unwrap();
        assert!(l.truncated);
        assert_eq!(l.entries.len(), 3);
        assert!(list_tree(&tmp.path().join("f0"), 3).is_err());
    }

    #[test]
    fn listings_without_parent_rows_still_nest() {
        let left = Listing {
            entries: vec![ListedEntry::file("a/b/c.txt", b"1", None)],
            truncated: false,
        };
        let c = compare_listings(&left, &Listing::default());
        let r = rows(&c);
        assert_eq!(r[0], ("a".into(), DiffStatus::LeftOnly, 0));
        assert_eq!(r[1], ("a/b".into(), DiffStatus::LeftOnly, 1));
        assert_eq!(r[2], ("a/b/c.txt".into(), DiffStatus::LeftOnly, 2));
        // Round trip through JSON as the UI and the agent protocol see it.
        let json = serde_json::to_string(&c).unwrap();
        assert!(json.contains("\"status\":\"leftOnly\""));
        assert_eq!(digest(b""), 0xcbf2_9ce4_8422_2325);
    }
}
