//! Unified patches as `git diff --binary --full-index` prints them: split into one section per
//! file (to shelve, list and unshelve files separately) and apply a section's hunks to the
//! base text (to show a shelved file's diff without touching the working tree).

use serde::{Deserialize, Serialize};

/// How a patch changes a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PatchStatus {
    Added,
    Modified,
    Deleted,
}

/// The section of a patch that changes one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchFile {
    /// Repository-relative path (the new path).
    pub path: String,
    pub status: PatchStatus,
    /// A `GIT binary patch` (no text hunks).
    pub binary: bool,
    /// Object id of the base version (`index <old>..<new>`), absent for new files.
    pub old_blob: Option<String>,
    /// The section text, from `diff --git` to the next one, with its line breaks.
    pub text: String,
}

/// Split a patch into its file sections, in order. Text before the first `diff --git` line is
/// dropped.
pub fn split_patch(patch: &str) -> Vec<PatchFile> {
    let mut sections: Vec<String> = Vec::new();
    for line in patch.split_inclusive('\n') {
        if line.starts_with("diff --git ") {
            sections.push(String::new());
        }
        if let Some(s) = sections.last_mut() {
            s.push_str(line);
        }
    }
    sections.into_iter().map(describe).collect()
}

fn describe(text: String) -> PatchFile {
    let mut status = PatchStatus::Modified;
    let mut binary = false;
    let mut old_blob = None;
    let mut path: Option<String> = None;
    let mut minus: Option<String> = None;
    let header = text
        .lines()
        .next()
        .and_then(|l| l.strip_prefix("diff --git "))
        .unwrap_or_default();
    for line in text.lines().skip(1) {
        if line.starts_with("@@ ") || line == "GIT binary patch" {
            binary |= line == "GIT binary patch";
            break;
        }
        if line.starts_with("new file mode") {
            status = PatchStatus::Added;
        } else if line.starts_with("deleted file mode") {
            status = PatchStatus::Deleted;
        } else if let Some(ids) = line.strip_prefix("index ") {
            let old = ids.split("..").next().unwrap_or_default();
            if !old.is_empty() && !old.trim_start_matches('0').is_empty() {
                old_blob = Some(old.to_string());
            }
        } else if let Some(p) = line.strip_prefix("+++ ") {
            if p != "/dev/null" {
                path = Some(unquote(p).trim_start_matches("b/").to_string());
            }
        } else if let Some(p) = line.strip_prefix("--- ") {
            if p != "/dev/null" {
                minus = Some(unquote(p).trim_start_matches("a/").to_string());
            }
        } else if let Some(p) = line.strip_prefix("rename to ") {
            path = Some(unquote(p));
        } else if line.starts_with("Binary files ") {
            binary = true;
        }
    }
    let path = path
        .or(minus)
        .unwrap_or_else(|| header_path(header).unwrap_or_default());
    if status == PatchStatus::Added {
        old_blob = None;
    }
    PatchFile {
        path,
        status,
        binary,
        old_blob,
        text,
    }
}

/// The `b/` path of a `diff --git a/x b/x` header (both paths are the same without renames).
fn header_path(header: &str) -> Option<String> {
    if let Some(quoted) = header.strip_prefix('"') {
        let end = quoted.find("\" ")? + 2;
        let b = header[end + 1..].trim();
        return Some(unquote(b).trim_start_matches("b/").to_string());
    }
    let rest = header.strip_prefix("a/")?;
    // `a/<p> b/<p>`: the two halves have the same length.
    let len = rest.len().checked_sub(3)? / 2;
    let (a, b) = rest.split_at(len);
    (b.strip_prefix(" b/") == Some(a)).then(|| a.to_string())
}

