//! Git Log actions that rewrite history (Edit Commit Message, Squash Commits, Drop Commits,
//! Undo Commit, Fixup / Squash Into) and the interactive rebase ("Rebasing Commits"). Pure:
//! which actions a log selection allows (with PHPStorm's reasons), the initial plan of the
//! rebase dialog (autosquash) and the `git-rebase-todo` Forge hands to git.
//!
//! Forge never opens the user's editor: the todo and the new commit messages are stored as
//! blobs (`git hash-object -w`), the sequence editor copies the todo blob over git's file and
//! a message is applied with an `exec` step that amends the commit from its blob. This works
//! the same on remote hosts, where Forge can only run git.

use serde::{Deserialize, Serialize};

/// What the rebase does with one commit (the "Rebasing Commits" table).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RebaseAction {
    Pick,
    /// Stop after applying the commit so it can be amended.
    Edit,
    /// Pick with a new message.
    Reword,
    /// Meld into the previous commit, combining the messages.
    Squash,
    /// Meld into the previous commit, keeping its message.
    Fixup,
    Drop,
}

/// One row of the rebase plan, in git's order (oldest first).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RebaseEntry {
    pub sha: String,
    pub action: RebaseAction,
    /// The new message: of the commit (reword) or of the commit a squash group results in.
    #[serde(default)]
    pub message: Option<String>,
}

/// A commit of the interactive rebase dialog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RebaseCommit {
    pub sha: String,
    pub subject: String,
    /// The full message (subject and body).
    pub message: String,
    pub author: String,
    /// Author time, seconds since the Unix epoch.
    pub time: i64,
}

/// Everything "Interactively Rebase from Here…" needs: the commits from the selected one up
/// to HEAD and the initial actions (`fixup!` / `squash!` commits already moved after their
/// target, like `git rebase --autosquash`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RebasePlan {
    /// The commit the rebase starts on (the parent of the oldest commit); `None` for `--root`.
    pub base: Option<String>,
    /// The branch being rebased, `None` on a detached HEAD.
    pub branch: Option<String>,
    /// Oldest first.
    pub commits: Vec<RebaseCommit>,
    /// Oldest first, in the order git will apply them.
    pub entries: Vec<RebaseEntry>,
}

/// One instruction of the todo file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "value")]
pub enum TodoStep {
    Pick(String),
    Edit(String),
    Squash(String),
    Fixup(String),
    Drop(String),
    /// Amend HEAD with message number `n` of [`plan_todo`]'s messages.
    Amend(usize),
}

/// Whether `sha` looks like an object name (so it cannot inject todo commands).
pub fn is_sha(sha: &str) -> bool {
    (4..=64).contains(&sha.len()) && sha.bytes().all(|b| b.is_ascii_hexdigit())
}

fn subject_of(message: &str) -> &str {
    message.lines().next().unwrap_or_default().trim()
}

/// The initial plan: every commit picked, `fixup! X` / `squash! X` (and `amend! X`) commits
/// moved right after the commit whose subject is `X` (or whose hash starts with `X`).
pub fn autosquash(commits: &[RebaseCommit]) -> Vec<RebaseEntry> {
    let mut entries: Vec<RebaseEntry> = Vec::with_capacity(commits.len());
    for (i, c) in commits.iter().enumerate() {
        let subject = c.subject.as_str();
        let (action, rest) = if let Some(r) = subject.strip_prefix("fixup! ") {
            (RebaseAction::Fixup, r)
        } else if let Some(r) = subject.strip_prefix("amend! ") {
            (RebaseAction::Fixup, r)
        } else if let Some(r) = subject.strip_prefix("squash! ") {
            (RebaseAction::Squash, r)
        } else {
            (RebaseAction::Pick, "")
        };
        let target = (action != RebaseAction::Pick)
            .then(|| {
                // Nested prefixes ("fixup! fixup! X") target X as well.
                let mut want = rest.trim();
                while let Some(r) = want
                    .strip_prefix("fixup! ")
                    .or_else(|| want.strip_prefix("squash! "))
                    .or_else(|| want.strip_prefix("amend! "))
                {
                    want = r.trim();
                }
                commits[..i]
                    .iter()
                    .position(|t| {
                        subject_of(&t.message) == want
                            || (is_sha(want) && want.len() >= 7 && t.sha.starts_with(want))
                    })
                    .map(|at| commits[at].sha.clone())
            })
            .flatten();
        match target {
            Some(target) => {
                // After the target and the fixups already attached to it.
                let mut at = entries
                    .iter()
                    .position(|e| e.sha == target)
                    .map_or(entries.len(), |p| p + 1);
                while at < entries.len()
                    && matches!(
                        entries[at].action,
                        RebaseAction::Fixup | RebaseAction::Squash
                    )
                {
                    at += 1;
                }
                entries.insert(
                    at,
                    RebaseEntry {
                        sha: c.sha.clone(),
                        action,
                        message: None,
                    },
                );
            }
            None => entries.push(RebaseEntry {
                sha: c.sha.clone(),
                action: RebaseAction::Pick,
                message: None,
            }),
        }
    }
    entries
}

