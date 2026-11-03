//! Git for guests. A guest's version control code (`forge_vcs::Repo`) issues git command
//! lines through `GitExec`; the guest never sends them. [`parse`] turns exactly the commands
//! Forge's version control code issues into a typed [`GitQuery`] or [`GitAction`] (anything
//! else fails with "not available in Code With Me sessions"), and the host builds the
//! command line again from the typed value with [`GitQuery::to_args`] /
//! [`GitAction::to_args`], so a guest can never smuggle in `-c core.sshCommand=...`, an alias,
//! `--upload-pack` or another option.
//!
//! The host also filters the output ([`Visible`]): files the guest may not see (secrets,
//! `.git/`, files outside the project folder of a larger repository) disappear from status,
//! diffs and file lists.

use forge_vcs::{GitRequest, LOG_FORMAT, REF_FORMAT};

use super::calls::{GitAction, GitQuery};

/// `git show -s --format=...` of [`forge_vcs::Repo::commit_details`].
pub use forge_vcs::COMMIT_META_FORMAT;

/// A guest's git command as typed values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitCall {
    Query(GitQuery),
    Action(GitAction),
}

fn unavailable(args: &[String]) -> String {
    format!(
        "git {} is not available in Code With Me sessions",
        args.first().map(String::as_str).unwrap_or("")
    )
}

fn strings(s: &[&str]) -> Vec<String> {
    s.iter().map(|s| s.to_string()).collect()
}

