//! Which project files git ignores, so the project tree can gray them out like PHPStorm.
//!
//! Rules are layered the way git evaluates them: `.gitignore` files from the repository
//! root down to the directory (the deepest match wins), then `.git/info/exclude`, then the
//! global excludes file. An ignored directory cannot be re-included from below, so
//! everything inside one is ignored too.

use std::fs;
use std::path::{Path, PathBuf};

use ignore::gitignore::{Gitignore, GitignoreBuilder};

/// The ignore rules that apply to the entries of one directory.
pub(crate) struct IgnoreRules {
    /// `.gitignore` files from the repository root down to the directory.
    layers: Vec<Gitignore>,
    /// `.git/info/exclude`, then the global excludes file.
    fallbacks: Vec<Gitignore>,
    /// The directory itself is inside an ignored directory, so every entry is ignored.
    inherited: bool,
}

impl IgnoreRules {
    /// Rules for the entries of `dir` (absolute, inside `workspace_root`). `None` when the
    /// workspace is not inside a git repository, where "ignored" has no meaning.
    pub(crate) fn for_dir(workspace_root: &Path, dir: &Path) -> Option<Self> {
        let repo = find_repo_root(workspace_root)?;
        let mut rules = Self {
            layers: Vec::new(),
            fallbacks: fallback_rules(&repo),
            inherited: false,
        };
        let mut current = repo.clone();
        rules.push_layer(&current);
        let below_repo = dir.strip_prefix(&repo).ok()?;
        for component in below_repo.components() {
            current.push(component);
            // Rules above `current` decide whether `current` itself is ignored.
            if rules.matches(&current, true) {
                rules.inherited = true;
                break;
            }
            rules.push_layer(&current);
        }
        Some(rules)
    }

    pub(crate) fn is_ignored(&self, path: &Path, is_dir: bool) -> bool {
        self.inherited || self.matches(path, is_dir)
    }

    fn push_layer(&mut self, dir: &Path) {
        let file = dir.join(".gitignore");
        if !file.is_file() {
            return;
        }
        let mut builder = GitignoreBuilder::new(dir);
        // A malformed line only loses that line; the rest of the file still applies.
        let _ = builder.add(&file);
        if let Ok(g) = builder.build() {
            self.layers.push(g);
        }
    }

    fn matches(&self, path: &Path, is_dir: bool) -> bool {
        for g in self.layers.iter().rev().chain(&self.fallbacks) {
            let m = g.matched(path, is_dir);
            if !m.is_none() {
                return m.is_ignore();
            }
        }
        false
    }
}

/// The closest directory at or above `start` containing `.git` (a directory, or a file
/// in worktrees and submodules).
fn find_repo_root(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .find(|d| d.join(".git").exists())
        .map(Path::to_path_buf)
}

fn fallback_rules(repo: &Path) -> Vec<Gitignore> {
    let mut out = Vec::new();
    if let Some(exclude) = common_git_dir(repo).map(|d| d.join("info").join("exclude")) {
        let mut builder = GitignoreBuilder::new(repo);
        if exclude.is_file() && builder.add(&exclude).is_none() {
            if let Ok(g) = builder.build() {
                out.push(g);
            }
        }
    }
    let (global, _) = GitignoreBuilder::new(repo).build_global();
    out.push(global);
    out
}

