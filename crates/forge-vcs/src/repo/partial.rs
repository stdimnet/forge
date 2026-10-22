//! Commits from the Commit tool window: whole files and single change hunks (partial
//! commits), with the options of the commit panel (amend, sign-off, author, Git hooks).
//!
//! A partial commit builds the commit in the index without touching the working tree: the
//! index is saved as a tree, reset to HEAD, the whole files are added, and for every partly
//! included file the base version with only the included hunks applied is written as a blob
//! and put into the index. After the commit the saved index comes back and the committed
//! paths are reset to the new HEAD, so the excluded hunks stay as local changes.

use serde::Deserialize;

use super::Repo;
use crate::hunks::apply_hunks;
use crate::signing::Signing;
use forge_core::{ForgeError, Result};

/// Options of the commit panel (the gear menu and the Amend / Sign-off checkboxes).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CommitOptions {
    pub amend: bool,
    /// `git commit --signoff`.
    pub sign_off: bool,
    /// "Run Git hooks" unchecked: `git commit --no-verify`.
    pub no_verify: bool,
    /// "Author": `Name <email>` instead of the configured identity.
    pub author: Option<String>,
    /// GPG/SSH signing (Settings | Version Control | Git), filled in by the IDE from its
    /// settings, never sent by the UI.
    #[serde(skip)]
    pub signing: Signing,
}

/// A file to commit: all of its changes, or only the hunks with these ids
/// ([`crate::hunks::change_hunks`]).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitFile {
    pub path: String,
    #[serde(default)]
    pub hunks: Option<Vec<String>>,
}

impl CommitOptions {
    fn args(&self) -> Result<Vec<String>> {
        let mut args = Vec::new();
        if self.amend {
            args.push("--amend".to_string());
        }
        if self.sign_off {
            args.push("--signoff".to_string());
        }
        if self.no_verify {
            args.push("--no-verify".to_string());
        }
        if let Some(author) = self
            .author
            .as_deref()
            .map(str::trim)
            .filter(|a| !a.is_empty())
        {
            if !(author.contains('<') && author.ends_with('>')) || author.contains('\0') {
                return Err(ForgeError::Other(format!(
                    "the author must look like \"Name <email>\", not {author:?}"
                )));
            }
            args.push(format!("--author={author}"));
        }
        self.signing.validate().map_err(ForgeError::Other)?;
        args.extend(self.signing.commit_args());
        Ok(args)
    }

    /// `git <these> commit …`: the per-invocation config the signing key needs.
    fn config_args(&self) -> Vec<String> {
        self.signing.config_args()
    }

    /// The full `commit` invocation: config options, `commit -q --cleanup=strip` and the
    /// options above.
    fn commit_command(&self) -> Result<Vec<String>> {
        let mut args = self.config_args();
        args.extend(["commit".into(), "-q".into(), "--cleanup=strip".into()]);
        args.extend(self.args()?);
        Ok(args)
    }
}

impl Repo {
    /// Commit with the commit panel's options. `paths` as in [`Repo::commit`].
    pub fn commit_opts(
        &self,
        message: &str,
        paths: Option<&[String]>,
        opts: &CommitOptions,
    ) -> Result<String> {
        if message.trim().is_empty() && !opts.amend {
            return Err(ForgeError::Other("commit message is empty".into()));
        }
        let specs = paths.map(|p| self.pathspecs(p)).unwrap_or_default();
        if !specs.is_empty() {
            self.add_untracked(&specs)?;
        }
        let command = opts.commit_command()?;
        let mut args: Vec<&str> = command.iter().map(String::as_str).collect();
        if message.trim().is_empty() {
            args.push("--no-edit");
        } else {
            args.extend(["-F", "-"]);
        }
        if !specs.is_empty() {
            args.extend(["--only", "--"]);
            args.extend(specs.iter().map(String::as_str));
        }
        self.git_stdin(&args, message)?;
        Ok(self.git(&["rev-parse", "HEAD"])?.trim().to_string())
    }

    /// Make new files among `specs` (repository paths) known to git so they can be committed.
    fn add_untracked(&self, specs: &[String]) -> Result<()> {
        let untracked: Vec<String> = self
            .status()?
            .entries
            .into_iter()
            .filter(|e| e.is_untracked())
            .map(|e| self.ws_to_repo(&e.path))
            .filter(|p| specs.contains(p))
            .collect();
        if !untracked.is_empty() {
            let mut args = vec!["add", "--"];
            args.extend(untracked.iter().map(String::as_str));
            self.git(&args)?;
        }
        Ok(())
    }

