//! Built-in plugins. Their manifests are TOML files in `app/src-tauri/plugins/`, compiled
//! into the binary. A test keeps `app/ui/src/mock/builtinPlugins.json` (the browser-mode
//! copy) in sync with them.

use forge_core::Result;
use forge_plugin_api::{PluginDescriptor, PluginRegistry};

const MANIFESTS: &[(&str, &str)] = &[
    ("core.toml", include_str!("../plugins/core.toml")),
    ("git.toml", include_str!("../plugins/git.toml")),
    (
        "code-hosts.toml",
        include_str!("../plugins/code-hosts.toml"),
    ),
    ("terminal.toml", include_str!("../plugins/terminal.toml")),
    ("vim.toml", include_str!("../plugins/vim.toml")),
    ("run.toml", include_str!("../plugins/run.toml")),
    ("database.toml", include_str!("../plugins/database.toml")),
    ("services.toml", include_str!("../plugins/services.toml")),
    ("php.toml", include_str!("../plugins/php.toml")),
    (
        "langservers.toml",
        include_str!("../plugins/langservers.toml"),
    ),
    ("phpforge.toml", include_str!("../plugins/phpforge.toml")),
    ("webforge.toml", include_str!("../plugins/webforge.toml")),
    ("luaforge.toml", include_str!("../plugins/luaforge.toml")),
    ("packs.toml", include_str!("../plugins/packs.toml")),
    ("langs.toml", include_str!("../plugins/langs.toml")),
    (
        "browser-companion.toml",
        include_str!("../plugins/browser-companion.toml"),
    ),
    (
        "natural-languages.toml",
        include_str!("../plugins/natural-languages.toml"),
    ),
    (
        "json-schema.toml",
        include_str!("../plugins/json-schema.toml"),
    ),
];

/// Setting holding the ids of plugins the user disabled.
pub const DISABLED_PLUGINS_KEY: &str = "plugins.disabled";

pub fn builtin_descriptors() -> Result<Vec<PluginDescriptor>> {
    let mut out = Vec::new();
    for (name, src) in MANIFESTS {
        let parsed = PluginDescriptor::parse_many(src).map_err(|e| {
            forge_core::ForgeError::Other(format!("built-in plugin manifest {name}: {e}"))
        })?;
        out.extend(parsed);
    }
    Ok(out)
}

