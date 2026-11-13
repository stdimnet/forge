//! The agent's view of its sandbox: every path is confined to the sandbox root (no absolute
//! paths outside it, no `..` escapes, no symlinks pointing out, never `.git`), files matched
//! by `.aiignore` stay invisible, and writes are limited to the task's scope.

use std::path::{Component, Path, PathBuf};

use forge_ai_editor::AiIgnore;

/// Folders that are listed but not descended into (dependencies and build output).
const HEAVY_DIRS: [&str; 7] = [
    "node_modules",
    "vendor",
    "target",
    ".idea",
    "dist",
    "build",
    "__pycache__",
];

/// Most entries `list_files` returns.
const LIST_LIMIT: usize = 400;
/// Most hits `search` returns.
const SEARCH_LIMIT: usize = 100;
/// Files larger than this are not searched or read.
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

/// A sandbox as the agent sees it.
#[derive(Debug, Clone)]
pub struct Workspace {
    root: PathBuf,
    ignore: AiIgnore,
    /// Project-relative files and folders the agent may change; empty: everything.
    scope: Vec<String>,
}

impl Workspace {
    pub fn new(root: impl Into<PathBuf>, ignore: AiIgnore, scope: Vec<String>) -> Self {
        let scope = scope
            .iter()
            .filter_map(|s| normalize(s).ok())
            .filter(|s| !s.is_empty())
            .collect();
        Self {
            root: root.into(),
            ignore,
            scope,
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn scope(&self) -> &[String] {
        &self.scope
    }

    /// The sandbox-relative form of `path` (relative, or absolute inside the sandbox).
    pub fn relative(&self, path: &str) -> Result<String, String> {
        let p = path.trim().replace('\\', "/");
        if is_absolute(&p) {
            let root = self.root.to_string_lossy().replace('\\', "/");
            let root = root.trim_end_matches('/');
            let rest = p
                .strip_prefix(root)
                .filter(|r| r.is_empty() || r.starts_with('/'))
                .ok_or_else(|| format!("{path} is outside the sandbox"))?;
            return normalize(rest.trim_start_matches('/'));
        }
        normalize(&p)
    }

    /// Resolve `path` to a file system path inside the sandbox, refusing escapes through
    /// symlinks and files excluded by `.aiignore`.
    pub fn resolve(&self, path: &str) -> Result<(String, PathBuf), String> {
        let rel = self.relative(path)?;
        if !rel.is_empty() && self.ignore.is_ignored(&rel) {
            return Err(format!("{rel} is excluded from AI by .aiignore"));
        }
        let full = if rel.is_empty() {
            self.root.clone()
        } else {
            self.root.join(&rel)
        };
        // The deepest existing ancestor must stay inside the sandbox once symlinks resolve.
        let root = self
            .root
            .canonicalize()
            .map_err(|e| format!("sandbox is gone: {e}"))?;
        let mut probe = full.as_path();
        loop {
            if let Ok(real) = probe.canonicalize() {
                if !real.starts_with(&root) {
                    return Err(format!("{rel} points outside the sandbox"));
                }
                break;
            }
            match probe.parent() {
                Some(p) => probe = p,
                None => break,
            }
        }
        Ok((rel, full))
    }

    /// Whether the agent may change `rel` (a normalized relative path).
    pub fn in_scope(&self, rel: &str) -> bool {
        self.scope.is_empty()
            || self
                .scope
                .iter()
                .any(|s| rel == s || rel.starts_with(&format!("{s}/")))
    }

    fn writable(&self, path: &str) -> Result<(String, PathBuf), String> {
        let (rel, full) = self.resolve(path)?;
        if rel.is_empty() {
            return Err("a file path is required".into());
        }
        if !self.in_scope(&rel) {
            return Err(format!(
                "{rel} is outside the task's scope ({})",
                self.scope.join(", ")
            ));
        }
        Ok((rel, full))
    }

    /// Files and folders below `path` (folders end with `/`), breadth first.
    pub fn list(&self, path: &str) -> Result<String, String> {
        let (rel, full) = self.resolve(path)?;
        if !full.is_dir() {
            return Err(format!("{} is not a folder", display(&rel)));
        }
        let mut out = Vec::new();
        let mut queue = vec![(rel.clone(), full)];
        let mut truncated = false;
        while let Some((dir_rel, dir)) = (!queue.is_empty()).then(|| queue.remove(0)) {
            let mut entries: Vec<_> = match std::fs::read_dir(&dir) {
                Ok(rd) => rd.flatten().collect(),
                Err(_) => continue,
            };
            entries.sort_by_key(|e| e.file_name());
            for e in entries {
                let name = e.file_name().to_string_lossy().into_owned();
                if name == ".git" {
                    continue;
                }
                let child = join(&dir_rel, &name);
                if self.ignore.is_ignored(&child) {
                    continue;
                }
                let is_dir = e.file_type().is_ok_and(|t| t.is_dir());
                if out.len() >= LIST_LIMIT {
                    truncated = true;
                    break;
                }
                if is_dir {
                    out.push(format!("{child}/"));
                    if !HEAVY_DIRS.contains(&name.as_str()) {
                        queue.push((child, e.path()));
                    }
                } else {
                    out.push(child);
                }
            }
            if truncated {
                break;
            }
        }
        if out.is_empty() {
            return Ok(format!("{} is empty", display(&rel)));
        }
        if truncated {
            out.push(format!(
                "… (more than {LIST_LIMIT} entries; list a subfolder)"
            ));
        }
        Ok(out.join("\n"))
    }

    /// A file's text, optionally lines `start..=end` (1-based).
    pub fn read(
        &self,
        path: &str,
        start: Option<usize>,
        end: Option<usize>,
    ) -> Result<String, String> {
        let (rel, full) = self.resolve(path)?;
        if full.is_dir() {
            return Err(format!("{rel} is a folder; use list_files"));
        }
        let meta = std::fs::metadata(&full).map_err(|_| format!("{rel} does not exist"))?;
        if meta.len() > MAX_FILE_BYTES {
            return Err(format!("{rel} is too large to read ({} bytes)", meta.len()));
        }
        let bytes = std::fs::read(&full).map_err(|e| format!("cannot read {rel}: {e}"))?;
        let text = String::from_utf8(bytes).map_err(|_| format!("{rel} is not a text file"))?;
        if start.is_none() && end.is_none() {
            return Ok(text);
        }
        let start = start.unwrap_or(1).max(1);
        let lines: Vec<&str> = text.lines().collect();
        let end = end.unwrap_or(lines.len()).min(lines.len());
        if start > end {
            return Ok(String::new());
        }
        Ok(lines[start - 1..end].join("\n"))
    }

    /// Lines containing `query` (case-insensitive) as `path:line: text`.
    pub fn search(&self, query: &str, path: &str) -> Result<String, String> {
        if query.trim().is_empty() {
            return Err("query is empty".into());
        }
        let needle = query.to_lowercase();
        let (rel, full) = self.resolve(path)?;
        let mut hits = Vec::new();
        let mut stack = vec![(rel, full)];
        'walk: while let Some((dir_rel, dir)) = stack.pop() {
            if dir.is_file() {
                self.search_file(&dir_rel, &dir, &needle, &mut hits);
                continue;
            }
            let mut entries: Vec<_> = match std::fs::read_dir(&dir) {
                Ok(rd) => rd.flatten().collect(),
                Err(_) => continue,
            };
            entries.sort_by_key(|e| std::cmp::Reverse(e.file_name()));
            for e in entries {
                let name = e.file_name().to_string_lossy().into_owned();
                if name == ".git" || HEAVY_DIRS.contains(&name.as_str()) {
                    continue;
                }
                let child = join(&dir_rel, &name);
                if self.ignore.is_ignored(&child) {
                    continue;
                }
                if e.file_type().is_ok_and(|t| t.is_dir()) {
                    stack.push((child, e.path()));
                } else {
                    self.search_file(&child, &e.path(), &needle, &mut hits);
                }
                if hits.len() >= SEARCH_LIMIT {
                    break 'walk;
                }
            }
        }
        if hits.is_empty() {
            return Ok(format!("No matches for {query:?}"));
        }
        if hits.len() >= SEARCH_LIMIT {
            hits.truncate(SEARCH_LIMIT);
            hits.push(format!("… (first {SEARCH_LIMIT} matches)"));
        }
        Ok(hits.join("\n"))
    }

    fn search_file(&self, rel: &str, path: &Path, needle: &str, hits: &mut Vec<String>) {
        if std::fs::metadata(path).map_or(true, |m| m.len() > MAX_FILE_BYTES) {
            return;
        }
        let Ok(text) = std::fs::read_to_string(path) else {
            return;
        };
        for (i, line) in text.lines().enumerate() {
            if line.to_lowercase().contains(needle) {
                let line: String = line.trim().chars().take(200).collect();
                hits.push(format!("{rel}:{}: {line}", i + 1));
                if hits.len() >= SEARCH_LIMIT {
                    return;
                }
            }
        }
    }

    /// Create or overwrite a file (folders are created).
    pub fn write(&self, path: &str, content: &str) -> Result<String, String> {
        let (rel, full) = self.writable(path)?;
        if full.is_dir() {
            return Err(format!("{rel} is a folder"));
        }
        let existed = full.exists();
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("cannot create {rel}: {e}"))?;
        }
        std::fs::write(&full, content).map_err(|e| format!("cannot write {rel}: {e}"))?;
        Ok(format!(
            "{} {rel} ({} lines)",
            if existed { "Updated" } else { "Created" },
            content.lines().count()
        ))
    }

    /// Replace the only occurrence of `old` in a file with `new`.
    pub fn edit(&self, path: &str, old: &str, new: &str) -> Result<String, String> {
        let (rel, full) = self.writable(path)?;
        if old.is_empty() {
            return Err("old text is empty; use write_file to create files".into());
        }
        let text = std::fs::read_to_string(&full).map_err(|_| format!("{rel} does not exist"))?;
        match text.matches(old).count() {
            0 => Err(format!("the old text was not found in {rel}")),
            1 => {
                std::fs::write(&full, text.replacen(old, new, 1))
                    .map_err(|e| format!("cannot write {rel}: {e}"))?;
                Ok(format!("Edited {rel}"))
            }
            n => Err(format!(
                "the old text occurs {n} times in {rel}; include more context"
            )),
        }
    }

    /// Delete a file.
    pub fn delete(&self, path: &str) -> Result<String, String> {
        let (rel, full) = self.writable(path)?;
        if !full.is_file() {
            return Err(format!("{rel} is not a file"));
        }
        std::fs::remove_file(&full).map_err(|e| format!("cannot delete {rel}: {e}"))?;
        Ok(format!("Deleted {rel}"))
    }
}

