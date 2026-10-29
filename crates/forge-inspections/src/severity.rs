//! Severities, as in PHPStorm: Error, Warning, Weak Warning, Server Problem, Typo, Info and
//! custom severities (Settings | Editor | Inspections | Severities Editor). A severity is
//! identified by its PHPStorm level name (`ERROR`, `WEAK WARNING`, ...), which profiles store;
//! the editor highlights it like one of the [`EditorSeverity`] kinds.

use serde::{Deserialize, Serialize};

pub const ERROR: &str = "ERROR";
pub const WARNING: &str = "WARNING";
pub const WEAK_WARNING: &str = "WEAK WARNING";
pub const SERVER_PROBLEM: &str = "SERVER PROBLEM";
pub const TYPO: &str = "TYPO";
pub const INFO: &str = "INFO";

/// How the editor highlights a severity (the UI's `Severity`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EditorSeverity {
    Error,
    Warning,
    Weak,
    Typo,
    Info,
}

/// A severity: built-in or defined by the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeverityDef {
    /// The level name stored in profiles (`WARNING`, `MY CUSTOM`).
    pub id: String,
    /// Shown in the UI (`Weak Warning`).
    pub name: String,
    /// Highlighted like this.
    pub editor: EditorSeverity,
    #[serde(default)]
    pub custom: bool,
}

/// The built-in severities, most severe first.
pub fn builtin() -> Vec<SeverityDef> {
    [
        (ERROR, "Error", EditorSeverity::Error),
        (WARNING, "Warning", EditorSeverity::Warning),
        (WEAK_WARNING, "Weak Warning", EditorSeverity::Weak),
        (SERVER_PROBLEM, "Server Problem", EditorSeverity::Warning),
        (TYPO, "Typo", EditorSeverity::Typo),
        (INFO, "Info", EditorSeverity::Info),
    ]
    .into_iter()
    .map(|(id, name, editor)| SeverityDef {
        id: id.into(),
        name: name.into(),
        editor,
        custom: false,
    })
    .collect()
}

/// The built-in severities followed by the custom ones (custom ids that clash with a
/// built-in one are dropped).
pub fn all(custom: &[SeverityDef]) -> Vec<SeverityDef> {
    let mut out = builtin();
    for c in custom {
        let id = normalize(&c.id);
        if id.is_empty() || out.iter().any(|s| s.id == id) {
            continue;
        }
        out.push(SeverityDef {
            id,
            name: c.name.clone(),
            editor: c.editor,
            custom: true,
        });
    }
    out
}

/// A level as profiles store it: upper case, single spaces; PHPStorm's aliases
/// (`INFORMATION`, `WEAK_WARNING`, `weak`) map onto the built-in ids.
pub fn normalize(level: &str) -> String {
    let up = level
        .trim()
        .replace('_', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_uppercase();
    match up.as_str() {
        "INFORMATION" | "TEXT ATTRIBUTES" | "HINT" => INFO.into(),
        "WEAK" => WEAK_WARNING.into(),
        "GRAMMAR" | "GRAMMAR ERROR" => TYPO.into(),
        _ => up,
    }
}

/// The level of an editor severity (`weak` -> `WEAK WARNING`), for problems that report
/// one (language servers, external tools).
pub fn from_editor(s: &str) -> String {
    match s {
        "error" => ERROR.into(),
        "warning" => WARNING.into(),
        "weak" => WEAK_WARNING.into(),
        "typo" | "grammar" => TYPO.into(),
        "info" | "hint" => INFO.into(),
        other => normalize(other),
    }
}

/// How the editor highlights `level` (unknown levels like a warning).
pub fn editor_of(level: &str, custom: &[SeverityDef]) -> EditorSeverity {
    let id = normalize(level);
    all(custom)
        .into_iter()
        .find(|s| s.id == id)
        .map_or(EditorSeverity::Warning, |s| s.editor)
}

/// Sort rank, most severe first (custom severities after Server Problem).
pub fn rank(level: &str) -> u8 {
    match normalize(level).as_str() {
        ERROR => 0,
        WARNING => 1,
        WEAK_WARNING => 2,
        SERVER_PROBLEM => 3,
        TYPO => 5,
        INFO => 6,
        _ => 4,
    }
}

/// The display name of a level.
pub fn display_name(level: &str, custom: &[SeverityDef]) -> String {
    let id = normalize(level);
    all(custom)
        .into_iter()
        .find(|s| s.id == id)
        .map_or_else(|| id.clone(), |s| s.name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_phpstorm_levels() {
        assert_eq!(normalize("weak_warning"), WEAK_WARNING);
        assert_eq!(normalize(" information "), INFO);
        assert_eq!(normalize("server  problem"), SERVER_PROBLEM);
        assert_eq!(from_editor("weak"), WEAK_WARNING);
        assert_eq!(from_editor("grammar"), TYPO);
    }

    #[test]
    fn custom_severities_follow_builtin_ones() {
        let custom = vec![
            SeverityDef {
                id: "Blocker".into(),
                name: "Blocker".into(),
                editor: EditorSeverity::Error,
                custom: true,
            },
            SeverityDef {
                id: "warning".into(),
                name: "Clash".into(),
                editor: EditorSeverity::Info,
                custom: true,
            },
        ];
        let all = all(&custom);
        assert_eq!(all.len(), 7);
        assert_eq!(all[6].id, "BLOCKER");
        assert_eq!(editor_of("blocker", &custom), EditorSeverity::Error);
        assert_eq!(
            editor_of("SERVER PROBLEM", &custom),
            EditorSeverity::Warning
        );
        assert_eq!(display_name("WEAK WARNING", &[]), "Weak Warning");
        assert!(rank(ERROR) < rank("BLOCKER") && rank("BLOCKER") < rank(INFO));
    }
}