/// The todo steps of a plan and the messages they amend with.
///
/// A commit and the squash/fixup commits that follow it form a group (dropped commits in
/// between do not end it). A group gets a new message from its last squash with a message,
/// else from a reworded first commit; it is then applied with `fixup` steps followed by one
/// amend, so git never asks for a message. Squashes without a message keep git's combined
/// message.
pub fn plan_todo(entries: &[RebaseEntry]) -> Result<(Vec<TodoStep>, Vec<String>), String> {
    if entries.is_empty() {
        return Err("there are no commits to rebase".into());
    }
    for e in entries {
        if !is_sha(&e.sha) {
            return Err(format!("invalid commit: {:?}", e.sha));
        }
        if matches!(e.action, RebaseAction::Reword | RebaseAction::Squash)
            && e.message.as_deref().is_some_and(|m| m.trim().is_empty())
        {
            return Err("the commit message is empty".into());
        }
    }
    let mut steps = Vec::new();
    let mut messages = Vec::new();
    let mut i = 0;
    while i < entries.len() {
        let head = &entries[i];
        match head.action {
            RebaseAction::Drop => {
                steps.push(TodoStep::Drop(head.sha.clone()));
                i += 1;
                continue;
            }
            RebaseAction::Squash | RebaseAction::Fixup => {
                return Err(format!(
                    "commit {} cannot be squashed or fixed up: there is no previous commit to meld it into",
                    &head.sha[..head.sha.len().min(8)]
                ));
            }
            _ => {}
        }
        // The group: the head plus following squash/fixup (and drop) entries.
        let mut end = i + 1;
        while end < entries.len()
            && matches!(
                entries[end].action,
                RebaseAction::Squash | RebaseAction::Fixup | RebaseAction::Drop
            )
        {
            end += 1;
        }
        let group = &entries[i..end];
        let message = group[1..]
            .iter()
            .rev()
            .filter(|e| e.action == RebaseAction::Squash)
            .find_map(|e| e.message.clone())
            .or_else(|| {
                (head.action == RebaseAction::Reword)
                    .then(|| head.message.clone())
                    .flatten()
            });
        steps.push(match head.action {
            RebaseAction::Edit => TodoStep::Edit(head.sha.clone()),
            _ => TodoStep::Pick(head.sha.clone()),
        });
        for e in &group[1..] {
            steps.push(match e.action {
                RebaseAction::Drop => TodoStep::Drop(e.sha.clone()),
                RebaseAction::Squash if message.is_none() => TodoStep::Squash(e.sha.clone()),
                _ => TodoStep::Fixup(e.sha.clone()),
            });
        }
        if let Some(m) = message {
            messages.push(m);
            // Drops are no-ops; the amend goes after the last melded commit.
            let at = steps
                .iter()
                .rposition(|s| !matches!(s, TodoStep::Drop(_)))
                .map_or(steps.len(), |p| p + 1);
            steps.insert(at, TodoStep::Amend(messages.len() - 1));
        }
        i = end;
    }
    Ok((steps, messages))
}

/// The `exec` command that replaces HEAD's message with the one stored in blob `blob`.
pub fn amend_command(blob: &str) -> String {
    format!(
        "git cat-file blob {blob} | git commit --amend --allow-empty --no-verify --cleanup=whitespace -F -"
    )
}