pub fn builtin_registry() -> Result<PluginRegistry> {
    let mut registry = PluginRegistry::new();
    for d in builtin_descriptors()? {
        registry.register_descriptor(d)?;
    }
    Ok(registry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::path::PathBuf;

    #[test]
    fn manifests_are_consistent() {
        let registry = builtin_registry().unwrap();
        assert!(registry.get("forge.core").unwrap().manifest.required);

        let mut keys = HashSet::new();
        for spec in registry.all_settings() {
            assert!(keys.insert(&spec.key), "duplicate setting {}", spec.key);
            spec.kind
                .validate(&spec.default)
                .unwrap_or_else(|e| panic!("default of {}: {e}", spec.key));
        }
        for p in registry.list() {
            for dep in &p.manifest.depends {
                assert!(registry.get(dep).is_some(), "{} needs {dep}", p.manifest.id);
            }
        }
        assert_eq!(
            registry
                .lsp_server_for_extension("php")
                .map(|s| s.id.as_str()),
            Some("intelephense")
        );
    }

    #[test]
    fn file_types_are_sane() {
        let registry = builtin_registry().unwrap();
        let mut ids = HashSet::new();
        for t in registry.file_types() {
            assert!(ids.insert(&t.id), "duplicate file type {}", t.id);
            assert!(
                !t.extensions.is_empty() || !t.file_names.is_empty(),
                "file type {} matches nothing",
                t.id
            );
            assert!(t
                .extensions
                .iter()
                .all(|e| !e.starts_with('.') && *e == e.to_lowercase()));
        }
        for id in [
            "shell",
            "python",
            "yaml",
            "json",
            "dotenv",
            "ignore",
            "dockerfile",
            "docker-compose",
            "postfix",
            "postfix-master",
            "eml",
            "symfony-services",
        ] {
            assert!(ids.contains(&id.to_string()), "missing file type {id}");
        }
    }

    /// File type detection over the real manifests (bundled types plus language packs).
    #[test]
    fn file_types_detect_like_phpstorm() {
        let types = builtin_registry().unwrap().file_type_registry();
        let id =
            |path: &str, first: Option<&str>| types.detect(path, first, &[]).map(|t| t.id.as_str());
        for (path, expected) in [
            ("src/app.py", "python"),
            ("scripts/deploy.sh", "shell"),
            ("config/a.yml", "yaml"),
            ("composer.json", "json"),
            ("Dockerfile", "dockerfile"),
            ("docker/Dockerfile.prod", "dockerfile"),
            ("docker-compose.override.yml", "docker-compose"),
            ("compose.yaml", "docker-compose"),
            ("composer.yml", "yaml"),
            ("/etc/postfix/main.cf", "postfix"),
            ("/etc/postfix/master.cf", "postfix-master"),
            ("pgsql-aliases.cf", "postfix"),
            ("mail/msg.eml", "eml"),
            ("a.EML", "eml"),
            ("src/Kernel.php", "php"),
            ("templates/x.blade.php", "blade"),
            (".env", "dotenv"),
            (".env.local", "dotenv"),
            (".env.test.local", "dotenv"),
            (".env.dist", "dotenv"),
            ("prod.env", "dotenv"),
            (".env.local.php", "php"),
            (".gitignore", "ignore"),
            (".dockerignore", "ignore"),
            ("app/.npmignore", "ignore"),
            (".prettierignore", "ignore"),
            (".ignore", "ignore"),
            ("config/services.yaml", "symfony-services"),
            ("config/services_test.yaml", "symfony-services"),
            ("app/config/services_dev.yml", "symfony-services"),
            ("app/config/services/api.yml", "symfony-services"),
            ("config/services/mail.yaml", "symfony-services"),
            ("config/packages/framework.yaml", "yaml"),
            ("php.ini", "ini"),
            ("/etc/mysql/my.cnf", "ini"),
            (".editorconfig", "ini"),
            ("nginx.conf", "nginx"),
            ("sysctl.conf", "conf"),
            ("supervisord.conf", "ini"),
            ("a.PY", "python"),
            ("src/game/world.lua", "lua"),
            ("penlight-1.14.0-1.rockspec", "lua"),
            (".luacheckrc", "lua"),
        ] {
            assert_eq!(id(path, None), Some(expected), "{path}");
        }
        assert_eq!(
            id("bin/run", Some("#!/usr/bin/env python3")),
            Some("python")
        );
        assert_eq!(id("bin/run", Some("#!/bin/bash")), Some("shell"));
        assert_eq!(id("bin/run", Some("#!/bin/sh")), Some("shell"));
        assert_eq!(id("bin/run", Some("#!/usr/bin/env luajit")), Some("lua"));
        assert_eq!(id("bin/run", Some("#!/usr/bin/php")), None);
        assert_eq!(id("bin/run", None), None);
        assert_eq!(id("notes.txt", None), None);

        // Outside Symfony projects the services files are plain YAML.
        let inactive = ["forge.symfony-services".to_string()];
        assert_eq!(
            types
                .detect("config/services.yaml", None, &inactive)
                .map(|t| t.name.as_str()),
            Some("YAML")
        );
    }

    /// The UI's browser mode serves the same plugins from a JSON copy. Run with
    /// `FORGE_UPDATE_SNAPSHOTS=1 cargo test -p forge-app` after editing a manifest.
    #[test]
    fn browser_mock_snapshot_is_current() {
        let registry = builtin_registry().unwrap();
        let json = serde_json::to_string_pretty(registry.list()).unwrap() + "\n";
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../ui/src/mock/builtinPlugins.json");
        if std::env::var_os("FORGE_UPDATE_SNAPSHOTS").is_some() {
            std::fs::write(&path, &json).unwrap();
        }
        let current = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(
            current.replace("\r\n", "\n") == json,
            "{} is out of date; rerun with FORGE_UPDATE_SNAPSHOTS=1",
            path.display()
        );
    }
}
