//! Show Context Actions (Alt+Enter): the entries of PHPStorm's intention popup, merged from
//! the language servers' code actions and a language pack's own intentions, in PHPStorm's
//! order (quick fixes first, then intentions and refactorings, then source actions).

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::edit::{Caret, WorkspaceEdit};

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
    /// Options in the entry's sub-menu (Right arrow), e.g. "Fix all ... in file".
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<ContextAction>,
    /// Where the caret goes after the edit (Create method: into the new body, possibly in
    /// another file).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caret: Option<Caret>,
    /// Arguments of `action`; such an entry is a fix of its own (Create class opens the
    /// Create New PHP Class dialog), not a link.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl ContextAction {
    /// The language server's own kind (`source.fixAll.eslint`), when it sent one.
    fn lsp_kind(&self) -> Option<&str> {
        self.raw.as_ref()?.get("kind")?.as_str()
    }

    /// A link to another action that opens its own popup or dialog ("Generate..."): it
    /// runs a UI action without data and, by PHPStorm's convention, its title ends with
    /// an ellipsis. Entries that run a UI action without one ("Edit SQL Fragment") are
    /// intentions.
    pub fn is_link(&self) -> bool {
        self.action.is_some()
            && self.data.is_none()
            && (self.title.ends_with("...") || self.title.ends_with('\u{2026}'))
    }

    /// "Fix all" style actions: they belong in the sub-menu of the quick fix they repeat.
    fn is_fix_all(&self) -> bool {
        self.lsp_kind()
            .is_some_and(|k| k.starts_with("source.fixAll"))
            || self.title.to_ascii_lowercase().starts_with("fix all")
    }
}

/// The light bulb at the caret line: red when a quick fix is available, yellow for
/// intentions and refactorings; none for source actions and links alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Bulb {
    Fix,
    Intention,
}

pub fn bulb_for(actions: &[ContextAction]) -> Option<Bulb> {
    let enabled = || {
        actions
            .iter()
            .filter(|a| a.disabled.is_none() && !a.is_link())
    };
    if enabled().any(|a| a.kind == ActionKind::QuickFix) {
        Some(Bulb::Fix)
    } else if enabled().any(|a| matches!(a.kind, ActionKind::Intention | ActionKind::Refactor)) {
        Some(Bulb::Intention)
    } else {
        None
    }
}

/// Move "fix all" actions into the sub-menu of the first quick fix of the same server
/// (PHPStorm lists "Fix all '...' problems in file" there). Without such a quick fix they
/// stay where they are.
pub fn nest_fix_all(actions: Vec<ContextAction>) -> Vec<ContextAction> {
    let mut out: Vec<ContextAction> = Vec::with_capacity(actions.len());
    let mut fix_all: Vec<ContextAction> = Vec::new();
    for a in actions {
        if a.is_fix_all() && a.disabled.is_none() {
            fix_all.push(a);
        } else {
            out.push(a);
        }
    }
    for a in fix_all {
        let parent = out
            .iter_mut()
            .find(|p| p.kind == ActionKind::QuickFix && p.server == a.server);
        match parent {
            Some(p) => p.children.push(a),
            None => out.push(a),
        }
    }
    out
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
        (a.disabled.is_some(), a.is_link(), kind)
    };
    let mut seen = std::collections::HashSet::new();
    let mut out: Vec<ContextAction> = actions
        .into_iter()
        .filter(|a| seen.insert(a.title.clone()))
        .collect();
    out.sort_by_key(rank);
    nest_fix_all(out)
}

/// Whether a language server's action already creates `name` from its usage (phpactor's
/// "Generate method", Intelephense's "Add method 'foo'", typescript's "Add missing function
/// declaration 'foo'"), so a pack's own "Create method 'foo'" would be a duplicate.
pub fn lsp_creates(lsp_titles: &[&str], name: &str) -> bool {
    let name = name.trim_start_matches('$').to_lowercase();
    if name.is_empty() {
        return false;
    }
    const VERBS: &[&str] = &["create", "add", "generate", "declare", "implement"];
    lsp_titles.iter().any(|t| {
        let t = t.to_lowercase();
        VERBS.iter().any(|v| t.starts_with(v))
            && t.split(|c: char| !(c.is_alphanumeric() || c == '_'))
                .any(|w| w == name)
    })
}

/// The language server code action kinds (typescript-language-server's `refactor.*`) that
/// implement a Refactor menu action in JS/TS files.
pub fn refactor_kinds(action: &str) -> &'static [&'static str] {
    match action {
        "IntroduceVariable" => &["refactor.extract.constant", "refactor.extract.variable"],
        "IntroduceConstant" => &["refactor.extract.constant"],
        "IntroduceField" => &["refactor.extract.field", "refactor.extract.property"],
        "ExtractMethod" => &["refactor.extract.function", "refactor.extract.method"],
        "Inline" => &["refactor.inline"],
        _ => &[],
    }
}

