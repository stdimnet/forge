//! File icon themes (`forge-icons`): the resolver every file icon asks first.
//!
//! The UI resolves icons synchronously while it renders, in the desktop app too, so the
//! resolver of the active theme lives here: `icons_set_theme` installs it (or removes it,
//! with no theme), `icons_resolve` answers per path from its cache.

use std::cell::RefCell;

use forge_icons::{Association, IconKind, IconResolver, IconThemeDef};
use serde::Deserialize;
use wasm_bindgen::prelude::*;

use crate::to_json;

thread_local! {
    static RESOLVER: RefCell<Option<IconResolver>> = const { RefCell::new(None) };
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetTheme {
    theme: Option<IconThemeDef>,
    #[serde(default)]
    associations: Vec<Association>,
}

/// `{ theme: IconThemeDef | null, associations }` → problems (`string[]`) of the theme and
/// the associations; the resolver is replaced either way (associations with problems are
/// skipped).
#[wasm_bindgen]
pub fn icons_set_theme(request_json: &str) -> String {
    let req: SetTheme = match serde_json::from_str(request_json) {
        Ok(r) => r,
        Err(e) => {
            RESOLVER.with(|r| *r.borrow_mut() = None);
            return to_json(&vec![format!("invalid icon theme: {e}")]);
        }
    };
    let Some(theme) = req.theme else {
        RESOLVER.with(|r| *r.borrow_mut() = None);
        return "[]".into();
    };
    let mut problems = theme.validate();
    let (resolver, more) = IconResolver::new(theme, &req.associations);
    problems.extend(more);
    RESOLVER.with(|r| *r.borrow_mut() = Some(resolver));
    to_json(&problems)
}

/// The icon id for `path` (`kind`: 0 file, 1 folder, 2 expanded folder), or "" for Forge's
/// own icon.
#[wasm_bindgen]
pub fn icons_resolve(path: &str, kind: u8) -> String {
    RESOLVER.with(|r| {
        r.borrow_mut()
            .as_mut()
            .and_then(|r| r.resolve(path, IconKind::from_code(kind)))
            .unwrap_or_default()
    })
}

/// `IconThemeDef` JSON → problems (`string[]`).
#[wasm_bindgen]
pub fn icons_validate_theme(theme_json: &str) -> String {
    match IconThemeDef::parse(theme_json) {
        Ok(t) => to_json(&t.validate()),
        Err(e) => to_json(&vec![e]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_resolve_and_clear() {
        let theme = r#"{"icons":{"php":"php.svg","dir":"dir.svg"},"folder":"dir","fileExtensions":{"php":"php"}}"#;
        let req = format!(
            r#"{{"theme":{theme},"associations":[{{"kind":"fileName","value":"x","icon":"nope"}}]}}"#
        );
        let problems: Vec<String> = serde_json::from_str(&icons_set_theme(&req)).unwrap();
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert_eq!(icons_resolve("src/a.php", 0), "php");
        assert_eq!(icons_resolve("src", 1), "dir");
        assert_eq!(icons_resolve("a.txt", 0), "");
        assert_eq!(icons_set_theme(r#"{"theme":null}"#), "[]");
        assert_eq!(icons_resolve("src/a.php", 0), "");
        assert_eq!(icons_validate_theme(theme), "[]");
    }
}