/// Undo git's C-style quoting of unusual paths (`"a/caf\303\251.php"`).
fn unquote(p: &str) -> String {
    let p = p.trim_end_matches('\t');
    let Some(inner) = p.strip_prefix('"').and_then(|s| s.strip_suffix('"')) else {
        return p.to_string();
    };
    let mut bytes = Vec::new();
    let mut it = inner.bytes().peekable();
    while let Some(b) = it.next() {
        if b != b'\\' {
            bytes.push(b);
            continue;
        }
        match it.next() {
            Some(b'n') => bytes.push(b'\n'),
            Some(b't') => bytes.push(b'\t'),
            Some(b'"') => bytes.push(b'"'),
            Some(b'\\') => bytes.push(b'\\'),
            Some(d @ b'0'..=b'7') => {
                let mut v = u32::from(d - b'0');
                for _ in 0..2 {
                    if let Some(&n @ b'0'..=b'7') = it.peek() {
                        v = v * 8 + u32::from(n - b'0');
                        it.next();
                    }
                }
                bytes.push(v as u8);
            }
            Some(other) => bytes.push(other),
            None => {}
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Concatenate sections into one patch.
pub fn join_patch<'a>(files: impl IntoIterator<Item = &'a PatchFile>) -> String {
    files.into_iter().map(|f| f.text.as_str()).collect()
}

/// Apply the text hunks of one file section to `base` (the file's old content, empty for new
/// files) and return the new content. Line breaks of `base` are kept; `\ No newline at end of
/// file` markers are honoured. Fails on binary sections and when the hunks do not fit `base`.
pub fn apply_file_patch(base: &str, section: &str) -> Result<String, String> {
    let base_lines: Vec<&str> = base.split_inclusive('\n').collect();
    let mut out = String::with_capacity(base.len() + section.len());
    let mut pos = 0usize; // next unread base line
    let mut in_hunk = false;
    // What the last hunk line produced, for `\ No newline at end of file`.
    let mut last_added: Option<usize> = None;
    for line in section.split_inclusive('\n') {
        let body = line.strip_suffix('\n').unwrap_or(line);
        if line == "GIT binary patch\n" || line.starts_with("Binary files ") {
            return Err("binary files cannot be shown as text".into());
        }
        if let Some(h) = body.strip_prefix("@@ ") {
            let (old_start, old_len) = hunk_old_range(h).ok_or("malformed hunk header")?;
            // Lines before the hunk; a pure insertion (`-n,0`) goes after line n.
            let first = if old_len == 0 {
                old_start
            } else {
                old_start.saturating_sub(1)
            };
            if first < pos || first > base_lines.len() {
                return Err("the patch does not fit the base version".into());
            }
            base_lines[pos..first].iter().for_each(|l| out.push_str(l));
            pos = first;
            in_hunk = true;
            last_added = None;
            continue;
        }
        if !in_hunk {
            continue;
        }
        match body.as_bytes().first() {
            Some(b' ') | Some(b'-') => {
                let expected = &body[1..];
                let actual = base_lines
                    .get(pos)
                    .ok_or("the patch is longer than the base")?;
                if actual.trim_end_matches('\n') != expected {
                    return Err(format!(
                        "the patch does not fit the base at line {}",
                        pos + 1
                    ));
                }
                if body.starts_with(' ') {
                    out.push_str(actual);
                }
                pos += 1;
                last_added = None;
            }
            Some(b'+') => {
                out.push_str(&body[1..]);
                out.push('\n');
                last_added = Some(out.len());
            }
            Some(b'\\') => {
                // The previous line had no line break: drop the one we added.
                if last_added == Some(out.len()) {
                    out.pop();
                }
            }
            None if line == "\n" => {
                // An empty context line some tools write without the leading space.
                let actual = base_lines
                    .get(pos)
                    .ok_or("the patch is longer than the base")?;
                out.push_str(actual);
                pos += 1;
                last_added = None;
            }
            _ => in_hunk = false,
        }
    }
    base_lines[pos.min(base_lines.len())..]
        .iter()
        .for_each(|l| out.push_str(l));
    Ok(out)
}

/// `-12,7 +12,9 @@ …` → (12, 7); a missing length means 1.
fn hunk_old_range(h: &str) -> Option<(usize, usize)> {
    let old = h.split_whitespace().next()?.strip_prefix('-')?;
    let (start, len) = old.split_once(',').unwrap_or((old, "1"));
    Some((start.parse().ok()?, len.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PATCH: &str = "\
diff --git a/src/Cart.php b/src/Cart.php
index 1111111111111111111111111111111111111111..2222222222222222222222222222222222222222 100644
--- a/src/Cart.php
+++ b/src/Cart.php
@@ -1,4 +1,5 @@
 <?php
-$a = 1;
+$a = 2;
+$b = 3;
 echo $a;
 echo 'end';
diff --git a/new file.txt b/new file.txt
new file mode 100644
index 0000000000000000000000000000000000000000..3333333333333333333333333333333333333333
--- /dev/null
+++ b/new file.txt
@@ -0,0 +1,2 @@
+hello
+world
\\ No newline at end of file
diff --git a/gone.txt b/gone.txt
deleted file mode 100644
index 4444444444444444444444444444444444444444..0000000000000000000000000000000000000000
--- a/gone.txt
+++ /dev/null
@@ -1 +0,0 @@
-bye
diff --git a/logo.png b/logo.png
index 5555555555555555555555555555555555555555..6666666666666666666666666666666666666666 100644
GIT binary patch
literal 3
KcmZ?wbYK7g00031

literal 3
KcmZ?wbYKJg0002<
";

    #[test]
    fn splits_a_patch_per_file() {
        let files = split_patch(PATCH);
        let summary: Vec<(&str, PatchStatus, bool)> = files
            .iter()
            .map(|f| (f.path.as_str(), f.status, f.binary))
            .collect();
        assert_eq!(
            summary,
            vec![
                ("src/Cart.php", PatchStatus::Modified, false),
                ("new file.txt", PatchStatus::Added, false),
                ("gone.txt", PatchStatus::Deleted, false),
                ("logo.png", PatchStatus::Modified, true),
            ]
        );
        assert_eq!(
            files[0].old_blob.as_deref(),
            Some("1111111111111111111111111111111111111111")
        );
        assert_eq!(files[1].old_blob, None);
        assert_eq!(join_patch(&files), PATCH);
        assert_eq!(join_patch([&files[1]]), files[1].text);
    }

    #[test]
    fn applies_hunks_to_the_base() {
        let files = split_patch(PATCH);
        let base = "<?php\n$a = 1;\necho $a;\necho 'end';\nmore\n";
        assert_eq!(
            apply_file_patch(base, &files[0].text).unwrap(),
            "<?php\n$a = 2;\n$b = 3;\necho $a;\necho 'end';\nmore\n"
        );
        assert_eq!(
            apply_file_patch("", &files[1].text).unwrap(),
            "hello\nworld"
        );
        assert_eq!(apply_file_patch("bye\n", &files[2].text).unwrap(), "");
        assert!(apply_file_patch("other\n", &files[2].text).is_err());
        assert!(apply_file_patch("", &files[3].text).is_err());
    }

    #[test]
    fn keeps_crlf_and_handles_several_hunks() {
        let base = "a\r\nb\r\nc\r\nd\r\ne\r\nf\r\ng\r\nh\r\n";
        let section = "diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1,2 +1,2 @@\n-a\r\n+A\r\n b\r\n@@ -7,2 +7,3 @@\n g\r\n h\r\n+i\r\n";
        assert_eq!(
            apply_file_patch(base, section).unwrap(),
            "A\r\nb\r\nc\r\nd\r\ne\r\nf\r\ng\r\nh\r\ni\r\n"
        );
    }

    #[test]
    fn reads_quoted_and_header_paths() {
        assert_eq!(unquote("\"caf\\303\\251.php\""), "café.php");
        assert_eq!(
            header_path("a/x y.php b/x y.php").as_deref(),
            Some("x y.php")
        );
        let mode_only = "diff --git a/run.sh b/run.sh\nold mode 100644\nnew mode 100755\n";
        assert_eq!(split_patch(mode_only)[0].path, "run.sh");
    }
}