/// The todo file; `blobs[n]` holds message `n`.
pub fn render_todo(steps: &[TodoStep], blobs: &[String]) -> String {
    let mut out = String::new();
    for s in steps {
        let line = match s {
            TodoStep::Pick(sha) => format!("pick {sha}"),
            TodoStep::Edit(sha) => format!("edit {sha}"),
            TodoStep::Squash(sha) => format!("squash {sha}"),
            TodoStep::Fixup(sha) => format!("fixup {sha}"),
            TodoStep::Drop(sha) => format!("drop {sha}"),
            TodoStep::Amend(n) => format!("exec {}", amend_command(&blobs[*n])),
        };
        out.push_str(&line);
        out.push('\n');
    }
    out
}

/// `sequence.editor` that copies the todo blob over git's todo file (git runs it through
/// `sh -c '<editor> "$@"'`, also with Git for Windows).
pub fn sequence_editor(todo_blob: &str) -> String {
    format!("git cat-file blob {todo_blob} >")
}

// ---- which Log actions a selection allows ------------------------------------------------------

/// Patterns of Settings | Version Control | Git, "Protected branches" (`;` or `,` separated).
pub fn parse_protected(patterns: &str) -> Vec<String> {
    patterns
        .split([';', ',', '\n'])
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .collect()
}

/// Whether the branch name (without the remote) matches a protected pattern (`*` wildcards,
/// e.g. `release/*`).
pub fn is_protected(branch: &str, patterns: &[String]) -> bool {
    fn matches(p: &[u8], s: &[u8]) -> bool {
        match (p.first(), s.first()) {
            (None, None) => true,
            (Some(b'*'), _) => matches(&p[1..], s) || (!s.is_empty() && matches(p, &s[1..])),
            (Some(a), Some(b)) if a == b => matches(&p[1..], &s[1..]),
            _ => false,
        }
    }
    patterns
        .iter()
        .any(|p| matches(p.as_bytes(), branch.as_bytes()))
}

/// What the repository knows about one selected commit.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CommitFacts {
    pub sha: String,
    /// Reachable from HEAD.
    pub in_current_branch: bool,
    /// HEAD itself.
    pub head: bool,
    pub merge: bool,
    pub root: bool,
    /// A remote branch that contains the commit (`origin/feature`).
    pub pushed_to: Option<String>,
    /// A protected remote branch that contains it (`origin/main`).
    pub protected_branch: Option<String>,
}

/// The selection of the Git Log and the state of the repository.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SelectionFacts {
    /// In log order (newest first).
    pub commits: Vec<CommitFacts>,
    /// Merge commits between the oldest selected commit and HEAD (a rebase would flatten them).
    pub merges_after: bool,
    /// The operation in progress ("rebase", "merge", "cherry-pick", "revert").
    pub operation: Option<String>,
    /// HEAD is detached.
    pub detached: bool,
}

/// Whether an action of the Log context menu can run, and why not.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionState {
    pub enabled: bool,
    /// Why the action is disabled (shown as the item's tooltip).
    pub reason: Option<String>,
    /// The action may run, but the user should confirm (commits already pushed).
    pub warning: Option<String>,
}

impl ActionState {
    fn on() -> Self {
        Self {
            enabled: true,
            ..Self::default()
        }
    }
    fn off(reason: impl Into<String>) -> Self {
        Self {
            enabled: false,
            reason: Some(reason.into()),
            warning: None,
        }
    }
}

/// The Log context menu actions that depend on the repository.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogActionStates {
    pub cherry_pick: ActionState,
    pub revert: ActionState,
    pub undo_commit: ActionState,
    pub edit_message: ActionState,
    pub fixup: ActionState,
    pub squash_into: ActionState,
    pub squash_commits: ActionState,
    pub drop_commits: ActionState,
    pub interactive_rebase: ActionState,
}

fn operation_reason(op: &str) -> String {
    let noun = match op {
        "rebase" => "a rebase",
        "merge" => "a merge",
        "cherryPick" | "cherry-pick" => "a cherry-pick",
        "revert" => "a revert",
        other => other,
    };
    format!("Not available while {noun} is in progress")
}

