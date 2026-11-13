//! Icon themes contributed by plugins (`contributes.iconThemes`): reading a theme file and
//! the SVGs it names from the plugin's files, with the checks of `forge_icons`.
//!
//! An icon theme is data only. It needs no permission and loads in Safe Mode: Forge reads
//! the theme file and exactly the SVGs its `icons` table names, all inside the plugin's own
//! package ([`theme_file_allowed`]), and draws them as images, which cannot run scripts.

use std::collections::BTreeMap;

use forge_icons::{check_svg, IconThemeDef};
use serde::Serialize;

use crate::manifest::{is_package_path, PluginManifest};

/// A theme ready for the UI: its definition and the SVG text of each icon id (light
/// variants from `iconsLight` under `<id>_light`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadedIconTheme {
    pub id: String,
    pub label: String,
    pub plugin_id: String,
    pub definition: IconThemeDef,
    pub svgs: BTreeMap<String, String>,
}

/// The package path of `relative` (a path in the theme file) next to the theme file
/// `theme_path`, or `None` when it would leave the package.
pub fn resolve_asset(theme_path: &str, relative: &str) -> Option<String> {
    let dir = theme_path.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    let joined = if dir.is_empty() {
        relative.to_string()
    } else {
        format!("{dir}/{relative}")
    };
    is_package_path(&joined).then_some(joined)
}

/// Whether a plugin may hand Forge `path` as part of an icon theme: the theme file of one of
/// its contributed themes, or an `.svg` file inside its package.
pub fn theme_file_allowed(manifest: &PluginManifest, path: &str) -> bool {
    is_package_path(path)
        && (manifest
            .contributes
            .icon_themes
            .iter()
            .any(|t| t.path == path)
            || forge_icons::is_svg_path(path))
}

