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
    /// The old path of a renamed file (`rename from`).
    pub old_path: Option<String>,
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

/// [`split_patch`] for a patch kept as raw bytes: each file section with its exact bytes
/// (files that are not UTF-8 keep their encoding; the [`PatchFile`] text is lossy).
pub fn split_patch_bytes(patch: &[u8]) -> Vec<(PatchFile, Vec<u8>)> {
    let mut sections: Vec<Vec<u8>> = Vec::new();
    for line in patch.split_inclusive(|&b| b == b'\n') {
        if line.starts_with(b"diff --git ") {
            sections.push(Vec::new());
        }
        if let Some(s) = sections.last_mut() {
            s.extend_from_slice(line);
        }
    }
    sections
        .into_iter()
        .map(|raw| (describe(String::from_utf8_lossy(&raw).into_owned()), raw))
        .collect()
}

fn describe(text: String) -> PatchFile {
    let mut status = PatchStatus::Modified;
    let mut binary = false;
    let mut old_blob = None;
    let mut path: Option<String> = None;
    let mut minus: Option<String> = None;
    let mut renamed_from: Option<String> = None;
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
        } else if let Some(p) = line.strip_prefix("rename from ") {
            renamed_from = Some(unquote(p));
        } else if let Some(p) = line.strip_prefix("rename to ") {
            path = Some(unquote(p));
        } else if line.starts_with("Binary files ") {
            binary = true;
        }
    }
    let old_path = renamed_from.or_else(|| minus.clone().filter(|m| path.as_ref() != Some(m)));
    let old_path = old_path.filter(|_| status == PatchStatus::Modified);
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
        old_path,
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

// ---- foreign patches, path prefixes and the Apply Patch preview ------------------------

/// What a line of a patch is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LineRole {
    /// File headers and anything between sections.
    Header,
    /// `@@` lines and the hunk body.
    Hunk,
}

/// Classify every line of a patch (with its line break): hunk bodies are counted by their
/// `@@ -a,b +c,d @@` lengths, so a removed line that starts with `--- ` is not a header.
fn line_roles(patch: &str) -> Vec<(LineRole, &str)> {
    let mut out = Vec::new();
    let (mut old_left, mut new_left) = (0usize, 0usize);
    for line in patch.split_inclusive('\n') {
        let body = line.trim_end_matches(['\n', '\r']);
        if old_left > 0 || new_left > 0 {
            match body.as_bytes().first() {
                Some(b' ') | None => {
                    old_left = old_left.saturating_sub(1);
                    new_left = new_left.saturating_sub(1);
                }
                Some(b'-') => old_left = old_left.saturating_sub(1),
                Some(b'+') => new_left = new_left.saturating_sub(1),
                Some(b'\\') => {}
                _ => {
                    old_left = 0;
                    new_left = 0;
                }
            }
            if old_left > 0
                || new_left > 0
                || matches!(
                    body.as_bytes().first(),
                    Some(b' ' | b'-' | b'+' | b'\\') | None
                )
            {
                out.push((LineRole::Hunk, line));
                continue;
            }
        }
        if let Some(h) = body.strip_prefix("@@ ") {
            let mut ranges = h.split_whitespace();
            let len = |r: Option<&str>| -> usize {
                r.and_then(|r| r.split_once(',').map(|(_, l)| l).or(Some("1")))
                    .and_then(|l| l.parse().ok())
                    .unwrap_or(0)
            };
            old_left = len(ranges.next());
            new_left = len(ranges.next());
            out.push((LineRole::Hunk, line));
            continue;
        }
        if body.starts_with('\\') {
            out.push((LineRole::Hunk, line));
            continue;
        }
        out.push((LineRole::Header, line));
    }
    out
}

/// A path of a `---`/`+++` line without quotes and timestamps (`diff -u` writes
/// `name<TAB>date`).
fn header_file(p: &str) -> String {
    let p = p.trim_end_matches(['\n', '\r']);
    let p = if p.starts_with('"') {
        p
    } else {
        p.split('\t').next().unwrap_or(p)
    };
    unquote(p)
}