/// The server actions for a Refactor menu action ("Extract to function in module scope",
/// "Extract to method in class 'Cart'"...), in the server's order.
pub fn refactor_actions(actions: Vec<ContextAction>, action: &str) -> Vec<ContextAction> {
    let kinds = refactor_kinds(action);
    actions
        .into_iter()
        .filter(|a| {
            a.lsp_kind().is_some_and(|k| {
                kinds
                    .iter()
                    .any(|p| k == *p || k.strip_prefix(p).is_some_and(|r| r.starts_with('.')))
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_the_servers_refactorings_for_a_menu_action() {
        let with_kind = |title: &str, kind: &str| ContextAction {
            raw: Some(serde_json::json!({ "kind": kind })),
            ..action(title, ActionKind::Refactor)
        };
        let all = vec![
            with_kind(
                "Extract to function in module scope",
                "refactor.extract.function",
            ),
            with_kind(
                "Extract to constant in enclosing scope",
                "refactor.extract.constant",
            ),
            with_kind("Extract to type alias", "refactor.extract.type"),
            with_kind("Inline variable", "refactor.inline.variable"),
            action("Fix", ActionKind::QuickFix),
        ];
        let titles = |a: Vec<ContextAction>| a.into_iter().map(|a| a.title).collect::<Vec<_>>();
        assert_eq!(
            titles(refactor_actions(all.clone(), "ExtractMethod")),
            vec!["Extract to function in module scope"]
        );
        assert_eq!(
            titles(refactor_actions(all.clone(), "IntroduceVariable")),
            vec!["Extract to constant in enclosing scope"]
        );
        assert_eq!(
            titles(refactor_actions(all.clone(), "Inline")),
            vec!["Inline variable"]
        );
        assert!(refactor_actions(all, "Move").is_empty());
    }

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
            children: vec![],
            caret: None,
            data: None,
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

    #[test]
    fn actions_with_data_are_fixes_not_links() {
        let create = ContextAction {
            action: Some("PhpCreateClass".into()),
            data: Some(serde_json::json!({ "name": "Order" })),
            ..action("Create class 'Order'", ActionKind::QuickFix)
        };
        let generate = ContextAction {
            action: Some("Generate".into()),
            ..action("Generate...", ActionKind::Intention)
        };
        assert_eq!(bulb_for(std::slice::from_ref(&create)), Some(Bulb::Fix));
        let ordered = order_actions(vec![
            generate,
            action("Add PHPDoc", ActionKind::Intention),
            create,
        ]);
        let titles: Vec<&str> = ordered.iter().map(|a| a.title.as_str()).collect();
        assert_eq!(
            titles,
            ["Create class 'Order'", "Add PHPDoc", "Generate..."]
        );
    }

    #[test]
    fn recognizes_create_actions_of_language_servers() {
        let titles = [
            "Add missing function declaration 'formatPrice'",
            "Generate method",
            "Import class",
        ];
        assert!(lsp_creates(&titles, "formatPrice"));
        assert!(!lsp_creates(&titles, "format"));
        assert!(!lsp_creates(&["Import class 'Order'"], "Order"));
        assert!(lsp_creates(&["Create class 'Order'"], "Order"));
        assert!(!lsp_creates(&titles, ""));
    }

    #[test]
    fn nests_fix_all_actions_and_picks_the_bulb() {
        let fix = |title: &str, server: &str| ContextAction {
            server: Some(server.into()),
            ..action(title, ActionKind::QuickFix)
        };
        let fix_all = ContextAction {
            server: Some("eslint".into()),
            raw: Some(serde_json::json!({ "kind": "source.fixAll.eslint" })),
            ..action("Fix all auto-fixable problems", ActionKind::Source)
        };
        let ordered = order_actions(vec![
            action("Organize imports", ActionKind::Source),
            fix_all,
            fix("Remove unused variable 'x'", "eslint"),
            fix("Add missing import", "typescript"),
        ]);
        let titles: Vec<&str> = ordered.iter().map(|a| a.title.as_str()).collect();
        assert_eq!(
            titles,
            [
                "Remove unused variable 'x'",
                "Add missing import",
                "Organize imports"
            ]
        );
        assert_eq!(
            ordered[0].children[0].title,
            "Fix all auto-fixable problems"
        );
        assert_eq!(bulb_for(&ordered), Some(Bulb::Fix));

        // Without a quick fix to attach to, it stays a top-level entry.
        let alone = nest_fix_all(vec![ContextAction {
            raw: Some(serde_json::json!({ "kind": "source.fixAll" })),
            ..action("Fix all", ActionKind::Source)
        }]);
        assert_eq!(alone.len(), 1);
        assert_eq!(bulb_for(&alone), None);

        let generate = ContextAction {
            action: Some("Generate".into()),
            ..action("Generate...", ActionKind::Intention)
        };
        assert_eq!(bulb_for(std::slice::from_ref(&generate)), None);
        // An intention that runs a UI action is not a link: it ranks and lights the bulb.
        let fragment = ContextAction {
            action: Some("EditInjectedFragment".into()),
            ..action("Edit SQL Fragment", ActionKind::Intention)
        };
        assert!(!fragment.is_link() && generate.is_link());
        assert_eq!(
            bulb_for(std::slice::from_ref(&fragment)),
            Some(Bulb::Intention)
        );
        let ordered = order_actions(vec![generate.clone(), fragment.clone()]);
        assert_eq!(ordered[0].title, "Edit SQL Fragment");
        assert_eq!(
            bulb_for(&[generate, action("Add PHPDoc", ActionKind::Intention)]),
            Some(Bulb::Intention)
        );
    }
}
