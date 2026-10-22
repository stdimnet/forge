//! File colors (Settings | Appearance & Behavior | File Colors): a color per scope, used as
//! the background of editor tabs and Project view rows. Colors are names of the theme's
//! palette; the UI maps them to the theme's `--file-color-*` variables (dark and light).

use serde::{Deserialize, Serialize};

use crate::{Scope, ScopeContext, ScopeFilter, ScopeSet};

/// The palette the themes provide, in PHPStorm's order.
pub const PALETTE: [&str; 7] = [
    "Blue", "Green", "Orange", "Rose", "Violet", "Yellow", "Gray",
];

/// The palette name for `color` (any case), `None` for unknown colors.
pub fn palette_color(color: &str) -> Option<&'static str> {
    PALETTE
        .into_iter()
        .find(|p| p.eq_ignore_ascii_case(color.trim()))
}

/// One row of the File Colors tables (`fileColors.local`, `fileColors.shared`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FileColor {
    /// A scope name: predefined ("Tests", "Non-Project Files", …) or from the Scopes page.
    pub scope: String,
    pub color: String,
}

/// A file or folder to color.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ColorTarget {
    pub path: String,
    pub dir: bool,
}

/// Which colors apply where.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ColorsRequest {
    pub scopes: ScopeSet,
    /// Local colors win over shared ones, like in PHPStorm.
    pub local: Vec<FileColor>,
    pub shared: Vec<FileColor>,
    pub context: ScopeContext,
    pub targets: Vec<ColorTarget>,
}

/// The color of every target (the palette name), `None` where no scope applies. The first
/// row whose scope contains the file wins.
pub fn resolve(req: &ColorsRequest) -> Vec<Option<String>> {
    let rows: Vec<(Scope, &'static str)> = req
        .local
        .iter()
        .chain(&req.shared)
        .filter_map(|row| {
            let color = palette_color(&row.color)?;
            let spec = req.scopes.resolve(row.scope.trim())?;
            let scope = Scope::new(&ScopeFilter {
                scope: spec,
                context: req.context.clone(),
            })
            .ok()?;
            Some((scope, color))
        })
        .collect();
    req.targets
        .iter()
        .map(|t| {
            rows.iter()
                .find(|(scope, _)| {
                    if t.dir {
                        scope.contains_dir(&t.path)
                    } else {
                        scope.contains(&t.path)
                    }
                })
                .map(|(_, color)| color.to_string())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NamedScope;

    #[test]
    fn first_matching_row_wins() {
        let req = ColorsRequest {
            scopes: ScopeSet {
                shared: vec![NamedScope {
                    name: "Models".into(),
                    pattern: "file:app/Models//*".into(),
                }],
                local: vec![],
            },
            local: vec![
                FileColor {
                    scope: "Non-Project Files".into(),
                    color: "Yellow".into(),
                },
                FileColor {
                    scope: "Tests".into(),
                    color: "green".into(),
                },
                FileColor {
                    scope: "Unknown".into(),
                    color: "Blue".into(),
                },
            ],
            shared: vec![
                FileColor {
                    scope: "Models".into(),
                    color: "Violet".into(),
                },
                FileColor {
                    scope: "Project Files".into(),
                    color: "Chartreuse".into(),
                },
            ],
            context: ScopeContext::default(),
            targets: [
                ("app/Models/User.php", false),
                ("app/Models", true),
                ("app", true),
                ("tests/Unit/UserTest.php", false),
                ("tests", true),
                ("library://php-stubs/Core/Core_c.php", false),
                ("routes/web.php", false),
            ]
            .into_iter()
            .map(|(p, dir)| ColorTarget {
                path: p.into(),
                dir,
            })
            .collect(),
        };
        let colors = resolve(&req);
        let got: Vec<Option<&str>> = colors.iter().map(|c| c.as_deref()).collect();
        assert_eq!(
            got,
            [
                Some("Violet"),
                Some("Violet"),
                None,
                Some("Green"),
                Some("Green"),
                Some("Yellow"),
                None
            ]
        );
        assert_eq!(palette_color(" rose "), Some("Rose"));
    }
}