/// The git directory shared by all worktrees of the repository at `repo`.
fn common_git_dir(repo: &Path) -> Option<PathBuf> {
    let dot_git = repo.join(".git");
    if dot_git.is_dir() {
        return Some(dot_git);
    }
    // Worktree or submodule: `.git` is a file reading `gitdir: <path>`.
    let text = fs::read_to_string(&dot_git).ok()?;
    let gitdir = PathBuf::from(text.trim().strip_prefix("gitdir:")?.trim());
    let gitdir = if gitdir.is_absolute() {
        gitdir
    } else {
        repo.join(gitdir)
    };
    match fs::read_to_string(gitdir.join("commondir")) {
        Ok(common) => {
            let common = PathBuf::from(common.trim());
            Some(if common.is_absolute() {
                common
            } else {
                gitdir.join(common)
            })
        }
        Err(_) => Some(gitdir),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join(".git")).unwrap();
        dir
    }

    fn write(root: &Path, rel: &str, text: &str) {
        let p = root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }

    fn ignored(root: &Path, dir: &str, name: &str, is_dir: bool) -> bool {
        let dir = root.join(dir);
        IgnoreRules::for_dir(root, &dir)
            .unwrap()
            .is_ignored(&dir.join(name), is_dir)
    }

    #[test]
    fn applies_root_patterns() {
        let r = repo();
        write(r.path(), ".gitignore", "/vendor\n*.log\n.env\n");
        assert!(ignored(r.path(), "", "vendor", true));
        assert!(ignored(r.path(), "", "debug.log", false));
        assert!(ignored(r.path(), "", ".env", false));
        assert!(!ignored(r.path(), "", "src", true));
        assert!(!ignored(r.path(), "", ".env.example", false));
    }

    #[test]
    fn directory_only_patterns_skip_files() {
        let r = repo();
        write(r.path(), ".gitignore", "build/\n");
        assert!(ignored(r.path(), "", "build", true));
        assert!(!ignored(r.path(), "", "build", false));
    }

    #[test]
    fn deeper_gitignore_overrides_shallower() {
        let r = repo();
        write(r.path(), ".gitignore", "*.gen.php\n");
        write(r.path(), "src/.gitignore", "!keep.gen.php\n/local.php\n");
        fs::create_dir_all(r.path().join("src")).unwrap();
        assert!(ignored(r.path(), "src", "other.gen.php", false));
        assert!(!ignored(r.path(), "src", "keep.gen.php", false));
        assert!(ignored(r.path(), "src", "local.php", false));
        // Anchored patterns of a nested file only reach its own directory.
        assert!(!ignored(r.path(), "", "local.php", false));
    }

    #[test]
    fn everything_inside_an_ignored_directory_is_ignored() {
        let r = repo();
        write(r.path(), ".gitignore", "var/\n!var/keep.txt\n");
        fs::create_dir_all(r.path().join("var/cache")).unwrap();
        assert!(ignored(r.path(), "var", "keep.txt", false));
        assert!(ignored(r.path(), "var/cache", "x.php", false));
    }

    #[test]
    fn honours_info_exclude() {
        let r = repo();
        write(r.path(), ".git/info/exclude", "scratch.txt\n");
        assert!(ignored(r.path(), "", "scratch.txt", false));
    }

    #[test]
    fn worktree_uses_the_common_git_dir() {
        let main = tempfile::tempdir().unwrap();
        write(main.path(), ".git/info/exclude", "notes.md\n");
        write(main.path(), ".git/worktrees/wt/commondir", "../..\n");
        let wt = tempfile::tempdir().unwrap();
        write(
            wt.path(),
            ".git",
            &format!(
                "gitdir: {}\n",
                main.path().join(".git/worktrees/wt").display()
            ),
        );
        assert!(ignored(wt.path(), "", "notes.md", false));
    }

    #[test]
    fn projects_below_the_repository_root_inherit_its_rules() {
        let r = repo();
        write(r.path(), ".gitignore", "dist/\n");
        fs::create_dir_all(r.path().join("app/ui")).unwrap();
        let project = r.path().join("app/ui");
        let rules = IgnoreRules::for_dir(&project, &project).unwrap();
        assert!(rules.is_ignored(&project.join("dist"), true));
    }

    #[test]
    fn no_repository_means_nothing_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), ".gitignore", "*.log\n");
        // The temp dir may itself sit inside a repository on exotic setups; only assert
        // when it does not.
        if find_repo_root(dir.path()).is_none() {
            assert!(IgnoreRules::for_dir(dir.path(), dir.path()).is_none());
        }
    }
}
