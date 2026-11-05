//! Icon themes (`forge-icons`): the resolver every file, symbol and UI icon asks first.
//!
//! The UI resolves icons synchronously while it renders, in the desktop app too, so the
//! resolver of the active theme lives here: `icons_set_theme` installs it (or removes it,
//! with no theme), `icons_set_options` sets what the answers depend on besides the theme
//! (light or dark variants, the project root, folder colors), and `icons_resolve`,
//! `icons_symbol` and `icons_ui` answer, per path from a cache.

use std::cell::RefCell;

use forge_icons::{
    folders_with_open_files, has_tint_layer, tint_svg, uses_light_variant, Association,
    FolderColor, FolderColors, IconResolver, IconThemeDef, Lookup, SymbolModifiers,
};
use serde::Deserialize;
use wasm_bindgen::prelude::*;

use crate::to_json;

#[derive(Default)]
struct State {
    resolver: Option<IconResolver>,
    colors: FolderColors,
    light: bool,
    root: Option<String>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetTheme {
    theme: Option<IconThemeDef>,
    #[serde(default)]
    associations: Vec<Association>,
}

/// `{ theme: IconThemeDef | null, associations }` → problems (`string[]`) of the theme
/// (errors and warnings) and the associations; the resolver is replaced either way
/// (associations with problems are skipped). The options of `icons_set_options` stay.
#[wasm_bindgen]
pub fn icons_set_theme(request_json: &str) -> String {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        let req: SetTheme = match serde_json::from_str(request_json) {
            Ok(r) => r,
            Err(e) => {
                s.resolver = None;
                return to_json(&vec![format!("invalid icon theme: {e}")]);
            }
        };
        let Some(theme) = req.theme else {
            s.resolver = None;
            return "[]".into();
        };
        let mut problems = theme.validate();
        problems.extend(theme.warnings());
        let (mut resolver, more) = IconResolver::new(theme, &req.associations);
        problems.extend(more);
        resolver.set_light(s.light);
        resolver.set_project_root(s.root.as_deref());
        s.resolver = Some(resolver);
        to_json(&problems)
    })
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Options {
    /// The UI theme id (`dark`, `light`, `high-contrast-light`, …).
    #[serde(default)]
    ui_theme: String,
    /// The `fileIcons.variant` setting: `auto`, `light` or `dark`.
    #[serde(default)]
    variant: String,
    #[serde(default)]
    project_root: Option<String>,
    /// The `fileIcons.folderColors` setting.
    #[serde(default)]
    folder_colors: Vec<FolderColor>,
}

/// `{ uiTheme, variant, projectRoot, folderColors }` → `{ light, problems }`: whether light
/// variants are drawn now and the problems of the folder colors (skipped rows). Clears the
/// caches when something changed.
#[wasm_bindgen]
pub fn icons_set_options(options_json: &str) -> String {
    let o: Options = serde_json::from_str(options_json).unwrap_or_default();
    let light = uses_light_variant(&o.ui_theme, &o.variant);
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        let root = o.project_root.filter(|r| !r.is_empty());
        let (colors, problems) = FolderColors::new(&o.folder_colors, root.as_deref());
        if let Some(r) = s.resolver.as_mut() {
            r.set_light(light);
            r.set_project_root(root.as_deref());
        }
        s.light = light;
        s.root = root;
        s.colors = colors;
        to_json(&serde_json::json!({ "light": light, "problems": problems }))
    })
}

/// The icon key for `path` (`flags`: 0 file, 1 folder, 2 expanded folder, plus 4 for the
/// project root and 8 for a hollow folder; `file_type`: the detected file type id or ""),
/// or "" for Forge's own icon.
#[wasm_bindgen]
pub fn icons_resolve(path: &str, flags: u8, file_type: &str) -> String {
    STATE.with(|s| {
        s.borrow_mut()
            .resolver
            .as_mut()
            .and_then(|r| r.lookup(&Lookup::from_code(path, flags, Some(file_type))))
            .unwrap_or_default()
    })
}

/// Whether the icon of `path` may depend on its detected file type (ask for it only then).
#[wasm_bindgen]
pub fn icons_needs_file_type(path: &str) -> bool {
    STATE.with(|s| {
        s.borrow()
            .resolver
            .as_ref()
            .is_some_and(|r| r.needs_file_type(path))
    })
}

/// The icon of a code symbol (`modifiers`: 1 abstract, 2 final, 4 static) as
/// `{ icon, markers }` JSON, or "" for Forge's own badge.
#[wasm_bindgen]
pub fn icons_symbol(kind: &str, modifiers: u8) -> String {
    let mods = SymbolModifiers {
        is_abstract: modifiers & 1 != 0,
        is_final: modifiers & 2 != 0,
        is_static: modifiers & 4 != 0,
    };
    STATE.with(|s| {
        s.borrow()
            .resolver
            .as_ref()
            .and_then(|r| r.symbol(kind, mods))
            .map(|icon| to_json(&icon))
            .unwrap_or_default()
    })
}

