//! The Navigation Bar (View | Appearance | Navigation Bar, Alt+Home): the path of the
//! active file as segments, `project › app › Models › User.php › User › posts()`, with the
//! members around the caret when "Members in Navigation Bar" is on.
//!
//! The members come from the editor's scopes ([`crate::structure`]); every segment knows
//! what its dropdown lists (a directory's entries, a file's or class's members).

use serde::{Deserialize, Serialize};

use crate::structure::{Scope, ScopeKind};

/// What a segment stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SegmentKind {
    Project,
    Directory,
    File,
    Member,
}

/// One segment of the bar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NavSegment {
    pub kind: SegmentKind,
    pub label: String,
    /// The directory or file (workspace-relative, `""` for the project).
    pub path: String,
    /// Members: the scope kind and where its name is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope_kind: Option<ScopeKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NavBarRequest {
    /// The project's name.
    pub project: String,
    /// The active file, workspace-relative (`/`-separated).
    pub path: String,
    /// The scopes around the caret, outermost first (`scopesAt` of the editor structure).
    #[serde(default)]
    pub scopes: Vec<Scope>,
    #[serde(default)]
    pub show_members: bool,
}

/// Scopes the bar shows as members (PHPStorm: classes and their functions).
fn is_member(kind: ScopeKind) -> bool {
    matches!(
        kind,
        ScopeKind::Class
            | ScopeKind::Interface
            | ScopeKind::Trait
            | ScopeKind::Enum
            | ScopeKind::Function
            | ScopeKind::Method
    )
}

/// The segments of the bar, project first.
pub fn nav_bar_segments(req: &NavBarRequest) -> Vec<NavSegment> {
    let mut out = vec![NavSegment {
        kind: SegmentKind::Project,
        label: req.project.clone(),
        path: String::new(),
        scope_kind: None,
        line: None,
        column: None,
    }];
    let parts: Vec<&str> = req.path.split('/').filter(|p| !p.is_empty()).collect();
    for (i, part) in parts.iter().enumerate() {
        let last = i + 1 == parts.len();
        out.push(NavSegment {
            kind: if last {
                SegmentKind::File
            } else {
                SegmentKind::Directory
            },
            label: part.to_string(),
            path: parts[..=i].join("/"),
            scope_kind: None,
            line: None,
            column: None,
        });
    }
    if req.show_members && !parts.is_empty() {
        for s in req.scopes.iter().filter(|s| is_member(s.kind)) {
            out.push(NavSegment {
                kind: SegmentKind::Member,
                label: s.name.clone(),
                path: req.path.clone(),
                scope_kind: Some(s.kind),
                line: Some(s.line),
                column: Some(s.column),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structure::editor_structure;

    #[test]
    fn segments_from_the_path_and_the_members_at_the_caret() {
        let text = "<?php\nclass User {\n    public function posts() {\n        if ($a) {\n            return 1;\n        }\n    }\n}\n";
        let structure = editor_structure(text, "php");
        // The caret inside `if` inside `posts()`.
        let scopes: Vec<Scope> = structure
            .scopes
            .iter()
            .filter(|s| s.line <= 4 && s.end_line >= 4)
            .cloned()
            .collect();
        let req = NavBarRequest {
            project: "shop".into(),
            path: "app/Models/User.php".into(),
            scopes,
            show_members: true,
        };
        let segs = nav_bar_segments(&req);
        let labels: Vec<&str> = segs.iter().map(|s| s.label.as_str()).collect();
        assert_eq!(
            labels,
            ["shop", "app", "Models", "User.php", "User", "posts()"]
        );
        assert_eq!(segs[2].path, "app/Models");
        assert_eq!(segs[3].kind, SegmentKind::File);
        assert_eq!(segs[5].kind, SegmentKind::Member);
        assert_eq!(segs[5].line, Some(2));

        let plain = nav_bar_segments(&NavBarRequest {
            show_members: false,
            ..req
        });
        assert_eq!(plain.len(), 4);
    }
}