/// Rules shared by the actions that rewrite the current branch.
fn rewrite_state(f: &SelectionFacts, flattens: bool) -> ActionState {
    let commits = &f.commits;
    let many = commits.len() > 1;
    if let Some(op) = &f.operation {
        return ActionState::off(operation_reason(op));
    }
    if f.detached {
        return ActionState::off("Not available on a detached HEAD");
    }
    if commits.iter().any(|c| !c.in_current_branch) {
        return ActionState::off(if many {
            "Some of the selected commits are not in the current branch"
        } else {
            "The selected commit is not in the current branch"
        });
    }
    if let Some(p) = commits.iter().find_map(|c| c.protected_branch.as_deref()) {
        return ActionState::off(format!(
            "The selected commit{} already pushed to protected branch '{p}'",
            if many { "s are" } else { " is" }
        ));
    }
    if commits.iter().any(|c| c.merge) {
        return ActionState::off("Not available for merge commits");
    }
    if flattens && f.merges_after {
        return ActionState::off(
            "The branch contains merge commits after the selected commit; a rebase would flatten them",
        );
    }
    let mut state = ActionState::on();
    if let Some(p) = commits.iter().find_map(|c| c.pushed_to.as_deref()) {
        state.warning = Some(format!(
            "The selected commit{} already pushed to '{p}'. Rewriting the history requires a force push.",
            if many { "s are" } else { " is" }
        ));
    }
    state
}

fn single(f: &SelectionFacts, state: ActionState) -> ActionState {
    if f.commits.len() == 1 {
        state
    } else {
        ActionState::off("Select a single commit")
    }
}