/// Turn a git command line of Forge's version control code into a typed call.
pub fn parse(req: &GitRequest) -> Result<GitCall, String> {
    use GitAction as A;
    use GitCall::{Action, Query};
    use GitQuery as Q;
    let owned = &req.args;
    let a: Vec<&str> = owned.iter().map(String::as_str).collect();
    let call = match a.as_slice() {
        ["rev-parse", "--show-toplevel", "--show-prefix"] => Query(Q::Discover),
        ["rev-parse", "--verify", "-q", "HEAD"] => Query(Q::HasHead),
        ["rev-parse", "HEAD"] => Query(Q::Head),
        ["status", "--porcelain=v2", "-z", "--branch", "--untracked-files=all"] => Query(Q::Status),
        ["show", "-s", fmt, rev] if *fmt == COMMIT_META_FORMAT => Query(Q::CommitMeta {
            rev: rev.to_string(),
        }),
        ["show", spec] => {
            let (rev, path) = spec.split_once(':').ok_or_else(|| unavailable(owned))?;
            Query(Q::Show {
                rev: rev.to_string(),
                path: path.to_string(),
            })
        }
        ["diff-tree", "-r", "-z", "--name-status", "-M", parent, sha] => Query(Q::CommitFiles {
            parent: Some(parent.to_string()),
            sha: sha.to_string(),
        }),
        ["diff-tree", "-r", "-z", "--name-status", "--root", sha] => Query(Q::CommitFiles {
            parent: None,
            sha: sha.to_string(),
        }),
        ["diff", "--no-ext-diff", "--no-color", "-M", rest @ ..] => {
            let (staged, rest) = match rest {
                ["--cached", r @ ..] => (true, r),
                r => (false, r),
            };
            let (against_head, rest) = match rest {
                ["HEAD", r @ ..] => (true, r),
                r => (false, r),
            };
            let ["--", paths @ ..] = rest else {
                return Err(unavailable(owned));
            };
            Query(Q::Diff {
                staged,
                against_head,
                paths: strings(paths),
            })
        }
        ["blame", "--porcelain", "--contents", "-", "--", path] => Query(Q::Blame {
            path: path.to_string(),
            contents: Some(req.stdin.clone().unwrap_or_default()),
        }),
        ["blame", "--porcelain", "--", path] => Query(Q::Blame {
            path: path.to_string(),
            contents: None,
        }),
        ["log", fmt, "--decorate=full", "--date-order", max, skip, rest @ ..]
            if *fmt == LOG_FORMAT =>
        {
            Query(parse_log(max, skip, rest).ok_or_else(|| unavailable(owned))?)
        }
        ["for-each-ref", fmt, "--sort=-committerdate", "refs/heads", "refs/remotes", "refs/tags"]
            if *fmt == REF_FORMAT =>
        {
            Query(Q::Refs)
        }
        ["reflog", "--format=%gs", "-n", "300"] => Query(Q::Reflog),
        ["--literal-pathspecs", "ls-files", "-z", "--cached", "--", paths @ ..] => {
            Query(Q::TrackedAmong {
                paths: strings(paths),
            })
        }
        ["ls-files", "-z", "--cached", "--", paths @ ..] => Query(Q::LsFiles {
            paths: strings(paths),
        }),
        ["ls-tree", "-r", "-z", "--name-only", "HEAD", "--", paths @ ..] => Query(Q::LsTreeHead {
            paths: strings(paths),
        }),
        ["remote"] => Query(Q::Remotes),
        ["remote", "-v"] => Query(Q::RemotesVerbose),
        ["show-ref", "--verify", "--quiet", r] if r.starts_with("refs/heads/") => {
            Query(Q::HasBranch {
                name: r["refs/heads/".len()..].to_string(),
            })
        }

        ["add", "-A", "--", paths @ ..] => Action(A::Stage {
            paths: strings(paths),
        }),
        ["add", "--", paths @ ..] => Action(A::AddNew {
            paths: strings(paths),
        }),
        ["restore", "--staged", "--", paths @ ..] => Action(A::Unstage {
            paths: strings(paths),
        }),
        ["rm", "--cached", "-r", "-q", "--", paths @ ..] => Action(A::UnstageInitial {
            paths: strings(paths),
        }),
        ["restore", "--source=HEAD", "--staged", "--worktree", "--", paths @ ..] => {
            Action(A::RestoreFromHead {
                paths: strings(paths),
            })
        }
        ["rm", "--cached", "-f", "-q", "--", paths @ ..] => Action(A::ForgetAdded {
            paths: strings(paths),
        }),
        ["commit", "-q", "--cleanup=strip", rest @ ..] => {
            let (amend, rest) = match rest {
                ["--amend", r @ ..] => (true, r),
                r => (false, r),
            };
            let (message, rest) = match rest {
                ["--no-edit", r @ ..] => (String::new(), r),
                ["-F", "-", r @ ..] => (req.stdin.clone().unwrap_or_default(), r),
                _ => return Err(unavailable(owned)),
            };
            let paths = match rest {
                [] => Vec::new(),
                ["--only", "--", p @ ..] => strings(p),
                _ => return Err(unavailable(owned)),
            };
            Action(A::Commit {
                message,
                amend,
                paths,
            })
        }
        ["checkout", rev, "--"] => Action(A::Checkout {
            rev: rev.to_string(),
        }),
        ["checkout", "-b", name, "--track", remote_branch] => Action(A::CheckoutTracking {
            name: name.to_string(),
            remote_branch: remote_branch.to_string(),
        }),
        ["checkout", "-b", name] => Action(A::CheckoutNew {
            name: name.to_string(),
            start: None,
        }),
        ["checkout", "-b", name, start] => Action(A::CheckoutNew {
            name: name.to_string(),
            start: Some(start.to_string()),
        }),
        ["branch", "-d", name] => Action(A::BranchDelete {
            name: name.to_string(),
            force: false,
        }),
        ["branch", "-D", name] => Action(A::BranchDelete {
            name: name.to_string(),
            force: true,
        }),
        ["branch", "-m", old, new] => Action(A::BranchRename {
            old: old.to_string(),
            new: new.to_string(),
        }),
        ["branch", name] => Action(A::BranchNew {
            name: name.to_string(),
            start: None,
        }),
        ["branch", name, start] => Action(A::BranchNew {
            name: name.to_string(),
            start: Some(start.to_string()),
        }),
        ["merge", "--no-edit", rev] => Action(A::Merge {
            rev: rev.to_string(),
        }),
        ["rebase", onto] => Action(A::Rebase {
            onto: onto.to_string(),
        }),
        ["push"] => Action(A::Push {
            force: false,
            set_upstream: None,
        }),
        ["push", "--force-with-lease"] => Action(A::Push {
            force: true,
            set_upstream: None,
        }),
        ["push", "-u", remote, "HEAD"] => Action(A::Push {
            force: false,
            set_upstream: Some(remote.to_string()),
        }),
        ["push", "--force-with-lease", "-u", remote, "HEAD"] => Action(A::Push {
            force: true,
            set_upstream: Some(remote.to_string()),
        }),
        ["pull", "--no-edit", "--rebase"] => Action(A::Pull { rebase: true }),
        ["pull", "--no-edit", "--no-rebase"] => Action(A::Pull { rebase: false }),
        ["fetch", "--all", "--prune"] => Action(A::Fetch),
        ["fetch", remote, refspec] => Action(A::FetchRefspec {
            remote: remote.to_string(),
            refspec: refspec.to_string(),
        }),
        _ => return Err(unavailable(owned)),
    };
    Ok(call)
}

