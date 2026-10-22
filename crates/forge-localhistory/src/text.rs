//! Text helpers: following a block of lines back through revisions (Show History for
//! Selection) and unified diffs (Create Patch).

use serde::{Deserialize, Serialize};
use similar::{DiffTag, TextDiff};

/// Lines with their line breaks.
pub fn lines(text: &str) -> Vec<&str> {
    text.split_inclusive('\n').collect()
}

/// The lines `start..end` (0-based, end exclusive) of `text`.
pub fn block(text: &str, start: usize, end: usize) -> String {
    let l = lines(text);
    let end = end.min(l.len());
    let start = start.min(end);
    l[start..end].concat()
}

/// Map the line range `start..end` of `newer` to the matching range of `older`. Lines that
/// changed in between widen the range to the whole changed hunk.
pub fn map_range(older: &str, newer: &str, start: usize, end: usize) -> (usize, usize) {
    let diff = TextDiff::from_lines(older, newer);
    let ops = diff.ops();
    let old_len = lines(older).len();
    let new_len = lines(newer).len();
    let mut s = if start >= new_len { old_len } else { 0 };
    let mut e = if end == 0 { 0 } else { old_len };
    let mut start_found = start >= new_len;
    let mut end_found = end == 0;
    for op in ops {
        let (tag, o, n) = op.as_tag_tuple();
        if !start_found && n.start <= start && start < n.end {
            s = if tag == DiffTag::Equal {
                o.start + (start - n.start)
            } else {
                o.start
            };
            start_found = true;
        }
        if !end_found && n.start < end && end <= n.end {
            e = if tag == DiffTag::Equal {
                o.start + (end - n.start)
            } else {
                o.end
            };
            end_found = true;
        }
    }
    (s, e.max(s))
}

/// One file of a patch: its content before and after (`None` = the file does not exist).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchFile {
    pub path: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

/// A git-style unified diff of the files (`git apply` and Apply Patch read it).
pub fn unified_patch(files: &[PatchFile]) -> String {
    let mut out = String::new();
    for f in files {
        if f.before == f.after {
            continue;
        }
        let before = f.before.as_deref().unwrap_or("");
        let after = f.after.as_deref().unwrap_or("");
        out.push_str(&format!("diff --git a/{0} b/{0}\n", f.path));
        if f.before.is_none() {
            out.push_str("new file mode 100644\n");
        } else if f.after.is_none() {
            out.push_str("deleted file mode 100644\n");
        }
        let a = if f.before.is_some() {
            format!("a/{}", f.path)
        } else {
            "/dev/null".into()
        };
        let b = if f.after.is_some() {
            format!("b/{}", f.path)
        } else {
            "/dev/null".into()
        };
        let diff = TextDiff::from_lines(before, after);
        let text = diff
            .unified_diff()
            .context_radius(3)
            .missing_newline_hint(true)
            .header(&a, &b)
            .to_string();
        out.push_str(&text);
        if !out.ends_with('\n') {
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_follow_inserted_and_changed_lines() {
        let older = "a\nb\nc\nd\n";
        let newer = "x\na\nb\nC\nd\n";
        // "b" and "C" (lines 2..4 in newer) were "b" and "c" (1..3) before.
        assert_eq!(map_range(older, newer, 2, 4), (1, 3));
        assert_eq!(block(older, 1, 3), "b\nc\n");
        // The inserted line maps to an empty range.
        assert_eq!(map_range(older, newer, 0, 1), (0, 0));
        // Unchanged text maps one to one.
        assert_eq!(map_range(newer, newer, 1, 3), (1, 3));
        // Past the end.
        assert_eq!(map_range(older, newer, 5, 5), (4, 4));
    }

    #[test]
    fn patches_have_git_headers() {
        let p = unified_patch(&[
            PatchFile {
                path: "a.txt".into(),
                before: Some("one\ntwo\n".into()),
                after: Some("one\n2\n".into()),
            },
            PatchFile {
                path: "new.txt".into(),
                before: None,
                after: Some("hi\n".into()),
            },
            PatchFile {
                path: "same.txt".into(),
                before: Some("x".into()),
                after: Some("x".into()),
            },
        ]);
        assert!(p.contains("diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1,2 +1,2 @@\n one\n-two\n+2\n"));
        assert!(
            p.contains("new file mode 100644\n--- /dev/null\n+++ b/new.txt\n@@ -0,0 +1 @@\n+hi\n")
        );
        assert!(!p.contains("same.txt"));
    }
}
