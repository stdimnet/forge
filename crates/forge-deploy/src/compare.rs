//! Comparing project files with deployed ones: the folder diff of Sync with Deployed
//! (`forge_diff::compare_listings`, by content, size or timestamp) and the per-file checks
//! of a transfer ("Overwrite up-to-date files", "Warn when uploading over newer file").

use serde::{Deserialize, Serialize};

use crate::options::{CompareBy, WarnNewer};

/// A file or folder on one side, its path relative to the compared folder (or, from a
/// server, relative to the root path).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Entry {
    pub path: String,
    pub dir: bool,
    pub size: u64,
    /// Modification time, seconds since the Unix epoch, when the side knows it.
    pub modified: Option<i64>,
    /// Content digest ([`forge_diff::digest`]) when it was read.
    pub digest: Option<u64>,
}

impl Entry {
    pub fn file(path: impl Into<String>, size: u64, modified: Option<i64>) -> Self {
        Self {
            path: path.into(),
            dir: false,
            size,
            modified,
            digest: None,
        }
    }

    pub fn dir(path: impl Into<String>, modified: Option<i64>) -> Self {
        Self {
            path: path.into(),
            dir: true,
            modified,
            ..Self::default()
        }
    }
}

/// The listing `forge_diff` compares: equal sizes and "digests" count as equal, so the
/// digest carries what `by` compares besides the size.
pub fn listing(entries: &[Entry], by: CompareBy) -> forge_diff::Listing {
    forge_diff::Listing {
        entries: entries
            .iter()
            .map(|e| forge_diff::ListedEntry {
                path: e.path.clone(),
                dir: e.dir,
                size: if e.dir { 0 } else { e.size },
                digest: match (e.dir, by) {
                    (true, _) | (_, CompareBy::Size) => 0,
                    (_, CompareBy::Content) => e.digest.unwrap_or(0),
                    (_, CompareBy::Timestamp) => e.modified.unwrap_or(0) as u64,
                },
                modified: e.modified,
            })
            .collect(),
        truncated: false,
    }
}

/// The folder diff of the project side (left) and the server side (right).
pub fn compare(local: &[Entry], remote: &[Entry], by: CompareBy) -> forge_diff::DirCompare {
    forge_diff::compare_listings(&listing(local, by), &listing(remote, by))
}

/// "Overwrite up-to-date files" off: the target has the same size and is not older than
/// the source (with unknown times, the same size and content when both digests are known).
pub fn is_up_to_date(source: &Entry, target: &Entry) -> bool {
    if source.dir || target.dir || source.size != target.size {
        return false;
    }
    if let (Some(a), Some(b)) = (source.digest, target.digest) {
        if a != b {
            return false;
        }
    }
    match (source.modified, target.modified) {
        (Some(s), Some(t)) => t >= s,
        _ => source.digest.is_some() && source.digest == target.digest,
    }
}

/// "Warn when uploading over newer file": the target was changed after the source and
/// differs from it (by size, or by content when `contents_differ` is known).
pub fn is_newer_on_target(
    source: &Entry,
    target: &Entry,
    warn: WarnNewer,
    contents_differ: Option<bool>,
) -> bool {
    let newer = matches!((source.modified, target.modified), (Some(s), Some(t)) if t > s);
    if !newer || source.dir || target.dir {
        return false;
    }
    match warn {
        WarnNewer::No => false,
        WarnNewer::TimestampAndSize => source.size != target.size,
        WarnNewer::Content => contents_differ.unwrap_or(source.size != target.size),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_diff::DiffStatus;

    fn file(path: &str, size: u64, modified: i64, digest: u64) -> Entry {
        Entry {
            digest: Some(digest),
            ..Entry::file(path, size, Some(modified))
        }
    }

    fn status(d: &forge_diff::DirCompare, path: &str) -> DiffStatus {
        d.entries.iter().find(|e| e.path == path).unwrap().status
    }

    #[test]
    fn compares_by_content_size_or_timestamp() {
        let local = vec![
            Entry::dir("app", Some(1)),
            file("app/a.php", 10, 100, 1),
            file("app/b.php", 10, 100, 2),
            file("app/c.php", 10, 100, 3),
            file("only-local.txt", 1, 1, 1),
        ];
        let remote = vec![
            Entry::dir("app", Some(5)),
            file("app/a.php", 10, 100, 1),
            file("app/b.php", 10, 100, 9),
            file("app/c.php", 10, 200, 3),
            file("only-remote.txt", 1, 1, 1),
        ];
        let by_content = compare(&local, &remote, CompareBy::Content);
        assert_eq!(status(&by_content, "app/a.php"), DiffStatus::Equal);
        assert_eq!(status(&by_content, "app/b.php"), DiffStatus::Different);
        assert_eq!(status(&by_content, "app/c.php"), DiffStatus::Equal);
        assert_eq!(status(&by_content, "app"), DiffStatus::Different);
        assert_eq!(status(&by_content, "only-local.txt"), DiffStatus::LeftOnly);
        assert_eq!(
            status(&by_content, "only-remote.txt"),
            DiffStatus::RightOnly
        );
        let by_size = compare(&local, &remote, CompareBy::Size);
        assert_eq!(status(&by_size, "app/b.php"), DiffStatus::Equal);
        let by_time = compare(&local, &remote, CompareBy::Timestamp);
        assert_eq!(status(&by_time, "app/b.php"), DiffStatus::Equal);
        assert_eq!(status(&by_time, "app/c.php"), DiffStatus::Different);
        assert_eq!(by_time.entries[0].right_modified, Some(5));
    }

    #[test]
    fn up_to_date_and_newer_checks() {
        let src = file("a", 10, 100, 1);
        assert!(is_up_to_date(&src, &file("a", 10, 100, 1)));
        assert!(is_up_to_date(&src, &file("a", 10, 150, 1)));
        assert!(!is_up_to_date(&src, &file("a", 10, 50, 1)));
        assert!(!is_up_to_date(&src, &file("a", 11, 150, 1)));
        assert!(!is_up_to_date(&src, &file("a", 10, 150, 2)));
        let no_times = Entry::file("a", 10, None);
        assert!(!is_up_to_date(&no_times, &Entry::file("a", 10, None)));

        let newer_other_size = file("a", 12, 200, 5);
        assert!(is_newer_on_target(
            &src,
            &newer_other_size,
            WarnNewer::TimestampAndSize,
            None
        ));
        assert!(!is_newer_on_target(
            &src,
            &newer_other_size,
            WarnNewer::No,
            None
        ));
        let newer_same_size = file("a", 10, 200, 5);
        assert!(!is_newer_on_target(
            &src,
            &newer_same_size,
            WarnNewer::TimestampAndSize,
            None
        ));
        assert!(is_newer_on_target(
            &src,
            &newer_same_size,
            WarnNewer::Content,
            Some(true)
        ));
        assert!(!is_newer_on_target(
            &src,
            &newer_same_size,
            WarnNewer::Content,
            Some(false)
        ));
        assert!(!is_newer_on_target(
            &src,
            &file("a", 12, 50, 5),
            WarnNewer::Content,
            Some(true)
        ));
    }
}