fn parse_log(max: &str, skip: &str, rest: &[&str]) -> Option<GitQuery> {
    let limit = max.strip_prefix("--max-count=")?.parse().ok()?;
    let skip = skip.strip_prefix("--skip=")?.parse().ok()?;
    let mut rest = rest;
    let mut text = None;
    if let ["-i", grep, r @ ..] = rest {
        text = Some(grep.strip_prefix("--grep=")?.to_string());
        rest = r;
    }
    let mut author = None;
    if let [a, r @ ..] = rest {
        if let Some(a) = a.strip_prefix("--author=") {
            author = Some(a.to_string());
            rest = r;
        }
    }
    let (rev, all, rest) = match rest {
        ["--branches", "--remotes", "--tags", "HEAD", r @ ..] => (None, true, r),
        ["HEAD", r @ ..] => (None, false, r),
        [rev, r @ ..] if *rev != "--" => (Some(rev.to_string()), false, r),
        _ => return None,
    };
    let path = match rest {
        ["--"] => None,
        ["--", p] => Some(p.to_string()),
        _ => return None,
    };
    Some(GitQuery::Log {
        limit,
        skip,
        text,
        author,
        rev,
        all,
        path,
    })
}

fn with_paths(mut args: Vec<String>, paths: &[String]) -> Vec<String> {
    args.extend(paths.iter().cloned());
    args
}

impl GitQuery {
    /// The command line (after `git`) the host runs for this query.
    pub fn to_args(&self) -> Vec<String> {
        match self {
            GitQuery::Discover => strings(&["rev-parse", "--show-toplevel", "--show-prefix"]),
            GitQuery::HasHead => strings(&["rev-parse", "--verify", "-q", "HEAD"]),
            GitQuery::Head => strings(&["rev-parse", "HEAD"]),
            GitQuery::Status => strings(&[
                "status",
                "--porcelain=v2",
                "-z",
                "--branch",
                "--untracked-files=all",
            ]),
            GitQuery::Show { rev, path } => vec!["show".into(), format!("{rev}:{path}")],
            GitQuery::CommitMeta { rev } => {
                vec![
                    "show".into(),
                    "-s".into(),
                    COMMIT_META_FORMAT.into(),
                    rev.clone(),
                ]
            }
            GitQuery::CommitFiles { parent, sha } => match parent {
                Some(p) => strings(&["diff-tree", "-r", "-z", "--name-status", "-M", p, sha]),
                None => strings(&["diff-tree", "-r", "-z", "--name-status", "--root", sha]),
            },
            GitQuery::Diff {
                staged,
                against_head,
                paths,
            } => {
                let mut args = strings(&["diff", "--no-ext-diff", "--no-color", "-M"]);
                if *staged {
                    args.push("--cached".into());
                }
                if *against_head {
                    args.push("HEAD".into());
                }
                args.push("--".into());
                with_paths(args, paths)
            }
            GitQuery::Blame { path, contents } => {
                let mut args = strings(&["blame", "--porcelain"]);
                if contents.is_some() {
                    args.extend(strings(&["--contents", "-"]));
                }
                args.push("--".into());
                args.push(path.clone());
                args
            }
            GitQuery::Log {
                limit,
                skip,
                text,
                author,
                rev,
                all,
                path,
            } => {
                let mut args = vec![
                    "log".to_string(),
                    LOG_FORMAT.to_string(),
                    "--decorate=full".to_string(),
                    "--date-order".to_string(),
                    format!("--max-count={limit}"),
                    format!("--skip={skip}"),
                ];
                if let Some(t) = text {
                    args.push("-i".into());
                    args.push(format!("--grep={t}"));
                }
                if let Some(a) = author {
                    args.push(format!("--author={a}"));
                }
                match rev {
                    Some(r) => args.push(r.clone()),
                    None if *all => {
                        args.extend(strings(&["--branches", "--remotes", "--tags", "HEAD"]))
                    }
                    None => args.push("HEAD".into()),
                }
                args.push("--".into());
                args.extend(path.clone());
                args
            }
            GitQuery::Refs => vec![
                "for-each-ref".into(),
                REF_FORMAT.into(),
                "--sort=-committerdate".into(),
                "refs/heads".into(),
                "refs/remotes".into(),
                "refs/tags".into(),
            ],
            GitQuery::Reflog => strings(&["reflog", "--format=%gs", "-n", "300"]),
            GitQuery::TrackedAmong { paths } => with_paths(
                strings(&["--literal-pathspecs", "ls-files", "-z", "--cached", "--"]),
                paths,
            ),
            GitQuery::LsFiles { paths } => {
                with_paths(strings(&["ls-files", "-z", "--cached", "--"]), paths)
            }
            GitQuery::LsTreeHead { paths } => with_paths(
                strings(&["ls-tree", "-r", "-z", "--name-only", "HEAD", "--"]),
                paths,
            ),
            GitQuery::Remotes => strings(&["remote"]),
            GitQuery::RemotesVerbose => strings(&["remote", "-v"]),
            GitQuery::HasBranch { name } => vec![
                "show-ref".into(),
                "--verify".into(),
                "--quiet".into(),
                format!("refs/heads/{name}"),
            ],
        }
    }