    /// Commit `files`, some of them partly. `read` returns the working copy of a file
    /// (workspace-relative path), read through the project's backend. Returns the new HEAD.
    pub fn commit_files(
        &self,
        message: &str,
        files: &[CommitFile],
        opts: &CommitOptions,
        read: &dyn Fn(&str) -> Result<String>,
    ) -> Result<String> {
        let whole: Vec<String> = files
            .iter()
            .filter(|f| f.hunks.is_none())
            .map(|f| f.path.clone())
            .collect();
        if files.iter().all(|f| f.hunks.is_none()) {
            return self.commit_opts(message, Some(&whole), opts);
        }
        if message.trim().is_empty() && !opts.amend {
            return Err(ForgeError::Other("commit message is empty".into()));
        }
        let status = self.status()?;
        if status.entries.iter().any(|e| e.conflict) {
            return Err(ForgeError::Other(
                "Committing is not possible because you have unmerged files".into(),
            ));
        }
        // The staged contents of each partial file, computed before the index changes.
        let mut staged: Vec<(String, String)> = Vec::new();
        for f in files {
            let Some(ids) = &f.hunks else { continue };
            let base = self.show("HEAD", &f.path)?.unwrap_or_default();
            let current = read(&f.path)?;
            let text = apply_hunks(&base, &current, ids);
            if text != base {
                staged.push((f.path.clone(), text));
            }
        }
        if staged.is_empty() && whole.is_empty() && !opts.amend {
            return Err(ForgeError::Other(
                "no changes are included in the commit".into(),
            ));
        }
        let saved = self.git(&["write-tree"])?.trim().to_string();
        let result = self.commit_built_index(message, &whole, &staged, opts);
        // Restore what was staged, then put the committed paths at the new HEAD.
        let restored = self.git(&["read-tree", &saved]);
        let mut committed: Vec<String> = self.pathspecs(&whole);
        committed.extend(staged.iter().map(|(p, _)| self.ws_to_repo(p)));
        if result.is_ok() && !committed.is_empty() {
            let mut args = vec!["--literal-pathspecs", "reset", "-q", "--"];
            args.extend(committed.iter().map(String::as_str));
            self.git(&args)?;
        }
        let _ = self.git(&["update-index", "-q", "--refresh"]);
        restored?;
        result
    }

    fn commit_built_index(
        &self,
        message: &str,
        whole: &[String],
        staged: &[(String, String)],
        opts: &CommitOptions,
    ) -> Result<String> {
        if self.has_head() {
            self.git(&["read-tree", "HEAD"])?;
        } else {
            self.git(&["read-tree", "--empty"])?;
        }
        if !whole.is_empty() {
            let specs = self.pathspecs(whole);
            let mut args = vec!["--literal-pathspecs", "add", "-A", "--"];
            args.extend(specs.iter().map(String::as_str));
            self.git(&args)?;
        }
        for (path, text) in staged {
            let repo_path = self.ws_to_repo(path);
            let blob = self.git_stdin(
                &["hash-object", "-w", "--stdin", "--path", &repo_path],
                text,
            )?;
            let mode = self
                .git(&["--literal-pathspecs", "ls-tree", "HEAD", "--", &repo_path])
                .ok()
                .and_then(|l| l.split_whitespace().next().map(str::to_string))
                .unwrap_or_else(|| "100644".into());
            let info = format!("{mode},{},{repo_path}", blob.trim());
            self.git(&["update-index", "--add", "--cacheinfo", &info])?;
        }
        let command = opts.commit_command()?;
        let mut args: Vec<&str> = command.iter().map(String::as_str).collect();
        if message.trim().is_empty() {
            args.push("--no-edit");
        } else {
            args.extend(["-F", "-"]);
        }
        self.git_stdin(&args, message)?;
        Ok(self.git(&["rev-parse", "HEAD"])?.trim().to_string())
    }

    /// The change hunks of a file: its HEAD version against `current` (the working copy or
    /// an unsaved editor buffer). New files have one hunk.
    pub fn file_hunks(&self, path: &str, current: &str) -> Result<Vec<crate::hunks::ChangeHunk>> {
        let base = if self.has_head() {
            self.show("HEAD", path)?.unwrap_or_default()
        } else {
            String::new()
        };
        Ok(crate::hunks::change_hunks(&base, current))
    }
}