fn join(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_string()
    } else {
        format!("{dir}/{name}")
    }
}

fn display(rel: &str) -> &str {
    if rel.is_empty() {
        "The project root"
    } else {
        rel
    }
}

fn is_absolute(p: &str) -> bool {
    p.starts_with('/') || p.as_bytes().get(1) == Some(&b':') || Path::new(p).is_absolute()
}

/// A relative path in normal form (`a/b`, `""` for the root). Refuses absolute paths, `..`
/// that leaves the root and anything inside `.git`.
pub fn normalize(path: &str) -> Result<String, String> {
    let p = path.trim().replace('\\', "/");
    if is_absolute(&p) {
        return Err(format!("{path}: use a path relative to the project root"));
    }
    let mut parts: Vec<String> = Vec::new();
    for c in Path::new(&p).components() {
        match c {
            Component::Normal(s) => parts.push(s.to_string_lossy().into_owned()),
            Component::CurDir => {}
            Component::ParentDir => {
                if parts.pop().is_none() {
                    return Err(format!("{path} is outside the sandbox"));
                }
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(format!("{path}: use a path relative to the project root"))
            }
        }
    }
    if parts.first().is_some_and(|s| s == ".git") {
        return Err("the .git folder is off limits".into());
    }
    Ok(parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws(scope: &[&str]) -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("src/Http")).unwrap();
        std::fs::create_dir_all(root.join("vendor/acme")).unwrap();
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::write(
            root.join("src/Order.php"),
            "<?php\nclass Order {\n  // total\n}\n",
        )
        .unwrap();
        std::fs::write(root.join("src/Http/Kernel.php"), "<?php\n// TOTAL here\n").unwrap();
        std::fs::write(root.join("vendor/acme/lib.php"), "total").unwrap();
        std::fs::write(root.join(".env"), "SECRET=1").unwrap();
        std::fs::write(root.join("README.md"), "one\ntwo\nthree\n").unwrap();
        let w = Workspace::new(
            root,
            AiIgnore::parse(".env\n"),
            scope.iter().map(|s| s.to_string()).collect(),
        );
        (dir, w)
    }

    #[test]
    fn normalizes_and_confines_paths() {
        assert_eq!(normalize("./src//a/../b.php").unwrap(), "src/b.php");
        assert_eq!(normalize("").unwrap(), "");
        assert!(normalize("../x").is_err());
        assert!(normalize("a/../../x").is_err());
        assert!(normalize("/etc/passwd").is_err());
        assert!(normalize("C:\\Windows").is_err());
        assert!(normalize(".git/config").is_err());
        let (dir, w) = ws(&[]);
        let abs = dir.path().join("src/Order.php");
        assert_eq!(w.relative(&abs.to_string_lossy()).unwrap(), "src/Order.php");
        assert!(w.relative("/tmp/elsewhere").is_err());
        assert!(w.resolve(".env").unwrap_err().contains(".aiignore"));
    }

    #[cfg(unix)]
    #[test]
    fn refuses_symlinks_leaving_the_sandbox() {
        let (dir, w) = ws(&[]);
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret.txt"), "x").unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join("link")).unwrap();
        assert!(w
            .read("link/secret.txt", None, None)
            .unwrap_err()
            .contains("outside"));
        assert!(w
            .write("link/new.txt", "x")
            .unwrap_err()
            .contains("outside"));
    }

    #[test]
    fn lists_reads_and_searches() {
        let (_dir, w) = ws(&[]);
        let list = w.list("").unwrap();
        assert!(list.contains("src/\n"));
        assert!(list.contains("src/Http/Kernel.php"));
        assert!(list.contains("vendor/"));
        assert!(!list.contains("vendor/acme"));
        assert!(!list.contains(".env"));
        assert!(!list.contains(".git"));
        assert_eq!(w.read("README.md", Some(2), Some(3)).unwrap(), "two\nthree");
        assert!(w.read("missing.txt", None, None).is_err());
        let hits = w.search("total", "").unwrap();
        assert!(hits.contains("src/Order.php:3:"));
        assert!(hits.contains("src/Http/Kernel.php:2: // TOTAL here"));
        assert!(!hits.contains("vendor"));
        assert!(w
            .search("nothing-like-this", "src")
            .unwrap()
            .starts_with("No matches"));
    }

    #[test]
    fn writes_only_inside_scope() {
        let (dir, w) = ws(&["src"]);
        assert!(w
            .write("src/New.php", "<?php\n")
            .unwrap()
            .starts_with("Created"));
        assert!(w.write("README.md", "x").unwrap_err().contains("scope"));
        assert!(w.write("srcx/a.php", "x").is_err());
        assert_eq!(
            w.edit("src/Order.php", "// total", "// sum").unwrap(),
            "Edited src/Order.php"
        );
        assert!(std::fs::read_to_string(dir.path().join("src/Order.php"))
            .unwrap()
            .contains("// sum"));
        assert!(w
            .edit("src/Order.php", "nope", "x")
            .unwrap_err()
            .contains("not found"));
        std::fs::write(dir.path().join("src/Twice.php"), "a a").unwrap();
        assert!(w
            .edit("src/Twice.php", "a", "b")
            .unwrap_err()
            .contains("2 times"));
        assert!(w.delete("src/New.php").is_ok());
        assert!(!dir.path().join("src/New.php").exists());
        assert!(w.write(".git/hooks/pre-commit", "x").is_err());
    }
}
