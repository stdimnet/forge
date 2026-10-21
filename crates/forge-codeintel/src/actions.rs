//! Show Context Actions (Alt+Enter): the entries of PHPStorm's intention popup, merged from
//! the language servers' code actions and a language pack's own intentions, in PHPStorm's
//! order (quick fixes first, then intentions and refactorings, then source actions).

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::edit::WorkspaceEdit;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ActionKind {
    /// Fixes a problem (red bulb).
    QuickFix,
    /// Changes working code (yellow bulb).
    Intention,
    /// `refactor.*` code actions.
    Refactor,
    /// `source.*` code actions (organize imports, fix all).
    Source,
}

impl ActionKind {
    /// The kind of an LSP code action (`quickfix`, `refactor.extract`, `source.organizeImports`).
    pub fn of_lsp(kind: Option<&str>) -> Self {
        match kind.unwrap_or_default() {
            k if k == "quickfix" || k.starts_with("quickfix.") => ActionKind::QuickFix,
            k if k.starts_with("refactor") => ActionKind::Refactor,
            k if k.starts_with("source") => ActionKind::Source,
            _ => ActionKind::Intention,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextAction {
    pub title: String,
    pub kind: ActionKind,
    /// `lsp` or the language pack (`php`).
    pub source: String,
    #[serde(default)]
    pub is_preferred: bool,
    /// Why it cannot run now (shown greyed out).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled: Option<String>,
    /// Workspace-relative paths.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edit: Option<WorkspaceEdit>,
    /// A server command to run (`{ title, command, arguments }`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<Value>,
    /// The edit has to be resolved first (`codeAction/resolve`).
    #[serde(default)]
    pub needs_resolve: bool,
    /// Language server that offered it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server: Option<String>,
    /// The server's original action, for resolving.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw: Option<Value>,
    /// A UI action to run instead, e.g. `Generate` (Alt+Insert).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
}

/// PHPStorm's order: preferred quick fixes, quick fixes, intentions and refactorings,
/// source actions, links to other actions, disabled entries last. Duplicate titles (two
/// servers offering the same fix) are dropped.
pub fn order_actions(actions: Vec<ContextAction>) -> Vec<ContextAction> {
    let rank = |a: &ContextAction| {
        let kind = match a.kind {
            ActionKind::QuickFix if a.is_preferred => 0,
            ActionKind::QuickFix => 1,
            ActionKind::Intention | ActionKind::Refactor => 2,
            ActionKind::Source => 3,
        };
        (a.disabled.is_some(), a.action.is_some(), kind)
    };
    let mut seen = std::collections::HashSet::new();
    let mut out: Vec<ContextAction> = actions
        .into_iter()
        .filter(|a| seen.insert(a.title.clone()))
        .collect();
    out.sort_by_key(rank);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn action(title: &str, kind: ActionKind) -> ContextAction {
        ContextAction {
            title: title.into(),
            kind,
            source: "lsp".into(),
            is_preferred: false,
            disabled: None,
            edit: None,
            command: None,
            needs_resolve: false,
            server: None,
            raw: None,
            action: None,
        }
    }

    #[test]
    fn orders_like_phpstorm() {
        assert_eq!(ActionKind::of_lsp(Some("quickfix")), ActionKind::QuickFix);
        assert_eq!(
            ActionKind::of_lsp(Some("refactor.extract")),
            ActionKind::Refactor
        );
        assert_eq!(
            ActionKind::of_lsp(Some("source.organizeImports")),
            ActionKind::Source
        );
        assert_eq!(ActionKind::of_lsp(None), ActionKind::Intention);

        let generate = ContextAction {
            action: Some("Generate".into()),
            ..action("Generate...", ActionKind::Intention)
        };
        let preferred = ContextAction {
            is_preferred: true,
            ..action("Import class", ActionKind::QuickFix)
        };
        let disabled = ContextAction {
            disabled: Some("Nothing selected".into()),
            ..action("Extract method", ActionKind::Refactor)
        };
        let ordered = order_actions(vec![
            generate,
            action("Organize imports", ActionKind::Source),
            disabled,
            action("Generate getter", ActionKind::Intention),
            action("Add return type", ActionKind::QuickFix),
            preferred,
            action("Add return type", ActionKind::QuickFix),
        ]);
        let titles: Vec<&str> = ordered.iter().map(|a| a.title.as_str()).collect();
        assert_eq!(
            titles,
            [
                "Import class",
                "Add return type",
                "Generate getter",
                "Organize imports",
                "Generate...",
                "Extract method"
            ]
        );
    }
}