/// Bring a unified diff from any tool into git's form, so it can be split and applied like
/// git's own patches: sections without a `diff --git` header (`diff -u`, `svn diff`,
/// PHPStorm patches) get one, and their paths get `a/` and `b/` prefixes. Git sections are
/// kept as they are; text outside sections (mail headers, `Index:` lines) is dropped.
pub fn normalize_patch(patch: &str) -> String {
    let roles = line_roles(patch);
    let mut out = String::with_capacity(patch.len() + 256);
    // Inside a git section (its lines are kept as they are) …
    let mut in_git = false;
    // … whose own `---`/`+++` pair has not been seen yet.
    let mut git_pending = false;
    let mut i = 0;
    while i < roles.len() {
        let (role, line) = roles[i];
        if role == LineRole::Hunk {
            out.push_str(line);
            git_pending = false;
            i += 1;
            continue;
        }
        if line.starts_with("diff --git ") {
            in_git = true;
            git_pending = true;
            out.push_str(line);
            i += 1;
            continue;
        }
        let plus = roles
            .get(i + 1)
            .map(|r| r.1)
            .filter(|n| line.starts_with("--- ") && n.starts_with("+++ "));
        if let Some(plus) = plus {
            if in_git && git_pending {
                out.push_str(line);
                out.push_str(plus);
                git_pending = false;
                i += 2;
                continue;
            }
            in_git = false;
            let old = header_file(&line[4..]);
            let new = header_file(&plus[4..]);
            let (a, b) = git_paths(&old, &new);
            let shown = if b == "/dev/null" { &a } else { &b };
            out.push_str(&format!("diff --git a/{shown} b/{shown}\n"));
            if a == "/dev/null" {
                out.push_str("new file mode 100644\n--- /dev/null\n");
            } else {
                if b == "/dev/null" {
                    out.push_str("deleted file mode 100644\n");
                }
                out.push_str(&format!("--- a/{a}\n"));
            }
            if b == "/dev/null" {
                out.push_str("+++ /dev/null\n");
            } else {
                out.push_str(&format!("+++ b/{b}\n"));
            }
            i += 2;
            continue;
        }
        if in_git {
            if line.starts_with("GIT binary patch") || line.starts_with("Binary files ") {
                git_pending = false;
            }
            out.push_str(line);
        }
        i += 1;
    }
    out
}

/// The `a/` and `b/` paths of a foreign header pair, without prefixes: `a/x` and `b/x` lose
/// them, `orig/x` and `new/x` (`diff -ru orig new`) lose the differing first component.
fn git_paths(old: &str, new: &str) -> (String, String) {
    let strip = |p: &str, prefix: &str| p.strip_prefix(prefix).unwrap_or(p).to_string();
    if old == "/dev/null" || new == "/dev/null" {
        let first = |p: &str| -> String {
            if p == "/dev/null" {
                return p.into();
            }
            if p.starts_with("a/") || p.starts_with("b/") {
                p[2..].into()
            } else {
                p.into()
            }
        };
        return (first(old), first(new));
    }
    if old.starts_with("a/") && new.starts_with("b/") {
        return (strip(old, "a/"), strip(new, "b/"));
    }
    let head = |p: &str| {
        p.split_once('/')
            .map(|(h, rest)| (h.to_string(), rest.to_string()))
    };
    match (head(old), head(new)) {
        (Some((ha, ra)), Some((hb, rb))) if ha != hb && ra == rb => (ra, rb),
        _ => (old.to_string(), new.to_string()),
    }
}

/// Rewrite the paths of a git patch made at the repository root so they are relative to
/// `prefix` (`"sub/dir/"`, the project inside the repository): Create Patch writes paths
/// relative to the project root, like PHPStorm. Hunk lines are left alone.
pub fn strip_path_prefix(patch: &str, prefix: &str) -> String {
    if prefix.is_empty() {
        return patch.to_string();
    }
    let a = format!("a/{prefix}");
    let b = format!("b/{prefix}");
    let mut out = String::with_capacity(patch.len());
    for (role, line) in line_roles(patch) {
        if role == LineRole::Hunk {
            out.push_str(line);
            continue;
        }
        let rewritten = if let Some(rest) = line.strip_prefix("diff --git ") {
            format!(
                "diff --git {}",
                rest.replacen(&a, "a/", 1).replacen(&b, "b/", 1)
            )
        } else if let Some(rest) = line.strip_prefix("--- ") {
            format!("--- {}", rest.replacen(&a, "a/", 1))
        } else if let Some(rest) = line.strip_prefix("+++ ") {
            format!("+++ {}", rest.replacen(&b, "b/", 1))
        } else {
            let mut l = line.to_string();
            for key in ["rename from ", "rename to ", "copy from ", "copy to "] {
                if let Some(rest) = line.strip_prefix(key) {
                    l = format!("{key}{}", rest.strip_prefix(prefix).unwrap_or(rest));
                }
            }
            l
        };
        out.push_str(&rewritten);
    }
    out
}

/// One file of the Apply Patch dialog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchPreviewFile {
    /// Target path, relative to the project root (the base directory applied).
    pub path: String,
    /// Where the file was before a rename.
    pub old_path: Option<String>,
    pub status: PatchStatus,
    pub binary: bool,
    pub additions: usize,
    pub deletions: usize,
    /// The file as it is now (`None`: it does not exist).
    pub before: Option<String>,
    /// The file with the patch applied, when it applies.
    pub after: Option<String>,
    /// The patch fits the file as it is now. Files that do not fit are merged three-way.
    pub applies: bool,
    /// Why it does not apply.
    pub message: Option<String>,
}

/// Join a base directory (`""` or `"sub/dir"`) and a patch path.
pub fn join_base(base: &str, path: &str) -> String {
    let base = base.trim_matches('/');
    if base.is_empty() {
        path.to_string()
    } else {
        format!("{base}/{path}")
    }
}