/// Load the theme `theme_id` of `manifest`, reading package files with `read`.
pub fn load(
    manifest: &PluginManifest,
    theme_id: &str,
    read: impl Fn(&str) -> Result<Vec<u8>, String>,
) -> Result<LoadedIconTheme, String> {
    let theme = manifest
        .contributes
        .icon_themes
        .iter()
        .find(|t| t.id == theme_id)
        .ok_or_else(|| format!("plugin `{}` has no icon theme `{theme_id}`", manifest.id))?;
    let text = |path: &str| -> Result<String, String> {
        if !theme_file_allowed(manifest, path) {
            return Err(format!("{path}: not a file an icon theme may use"));
        }
        String::from_utf8(read(path)?).map_err(|_| format!("{path} is not UTF-8"))
    };
    let definition =
        IconThemeDef::parse(&text(&theme.path)?).map_err(|e| format!("{}: {e}", theme.path))?;
    let problems = definition.validate();
    if !problems.is_empty() {
        return Err(format!("{}: {}", theme.path, problems.join("; ")));
    }
    let mut svgs = BTreeMap::new();
    // `icons` under their ids, `iconsLight` under `<id>_light` (the resolver's keys).
    for (key, rel) in definition.svg_files() {
        let path = resolve_asset(&theme.path, rel)
            .ok_or_else(|| format!("icons.{key}: `{rel}` leaves the plugin"))?;
        let svg = text(&path)?;
        check_svg(&svg).map_err(|e| format!("{path} {e}"))?;
        svgs.insert(key, svg);
    }
    Ok(LoadedIconTheme {
        id: theme.id.clone(),
        label: theme.label.clone(),
        plugin_id: manifest.id.clone(),
        definition,
        svgs,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub const SHAPES_MANIFEST: &str = r#"{
      "manifestVersion": 1,
      "id": "example.shapes-icons",
      "name": "Shapes Icons",
      "version": "1.0.0",
      "engines": { "forge": ">=0.1.0" },
      "contributes": {
        "iconThemes": [{ "id": "example.shapes-icons.shapes", "label": "Shapes", "path": "theme/icon-theme.json" }]
      }
    }"#;
    pub const SHAPES_THEME: &str = r#"{
      "icons": { "file": "icons/file.svg", "php": "icons/php.svg", "folder": "icons/folder.svg" },
      "file": "file",
      "folder": "folder",
      "fileExtensions": { "php": "php" }
    }"#;
    pub const SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><circle cx="8" cy="8" r="6"/></svg>"#;

    pub fn shapes_files() -> BTreeMap<String, Vec<u8>> {
        [
            ("forge-plugin.json", SHAPES_MANIFEST),
            ("theme/icon-theme.json", SHAPES_THEME),
            ("theme/icons/file.svg", SVG),
            ("theme/icons/php.svg", SVG),
            ("theme/icons/folder.svg", SVG),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.as_bytes().to_vec()))
        .collect()
    }

    fn reader(files: BTreeMap<String, Vec<u8>>) -> impl Fn(&str) -> Result<Vec<u8>, String> {
        move |p| {
            files
                .get(p)
                .cloned()
                .ok_or_else(|| format!("{p} is missing"))
        }
    }

    #[test]
    fn loads_a_theme_with_its_svgs() {
        let m = PluginManifest::parse(SHAPES_MANIFEST).unwrap();
        assert!(m.validate("0.1.0").is_empty(), "{:?}", m.validate("0.1.0"));
        let t = load(&m, "example.shapes-icons.shapes", reader(shapes_files())).unwrap();
        assert_eq!(t.label, "Shapes");
        assert_eq!(t.svgs.len(), 3);
        assert_eq!(t.svgs["php"], SVG);
        assert!(load(&m, "example.shapes-icons.other", reader(shapes_files())).is_err());
    }

    #[test]
    fn refuses_missing_escaping_or_unsafe_files() {
        let m = PluginManifest::parse(SHAPES_MANIFEST).unwrap();
        let id = "example.shapes-icons.shapes";
        let mut files = shapes_files();
        files.remove("theme/icons/php.svg");
        assert!(load(&m, id, reader(files))
            .unwrap_err()
            .contains("php.svg is missing"));

        let mut files = shapes_files();
        files.insert(
            "theme/icon-theme.json".into(),
            SHAPES_THEME
                .replace("icons/php.svg", "../../php.svg")
                .into_bytes(),
        );
        assert!(load(&m, id, reader(files)).is_err());

        let mut files = shapes_files();
        files.insert(
            "theme/icons/php.svg".into(),
            b"<svg><script>alert(1)</script></svg>".to_vec(),
        );
        assert!(load(&m, id, reader(files)).unwrap_err().contains("script"));
    }

    #[test]
    fn the_sdk_example_loads() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../sdk/examples/shapes-icons");
        let m = PluginManifest::parse(
            &std::fs::read_to_string(root.join("forge-plugin.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(m.validate("0.1.0"), vec![]);
        let t = load(&m, "example.shapes-icons.shapes", |p| {
            std::fs::read(root.join(p)).map_err(|e| e.to_string())
        })
        .unwrap();
        assert_eq!(t.svgs.len(), t.definition.svg_files().len());
        assert!(
            t.svgs.contains_key("folder_light"),
            "iconsLight under `<id>_light`"
        );
        assert!(forge_icons::has_tint_layer(&t.svgs["folder"]));
        assert_eq!(t.definition.warnings(), Vec::<String>::new());
        let (mut r, problems) = forge_icons::IconResolver::new(t.definition, &[]);
        assert!(problems.is_empty());
        let kind = forge_icons::IconKind::File;
        assert_eq!(
            r.resolve("resources/views/welcome.blade.php", kind)
                .as_deref(),
            Some("blade")
        );
        assert_eq!(
            r.resolve("tests/Feature/UserTest.php", kind).as_deref(),
            Some("test")
        );
        assert_eq!(r.resolve(".env.example", kind).as_deref(), Some("env"));
        assert_eq!(
            r.resolve("app", forge_icons::IconKind::FolderOpen)
                .as_deref(),
            Some("folder-src-open")
        );
        // Format 2: path rules, the root folder, symbols, UI icons and light variants.
        assert_eq!(
            r.resolve("app/Http/Controllers/UserController.php", kind)
                .as_deref(),
            Some("controller")
        );
        let root = forge_icons::Lookup {
            root: true,
            ..forge_icons::Lookup::new("shop", forge_icons::IconKind::FolderOpen)
        };
        assert_eq!(r.lookup(&root).as_deref(), Some("folder-root-open"));
        let class = r.symbol("class", Default::default()).unwrap();
        assert_eq!(class.icon, "sym-class");
        assert_eq!(r.ui("toolwindow.project").as_deref(), Some("ui-project"));
        assert_eq!(
            r.ui("runconfig.php-script").as_deref(),
            Some("runconfig-php")
        );
        r.set_light(true);
        assert_eq!(r.resolve("a.php", kind).as_deref(), Some("php_light"));
    }

    #[test]
    fn assets_resolve_next_to_the_theme_file() {
        assert_eq!(
            resolve_asset("theme/icon-theme.json", "icons/a.svg").as_deref(),
            Some("theme/icons/a.svg")
        );
        assert_eq!(
            resolve_asset("icon-theme.json", "a.svg").as_deref(),
            Some("a.svg")
        );
        assert_eq!(resolve_asset("theme/icon-theme.json", "../a.svg"), None);
        let m = PluginManifest::parse(SHAPES_MANIFEST).unwrap();
        assert!(theme_file_allowed(&m, "theme/icon-theme.json"));
        assert!(theme_file_allowed(&m, "theme/icons/a.svg"));
        assert!(!theme_file_allowed(&m, "dist/plugin.js"));
        assert!(!theme_file_allowed(&m, "other.json"));
    }
}
