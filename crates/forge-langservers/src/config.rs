//! Turning a catalog entry into a [`LspServerConfig`] for one project: file routing,
//! user overrides and the project specific bits some servers need (TypeScript SDK, the
//! Vue TypeScript plugin, ESLint's workspace folder, Lua runtime).

use std::path::{Path, PathBuf};

use forge_lsp::LspServerConfig;
use serde_json::{json, Value};

use crate::catalog::{Catalog, ServerSpec};
use crate::detect::Detector;
use crate::prefs::{LuaEnvironment, Prefs};
use crate::scan::ProjectScan;

/// What config generation needs to know about the project and machine.
pub struct ConfigContext<'a> {
    pub root: &'a Path,
    /// Per-user cache directory for server data (`<cache>/forge`).
    pub cache_dir: PathBuf,
    pub detector: &'a Detector,
    pub prefs: &'a Prefs,
    pub scan: &'a ProjectScan,
}

/// Recursively merge `patch` into `base`: objects merge key by key, everything else
/// (including arrays) is replaced.
pub fn merge_json(base: &mut Value, patch: &Value) {
    match (base, patch) {
        (Value::Object(b), Value::Object(p)) => {
            for (k, v) in p {
                match b.get_mut(k) {
                    Some(existing) => merge_json(existing, v),
                    None => {
                        b.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        (b, p) => *b = p.clone(),
    }
}

fn file_uri(path: &Path) -> String {
    forge_lsp::path_to_uri(path)
        .map(|u| u.to_string())
        .unwrap_or_else(|_| format!("file://{}", path.display()))
}

/// `node_modules` directories to look in for npm packages: the project's, Forge's managed
/// one, then the one next to the given executable.
fn node_modules_dirs(ctx: &ConfigContext, executable: Option<&Path>) -> Vec<PathBuf> {
    let mut dirs = vec![ctx.root.join("node_modules")];
    dirs.extend(ctx.detector.managed_node_modules());
    if let Some(global) = executable.and_then(|e| ctx.detector.global_node_modules(e)) {
        dirs.push(global);
    }
    dirs
}

/// First `<node_modules>/<package>` that exists.
fn find_package(dirs: &[PathBuf], package: &str) -> Option<PathBuf> {
    dirs.iter().map(|d| d.join(package)).find(|p| p.is_dir())
}

/// Location of `@vue/typescript-plugin` for TypeScript servers in Vue projects.
fn vue_plugin(ctx: &ConfigContext, catalog: &Catalog) -> Option<PathBuf> {
    if !ctx.scan.languages.contains("vue") {
        return None;
    }
    let vue = catalog.server("vue-language-server")?;
    let custom = ctx.prefs.server(&vue.id).path;
    let exe = ctx.detector.resolve(&vue.command, custom.as_deref());
    let dirs = node_modules_dirs(ctx, exe.as_deref());
    find_package(&dirs, "@vue/typescript-plugin")
        .or_else(|| find_package(&dirs, "@vue/language-server"))
}

/// Build the config for `spec`, started from `executable`.
/// The Lua environment to configure: `Auto` looks at the project root (LÖVE: `conf.lua` or a
/// `main.lua` using `love.`; Neovim: `init.lua` next to `lua/`, or a `lua/` + `plugin/` plugin).
pub fn lua_environment(env: LuaEnvironment, root: &Path) -> LuaEnvironment {
    if env != LuaEnvironment::Auto {
        return env;
    }
    let main_uses_love = || {
        std::fs::read_to_string(root.join("main.lua"))
            .map(|s| s.contains("love."))
            .unwrap_or(false)
    };
    if root.join("conf.lua").is_file() || main_uses_love() {
        return LuaEnvironment::Love;
    }
    let lua_dir = root.join("lua").is_dir();
    if lua_dir && (root.join("init.lua").is_file() || root.join("plugin").is_dir()) {
        return LuaEnvironment::Neovim;
    }
    LuaEnvironment::Plain
}

/// typescript-language-server's inlay hint preferences (`initializationOptions.preferences`
/// and the `typescript` / `javascript` settings).
static TS_INLAY_PREFERENCES: std::sync::LazyLock<Value> = std::sync::LazyLock::new(|| {
    json!({
        "includeInlayParameterNameHints": "literals",
        "includeInlayParameterNameHintsWhenArgumentMatchesName": false,
        "includeInlayVariableTypeHints": true,
        "includeInlayFunctionLikeReturnTypeHints": true,
        "includeInlayPropertyDeclarationTypeHints": true,
    })
});

pub fn build_config(
    catalog: &Catalog,
    spec: &ServerSpec,
    executable: &Path,
    ctx: &ConfigContext,
) -> LspServerConfig {
    let mut file_extensions = Vec::new();
    let mut language_ids = Vec::new();
    for p in catalog.patterns_of(spec) {
        let entry = if p.whole_name {
            format!("={}", p.pattern)
        } else {
            p.pattern
        };
        if !file_extensions.contains(&entry) {
            file_extensions.push(entry);
            language_ids.push(p.lsp_language_id);
        }
    }
    let mut args = spec.args.clone();
    let mut init = match &spec.initialization_options {
        Value::Null => json!({}),
        v => v.clone(),
    };
    let mut settings = match &spec.settings {
        Value::Null => json!({}),
        v => v.clone(),
    };

    match spec.id.as_str() {
        "typescript-language-server" | "vtsls" => {
            // Inlay hints: the servers report every kind; Forge filters them by the
            // Inlay Hints settings (`forge_codeintel::inlay`).
            merge_json(
                &mut init,
                &json!({ "preferences": TS_INLAY_PREFERENCES.clone() }),
            );
            // Auto Import (Settings | Editor | General | Auto Import).
            let imports = json!({
                "includeCompletionsForModuleExports": true,
                "importModuleSpecifierPreference": ctx.prefs.js_import_paths,
                "preferTypeOnlyAutoImports": ctx.prefs.ts_type_imports,
            });
            merge_json(&mut init, &json!({ "preferences": imports }));
            for language in ["typescript", "javascript"] {
                merge_json(
                    &mut settings,
                    &json!({ language: { "preferences": {
                        "importModuleSpecifier": ctx.prefs.js_import_paths,
                        "preferTypeOnlyAutoImports": ctx.prefs.ts_type_imports,
                    } } }),
                );
                merge_json(
                    &mut settings,
                    &json!({ language: { "inlayHints": TS_INLAY_PREFERENCES.clone() } }),
                );
                merge_json(
                    &mut settings,
                    &json!({ language: { "inlayHints": {
                        "parameterNames": { "enabled": "literals" },
                        "variableTypes": { "enabled": true },
                        "functionLikeReturnTypes": { "enabled": true },
                        "propertyDeclarationTypes": { "enabled": true },
                    } } }),
                );
            }
            if let Some(plugin) = vue_plugin(ctx, catalog) {
                let entry = json!({
                    "name": "@vue/typescript-plugin",
                    "location": plugin.display().to_string(),
                    "languages": ["vue"],
                    "configNamespace": "typescript",
                    "enableForWorkspaceTypeScriptVersions": true,
                });
                if spec.id == "vtsls" {
                    merge_json(
                        &mut settings,
                        &json!({"vtsls": {"tsserver": {"globalPlugins": [entry]}}}),
                    );
                } else {
                    merge_json(&mut init, &json!({ "plugins": [entry] }));
                }
                file_extensions.push("vue".into());
                language_ids.push("vue".into());
            }
        }
        "basedpyright" | "pyright" => {
            // The project's interpreter (Settings | Python Interpreter): imports resolve
            // against its site-packages.
            if let Some(python) = &ctx.prefs.python_path {
                merge_json(
                    &mut settings,
                    &json!({"python": {"pythonPath": python.display().to_string()}}),
                );
            }
        }
        "vue-language-server" | "astro-ls" => {
            let dirs = node_modules_dirs(ctx, Some(executable));
            if let Some(ts) = find_package(&dirs, "typescript/lib") {
                merge_json(
                    &mut init,
                    &json!({"typescript": {"tsdk": ts.display().to_string()}}),
                );
            }
        }
        "ngserver" => {
            let probes: Vec<String> = node_modules_dirs(ctx, Some(executable))
                .iter()
                .filter(|d| d.is_dir())
                .map(|d| d.display().to_string())
                .collect();
            let probes = if probes.is_empty() {
                ctx.root.join("node_modules").display().to_string()
            } else {
                probes.join(",")
            };
            args.extend([
                "--tsProbeLocations".into(),
                probes.clone(),
                "--ngProbeLocations".into(),
                probes,
            ]);
        }
        "vscode-eslint-language-server" => {
            let name = ctx
                .root
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            merge_json(
                &mut settings,
                &json!({"workspaceFolder": {"uri": file_uri(ctx.root), "name": name}}),
            );
        }
        "lua-language-server" => {
            let lua = match lua_environment(ctx.prefs.lua_environment, ctx.root) {
                LuaEnvironment::Plain | LuaEnvironment::Auto => {
                    json!({"runtime": {"version": ctx.prefs.lua_version}})
                }
                LuaEnvironment::Love => json!({
                    "runtime": {"version": "LuaJIT"},
                    "diagnostics": {"globals": ["love"]},
                    "workspace": {"library": ["${3rd}/love2d/library"]},
                }),
                LuaEnvironment::Neovim => {
                    let mut library = vec![json!("${3rd}/luv/library")];
                    if let Some(rt) = std::env::var_os("VIMRUNTIME") {
                        library.push(json!(PathBuf::from(rt).join("lua").display().to_string()));
                    }
                    json!({
                        "runtime": {"version": "LuaJIT", "path": ["lua/?.lua", "lua/?/init.lua"]},
                        "diagnostics": {"globals": ["vim"]},
                        "workspace": {"library": library},
                    })
                }
            };
            merge_json(&mut settings, &json!({ "Lua": lua }));
            // Inlay hints (filtered by Forge's Inlay Hints settings).
            merge_json(
                &mut settings,
                &json!({ "Lua": { "hint": { "enable": true, "paramName": "Literal", "setType": true } } }),
            );
            // The user's library folders come after the environment's.
            if !ctx.prefs.lua_libraries.is_empty() {
                let home = std::env::var_os("HOME").map(PathBuf::from);
                let extra: Vec<Value> = ctx
                    .prefs
                    .lua_libraries
                    .iter()
                    .map(|l| match (l.strip_prefix("~/"), &home) {
                        (Some(rest), Some(h)) => json!(h.join(rest).display().to_string()),
                        _ => json!(l),
                    })
                    .collect();
                let lua = settings
                    .as_object_mut()
                    .and_then(|s| s.get_mut("Lua"))
                    .and_then(Value::as_object_mut);
                if let Some(lua) = lua {
                    let workspace = lua.entry("workspace").or_insert_with(|| json!({}));
                    if let Some(ws) = workspace.as_object_mut() {
                        let library = ws.entry("library").or_insert_with(|| json!([]));
                        if let Some(list) = library.as_array_mut() {
                            list.extend(extra);
                        }
                    }
                }
            }
        }
        _ => {}
    }

    // The user's extra JSON wins over everything above.
    if let Ok(Some(extra)) = ctx.prefs.server(&spec.id).extra_settings() {
        let mut extra = extra;
        if let Some(init_patch) = extra
            .as_object_mut()
            .and_then(|o| o.remove("initializationOptions"))
        {
            merge_json(&mut init, &init_patch);
        }
        merge_json(&mut settings, &extra);
    }

    let empty = |v: &Value| v.as_object().is_some_and(|o| o.is_empty());
    // npm servers in Forge's directory run with Forge's (or the machine's) Node.js.
    let (command, args) = ctx.detector.launch(executable, &args);
    LspServerConfig {
        id: spec.id.clone(),
        name: Some(spec.name.clone()),
        language_ids,
        file_extensions,
        command,
        args,
        initialization_options: if empty(&init) { Value::Null } else { init },
        settings: if empty(&settings) {
            Value::Null
        } else {
            settings
        },
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detect::tests::fake_bin;
    use crate::detect::DetectEnv;
    use crate::prefs::ServerPrefs;
    use std::path::Path;

    fn ctx<'a>(
        root: &'a Path,
        detector: &'a Detector,
        prefs: &'a Prefs,
        scan: &'a ProjectScan,
    ) -> ConfigContext<'a> {
        ConfigContext {
            root,
            cache_dir: root.join(".cache"),
            detector,
            prefs,
            scan,
        }
    }

    #[test]
    fn routes_extensions_with_language_ids() {
        let tmp = tempfile::tempdir().unwrap();
        let d = Detector::new(DetectEnv::default());
        let (prefs, scan) = (Prefs::default(), ProjectScan::default());
        let c = Catalog::builtin();
        let ts = c.server("typescript-language-server").unwrap();
        let cfg = build_config(
            c,
            ts,
            Path::new("/bin/tls"),
            &ctx(tmp.path(), &d, &prefs, &scan),
        );
        assert_eq!(cfg.command, "/bin/tls");
        assert_eq!(cfg.args, vec!["--stdio"]);
        assert_eq!(cfg.file_extensions.len(), cfg.language_ids.len());
        assert_eq!(
            cfg.language_id_for(Path::new("/p/App.tsx")),
            "typescriptreact"
        );
        assert_eq!(
            cfg.language_id_for(Path::new("/p/a.jsx")),
            "javascriptreact"
        );
        assert_eq!(cfg.language_id_for(Path::new("/p/a.mjs")), "javascript");
        assert_eq!(cfg.initialization_options["hostInfo"], "forge");
        let ts_prefs = &cfg.initialization_options["preferences"];
        assert_eq!(ts_prefs["includeInlayParameterNameHints"], "literals");
        assert_eq!(ts_prefs["importModuleSpecifierPreference"], "shortest");
        assert_eq!(ts_prefs["preferTypeOnlyAutoImports"], false);
        let relative = Prefs {
            js_import_paths: "relative".into(),
            ts_type_imports: true,
            ..Prefs::default()
        };
        let cfg2 = build_config(
            c,
            ts,
            Path::new("/bin/tls"),
            &ctx(tmp.path(), &d, &relative, &scan),
        );
        assert_eq!(
            cfg2.initialization_options["preferences"]["importModuleSpecifierPreference"],
            "relative"
        );
        assert_eq!(
            cfg2.settings["typescript"]["preferences"]["preferTypeOnlyAutoImports"],
            true
        );
        assert_eq!(
            cfg.settings["typescript"]["inlayHints"]["variableTypes"]["enabled"],
            true
        );

        let docker = c.server("docker-langserver").unwrap();
        let cfg = build_config(
            c,
            docker,
            Path::new("/bin/d"),
            &ctx(tmp.path(), &d, &prefs, &scan),
        );
        assert!(cfg.file_extensions.contains(&"=Dockerfile".to_string()));
        assert!(cfg.match_len(Path::new("/p/Dockerfile")).is_some());
        assert_eq!(cfg.initialization_options, Value::Null);
    }

    #[test]
    fn json_and_yaml_ship_schemas() {
        let tmp = tempfile::tempdir().unwrap();
        let d = Detector::new(DetectEnv::default());
        let (prefs, scan) = (Prefs::default(), ProjectScan::default());
        let c = Catalog::builtin();
        let json = build_config(
            c,
            c.server("vscode-json-language-server").unwrap(),
            Path::new("j"),
            &ctx(tmp.path(), &d, &prefs, &scan),
        );
        let schemas = json.setting(Some("json.schemas"));
        let matches: Vec<String> = schemas
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|s| {
                s["fileMatch"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|m| m.as_str().unwrap().to_string())
            })
            .collect();
        for f in ["composer.json", "package.json", "tsconfig.json"] {
            assert!(matches.contains(&f.to_string()), "{f}");
        }
        let yaml = build_config(
            c,
            c.server("yaml-language-server").unwrap(),
            Path::new("y"),
            &ctx(tmp.path(), &d, &prefs, &scan),
        );
        let schemas = yaml.setting(Some("yaml.schemas"));
        assert!(schemas["kubernetes"].is_array());
        assert!(schemas["https://json.schemastore.org/github-workflow.json"].is_string());
        assert!(yaml.match_len(Path::new("/p/docker-compose.yml")).is_some());
    }

    #[test]
    fn vue_hybrid_mode_wires_the_typescript_plugin() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("app");
        let prefix = tmp.path().join("npm");
        fake_bin(&prefix.join("bin/vue-language-server"));
        std::fs::create_dir_all(prefix.join("lib/node_modules/@vue/typescript-plugin")).unwrap();
        std::fs::create_dir_all(root.join("node_modules/typescript/lib")).unwrap();
        let d = Detector::new(DetectEnv {
            npm_prefix: Some(prefix.clone()),
            ..Default::default()
        });
        let prefs = Prefs::default();
        let mut scan = ProjectScan::default();
        scan.languages.insert("vue".into());
        let c = Catalog::builtin();
        let cx = ctx(&root, &d, &prefs, &scan);

        let ts = build_config(
            c,
            c.server("typescript-language-server").unwrap(),
            Path::new("t"),
            &cx,
        );
        let plugin = &ts.initialization_options["plugins"][0];
        assert_eq!(plugin["name"], "@vue/typescript-plugin");
        assert_eq!(
            plugin["location"],
            prefix
                .join("lib/node_modules/@vue/typescript-plugin")
                .display()
                .to_string()
        );
        assert!(ts.match_len(Path::new("/p/App.vue")).is_some());
        assert_eq!(ts.language_id_for(Path::new("/p/App.vue")), "vue");

        let vtsls = build_config(c, c.server("vtsls").unwrap(), Path::new("v"), &cx);
        assert_eq!(
            vtsls.settings["vtsls"]["tsserver"]["globalPlugins"][0]["languages"][0],
            "vue"
        );
        assert_eq!(vtsls.settings["vtsls"]["autoUseWorkspaceTsdk"], true);

        let vue = build_config(
            c,
            c.server("vue-language-server").unwrap(),
            &prefix.join("bin/vue-language-server"),
            &cx,
        );
        assert_eq!(vue.initialization_options["vue"]["hybridMode"], true);
        assert_eq!(
            vue.initialization_options["typescript"]["tsdk"],
            root.join("node_modules/typescript/lib")
                .display()
                .to_string()
        );

        // Without .vue files the TypeScript server stays plain.
        let plain = ProjectScan::default();
        let ts = build_config(
            c,
            c.server("typescript-language-server").unwrap(),
            Path::new("t"),
            &ctx(&root, &d, &prefs, &plain),
        );
        assert!(ts.initialization_options.get("plugins").is_none());
    }

    #[test]
    fn project_specific_arguments_and_settings() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("shop");
        std::fs::create_dir_all(root.join("node_modules")).unwrap();
        let d = Detector::new(DetectEnv::default());
        let mut prefs = Prefs::default();
        let scan = ProjectScan::default();
        let c = Catalog::builtin();

        let ng = build_config(
            c,
            c.server("ngserver").unwrap(),
            Path::new("n"),
            &ctx(&root, &d, &prefs, &scan),
        );
        assert_eq!(ng.args[1], "--tsProbeLocations");
        assert_eq!(ng.args[2], root.join("node_modules").display().to_string());

        let eslint = build_config(
            c,
            c.server("vscode-eslint-language-server").unwrap(),
            Path::new("e"),
            &ctx(&root, &d, &prefs, &scan),
        );
        assert_eq!(eslint.settings["workspaceFolder"]["name"], "shop");
        assert!(eslint.settings["workspaceFolder"]["uri"]
            .as_str()
            .unwrap()
            .starts_with("file://"));
        assert_eq!(eslint.setting(None)["validate"], "on");

        prefs.lua_environment = LuaEnvironment::Love;
        let lua = build_config(
            c,
            c.server("lua-language-server").unwrap(),
            Path::new("l"),
            &ctx(&root, &d, &prefs, &scan),
        );
        assert_eq!(lua.settings["Lua"]["runtime"]["version"], "LuaJIT");
        assert_eq!(lua.settings["Lua"]["diagnostics"]["globals"][0], "love");
        assert_eq!(lua.settings["Lua"]["telemetry"]["enable"], false);
        assert_eq!(lua.settings["Lua"]["hint"]["enable"], true);
        // Detected from the project: a LÖVE game has conf.lua.
        prefs.lua_environment = LuaEnvironment::Auto;
        std::fs::write(root.join("conf.lua"), "function love.conf(t) end").unwrap();
        let lua = build_config(
            c,
            c.server("lua-language-server").unwrap(),
            Path::new("l"),
            &ctx(&root, &d, &prefs, &scan),
        );
        assert_eq!(lua.settings["Lua"]["diagnostics"]["globals"][0], "love");
        std::fs::remove_file(root.join("conf.lua")).unwrap();
        prefs.lua_environment = LuaEnvironment::Neovim;
        let lua = build_config(
            c,
            c.server("lua-language-server").unwrap(),
            Path::new("l"),
            &ctx(&root, &d, &prefs, &scan),
        );
        assert_eq!(lua.settings["Lua"]["diagnostics"]["globals"][0], "vim");
        prefs.lua_environment = LuaEnvironment::Love;
        prefs.lua_libraries = vec!["vendor/lua".into()];
        let lua = build_config(
            c,
            c.server("lua-language-server").unwrap(),
            Path::new("l"),
            &ctx(&root, &d, &prefs, &scan),
        );
        assert_eq!(
            lua.settings["Lua"]["workspace"]["library"],
            json!(["${3rd}/love2d/library", "vendor/lua"])
        );
    }

    #[test]
    fn user_json_is_merged_last() {
        let tmp = tempfile::tempdir().unwrap();
        let d = Detector::new(DetectEnv::default());
        let mut prefs = Prefs::default();
        prefs.servers.insert(
            "gopls".into(),
            ServerPrefs {
                settings_json: Some(
                    r#"{"gopls": {"staticcheck": true}, "initializationOptions": {"x": 1}}"#.into(),
                ),
                ..Default::default()
            },
        );
        let scan = ProjectScan::default();
        let c = Catalog::builtin();
        let cfg = build_config(
            c,
            c.server("gopls").unwrap(),
            Path::new("g"),
            &ctx(tmp.path(), &d, &prefs, &scan),
        );
        assert_eq!(cfg.settings["gopls"]["staticcheck"], true);
        assert_eq!(
            cfg.settings["gopls"]["semanticTokens"], true,
            "defaults kept"
        );
        assert!(cfg.settings.get("initializationOptions").is_none());
        assert_eq!(cfg.initialization_options["x"], 1);
    }

    #[test]
    fn merge_replaces_arrays_and_scalars() {
        let mut a = json!({"a": {"b": 1, "c": [1, 2]}, "d": 1});
        merge_json(&mut a, &json!({"a": {"c": [3]}, "d": {"e": 2}}));
        assert_eq!(a, json!({"a": {"b": 1, "c": [3]}, "d": {"e": 2}}));
    }
}