    /// Standard input for the command.
    pub fn stdin(&self) -> Option<String> {
        match self {
            GitQuery::Blame { contents, .. } => contents.clone(),
            _ => None,
        }
    }
}

impl GitAction {
    pub fn to_args(&self) -> Vec<String> {
        match self {
            GitAction::Stage { paths } => with_paths(strings(&["add", "-A", "--"]), paths),
            GitAction::AddNew { paths } => with_paths(strings(&["add", "--"]), paths),
            GitAction::Unstage { paths } => {
                with_paths(strings(&["restore", "--staged", "--"]), paths)
            }
            GitAction::UnstageInitial { paths } => {
                with_paths(strings(&["rm", "--cached", "-r", "-q", "--"]), paths)
            }
            GitAction::RestoreFromHead { paths } => with_paths(
                strings(&["restore", "--source=HEAD", "--staged", "--worktree", "--"]),
                paths,
            ),
            GitAction::ForgetAdded { paths } => {
                with_paths(strings(&["rm", "--cached", "-f", "-q", "--"]), paths)
            }
            GitAction::Commit {
                message,
                amend,
                paths,
            } => {
                let mut args = strings(&["commit", "-q", "--cleanup=strip"]);
                if *amend {
                    args.push("--amend".into());
                }
                if message.trim().is_empty() {
                    args.push("--no-edit".into());
                } else {
                    args.extend(strings(&["-F", "-"]));
                }
                if !paths.is_empty() {
                    args.extend(strings(&["--only", "--"]));
                    args.extend(paths.iter().cloned());
                }
                args
            }
            GitAction::Checkout { rev } => vec!["checkout".into(), rev.clone(), "--".into()],
            GitAction::CheckoutTracking {
                name,
                remote_branch,
            } => strings(&["checkout", "-b", name, "--track", remote_branch]),
            GitAction::CheckoutNew { name, start } => {
                let mut args = strings(&["checkout", "-b", name]);
                args.extend(start.clone());
                args
            }
            GitAction::BranchNew { name, start } => {
                let mut args = strings(&["branch", name]);
                args.extend(start.clone());
                args
            }
            GitAction::BranchDelete { name, force } => {
                strings(&["branch", if *force { "-D" } else { "-d" }, name])
            }
            GitAction::BranchRename { old, new } => strings(&["branch", "-m", old, new]),
            GitAction::Merge { rev } => strings(&["merge", "--no-edit", rev]),
            GitAction::Rebase { onto } => strings(&["rebase", onto]),
            GitAction::Push {
                force,
                set_upstream,
            } => {
                let mut args = strings(&["push"]);
                if *force {
                    args.push("--force-with-lease".into());
                }
                if let Some(remote) = set_upstream {
                    args.extend(strings(&["-u", remote, "HEAD"]));
                }
                args
            }
            GitAction::Pull { rebase } => strings(&[
                "pull",
                "--no-edit",
                if *rebase { "--rebase" } else { "--no-rebase" },
            ]),
            GitAction::Fetch => strings(&["fetch", "--all", "--prune"]),
            GitAction::FetchRefspec { remote, refspec } => strings(&["fetch", remote, refspec]),
        }
    }

