//! Branches grouped into folders at each `/` (PHPStorm's "Group By Directory"):
//! `feat/ui/tabs` is the branch `tabs` in the folder `ui` in the folder `feat`. Remote
//! branches are grouped below their remote first (`origin` > `feat` > `tabs`).
//!
//! On every level folders come first, then branches; the current branch leads the branches.
//! Names sort without regard to case, and numbers by value (`release/1.9` before
//! `release/1.10`).

use std::cmp::Ordering;

use serde::Serialize;

use crate::ops::split_remote_branch;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BranchNodeKind {
    /// A remote (`origin`), the top level of the remote tree.
    Remote,
    /// A name prefix up to a `/`.
    Folder,
    Branch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchNode {
    pub kind: BranchNodeKind,
    /// The last segment of the name: `tabs` of `feat/ui/tabs`.
    pub label: String,
    /// A branch: its short name (`feat/ui/tabs`, `origin/main`). A folder or remote: its
    /// prefix without the trailing slash (`feat/ui`, `origin/feat`, `origin`).
    pub path: String,
    /// The current branch, or a folder that contains it.
    pub current: bool,
    pub children: Vec<BranchNode>,
}

impl BranchNode {
    fn folder(kind: BranchNodeKind, label: &str, path: String) -> Self {
        BranchNode {
            kind,
            label: label.to_string(),
            path,
            current: false,
            children: Vec::new(),
        }
    }

    fn is_branch(&self) -> bool {
        self.kind == BranchNodeKind::Branch
    }
}

/// The tree of local branches from `(short name, is current)` pairs.
pub fn local_branch_tree<'a>(
    branches: impl IntoIterator<Item = (&'a str, bool)>,
) -> Vec<BranchNode> {
    let mut roots = Vec::new();
    for (name, current) in branches {
        insert(&mut roots, "", name, name, current);
    }
    sort(&mut roots);
    roots
}

/// The tree of remote-tracking branches (`origin/feat/x`): one node per remote, folders
/// below. `remotes` are the configured remote names (they may contain slashes); a branch of
/// no known remote belongs to the remote named by its first segment.
pub fn remote_branch_tree<'a>(
    branches: impl IntoIterator<Item = &'a str>,
    remotes: &[String],
) -> Vec<BranchNode> {
    let mut roots: Vec<BranchNode> = Vec::new();
    for name in branches {
        let Some((remote, rest)) =
            split_remote_branch(name, remotes).or_else(|| name.split_once('/'))
        else {
            // Not `<remote>/<branch>`: keep it visible at the top level.
            insert(&mut roots, "", name, name, false);
            continue;
        };
        let at = match roots
            .iter()
            .position(|n| n.kind == BranchNodeKind::Remote && n.path == remote)
        {
            Some(i) => i,
            None => {
                roots.push(BranchNode::folder(
                    BranchNodeKind::Remote,
                    remote,
                    remote.to_string(),
                ));
                roots.len() - 1
            }
        };
        insert(&mut roots[at].children, remote, rest, name, false);
    }
    sort(&mut roots);
    roots
}

/// Add the branch `name` to `level`; `rest` is what is left of its name below `prefix`.
fn insert(level: &mut Vec<BranchNode>, prefix: &str, rest: &str, name: &str, current: bool) {
    // A leading, trailing or doubled slash is no folder (git rejects such names anyway).
    let split = rest
        .split_once('/')
        .filter(|(dir, tail)| !dir.is_empty() && !tail.is_empty());
    let Some((dir, tail)) = split else {
        level.push(BranchNode {
            kind: BranchNodeKind::Branch,
            label: rest.to_string(),
            path: name.to_string(),
            current,
            children: Vec::new(),
        });
        return;
    };
    let path = if prefix.is_empty() {
        dir.to_string()
    } else {
        format!("{prefix}/{dir}")
    };
    // A branch may be named like a folder (`feat` next to `feat/x`): they stay two nodes.
    let at = match level
        .iter()
        .position(|n| n.kind == BranchNodeKind::Folder && n.label == dir)
    {
        Some(i) => i,
        None => {
            level.push(BranchNode::folder(
                BranchNodeKind::Folder,
                dir,
                path.clone(),
            ));
            level.len() - 1
        }
    };
    let folder = &mut level[at];
    folder.current |= current;
    insert(&mut folder.children, &path, tail, name, current);
}

