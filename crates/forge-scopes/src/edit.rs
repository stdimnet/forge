//! The Include / Include Recursively / Exclude / Exclude Recursively buttons of the Scopes
//! page: they add a `file:` pattern for the selected file or folder to the scope's pattern.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EditOp {
    Include,
    IncludeRecursively,
    Exclude,
    ExcludeRecursively,
}

/// One button press on the Scopes page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditRequest {
    pub pattern: String,
    pub op: EditOp,
    /// Project-relative file or folder; empty for the project root.
    pub path: String,
    #[serde(default)]
    pub dir: bool,
}

/// The `file:` pattern of a file or folder: `file:app/*` (the folder's files),
/// `file:app//*` (recursively), `file:app/User.php`.
pub fn file_pattern(path: &str, dir: bool, recursive: bool) -> String {
    let p = path.trim_matches('/');
    match (dir, recursive, p.is_empty()) {
        (true, true, true) => "file:*".into(),
        (true, false, true) => "file:/*".into(),
        (true, true, false) => format!("file:{p}//*"),
        (true, false, false) => format!("file:{p}/*"),
        (false, _, _) => format!("file:{p}"),
    }
}

/// `pattern` split at its top-level `||` (outside parentheses), trimmed.
fn alternatives(pattern: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut current = String::new();
    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '(' => depth += 1,
            ')' => depth -= 1,
            '|' if depth == 0 && chars.get(i + 1) == Some(&'|') => {
                out.push(current.trim().to_string());
                current.clear();
                i += 2;
                continue;
            }
            _ => {}
        }
        current.push(chars[i]);
        i += 1;
    }
    out.push(current.trim().to_string());
    out.retain(|p| !p.is_empty());
    out
}

/// The pattern after a button press.
pub fn apply(req: &EditRequest) -> String {
    let recursive = matches!(
        req.op,
        EditOp::IncludeRecursively | EditOp::ExcludeRecursively
    );
    let piece = file_pattern(&req.path, req.dir || req.path.is_empty(), recursive);
    let pattern = req.pattern.trim();
    match req.op {
        EditOp::Include | EditOp::IncludeRecursively => {
            // Undo an exclusion of the same thing first.
            let negated = format!("&&!{piece}");
            let pattern = pattern.replace(&negated, "");
            let pattern = pattern.trim();
            if pattern.is_empty() {
                return piece;
            }
            if alternatives(pattern).contains(&piece) {
                return pattern.to_string();
            }
            format!("{pattern}||{piece}")
        }
        EditOp::Exclude | EditOp::ExcludeRecursively => {
            let alts = alternatives(pattern);
            if alts.contains(&piece) {
                // Excluding what an alternative added just drops that alternative.
                return alts
                    .into_iter()
                    .filter(|a| *a != piece)
                    .collect::<Vec<_>>()
                    .join("||");
            }
            if pattern.is_empty() {
                return format!("!{piece}");
            }
            if pattern.ends_with(&format!("&&!{piece}")) {
                return pattern.to_string();
            }
            if alts.len() > 1 {
                format!("({pattern})&&!{piece}")
            } else {
                format!("{pattern}&&!{piece}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Scope, ScopeContext};

    fn press(pattern: &str, op: EditOp, path: &str, dir: bool) -> String {
        apply(&EditRequest {
            pattern: pattern.into(),
            op,
            path: path.into(),
            dir,
        })
    }

    #[test]
    fn include_and_exclude_build_patterns() {
        let p = press("", EditOp::IncludeRecursively, "app", true);
        assert_eq!(p, "file:app//*");
        let p = press(&p, EditOp::Include, "routes", true);
        assert_eq!(p, "file:app//*||file:routes/*");
        assert_eq!(press(&p, EditOp::Include, "routes", true), p);
        let p = press(&p, EditOp::ExcludeRecursively, "app/Legacy", true);
        assert_eq!(p, "(file:app//*||file:routes/*)&&!file:app/Legacy//*");
        let scope = Scope::pattern(&p, ScopeContext::default()).unwrap();
        assert!(scope.contains("app/Models/User.php"));
        assert!(scope.contains("routes/web.php"));
        assert!(!scope.contains("app/Legacy/Old.php"));
        // Including it again lifts the exclusion.
        let p = press(&p, EditOp::IncludeRecursively, "app/Legacy", true);
        assert_eq!(p, "(file:app//*||file:routes/*)||file:app/Legacy//*");
        assert_eq!(
            press(
                "file:app//*||file:routes/*",
                EditOp::Exclude,
                "routes",
                true
            ),
            "file:app//*"
        );
        assert_eq!(
            press("file:app//*", EditOp::Exclude, "app/User.php", false),
            "file:app//*&&!file:app/User.php"
        );
        assert_eq!(press("", EditOp::Exclude, "vendor", true), "!file:vendor/*");
        assert_eq!(press("", EditOp::IncludeRecursively, "", true), "file:*");
        assert_eq!(file_pattern("", true, false), "file:/*");
        let root = Scope::pattern("file:/*", ScopeContext::default()).unwrap();
        assert!(root.contains("composer.json") && !root.contains("app/User.php"));
    }
}