    pub fn stdin(&self) -> Option<String> {
        match self {
            GitAction::Commit { message, .. } => Some(message.clone()),
            _ => None,
        }
    }

    /// Network operations never prompt for credentials.
    pub fn non_interactive(&self) -> bool {
        matches!(
            self,
            GitAction::Push { .. }
                | GitAction::Pull { .. }
                | GitAction::Fetch
                | GitAction::FetchRefspec { .. }
        )
    }
}

/// What a guest may see of the repository: paths are repository-relative, as git prints
/// them. The project folder is the repository root or a folder inside it (`prefix`, `""` or
/// `"sub/dir/"`).
pub struct Visible<'a> {
    pub prefix: &'a str,
    /// Decides a project-relative path (see `Policy::check_path`).
    pub allowed: &'a dyn Fn(&str) -> bool,
}

impl Visible<'_> {
    /// Whether a repository path is inside the project folder and visible to the guest.
    pub fn path(&self, repo_path: &str) -> bool {
        match repo_path.strip_prefix(self.prefix) {
            Some(rest) if !rest.is_empty() => (self.allowed)(rest.trim_end_matches('/')),
            _ => false,
        }
    }

    /// A pathspec from the guest: the project folder itself, or something inside it.
    pub fn pathspec(&self, spec: &str) -> bool {
        let folder = self.prefix.trim_end_matches('/');
        if spec.is_empty() || spec == "." {
            return self.prefix.is_empty();
        }
        if spec.trim_end_matches('/') == folder {
            return true;
        }
        self.path(spec)
    }

    /// `status --porcelain=v2 -z`: drop the entries of hidden files.
    pub fn status(&self, raw: &str) -> String {
        let mut out = String::new();
        let mut tokens = raw.split('\0').filter(|t| !t.is_empty());
        while let Some(t) = tokens.next() {
            let mut push = |tokens: &[&str]| {
                for t in tokens {
                    out.push_str(t);
                    out.push('\0');
                }
            };
            match t.as_bytes()[0] {
                b'#' => push(&[t]),
                b'1' if t.splitn(9, ' ').nth(8).is_some_and(|p| self.path(p)) => push(&[t]),
                b'u' if t.splitn(11, ' ').nth(10).is_some_and(|p| self.path(p)) => push(&[t]),
                b'2' => {
                    // A rename is followed by its original path; hidden if either end is.
                    let orig = tokens.next().unwrap_or_default();
                    if t.splitn(10, ' ').nth(9).is_some_and(|p| self.path(p)) && self.path(orig) {
                        push(&[t, orig]);
                    }
                }
                b'?' | b'!' if t.get(2..).is_some_and(|p| self.path(p)) => push(&[t]),
                _ => {}
            }
        }
        out
    }

    /// A NUL separated list of paths (`ls-files -z`, `ls-tree -z`).
    pub fn names(&self, raw: &str) -> String {
        raw.split('\0')
            .filter(|p| !p.is_empty() && self.path(p))
            .map(|p| format!("{p}\0"))
            .collect()
    }

    /// `diff-tree -z --name-status`: `M\0path\0`, `R100\0old\0new\0`.
    pub fn name_status(&self, raw: &str) -> String {
        let mut out = String::new();
        let mut tokens = raw.split('\0').filter(|t| !t.is_empty()).peekable();
        // `--root <commit>` starts with the commit id.
        if let Some(id) = tokens.next_if(|t| forge_vcs::is_object_id(t)) {
            out.push_str(id);
            out.push('\0');
        }
        while let Some(status) = tokens.next() {
            let two = status.starts_with('R') || status.starts_with('C');
            let first = tokens.next();
            let second = if two { tokens.next() } else { None };
            let ok = first.is_some_and(|p| self.path(p)) && second.is_none_or(|p| self.path(p));
            if ok {
                out.push_str(status);
                out.push('\0');
                out.push_str(first.unwrap_or_default());
                out.push('\0');
                if let Some(s) = second {
                    out.push_str(s);
                    out.push('\0');
                }
            }
        }
        out
    }

    /// A unified diff: drop the sections of hidden files. A section whose header cannot be
    /// read is dropped too.
    pub fn diff(&self, raw: &str) -> String {
        let mut out = String::new();
        let mut keep = true;
        for line in raw.split_inclusive('\n') {
            if let Some(rest) = line.strip_prefix("diff --git ") {
                keep = self.diff_header(rest.trim_end_matches('\n'));
            }
            if keep {
                out.push_str(line);
            }
        }
        out
    }

    /// `a/<old> b/<new>`: every way to split it must name visible files.
    fn diff_header(&self, rest: &str) -> bool {
        let Some(rest) = rest.strip_prefix("a/") else {
            return false;
        };
        let mut found = false;
        for (i, _) in rest.match_indices(" b/") {
            found = true;
            let (old, new) = (&rest[..i], &rest[i + 3..]);
            if !self.path(old) || !self.path(new) {
                return false;
            }
        }
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(args: &[&str]) -> GitRequest {
        GitRequest {
            cwd: "/p".into(),
            args: strings(args),
            stdin: None,
            non_interactive: false,
        }
    }

    fn round_trip_query(q: GitQuery) {
        let mut r = req(&[]);
        r.args = q.to_args();
        r.stdin = q.stdin();
        assert_eq!(parse(&r), Ok(GitCall::Query(q.clone())), "{:?}", r.args);
    }

    fn round_trip_action(a: GitAction) {
        let mut r = req(&[]);
        r.args = a.to_args();
        r.stdin = a.stdin();
        assert_eq!(parse(&r), Ok(GitCall::Action(a.clone())), "{:?}", r.args);
    }

    fn s(x: &str) -> String {
        x.to_string()
    }

    #[test]
    fn queries_round_trip() {
        use GitQuery::*;
        for q in [
            Discover,
            HasHead,
            Head,
            Status,
            Show {
                rev: "HEAD".into(),
                path: "app/User.php".into(),
            },
            Show {
                rev: "".into(),
                path: "app/User.php".into(),
            },
            CommitMeta {
                rev: "abc123".into(),
            },
            CommitFiles {
                parent: Some("p1".into()),
                sha: "c1".into(),
            },
            CommitFiles {
                parent: None,
                sha: "c1".into(),
            },
            Diff {
                staged: true,
                against_head: false,
                paths: vec![".".into()],
            },
            Diff {
                staged: false,
                against_head: false,
                paths: vec!["a.php".into(), "b/".into()],
            },
            Diff {
                staged: false,
                against_head: true,
                paths: vec![],
            },
            Blame {
                path: "a.php".into(),
                contents: None,
            },
            Blame {
                path: "a.php".into(),
                contents: Some("<?php\n".into()),
            },
            Log {
                limit: 200,
                skip: 40,
                text: Some("fix".into()),
                author: Some("alex".into()),
                rev: Some("origin/main".into()),
                all: false,
                path: Some("app/x.php".into()),
            },
            Log {
                limit: 5,
                skip: 0,
                text: None,
                author: None,
                rev: None,
                all: true,
                path: None,
            },
            Log {
                limit: 5,
                skip: 0,
                text: None,
                author: None,
                rev: None,
                all: false,
                path: None,
            },
            Refs,
            Reflog,
            TrackedAmong {
                paths: vec!["vendor".into(), "x.log".into()],
            },
            LsFiles {
                paths: vec!["a".into()],
            },
            LsTreeHead {
                paths: vec!["a".into()],
            },
            Remotes,
            RemotesVerbose,
            HasBranch {
                name: "feature/x".into(),
            },
        ] {
            round_trip_query(q);
        }
    }

    #[test]
    fn actions_round_trip() {
        use GitAction::*;
        let p = vec![s("a.php"), s("b/c.php")];
        for a in [
            Stage { paths: p.clone() },
            AddNew { paths: p.clone() },
            Unstage { paths: p.clone() },
            UnstageInitial { paths: p.clone() },
            RestoreFromHead { paths: p.clone() },
            ForgetAdded { paths: p.clone() },
            Commit {
                message: "Add routes".into(),
                amend: false,
                paths: vec![],
            },
            Commit {
                message: "Add routes".into(),
                amend: true,
                paths: p.clone(),
            },
            Commit {
                message: "".into(),
                amend: true,
                paths: vec![],
            },
            Checkout { rev: "main".into() },
            CheckoutTracking {
                name: "x".into(),
                remote_branch: "origin/x".into(),
            },
            CheckoutNew {
                name: "x".into(),
                start: None,
            },
            CheckoutNew {
                name: "x".into(),
                start: Some("main".into()),
            },
            BranchNew {
                name: "x".into(),
                start: Some("main".into()),
            },
            BranchNew {
                name: "x".into(),
                start: None,
            },
            BranchDelete {
                name: "x".into(),
                force: true,
            },
            BranchDelete {
                name: "x".into(),
                force: false,
            },
            BranchRename {
                old: "a".into(),
                new: "b".into(),
            },
            Merge { rev: "dev".into() },
            Rebase {
                onto: "main".into(),
            },
            Push {
                force: false,
                set_upstream: None,
            },
            Push {
                force: true,
                set_upstream: Some("origin".into()),
            },
            Pull { rebase: true },
            Pull { rebase: false },
            Fetch,
            FetchRefspec {
                remote: "origin".into(),
                refspec: "+refs/pull/7/head:refs/heads/pr-7".into(),
            },
        ] {
            round_trip_action(a);
        }
    }

    /// Not parsed, or parsed into a call the host's policy refuses.
    fn refused(args: &[&str]) -> bool {
        use crate::excludes::Excludes;
        use crate::guest_backend::calls::HostCall;
        use crate::permissions::{Permissions, Policy};
        let policy = Policy::new(Permissions::FULL_ACCESS, Excludes::defaults());
        match parse(&req(args)) {
            Err(e) => {
                assert!(e.contains("not available in Code With Me sessions"), "{e}");
                true
            }
            Ok(GitCall::Query(q)) => policy.check_call(&HostCall::Git(q)).is_err(),
            Ok(GitCall::Action(a)) => policy.check_call(&HostCall::GitAction(a)).is_err(),
        }
    }

    #[test]
    fn commands_forge_never_issues_are_refused() {
        for args in [
            &[
                "-c",
                "core.sshCommand=touch /tmp/x",
                "fetch",
                "--all",
                "--prune",
            ][..],
            &["config", "--global", "alias.x", "!sh"],
            &[
                "status",
                "--porcelain=v2",
                "-z",
                "--branch",
                "--untracked-files=all",
                "--ext-diff",
            ],
            &["show", "HEAD"],
            &["diff", "--ext-diff", "--"],
            &["push", "--receive-pack=sh"],
            &["log", "--output=/tmp/x"],
            &["clean", "-fdx"],
            &["reset", "--hard"],
            &["clone", "x", "y"],
            &[],
        ] {
            assert!(refused(args), "{args:?}");
        }
    }

    #[test]
    fn options_in_revisions_remotes_and_pathspecs_are_refused_by_the_policy() {
        for args in [
            &["fetch", "--upload-pack=sh", "origin"][..],
            &["fetch", "origin", "--upload-pack=sh"],
            &["checkout", "--orphan", "--"],
            &["merge", "--no-edit", "--strategy-option=x"],
            &["rebase", "--exec=sh"],
            &["branch", "--set-upstream-to=x"],
            &["show", "HEAD:../../etc/passwd"],
            &["show", "-s", COMMIT_META_FORMAT, "HEAD:.env"],
            &["show", ":.env"],
            &[
                "diff",
                "--no-ext-diff",
                "--no-color",
                "-M",
                "--",
                ":(top)../x",
            ],
            &["diff", "--no-ext-diff", "--no-color", "-M", "--", "../x"],
            &["blame", "--porcelain", "--", "../x"],
            &["ls-files", "-z", "--cached", "--", ":(glob)**"],
        ] {
            assert!(refused(args), "{args:?}");
        }
    }

    #[test]
    fn the_host_rebuilds_the_command_from_the_typed_value() {
        let GitCall::Action(a) = parse(&req(&["fetch", "origin", "refs/heads/x"])).unwrap() else {
            panic!("an action")
        };
        assert_eq!(a.to_args(), vec!["fetch", "origin", "refs/heads/x"]);
    }

    fn allowed(p: &str) -> bool {
        !p.starts_with(".git") && !p.ends_with(".env") && !p.contains("secret")
    }

    fn visible(prefix: &str) -> Visible<'_> {
        Visible {
            prefix,
            allowed: &allowed,
        }
    }

    #[test]
    fn status_drops_hidden_files_and_other_folders_of_the_repository() {
        let raw = "# branch.oid abc\0# branch.head main\0\
1 .M N... 100644 100644 100644 aaa bbb app/User.php\0\
1 .M N... 100644 100644 100644 aaa bbb .env\0\
2 R. N... 100644 100644 100644 aaa bbb R100 app/New.php\0app/Old.php\0\
2 R. N... 100644 100644 100644 aaa bbb R100 app/Leak.php\0config/secret.key\0\
u UU N... 100644 100644 100644 100644 a b c app/Merge.php\0\
? docs/.env\0? app/Free.php\0! secret/x\0? other/file.php\0";
        let out = visible("").status(raw);
        let kept: Vec<&str> = out.split('\0').filter(|t| !t.is_empty()).collect();
        assert_eq!(
            kept,
            vec![
                "# branch.oid abc",
                "# branch.head main",
                "1 .M N... 100644 100644 100644 aaa bbb app/User.php",
                "2 R. N... 100644 100644 100644 aaa bbb R100 app/New.php",
                "app/Old.php",
                "u UU N... 100644 100644 100644 100644 a b c app/Merge.php",
                "? app/Free.php",
                "? other/file.php",
            ]
        );
        // The project is a folder of a larger repository.
        let out = visible("app/").status(raw);
        let kept: Vec<&str> = out.split('\0').filter(|t| !t.is_empty()).collect();
        assert!(kept
            .iter()
            .all(|t| !t.contains("other/") && !t.contains("docs/")));
        assert!(kept.contains(&"? app/Free.php"));
    }

    #[test]
    fn diffs_lose_the_sections_of_hidden_files() {
        let raw = "diff --git a/app/A.php b/app/A.php\n--- a/app/A.php\n+++ b/app/A.php\n@@ -1 +1 @@\n-x\n+y\n\
diff --git a/.env b/.env\n--- a/.env\n+++ b/.env\n@@ -1 +1 @@\n-APP_KEY=1\n+APP_KEY=2\n\
diff --git a/old.php b/config/secret.php\nsimilarity index 90%\nrename from old.php\nrename to config/secret.php\n\
diff --git a/b.php b/b.php\n+z\n";
        let out = visible("").diff(raw);
        assert!(out.contains("app/A.php") && out.contains("b/b.php"));
        assert!(!out.contains("APP_KEY") && !out.contains("secret") && !out.contains(".env"));
        // Unreadable headers are dropped.
        assert_eq!(visible("").diff("diff --git \"a/x\" \"b/x\"\n+boo\n"), "");
    }

    #[test]
    fn file_lists_and_commit_files_are_filtered() {
        let v = visible("");
        assert_eq!(v.names("a.php\0.env\0b.php\0"), "a.php\0b.php\0");
        assert_eq!(
            v.name_status("M\0a.php\0A\0.env\0R100\0old.php\0new.php\0R90\0x.php\0secret/y.php\0"),
            "M\0a.php\0R100\0old.php\0new.php\0"
        );
        let id = "0123456789abcdef0123456789abcdef01234567";
        assert_eq!(
            v.name_status(&format!("{id}\0A\0a.php\0A\0.env\0")),
            format!("{id}\0A\0a.php\0")
        );
    }

    #[test]
    fn pathspecs_must_name_the_project_folder_or_something_in_it() {
        let v = visible("sub/app/");
        assert!(v.pathspec("sub/app"));
        assert!(v.pathspec("sub/app/"));
        assert!(v.pathspec("sub/app/src/a.php"));
        assert!(!v.pathspec("."));
        assert!(!v.pathspec("sub/other/a.php"));
        assert!(!v.pathspec("sub/app/.env"));
        assert!(!v.pathspec("../x"));
        let root = visible("");
        assert!(root.pathspec("."));
        assert!(root.pathspec(""));
        assert!(root.pathspec("a/b.php"));
        assert!(!root.pathspec(".git/config"));
    }
}
