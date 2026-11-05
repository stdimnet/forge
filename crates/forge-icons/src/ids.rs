//! The stable ids a theme maps to icons: symbol kinds (the `symbols` section) and UI icons
//! (the `ui` section). They are part of the theme format and never change meaning; new ids
//! may be added.

/// Stable symbol kinds of the `symbols` section, with what they stand for. The Structure
/// tool window, completion, Go to Class / Symbol, Search Everywhere, breadcrumbs and the
/// navigation bar ask for them.
pub const SYMBOL_KINDS: &[(&str, &str)] = &[
    ("namespace", "a namespace or package"),
    ("module", "a module (Lua, …)"),
    ("class", "a class"),
    (
        "abstractClass",
        "an abstract class (else `class` plus `marker.abstract`)",
    ),
    (
        "finalClass",
        "a final class (else `class` plus `marker.final`)",
    ),
    ("interface", "an interface"),
    ("trait", "a trait"),
    ("enum", "an enum"),
    ("enumCase", "an enum case (else `constant`)"),
    ("method", "a method"),
    (
        "abstractMethod",
        "an abstract method (else `method` plus `marker.abstract`)",
    ),
    (
        "finalMethod",
        "a final method (else `method` plus `marker.final`)",
    ),
    (
        "staticMethod",
        "a static method (else `method` plus `marker.static`)",
    ),
    ("constructor", "a constructor (else `method`)"),
    ("function", "a function"),
    ("property", "a property"),
    (
        "staticProperty",
        "a static property (else `property` plus `marker.static`)",
    ),
    ("field", "a field (else `property`)"),
    ("constant", "a constant"),
    ("variable", "a variable"),
    ("parameter", "a parameter (else `variable`)"),
    ("alias", "a type alias (else `class`)"),
    ("tag", "a markup tag"),
    ("rule", "a style sheet rule"),
    ("region", "a folding region"),
    ("marker.abstract", "overlay drawn on abstract symbols"),
    ("marker.final", "overlay drawn on final symbols"),
    ("marker.static", "overlay drawn on static symbols"),
];

/// The kind a symbol kind falls back to when the theme has no icon for it.
pub fn symbol_fallback(kind: &str) -> Option<&'static str> {
    Some(match kind {
        "abstractClass" | "finalClass" | "alias" => "class",
        "abstractMethod" | "finalMethod" | "staticMethod" | "constructor" => "method",
        "staticProperty" | "field" => "property",
        "enumCase" => "constant",
        "parameter" => "variable",
        "module" => "namespace",
        _ => return None,
    })
}

/// Prefixes of the `ui` section. Each takes any suffix (`toolwindow.<tool window id>`,
/// `runconfig.<run configuration type id>`); the documented ids below are the ones Forge
/// asks for.
pub const UI_PREFIXES: &[&str] = &["toolwindow.", "action.", "runconfig.", "vcs."];

/// UI icon ids Forge asks a theme for, with where they are shown.
pub const UI_ICONS: &[(&str, &str)] = &[
    ("toolwindow.project", "Project tool window stripe button"),
    ("toolwindow.structure", "Structure"),
    ("toolwindow.commit", "Commit"),
    ("toolwindow.git", "Git"),
    ("toolwindow.terminal", "Terminal"),
    ("toolwindow.run", "Run"),
    ("toolwindow.debug", "Debug"),
    ("toolwindow.problems", "Problems"),
    ("toolwindow.services", "Services"),
    ("toolwindow.database", "Database"),
    ("toolwindow.todo", "TODO"),
    ("toolwindow.find", "Find"),
    ("toolwindow.bookmarks", "Bookmarks"),
    ("toolwindow.notifications", "Notifications"),
    ("toolwindow.ai-chat", "AI Chat"),
    ("toolwindow.<id>", "any other tool window by its id"),
    ("action.run", "Run button of the main toolbar"),
    ("action.rerun", "Rerun button (while running)"),
    ("action.debug", "Debug button"),
    ("action.stop", "Stop button"),
    ("action.build", "Build Project button"),
    ("action.search", "Search Everywhere button"),
    ("action.settings", "Settings button"),
    ("action.aiChat", "AI Chat button"),
    (
        "runconfig.<type>",
        "run configuration type, e.g. `runconfig.php-script`, `runconfig.phpunit`, `runconfig.npm`",
    ),
    ("vcs.modified", "modified file in the Commit tool window"),
    ("vcs.added", "added file"),
    ("vcs.deleted", "deleted file"),
    ("vcs.untracked", "unversioned file"),
    ("vcs.conflict", "file with merge conflicts"),
];

/// The ids `id` falls back to, most specific first: the id itself, then the id with its
/// last `-part` removed while it keeps its prefix (`runconfig.php-builtin-server` →
/// `runconfig.php-builtin` → `runconfig.php`).
pub fn ui_fallbacks(id: &str) -> Vec<&str> {
    let mut out = vec![id];
    let prefix = UI_PREFIXES
        .iter()
        .find(|p| id.starts_with(*p))
        .map_or(0, |p| p.len());
    let mut rest = id;
    while let Some((head, _)) = rest.rsplit_once('-') {
        if head.len() <= prefix {
            break;
        }
        out.push(head);
        rest = head;
    }
    out
}

pub fn is_symbol_kind(kind: &str) -> bool {
    SYMBOL_KINDS.iter().any(|(k, _)| *k == kind)
}

pub fn is_ui_id(id: &str) -> bool {
    UI_PREFIXES
        .iter()
        .any(|p| id.len() > p.len() && id.starts_with(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_ids_fall_back_to_shorter_ids_within_their_prefix() {
        assert_eq!(
            ui_fallbacks("runconfig.php-builtin-server"),
            vec![
                "runconfig.php-builtin-server",
                "runconfig.php-builtin",
                "runconfig.php"
            ]
        );
        assert_eq!(
            ui_fallbacks("toolwindow.ai-chat"),
            vec!["toolwindow.ai-chat", "toolwindow.ai"]
        );
        assert_eq!(ui_fallbacks("action.run"), vec!["action.run"]);
        assert!(is_ui_id("vcs.modified") && !is_ui_id("vcs.") && !is_ui_id("other.x"));
    }

    #[test]
    fn symbol_kinds_are_known_and_fall_back() {
        assert!(is_symbol_kind("abstractClass") && is_symbol_kind("marker.static"));
        assert!(!is_symbol_kind("klass"));
        assert_eq!(symbol_fallback("enumCase"), Some("constant"));
        assert_eq!(symbol_fallback("class"), None);
        for (kind, _) in SYMBOL_KINDS {
            if let Some(f) = symbol_fallback(kind) {
                assert!(is_symbol_kind(f), "{kind} falls back to an unknown kind");
            }
        }
    }
}