fn sort(level: &mut [BranchNode]) {
    level.sort_by(|a, b| {
        a.is_branch()
            .cmp(&b.is_branch())
            .then_with(|| {
                // Among branches the current one leads; folders stay in name order.
                if a.is_branch() {
                    b.current.cmp(&a.current)
                } else {
                    Ordering::Equal
                }
            })
            .then_with(|| natural_cmp(&a.label, &b.label))
    });
    for node in level {
        sort(&mut node.children);
    }
}

/// Case-insensitive order with digit runs compared by value; ties fall back to the exact
/// strings, so the order is total.
fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut x, mut y) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (x.peek().copied(), y.peek().copied()) {
            (None, None) => return a.cmp(b),
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(c), Some(d)) if c.is_ascii_digit() && d.is_ascii_digit() => {
                let run = |it: &mut std::iter::Peekable<std::str::Chars>| {
                    let mut digits = String::new();
                    while let Some(n) = it.peek().copied().filter(char::is_ascii_digit) {
                        digits.push(n);
                        it.next();
                    }
                    digits
                };
                let (m, n) = (run(&mut x), run(&mut y));
                let (m, n) = (m.trim_start_matches('0'), n.trim_start_matches('0'));
                let ord = m.len().cmp(&n.len()).then_with(|| m.cmp(n));
                if ord != Ordering::Equal {
                    return ord;
                }
            }
            (Some(c), Some(d)) => {
                let ord = c.to_lowercase().cmp(d.to_lowercase());
                if ord != Ordering::Equal {
                    return ord;
                }
                x.next();
                y.next();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tree as indented lines: `feat/` for folders, `origin@` for remotes, `*` on the
    /// current branch or a folder with it.
    fn outline(nodes: &[BranchNode]) -> Vec<String> {
        fn walk(nodes: &[BranchNode], depth: usize, out: &mut Vec<String>) {
            for n in nodes {
                let mark = match n.kind {
                    BranchNodeKind::Remote => "@",
                    BranchNodeKind::Folder => "/",
                    BranchNodeKind::Branch => "",
                };
                let star = if n.current { " *" } else { "" };
                out.push(format!("{}{}{mark}{star}", "  ".repeat(depth), n.label));
                walk(&n.children, depth + 1, out);
            }
        }
        let mut out = Vec::new();
        walk(nodes, 0, &mut out);
        out
    }

    fn local(names: &[&str], current: &str) -> Vec<BranchNode> {
        local_branch_tree(names.iter().map(|n| (*n, *n == current)))
    }

    #[test]
    fn branches_without_a_slash_stay_at_the_top_level() {
        let tree = local(&["main", "develop"], "main");
        assert_eq!(outline(&tree), ["main *", "develop"]);
        assert_eq!(tree[1].path, "develop");
        assert!(tree.iter().all(|n| n.children.is_empty()));
    }

    #[test]
    fn groups_by_prefix_and_nests_nested_prefixes() {
        let tree = local(
            &[
                "main",
                "feat/ui/tabs",
                "bugfix/login",
                "feat/launch-profiles",
                "feat/ui/tree",
            ],
            "",
        );
        assert_eq!(
            outline(&tree),
            [
                "bugfix/",
                "  login",
                "feat/",
                "  ui/",
                "    tabs",
                "    tree",
                "  launch-profiles",
                "main",
            ]
        );
        // Folders carry their prefix, branches their full name.
        let feat = &tree[1];
        assert_eq!(
            (feat.kind, feat.path.as_str()),
            (BranchNodeKind::Folder, "feat")
        );
        assert_eq!(feat.children[0].path, "feat/ui");
        assert_eq!(feat.children[0].children[0].path, "feat/ui/tabs");
        assert_eq!(feat.children[0].children[0].label, "tabs");
    }

    #[test]
    fn a_branch_named_like_a_folder_stays_next_to_it() {
        let tree = local(&["feat/x", "feat", "feat/y"], "");
        assert_eq!(outline(&tree), ["feat/", "  x", "  y", "feat"]);
        assert_eq!(tree[0].kind, BranchNodeKind::Folder);
        assert_eq!(tree[1].kind, BranchNodeKind::Branch);
        assert_eq!(
            (tree[0].path.as_str(), tree[1].path.as_str()),
            ("feat", "feat")
        );
    }

    #[test]
    fn folders_first_then_branches_with_the_current_one_leading() {
        let tree = local(
            &["zeta", "Alpha", "main", "wip/b", "Hotfix/a", "beta"],
            "main",
        );
        assert_eq!(
            outline(&tree),
            ["Hotfix/", "  a", "wip/", "  b", "main *", "Alpha", "beta", "zeta"]
        );
    }

    #[test]
    fn marks_the_folders_of_the_current_branch() {
        let tree = local(
            &["feat/ui/tabs", "feat/ui/tree", "feat/api", "fix/a"],
            "feat/ui/tree",
        );
        assert_eq!(
            outline(&tree),
            [
                "feat/ *",
                "  ui/ *",
                "    tree *",
                "    tabs",
                "  api",
                "fix/",
                "  a",
            ]
        );
    }

    #[test]
    fn numbers_sort_by_value() {
        let tree = local(
            &["release/1.10", "release/1.9", "release/1.2", "v10", "v9"],
            "",
        );
        assert_eq!(
            outline(&tree),
            ["release/", "  1.2", "  1.9", "  1.10", "v9", "v10"]
        );
        assert_eq!(natural_cmp("a01", "a1"), Ordering::Less);
        assert_eq!(natural_cmp("Main", "main"), Ordering::Less);
        assert_eq!(natural_cmp("main", "main"), Ordering::Equal);
    }

    #[test]
    fn remote_branches_are_grouped_below_their_remote() {
        let tree = remote_branch_tree(
            [
                "origin/main",
                "upstream/main",
                "origin/feat/launch-profiles",
                "origin/feat/ui/tabs",
                "origin/release/1.1",
            ],
            &["origin".to_string(), "upstream".to_string()],
        );
        assert_eq!(
            outline(&tree),
            [
                "origin@",
                "  feat/",
                "    ui/",
                "      tabs",
                "    launch-profiles",
                "  release/",
                "    1.1",
                "  main",
                "upstream@",
                "  main",
            ]
        );
        let origin = &tree[0];
        assert_eq!(
            (origin.kind, origin.path.as_str()),
            (BranchNodeKind::Remote, "origin")
        );
        assert_eq!(origin.children[0].path, "origin/feat");
        assert_eq!(origin.children[0].children[0].path, "origin/feat/ui");
        // The branch keeps its full name, the one git commands take.
        assert_eq!(
            origin.children[0].children[1].path,
            "origin/feat/launch-profiles"
        );
        assert_eq!(origin.children[0].children[1].label, "launch-profiles");
    }

    #[test]
    fn a_remote_name_may_contain_a_slash() {
        let remotes = ["team/fork".to_string(), "team".to_string()];
        let tree = remote_branch_tree(["team/fork/feat/x", "team/main"], &remotes);
        assert_eq!(
            outline(&tree),
            ["team@", "  main", "team/fork@", "  feat/", "    x"]
        );
        assert_eq!(tree[1].children[0].path, "team/fork/feat");
        assert_eq!(tree[1].children[0].children[0].path, "team/fork/feat/x");
    }

    #[test]
    fn an_unknown_remote_is_the_first_segment() {
        let tree = remote_branch_tree(["origin/feat/x", "odd"], &[]);
        assert_eq!(outline(&tree), ["origin@", "  feat/", "    x", "odd"]);
    }

    #[test]
    fn odd_slashes_make_no_empty_folders() {
        let tree = local(&["feat//x", "/lead", "trail/"], "");
        assert_eq!(outline(&tree), ["feat/", "  /x", "/lead", "trail/"]);
        assert!(tree[1..].iter().all(BranchNode::is_branch));
    }

    #[test]
    fn serializes_in_camel_case() {
        let tree = local(&["feat/x"], "feat/x");
        let json = serde_json::to_value(&tree).unwrap();
        assert_eq!(json[0]["kind"], "folder");
        assert_eq!(json[0]["current"], true);
        assert_eq!(json[0]["children"][0]["kind"], "branch");
        assert_eq!(json[0]["children"][0]["path"], "feat/x");
    }
}
