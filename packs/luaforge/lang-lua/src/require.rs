//! `require("a.b")` resolution through `package.path`-like templates relative to the
//! project root (`?.lua`, `?/init.lua`, `lua/?.lua`, …), extended by the `lua.requirePaths`
//! setting and the `runtime.path` of a `.luarc.json`.

/// Templates tried for every module, after the configured ones.
pub const DEFAULT_PATHS: &[&str] = &[
    "?.lua",
    "?/init.lua",
    "lua/?.lua",
    "lua/?/init.lua",
    "src/?.lua",
    "src/?/init.lua",
    "lib/?.lua",
    "lib/?/init.lua",
];

/// Normalize a template: `./?.lua` -> `?.lua`, backslashes to slashes. Templates without
/// `?` are taken as directories (`scripts` -> `scripts/?.lua`, `scripts/?/init.lua`).
pub fn normalize(template: &str) -> Vec<String> {
    let t = template.trim().replace('\\', "/");
    let t = t.trim_start_matches("./").to_string();
    if t.is_empty() {
        return Vec::new();
    }
    if t.contains('?') {
        return vec![t];
    }
    let dir = t.trim_end_matches('/');
    vec![format!("{dir}/?.lua"), format!("{dir}/?/init.lua")]
}

/// The templates to try: configured ones (`package.path` strings may hold several
/// separated by `;`) first, then [`DEFAULT_PATHS`].
pub fn templates(configured: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for c in configured {
        for part in c.split(';') {
            for t in normalize(part) {
                if !out.contains(&t) {
                    out.push(t);
                }
            }
        }
    }
    for d in DEFAULT_PATHS {
        if !out.iter().any(|t| t == d) {
            out.push(d.to_string());
        }
    }
    out
}

/// Candidate files for a module name, in order.
pub fn candidates(module: &str, templates: &[String]) -> Vec<String> {
    let module = module.trim();
    if module.is_empty() {
        return Vec::new();
    }
    // `require("lib/vector")` and `require("lib.vector")` both work in practice.
    let as_path = module.trim_end_matches(".lua").replace('.', "/");
    templates.iter().map(|t| t.replace('?', &as_path)).collect()
}

/// The first candidate that exists.
pub fn resolve(
    module: &str,
    templates: &[String],
    exists: impl Fn(&str) -> bool,
) -> Option<String> {
    candidates(module, templates)
        .into_iter()
        .find(|c| exists(c))
}

/// The module name a file is required by (the reverse of [`resolve`]). The first template
/// in order wins, except that `x/init.lua` is `x` and Neovim's `lua/` folder is a root.
pub fn module_name(path: &str, templates: &[String]) -> Option<String> {
    let mut fallback = None;
    for t in templates {
        let Some((prefix, suffix)) = t.split_once('?') else {
            continue;
        };
        if let Some(mid) = path
            .strip_prefix(prefix)
            .and_then(|r| r.strip_suffix(suffix))
        {
            if mid.is_empty() || mid.contains('.') {
                continue;
            }
            let name = mid.replace('/', ".");
            if mid == "init" || mid.ends_with("/init") || mid.starts_with("lua/") {
                fallback.get_or_insert(name);
                continue;
            }
            return Some(name);
        }
    }
    fallback
}

/// `runtime.path` entries of a `.luarc.json` (both the nested and the dotted key form).
pub fn luarc_paths(json: &str) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    let list = value
        .pointer("/runtime/path")
        .or_else(|| value.get("runtime.path"))
        .or_else(|| value.pointer("/Lua/runtime/path"));
    list.and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_through_templates() {
        let files = [
            "lib/vector.lua",
            "lua/plugin/init.lua",
            "src/game/world.lua",
            "util.lua",
        ];
        let exists = |p: &str| files.contains(&p);
        let t = templates(&[]);
        assert_eq!(
            resolve("lib.vector", &t, exists).as_deref(),
            Some("lib/vector.lua")
        );
        assert_eq!(
            resolve("plugin", &t, exists).as_deref(),
            Some("lua/plugin/init.lua")
        );
        assert_eq!(
            resolve("game.world", &t, exists).as_deref(),
            Some("src/game/world.lua")
        );
        assert_eq!(resolve("util", &t, exists).as_deref(), Some("util.lua"));
        assert_eq!(
            resolve("lib/vector", &t, exists).as_deref(),
            Some("lib/vector.lua")
        );
        assert_eq!(resolve("missing", &t, exists), None);
    }

    #[test]
    fn configured_paths_come_first() {
        let t = templates(&["./scripts/?.lua;./?.lua".into(), "vendor".into()]);
        assert_eq!(
            &t[..4],
            [
                "scripts/?.lua",
                "?.lua",
                "vendor/?.lua",
                "vendor/?/init.lua"
            ]
        );
        let files = ["scripts/a.lua", "a.lua"];
        assert_eq!(
            resolve("a", &t, |p| files.contains(&p)).as_deref(),
            Some("scripts/a.lua")
        );
    }

    #[test]
    fn module_names_of_files() {
        let t = templates(&[]);
        assert_eq!(
            module_name("lib/vector.lua", &t).as_deref(),
            Some("lib.vector")
        );
        assert_eq!(
            module_name("lua/plugin/init.lua", &t).as_deref(),
            Some("plugin")
        );
    }

    #[test]
    fn luarc_runtime_path() {
        assert_eq!(
            luarc_paths(r#"{"runtime": {"path": ["?.lua", "mods/?.lua"]}}"#),
            vec!["?.lua", "mods/?.lua"]
        );
        assert_eq!(
            luarc_paths(r#"{"runtime.path": ["x/?.lua"]}"#),
            vec!["x/?.lua"]
        );
        assert!(luarc_paths("not json").is_empty());
    }
}