/// The icon key of a UI id (`toolwindow.project`, `action.run`, …), or "".
#[wasm_bindgen]
pub fn icons_ui(id: &str) -> String {
    STATE.with(|s| {
        s.borrow()
            .resolver
            .as_ref()
            .and_then(|r| r.ui(id))
            .unwrap_or_default()
    })
}

/// The user's color of the folder at `path` (`fileIcons.folderColors`), or "".
#[wasm_bindgen]
pub fn icons_folder_color(path: &str) -> String {
    STATE.with(|s| s.borrow_mut().colors.color_of(path).unwrap_or_default())
}

/// `svg` with its tint layer in `color`, or "" when it has none (draw it as a mask then).
#[wasm_bindgen]
pub fn icons_tint(svg: &str, color: &str) -> String {
    tint_svg(svg, color).unwrap_or_default()
}

/// Whether `svg` marks a tint layer (`data-forge-tint`, class `forge-tint`).
#[wasm_bindgen]
pub fn icons_has_tint_layer(svg: &str) -> bool {
    has_tint_layer(svg)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpenFolders {
    open_files: Vec<String>,
    #[serde(default)]
    project_root: Option<String>,
}

/// `{ openFiles, projectRoot }` → the folders (relative, `string[]`) that hold open files.
#[wasm_bindgen]
pub fn icons_open_folders(request_json: &str) -> String {
    match serde_json::from_str::<OpenFolders>(request_json) {
        Ok(r) => to_json(&folders_with_open_files(
            &r.open_files,
            r.project_root.as_deref(),
        )),
        Err(_) => "[]".into(),
    }
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
        let theme = r#"{"icons":{"php":"php.svg","dir":"dir.svg","dir_light":"dl.svg","root":"r.svg","cls":"c.svg","run":"run.svg","sh":"sh.svg"},
          "version":2,"folder":"dir","rootFolder":"root","fileExtensions":{"php":"php"},"fileTypes":{"shell":"sh"},
          "symbols":{"class":"cls"},"ui":{"action.run":"run"}}"#;
        let req = format!(
            r#"{{"theme":{theme},"associations":[{{"kind":"fileName","value":"x","icon":"nope"}}]}}"#
        );
        let problems: Vec<String> = serde_json::from_str(&icons_set_theme(&req)).unwrap();
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert_eq!(icons_resolve("src/a.php", 0, ""), "php");
        assert_eq!(icons_resolve("src", 1, ""), "dir");
        assert_eq!(icons_resolve("shop", 1 | 4, ""), "root");
        assert_eq!(icons_resolve("a.txt", 0, ""), "");
        assert!(icons_needs_file_type("deploy"));
        assert_eq!(icons_resolve("deploy", 0, "shell"), "sh");
        assert_eq!(icons_symbol("class", 1), r#"{"icon":"cls","markers":[]}"#);
        assert_eq!(icons_symbol("function", 0), "");
        assert_eq!(icons_ui("action.run"), "run");

        let out = icons_set_options(
            r##"{"uiTheme":"high-contrast-light","variant":"auto","folderColors":[{"pattern":"vendor","color":"#888"},{"pattern":"x","color":"red"}]}"##,
        );
        assert_eq!(
            out,
            r#"{"light":true,"problems":["folderColors[1]: `red` is not a color like #e0a040"]}"#
        );
        assert_eq!(icons_resolve("src", 1, ""), "dir_light");
        assert_eq!(icons_folder_color("vendor"), "#888");
        icons_set_options(r#"{"uiTheme":"dark"}"#);
        assert_eq!(icons_resolve("src", 1, ""), "dir");
        assert_eq!(icons_folder_color("vendor"), "");

        assert_eq!(icons_set_theme(r#"{"theme":null}"#), "[]");
        assert_eq!(icons_resolve("src/a.php", 0, ""), "");
        assert_eq!(icons_validate_theme(theme), "[]");
    }

    #[test]
    fn tint_and_open_folders() {
        let svg = r##"<svg viewBox="0 0 16 16"><path data-forge-tint fill="#000"/></svg>"##;
        assert!(icons_has_tint_layer(svg));
        assert!(icons_tint(svg, "#123456").contains("#123456"));
        assert_eq!(icons_tint("<svg/>", "#123456"), "");
        assert_eq!(
            icons_open_folders(r#"{"openFiles":["/p/a/b/c.php"],"projectRoot":"/p"}"#),
            r#"["a","a/b"]"#
        );
    }
}
