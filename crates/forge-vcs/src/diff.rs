//! Parser for unified diffs as printed by `git diff`.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LineKind {
    Context,
    Added,
    Removed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffLine {
    pub kind: LineKind,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hunk {
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    /// Text after the second `@@` (usually the enclosing function).
    pub header: String,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDiff {
    pub path: String,
    pub old_path: Option<String>,
    pub binary: bool,
    pub new_file: bool,
    pub deleted_file: bool,
    pub hunks: Vec<Hunk>,
}

pub fn parse_diff(out: &str) -> Vec<FileDiff> {
    let mut files: Vec<FileDiff> = Vec::new();
    for line in out.lines() {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            let path = rest
                .rsplit_once(" b/")
                .map(|(_, b)| b.to_string())
                .unwrap_or_default();
            files.push(FileDiff {
                path,
                ..FileDiff::default()
            });
            continue;
        }
        let Some(file) = files.last_mut() else {
            continue;
        };
        if let Some(h) = line.strip_prefix("@@ ") {
            if let Some(hunk) = parse_hunk_header(h) {
                file.hunks.push(hunk);
            }
        } else if let Some(hunk) = file.hunks.last_mut() {
            let (kind, text) = match line.as_bytes().first() {
                Some(b'+') => (LineKind::Added, &line[1..]),
                Some(b'-') => (LineKind::Removed, &line[1..]),
                Some(b' ') => (LineKind::Context, &line[1..]),
                Some(b'\\') => continue, // "\ No newline at end of file"
                None => (LineKind::Context, ""),
                _ => continue,
            };
            hunk.lines.push(DiffLine {
                kind,
                text: text.to_string(),
            });
        } else if line.starts_with("new file mode") {
            file.new_file = true;
        } else if line.starts_with("deleted file mode") {
            file.deleted_file = true;
        } else if let Some(p) = line.strip_prefix("rename from ") {
            file.old_path = Some(p.to_string());
        } else if let Some(p) = line.strip_prefix("rename to ") {
            file.path = p.to_string();
        } else if let Some(p) = line.strip_prefix("+++ b/") {
            file.path = p.to_string();
        } else if line.starts_with("Binary files ") {
            file.binary = true;
        }
    }
    files
}

/// `-12,7 +12,9 @@ fn name` (the leading `@@ ` already stripped).
fn parse_hunk_header(h: &str) -> Option<Hunk> {
    let (ranges, header) = h.split_once(" @@").unwrap_or((h, ""));
    let mut it = ranges.split_whitespace();
    let (old_start, old_lines) = parse_range(it.next()?.strip_prefix('-')?);
    let (new_start, new_lines) = parse_range(it.next()?.strip_prefix('+')?);
    Some(Hunk {
        old_start,
        old_lines,
        new_start,
        new_lines,
        header: header.trim().to_string(),
        lines: Vec::new(),
    })
}

fn parse_range(r: &str) -> (u32, u32) {
    match r.split_once(',') {
        Some((s, n)) => (s.parse().unwrap_or(0), n.parse().unwrap_or(0)),
        None => (r.parse().unwrap_or(0), 1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_files_and_hunks() {
        let out = "\
diff --git a/a.php b/a.php
index 111..222 100644
--- a/a.php
+++ b/a.php
@@ -1,3 +1,4 @@ class A
 <?php
-echo 1;
+echo 2;
+echo 3;

\\ No newline at end of file
diff --git a/new.php b/new.php
new file mode 100644
--- /dev/null
+++ b/new.php
@@ -0 +1 @@
+x
diff --git a/img.png b/img.png
Binary files a/img.png and b/img.png differ
";
        let files = parse_diff(out);
        assert_eq!(files.len(), 3);
        let a = &files[0];
        assert_eq!(a.path, "a.php");
        assert_eq!(a.hunks.len(), 1);
        let h = &a.hunks[0];
        assert_eq!(
            (h.old_start, h.old_lines, h.new_start, h.new_lines),
            (1, 3, 1, 4)
        );
        assert_eq!(h.header, "class A");
        assert_eq!(h.lines.len(), 5);
        assert_eq!(h.lines[1].kind, LineKind::Removed);
        assert_eq!(h.lines[2].text, "echo 2;");
        assert!(files[1].new_file);
        assert_eq!(files[1].hunks[0].new_lines, 1);
        assert!(files[2].binary);
    }
}