/// Preview a patch against the files under `base`: per file the current text (`read`), the
/// patched text and whether the patch fits. Works for git and foreign unified diffs.
pub fn preview_patch(
    patch: &str,
    base: &str,
    read: &dyn Fn(&str) -> Option<String>,
) -> Vec<PatchPreviewFile> {
    split_patch(&normalize_patch(patch))
        .into_iter()
        .map(|f| {
            let (additions, deletions) = count_changes(&f.text);
            let path = join_base(base, &f.path);
            let old_path = f.old_path.as_deref().map(|p| join_base(base, p));
            let before = read(old_path.as_deref().unwrap_or(&path));
            let mut file = PatchPreviewFile {
                path,
                old_path,
                status: f.status,
                binary: f.binary,
                additions,
                deletions,
                before: None,
                after: None,
                applies: false,
                message: None,
            };
            let fit = match (f.status, &before) {
                _ if f.binary => Err("binary patch".to_string()),
                (PatchStatus::Added, Some(_)) => Err("the file already exists".into()),
                (PatchStatus::Added, None) => apply_file_patch("", &f.text),
                (_, None) => Err("the file does not exist".into()),
                (_, Some(text)) => apply_file_patch(text, &f.text),
            };
            match fit {
                Ok(after) => {
                    file.applies = true;
                    file.after = (f.status != PatchStatus::Deleted).then_some(after);
                }
                Err(e) => {
                    // Binary patches are left to git.
                    file.applies = f.binary;
                    file.message = Some(e);
                }
            }
            file.before = before;
            file
        })
        .collect()
}

/// Added and removed lines of a file section.
fn count_changes(section: &str) -> (usize, usize) {
    line_roles(section)
        .into_iter()
        .filter(|(r, l)| *r == LineRole::Hunk && !l.starts_with("@@"))
        .fold((0, 0), |(a, d), (_, l)| match l.as_bytes().first() {
            Some(b'+') => (a + 1, d),
            Some(b'-') => (a, d + 1),
            _ => (a, d),
        })
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
    fn normalizes_foreign_patches() {
        let plain = "Index: src/a.php\n===\n--- src/a.php\t2026-01-01 10:00\n+++ src/a.php\t2026-01-02 10:00\n@@ -1,2 +1,2 @@\n-x\n--- y\n+x2\n+--- y\n--- /dev/null\n+++ new.txt\n@@ -0,0 +1 @@\n+hi\n";
        let git = normalize_patch(plain);
        let files = split_patch(&git);
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, "src/a.php");
        assert!(files[0].text.contains("--- a/src/a.php\n+++ b/src/a.php\n"));
        assert!(files[0].text.contains("\n--- y\n+x2\n+--- y\n"));
        assert_eq!(files[1].path, "new.txt");
        assert_eq!(files[1].status, PatchStatus::Added);
        assert_eq!(normalize_patch(PATCH), PATCH);
        // A git patch followed by a foreign section.
        let mixed = format!("{PATCH}--- a/x.txt\n+++ b/x.txt\n@@ -1 +1 @@\n-a\n+b\n");
        let files = split_patch(&normalize_patch(&mixed));
        assert_eq!(files.len(), 5);
        assert_eq!(files[3].text, split_patch(PATCH)[3].text);
        assert_eq!(files[4].path, "x.txt");
        let ru = normalize_patch("--- orig/x.txt\n+++ new/x.txt\n@@ -1 +1 @@\n-a\n+b\n");
        assert_eq!(split_patch(&ru)[0].path, "x.txt");
    }

    #[test]
    fn strips_the_project_prefix() {
        let patch = "diff --git a/web/x.php b/web/x.php\n--- a/web/x.php\n+++ b/web/x.php\n@@ -1 +1 @@\n-a/web/\n+b/web/\n";
        assert_eq!(
            strip_path_prefix(patch, "web/"),
            "diff --git a/x.php b/x.php\n--- a/x.php\n+++ b/x.php\n@@ -1 +1 @@\n-a/web/\n+b/web/\n"
        );
    }

    #[test]
    fn previews_a_patch() {
        let read = |p: &str| match p {
            "src/Cart.php" => Some("<?php\n$a = 1;\necho $a;\necho 'end';\n".to_string()),
            "gone.txt" => Some("other\n".to_string()),
            _ => None,
        };
        let files = preview_patch(PATCH, "", &read);
        assert_eq!(files.len(), 4);
        assert!(files[0].applies);
        assert_eq!((files[0].additions, files[0].deletions), (2, 1));
        assert_eq!(
            files[0].after.as_deref(),
            Some("<?php\n$a = 2;\n$b = 3;\necho $a;\necho 'end';\n")
        );
        assert!(files[1].applies);
        assert_eq!(files[1].after.as_deref(), Some("hello\nworld"));
        assert!(!files[2].applies, "the deleted file's content differs");
        assert!(files[2].message.is_some());
        assert!(files[3].binary && files[3].applies);
        let based = preview_patch(PATCH, "sub/", &|_| None);
        assert_eq!(based[0].path, "sub/src/Cart.php");
        assert!(!based[0].applies);
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