/// The states of the Log actions for a selection, with PHPStorm's reasons.
pub fn log_action_states(f: &SelectionFacts) -> LogActionStates {
    let commits = &f.commits;
    if commits.is_empty() {
        let none = ActionState::off("No commits are selected");
        return LogActionStates {
            cherry_pick: none.clone(),
            revert: none.clone(),
            undo_commit: none.clone(),
            edit_message: none.clone(),
            fixup: none.clone(),
            squash_into: none.clone(),
            squash_commits: none.clone(),
            drop_commits: none.clone(),
            interactive_rebase: none,
        };
    }
    let many = commits.len() > 1;
    let busy = f.operation.as_deref().map(operation_reason);

    let cherry_pick = if let Some(r) = &busy {
        ActionState::off(r.clone())
    } else if commits.iter().any(|c| c.in_current_branch) {
        ActionState::off(if many {
            "Some of the selected commits are already in the current branch"
        } else {
            "The selected commit is already in the current branch"
        })
    } else if commits.iter().any(|c| c.merge) {
        ActionState::off("Cherry-picking merge commits is not supported")
    } else {
        ActionState::on()
    };

    let revert = if let Some(r) = &busy {
        ActionState::off(r.clone())
    } else if commits.iter().any(|c| !c.in_current_branch) {
        ActionState::off(if many {
            "Some of the selected commits are not in the current branch"
        } else {
            "The selected commit is not in the current branch"
        })
    } else if commits.iter().any(|c| c.merge) {
        ActionState::off("Reverting merge commits is not supported")
    } else {
        ActionState::on()
    };

    let undo_commit = {
        let base = single(f, rewrite_state(f, false));
        if !base.enabled {
            base
        } else if !commits[0].head {
            ActionState::off(
                "Undo Commit is available only for the last commit of the current branch",
            )
        } else if let Some(p) = &commits[0].pushed_to {
            ActionState::off(format!("The selected commit is already pushed to '{p}'"))
        } else {
            ActionState::on()
        }
    };

    // Rewording HEAD amends it; older commits need a rebase.
    let edit_message = single(f, rewrite_state(f, !commits[0].head));
    let fixup = single(f, rewrite_state(f, true));
    let squash_into = fixup.clone();
    let squash_commits = if many {
        rewrite_state(f, true)
    } else {
        ActionState::off("Select at least two commits to squash")
    };
    let drop_commits = rewrite_state(f, true);
    let interactive_rebase = single(f, rewrite_state(f, true));

    LogActionStates {
        cherry_pick,
        revert,
        undo_commit,
        edit_message,
        fixup,
        squash_into,
        squash_commits,
        drop_commits,
        interactive_rebase,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sha(n: u8) -> String {
        format!("{n:02x}").repeat(20)
    }

    fn entry(n: u8, action: RebaseAction, message: Option<&str>) -> RebaseEntry {
        RebaseEntry {
            sha: sha(n),
            action,
            message: message.map(str::to_string),
        }
    }

    fn commit(n: u8, subject: &str) -> RebaseCommit {
        RebaseCommit {
            sha: sha(n),
            subject: subject.into(),
            message: format!("{subject}\n\nBody"),
            author: "Marlon".into(),
            time: 0,
        }
    }

    #[test]
    fn plain_picks_render_as_is() {
        use RebaseAction::*;
        let entries = [
            entry(1, Pick, None),
            entry(2, Edit, None),
            entry(3, Drop, None),
        ];
        let (steps, messages) = plan_todo(&entries).unwrap();
        assert!(messages.is_empty());
        assert_eq!(
            render_todo(&steps, &[]),
            format!("pick {}\nedit {}\ndrop {}\n", sha(1), sha(2), sha(3))
        );
    }

    #[test]
    fn reword_and_squash_messages_become_amends() {
        use RebaseAction::*;
        let entries = [
            entry(1, Reword, Some("Better subject")),
            entry(2, Pick, None),
            entry(3, Squash, None),
            entry(4, Drop, None),
            entry(5, Squash, Some("United\n\nBody")),
            entry(6, Fixup, None),
            entry(7, Pick, None),
            entry(8, Squash, None),
        ];
        let (steps, messages) = plan_todo(&entries).unwrap();
        assert_eq!(messages, vec!["Better subject", "United\n\nBody"]);
        let todo = render_todo(&steps, &["aaaa".into(), "bbbb".into()]);
        let lines: Vec<&str> = todo.lines().collect();
        assert_eq!(lines[0], format!("pick {}", sha(1)));
        assert_eq!(lines[1], format!("exec {}", amend_command("aaaa")));
        assert_eq!(lines[2], format!("pick {}", sha(2)));
        // The group has a message: its squashes become fixups and one amend follows.
        assert_eq!(lines[3], format!("fixup {}", sha(3)));
        assert_eq!(lines[4], format!("drop {}", sha(4)));
        assert_eq!(lines[5], format!("fixup {}", sha(5)));
        assert_eq!(lines[6], format!("fixup {}", sha(6)));
        assert_eq!(lines[7], format!("exec {}", amend_command("bbbb")));
        // Without a message git combines the messages itself.
        assert_eq!(lines[8], format!("pick {}", sha(7)));
        assert_eq!(lines[9], format!("squash {}", sha(8)));
        assert_eq!(lines.len(), 10);
    }

    #[test]
    fn an_amend_skips_trailing_drops() {
        use RebaseAction::*;
        let entries = [
            entry(1, Reword, Some("New")),
            entry(2, Drop, None),
            entry(3, Drop, None),
        ];
        let (steps, _) = plan_todo(&entries).unwrap();
        assert_eq!(
            steps,
            vec![
                TodoStep::Pick(sha(1)),
                TodoStep::Amend(0),
                TodoStep::Drop(sha(2)),
                TodoStep::Drop(sha(3)),
            ]
        );
    }

    #[test]
    fn invalid_plans_are_rejected() {
        use RebaseAction::*;
        assert!(plan_todo(&[]).is_err());
        assert!(plan_todo(&[entry(1, Squash, None), entry(2, Pick, None)]).is_err());
        assert!(plan_todo(&[entry(1, Drop, None), entry(2, Fixup, None)]).is_err());
        assert!(plan_todo(&[entry(1, Reword, Some("  "))]).is_err());
        let evil = RebaseEntry {
            sha: "abc\nexec rm -rf /".into(),
            action: Pick,
            message: None,
        };
        assert!(plan_todo(&[evil]).is_err());
        // A reword without a message keeps the old one.
        let (steps, messages) = plan_todo(&[entry(1, Reword, None)]).unwrap();
        assert_eq!((steps, messages.len()), (vec![TodoStep::Pick(sha(1))], 0));
    }

    #[test]
    fn autosquash_moves_fixups_after_their_target() {
        let commits = [
            commit(1, "Add cart"),
            commit(2, "Add checkout"),
            commit(3, "fixup! Add cart"),
            commit(4, "squash! Add cart"),
            commit(5, "fixup! Unknown commit"),
            commit(6, &format!("fixup! {}", &sha(2)[..8])),
        ];
        let plan = autosquash(&commits);
        let got: Vec<(String, RebaseAction)> =
            plan.iter().map(|e| (e.sha.clone(), e.action)).collect();
        use RebaseAction::*;
        assert_eq!(
            got,
            vec![
                (sha(1), Pick),
                (sha(3), Fixup),
                (sha(4), Squash),
                (sha(2), Pick),
                (sha(6), Fixup),
                (sha(5), Pick),
            ]
        );
    }

    #[test]
    fn protected_patterns() {
        let p = parse_protected("master; main ,release/*");
        assert_eq!(p, vec!["master", "main", "release/*"]);
        assert!(is_protected("main", &p));
        assert!(is_protected("release/1.2", &p));
        assert!(!is_protected("feature/main", &p));
        assert!(!is_protected("maintenance", &p));
    }

    fn facts(commits: Vec<CommitFacts>) -> SelectionFacts {
        SelectionFacts {
            commits,
            ..SelectionFacts::default()
        }
    }

    fn local(n: u8) -> CommitFacts {
        CommitFacts {
            sha: sha(n),
            in_current_branch: true,
            ..CommitFacts::default()
        }
    }

    #[test]
    fn unpushed_head_allows_every_rewrite() {
        let s = log_action_states(&facts(vec![CommitFacts {
            head: true,
            ..local(1)
        }]));
        assert!(s.undo_commit.enabled && s.edit_message.enabled && s.interactive_rebase.enabled);
        assert!(s.drop_commits.enabled && s.fixup.enabled && s.revert.enabled);
        assert!(!s.cherry_pick.enabled);
        assert!(!s.squash_commits.enabled, "needs two commits");
    }

    #[test]
    fn pushed_commits_warn_and_protected_ones_are_disabled() {
        let pushed = log_action_states(&facts(vec![CommitFacts {
            head: true,
            pushed_to: Some("origin/feature".into()),
            ..local(1)
        }]));
        assert!(!pushed.undo_commit.enabled);
        assert!(pushed.edit_message.enabled);
        assert!(pushed
            .edit_message
            .warning
            .as_deref()
            .unwrap()
            .contains("'origin/feature'"));

        let protected = log_action_states(&facts(vec![CommitFacts {
            pushed_to: Some("origin/main".into()),
            protected_branch: Some("origin/main".into()),
            ..local(1)
        }]));
        assert!(!protected.edit_message.enabled);
        assert_eq!(
            protected.drop_commits.reason.as_deref(),
            Some("The selected commit is already pushed to protected branch 'origin/main'")
        );
        // Reverting a pushed commit is the safe way to undo it.
        assert!(protected.revert.enabled);
    }

    #[test]
    fn merges_operations_and_other_branches() {
        let mut f = facts(vec![local(2), local(1)]);
        f.merges_after = true;
        let s = log_action_states(&f);
        assert!(!s.squash_commits.enabled && !s.drop_commits.enabled);
        assert!(!s.edit_message.enabled, "single selection only");

        let mut f = facts(vec![CommitFacts {
            head: true,
            ..local(1)
        }]);
        f.operation = Some("rebase".into());
        let s = log_action_states(&f);
        assert_eq!(
            s.undo_commit.reason.as_deref(),
            Some("Not available while a rebase is in progress")
        );
        assert!(!s.revert.enabled);

        let other = log_action_states(&facts(vec![CommitFacts {
            sha: sha(9),
            ..CommitFacts::default()
        }]));
        assert!(other.cherry_pick.enabled);
        assert!(!other.revert.enabled && !other.edit_message.enabled);

        let merge = log_action_states(&facts(vec![CommitFacts {
            merge: true,
            ..CommitFacts::default()
        }]));
        assert!(!merge.cherry_pick.enabled);
    }
}
