use super::*;

fn spec(id: &str, name: &str, extensions: &[&str], file_names: &[&str]) -> FileTypeSpec {
    FileTypeSpec {
        id: id.into(),
        name: name.into(),
        extensions: extensions.iter().map(|s| s.to_string()).collect(),
        file_names: file_names.iter().map(|s| s.to_string()).collect(),
        ..Default::default()
    }
}

fn label(r: &FileTypeRegistry, path: &str) -> String {
    let req = DetectRequest::default();
    r.info(
        &FileRef {
            path: path.into(),
            first_line: None,
        },
        &req,
    )
    .label
}

#[test]
fn bundled_types_by_name_and_extension() {
    let r = FileTypeRegistry::with_builtins();
    for (path, expected) in [
        ("app/Models/User.php", "PHP"),
        ("resources/views/welcome.blade.php", "Blade"),
        ("composer.json", "JSON"),
        ("composer.lock", "JSON"),
        ("docker-compose.yml", "YAML"),
        ("config/services.yaml", "YAML"),
        ("database/schema.sql", "SQL"),
        (".env", "Dotenv"),
        (".env.example", "Dotenv"),
        ("deploy/prod.env", "Dotenv"),
        ("Dockerfile", "Dockerfile"),
        ("Dockerfile.dev", "Dockerfile"),
        ("README.md", "Markdown"),
        ("resources/js/app.ts", "TypeScript"),
        ("phpunit.xml", "XML"),
        ("phpunit.xml.dist", "XML"),
        ("Cargo.toml", "TOML"),
        (".gitignore", "Ignore File"),
        ("templates/order/show.html.twig", "Twig"),
        ("app/views/orders/show.html.erb", "ERB"),
        ("legacy/templates/cart.tpl", "Smarty"),
        ("app/Presenters/templates/Home.latte", "Latte"),
        ("templates/mail.hbs", "Handlebars"),
        ("shop/templates/base.djhtml", "Django Template"),
        ("deploy.tmpl", "Go Template"),
        ("views/index.html.tmpl", "Go Template"),
        ("db/query.sql.tmpl", "Go Template (SQL)"),
        ("cmd/main.go.tmpl", "Go Template (Go)"),
        ("conf/app.gotmpl", "Go Template"),
        ("web/page.gohtml", "Go Template"),
        ("src/App.vue", "Vue"),
        ("src/main.tsx", "TypeScript React"),
        ("styles/app.scss", "SCSS"),
        ("main.go", "Go"),
        ("script.py", "Python"),
        ("Gemfile", "Ruby"),
        ("Jenkinsfile", "Groovy"),
        ("scripts/release.gy", "Groovy"),
        ("src/test/groovy/CartSpec.groovy", "Groovy"),
        ("build.sbt", "Scala"),
        ("project/plugins.sbt", "Scala"),
        ("src/main/scala/Main.scala", "Scala"),
        ("Makefile", "Makefile"),
        (".htaccess", "Apache Config"),
        ("requests.http", "HTTP Request"),
        ("nginx.conf", "Nginx"),
        ("LICENSE", "Plain text"),
        ("Assets/Scripts/Game.asmdef", "JSON"),
        ("Assets/Plugins/Shared.asmref", "JSON"),
        ("Assets/UI/Main.uss", "CSS"),
        ("Assets/UI/Main.uxml", "XML"),
    ] {
        assert_eq!(label(&r, path), expected, "{path}");
    }
}

#[test]
fn longer_extensions_and_names_win() {
    let r = FileTypeRegistry::with_builtins();
    assert_eq!(r.detect("x.blade.php", None, &[]).unwrap().id, "blade");
    assert_eq!(r.detect("a.PY", None, &[]).unwrap().id, "python");
}

#[test]
fn plugins_override_bundled_types_and_fall_back_when_inactive() {
    let mut r = FileTypeRegistry::with_builtins();
    let mut json = spec("json", "JSON", &["json"], &["symfony.lock"]);
    json.indent_size = Some(2);
    r.register(json, Some("forge.json"));
    r.register(
        spec(
            "symfony-services",
            "Symfony Services",
            &[],
            &["config/services*.yaml", "config/services/*.yaml"],
        ),
        Some("forge.symfony-services"),
    );
    let id = |path: &str, inactive: &[&str]| {
        let inactive: Vec<String> = inactive.iter().map(|s| s.to_string()).collect();
        r.detect(path, None, &inactive).map(|t| t.id.clone())
    };
    assert_eq!(id("symfony.lock", &[]).as_deref(), Some("json"));
    assert_eq!(id("symfony.lock", &["forge.json"]).as_deref(), Some("json"));
    assert_eq!(
        id("config/services.yaml", &[]).as_deref(),
        Some("symfony-services")
    );
    assert_eq!(
        id("config/services_test.yaml", &[]).as_deref(),
        Some("symfony-services")
    );
    assert_eq!(
        id("app/config/services/api.yaml", &[]).as_deref(),
        Some("symfony-services")
    );
    assert_eq!(
        id("config/packages/twig.yaml", &[]).as_deref(),
        Some("yaml")
    );
    assert_eq!(
        id("config/services.yaml", &["forge.symfony-services"]).as_deref(),
        Some("yaml")
    );

    let req = DetectRequest {
        files: vec![FileRef {
            path: "package.json".into(),
            first_line: None,
        }],
        inactive_plugins: vec!["forge.json".into()],
        ..Default::default()
    };
    assert_eq!(r.resolve(&req)[0].indent, "    ");
    let req = DetectRequest {
        inactive_plugins: vec![],
        ..req
    };
    assert_eq!(r.resolve(&req)[0].indent, "  ");
}

#[test]
fn registering_the_same_id_from_the_same_source_replaces_it() {
    let mut r = FileTypeRegistry::with_builtins();
    let before = r.specs().count();
    r.register(
        spec("twig", "Twig (Symfony)", &["html.twig", "twig"], &[]),
        None,
    );
    assert_eq!(r.specs().count(), before);
    assert_eq!(
        r.detect("templates/base.html.twig", None, &[])
            .unwrap()
            .name,
        "Twig (Symfony)"
    );
}

#[test]
fn detects_scripts_by_shebang_only_without_a_known_type() {
    let mut r = FileTypeRegistry::with_builtins();
    let mut sh = spec("shell", "Shell Script", &["sh"], &[]);
    sh.interpreters = ["sh", "bash", "zsh", "ksh", "dash", "ash"]
        .map(String::from)
        .to_vec();
    r.register(sh, Some("forge.bash"));
    let mut py = spec("python", "Python", &["py"], &[]);
    py.interpreters = vec!["python".into()];
    r.register(py, Some("forge.python"));
    let id = |first: &str| r.detect("bin/run", Some(first), &[]).map(|t| t.id.clone());
    assert_eq!(id("#!/usr/bin/env python3").as_deref(), Some("python"));
    assert_eq!(
        id("#!/usr/bin/env -S python3.12 -u").as_deref(),
        Some("python")
    );
    assert_eq!(id("#!/bin/bash").as_deref(), Some("shell"));
    assert_eq!(id("#!/bin/sh -e").as_deref(), Some("shell"));
    assert_eq!(id("echo hi"), None);
    assert_eq!(id("#!"), None);
    assert_eq!(r.detect("bin/run", None, &[]), None);
    // A known name beats the shebang.
    assert_eq!(
        r.detect("x.php", Some("#!/bin/bash"), &[]).unwrap().id,
        "php"
    );
}

#[test]
fn detects_every_bundled_interpreter_through_env_variants() {
    let r = FileTypeRegistry::with_builtins();
    let id = |first: &str| r.detect("bin/run", Some(first), &[]).map(|t| t.id.clone());
    for (line, expected) in [
        ("#!/usr/bin/php", "php"),
        ("#!/usr/bin/env php", "php"),
        ("#!/usr/bin/env php8.3", "php"),
        ("#!/usr/local/bin/php -q", "php"),
        ("#!/opt/homebrew/bin/php", "php"),
        ("#! /usr/bin/env php", "php"),
        ("#!/usr/bin/env -S php -d memory_limit=-1", "php"),
        ("#!/usr/bin/env -i php", "php"),
        ("#!/usr/bin/env FOO=1 BAR=2 php", "php"),
        ("#!/usr/bin/env -u HOME php", "php"),
        ("#!/usr/bin/env --split-string=php -q", "php"),
        ("#!/usr/bin/env -- php", "php"),
        ("#!C:\\php\\php.exe", "php"),
        ("#!/usr/bin/env node", "javascript"),
        ("#!/usr/bin/env nodejs", "javascript"),
        ("#!/usr/bin/env bun", "javascript"),
        ("#!/usr/bin/env -S deno run --allow-net", "typescript"),
        ("#!/usr/bin/env ts-node", "typescript"),
        ("#!/usr/bin/env tsx", "typescript"),
        ("#!/usr/bin/perl -w", "perl"),
        ("#!/usr/bin/env groovy", "groovy"),
        ("#!/usr/bin/env kotlin", "kotlin"),
        ("#!/usr/bin/env gorun", "go"),
        ("#!/usr/bin/env elixir", "elixir"),
        ("#!/usr/bin/env Rscript", "r"),
        ("#!/usr/bin/env julia", "julia"),
        ("#!/usr/bin/tclsh", "tcl"),
        ("#!/usr/bin/env fish", "shell"),
        ("#!/usr/bin/env pwsh", "powershell"),
        ("#!/usr/bin/make -f", "makefile"),
        ("#!/usr/bin/env python3.12", "python"),
        ("#!/usr/bin/env -S uv run --script", "python"),
        ("#!/bin/bash", "shell"),
    ] {
        assert_eq!(id(line).as_deref(), Some(expected), "{line}");
    }
    for line in [
        "#!/usr/bin/env",
        "#!/usr/bin/env -i",
        "#!/usr/bin/phpx",
        "#!/usr/bin/env FOO=1",
    ] {
        assert_eq!(id(line), None, "{line}");
    }
    // Laravel and Symfony entry points.
    assert_eq!(
        r.detect("artisan", Some("#!/usr/bin/env php"), &[])
            .unwrap()
            .id,
        "php"
    );
    assert_eq!(
        r.detect("bin/console", Some("#!/usr/bin/env php"), &[])
            .unwrap()
            .id,
        "php"
    );
    // A known extension still wins.
    assert_eq!(
        r.detect("deploy.sh", Some("#!/usr/bin/env php"), &[])
            .unwrap()
            .id,
        "shell"
    );
}

#[test]
fn plugin_types_without_interpreters_keep_the_bundled_ones() {
    let mut r = FileTypeRegistry::with_builtins();
    // The CMS plugin replaces `php` with more extensions but no interpreters.
    r.register(
        spec("php", "PHP", &["php", "module"], &[]),
        Some("forge.cms"),
    );
    let detected = r
        .detect("artisan", Some("#!/usr/bin/env php"), &[])
        .unwrap();
    assert_eq!(detected.id, "php");
    assert_eq!(detected.extensions, ["php", "module"]);
}

#[test]
fn modelines_select_a_type_after_names_and_shebangs() {
    let r = FileTypeRegistry::with_builtins();
    let id = |path: &str, first: &str| r.detect(path, Some(first), &[]).map(|t| t.id.clone());
    assert_eq!(
        id("scripts/run", "# vim: set ft=php:").as_deref(),
        Some("php")
    );
    assert_eq!(
        id("scripts/run", "# -*- mode: python -*-").as_deref(),
        Some("python")
    );
    assert_eq!(id("scripts/run", "# vim: ft=sh").as_deref(), Some("shell"));
    assert_eq!(
        id("scripts/run", "// -*- js -*-").as_deref(),
        Some("javascript")
    );
    assert_eq!(id("scripts/run", "# vim: ft=klingon").as_deref(), None);
    // Names win over modelines.
    assert_eq!(id("a.yaml", "# vim: set ft=php:").as_deref(), Some("yaml"));
}

fn resolve_one(
    r: &FileTypeRegistry,
    req: &DetectRequest,
    path: &str,
    first: Option<&str>,
) -> FileTypeInfo {
    let req = DetectRequest {
        files: vec![FileRef {
            path: path.into(),
            first_line: first.map(str::to_string),
        }],
        ..req.clone()
    };
    r.resolve(&req).remove(0)
}

#[test]
fn overrides_win_over_names_extensions_and_shebangs() {
    let mut r = FileTypeRegistry::with_builtins();
    r.register(
        spec(
            "symfony-services",
            "Symfony Services",
            &[],
            &["config/services.yaml"],
        ),
        Some("forge.symfony-services"),
    );
    let req = DetectRequest {
        overrides: vec![
            serde_json::json!({ "path": "LICENSE", "fileType": "php" }),
            serde_json::json!({ "path": "deploy.sh", "fileType": "python" }),
            serde_json::json!({ "path": "bin/tool", "fileType": "shell" }),
            serde_json::json!({ "path": "app/User.php", "fileType": "text" }),
            serde_json::json!({ "path": "gone.txt", "fileType": "no-such-type" }),
            serde_json::json!({ "path": "config/other.yaml", "fileType": "symfony-services" }),
            serde_json::json!({ "path": "notes.md", "fileType": "my-type" }),
        ],
        inactive_plugins: vec!["forge.symfony-services".into()],
        custom_types: vec![serde_json::json!({ "name": "My Type" })],
        ..Default::default()
    };
    let info = resolve_one(&r, &req, "LICENSE", None);
    assert_eq!((info.id.as_deref(), info.overridden), (Some("php"), true));
    assert_eq!(resolve_one(&r, &req, "deploy.sh", None).label, "Python");
    assert_eq!(
        resolve_one(&r, &req, "bin/tool", Some("#!/usr/bin/env php"))
            .id
            .as_deref(),
        Some("shell")
    );
    let plain = resolve_one(&r, &req, "app/User.php", None);
    assert_eq!(
        (plain.id, plain.label.as_str(), plain.overridden),
        (None, "Plain text", true)
    );
    // An unknown type is ignored.
    let unknown = resolve_one(&r, &req, "gone.txt", None);
    assert_eq!((unknown.id, unknown.overridden), (None, false));
    // A type of an inactive plugin is activated for the overridden file only.
    assert_eq!(
        resolve_one(&r, &req, "config/other.yaml", None)
            .id
            .as_deref(),
        Some("symfony-services")
    );
    assert_eq!(
        resolve_one(&r, &req, "config/services.yaml", None)
            .id
            .as_deref(),
        Some("yaml")
    );
    // Custom file types can be chosen too.
    let custom = custom_id("My Type");
    let req = DetectRequest {
        overrides: vec![serde_json::json!({ "path": "notes.md", "fileType": custom })],
        ..req
    };
    let info = resolve_one(&r, &req, "notes.md", None);
    assert_eq!(info.label, "My Type");
    assert!(info.custom.is_some());
    // Not overridden: detected as usual.
    assert!(!resolve_one(&r, &req, "README.md", None).overridden);
}

#[test]
fn override_choices_list_every_type_once() {
    let mut r = FileTypeRegistry::with_builtins();
    r.register(
        spec("php", "PHP", &["php", "module"], &[]),
        Some("forge.cms"),
    );
    r.register(
        spec("symfony-services", "Symfony Services", &[], &[]),
        Some("forge.symfony-services"),
    );
    let inactive = vec!["forge.symfony-services".to_string()];
    let overrides = vec![serde_json::json!({ "path": "artisan", "fileType": "php" })];
    let c = r.override_choices("artisan", &inactive, &UserFileTypes::default(), &overrides);
    assert_eq!(c.choices.iter().filter(|c| c.id == "php").count(), 1);
    assert!(c
        .choices
        .iter()
        .any(|c| c.id == "text" && c.name == "Plain text"));
    let symfony = c
        .choices
        .iter()
        .find(|c| c.id == "symfony-services")
        .unwrap();
    assert_eq!(
        symfony.inactive_plugin.as_deref(),
        Some("forge.symfony-services")
    );
    let names: Vec<String> = c.choices.iter().map(|c| c.name.to_lowercase()).collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted);
    assert_eq!(c.current.as_deref(), Some("php"));
    assert_eq!(c.pattern, "artisan");
}

#[test]
fn template_files_get_their_data_language() {
    let r = FileTypeRegistry::with_builtins();
    let req = DetectRequest {
        files: vec![
            FileRef {
                path: "legacy/latex/letter.tpl".into(),
                first_line: None,
            },
            FileRef {
                path: "templates/invoice.tex.twig".into(),
                first_line: None,
            },
            FileRef {
                path: "base.html.twig".into(),
                first_line: None,
            },
            FileRef {
                path: "README.md".into(),
                first_line: None,
            },
            FileRef {
                path: "app/views/user_mailer/welcome.text.erb".into(),
                first_line: None,
            },
            FileRef {
                path: "app/views/orders/show.html.erb".into(),
                first_line: None,
            },
        ],
        data_languages: vec!["legacy/latex/*=latex".into()],
        soft_wrap_files: Some("*.md; *.txt".into()),
        ..Default::default()
    };
    let out = r.resolve(&req);
    assert_eq!(out[0].label, "Smarty (LaTeX)");
    assert_eq!(out[0].data_language.unwrap().id, "latex");
    assert_eq!(out[1].label, "Twig (LaTeX)");
    assert_eq!(out[2].label, "Twig");
    assert_eq!(out[2].data_language.unwrap().id, "html");
    assert_eq!(out[3].data_language, None);
    assert!(out[3].soft_wrap && !out[0].soft_wrap);
    assert_eq!(out[4].label, "ERB (Plain text)");
    assert_eq!(out[5].label, "ERB");
    assert_eq!(out[5].data_language.unwrap().id, "html");
}

#[test]
fn info_serializes_for_the_ui() {
    let r = FileTypeRegistry::with_builtins();
    let req: DetectRequest = serde_json::from_str(
        r#"{"files":[{"path":"a.sql.tmpl"},{"path":"LICENSE"}],"inactivePlugins":[]}"#,
    )
    .unwrap();
    let json = serde_json::to_value(r.resolve(&req)).unwrap();
    assert_eq!(json[0]["dataLanguage"]["label"], "SQL");
    assert_eq!(json[0]["softWrap"], false);
    assert_eq!(json[1]["id"], serde_json::Value::Null);
    assert_eq!(json[1]["label"], "Plain text");
}

#[test]
fn template_files_get_the_delimiters_of_their_pair() {
    let r = FileTypeRegistry::with_builtins();
    let req: DetectRequest = serde_json::from_str(
        r#"{
            "files": [{"path":"invoice.tex.tpl"},{"path":"cart.tpl"},{"path":"a.latte"},{"path":"x.php"}],
            "templateDelimiters": [{"template":"smarty","language":"latex","left":"[[","right":"]]"}]
        }"#,
    )
    .unwrap();
    let out = r.resolve(&req);
    let left = |i: usize| out[i].delimiters.as_ref().map(|d| d["left"].clone());
    assert_eq!(left(0).as_deref(), Some("[["));
    assert_eq!(left(1).as_deref(), Some("{"));
    // Latte's delimiters are fixed; non-template files have none.
    assert_eq!(out[2].delimiters, None);
    assert_eq!(out[3].delimiters, None);
    let json = serde_json::to_value(&out).unwrap();
    assert_eq!(json[0]["delimiters"]["right"], "]]");
    assert_eq!(json[3]["delimiters"], serde_json::Value::Null);
}

fn detect_with(req: &DetectRequest, path: &str) -> FileTypeInfo {
    let r = FileTypeRegistry::with_builtins();
    let mut req = req.clone();
    req.files = vec![FileRef {
        path: path.into(),
        first_line: None,
    }];
    r.resolve(&req).remove(0)
}

#[test]
fn user_patterns_reassign_and_remove() {
    let req = DetectRequest {
        associations: vec![
            serde_json::json!({ "fileType": "php", "pattern": "*.module", "action": "add" }),
            // Taken away from Smarty and given to PHP.
            serde_json::json!({ "fileType": "php", "pattern": "*.tpl" }),
            serde_json::json!({ "fileType": "php", "pattern": "*.phtml", "action": "remove" }),
            serde_json::json!({ "fileType": "yaml", "pattern": "Procfile*" }),
            // Incomplete entries are skipped.
            serde_json::json!({ "fileType": "php" }),
        ],
        ..Default::default()
    };
    assert_eq!(detect_with(&req, "web/hooks.module").label, "PHP");
    assert_eq!(detect_with(&req, "legacy/cart.tpl").label, "PHP");
    assert_eq!(detect_with(&req, "views/old.phtml").label, PLAIN_TEXT);
    assert_eq!(detect_with(&req, "Procfile.dev").label, "YAML");
    // Without the settings nothing changes.
    let plain = DetectRequest::default();
    assert_eq!(detect_with(&plain, "legacy/cart.tpl").label, "Smarty");
    assert_eq!(detect_with(&plain, "views/old.phtml").label, "PHP");

    let rows = FileTypeRegistry::with_builtins().rows(&[], &req.user_file_types());
    let php = rows.iter().find(|r| r.id == "php").unwrap();
    assert!(php.patterns.contains(&"*.module".to_string()));
    assert_eq!(php.added, ["*.module", "*.tpl"]);
    assert_eq!(php.removed, ["*.phtml"]);
    let smarty = rows.iter().find(|r| r.id == "smarty").unwrap();
    assert_eq!(smarty.removed, ["*.tpl"]);
    let yaml = rows.iter().find(|r| r.id == "yaml").unwrap();
    assert_eq!(yaml.added, ["Procfile*"]);
    // Sorted by name.
    let names: Vec<_> = rows.iter().map(|r| r.name.to_lowercase()).collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted);
}

#[test]
fn custom_file_types_bring_their_syntax() {
    let req = DetectRequest {
        custom_types: vec![
            serde_json::json!({
                "name": "My DSL",
                "description": "Build scripts",
                "lineComment": "#",
                "blockCommentStart": "/*",
                "blockCommentEnd": "*/",
                "hexPrefix": "0x",
                "numberPostfixes": "lL",
                "keywords1": "IF then\nELSE if",
                "keywords2": "print",
                "ignoreCase": "true",
                "pairedBrackets": "false",
            }),
            // Same name again: ignored.
            serde_json::json!({ "name": "my dsl", "lineComment": "//" }),
            serde_json::json!({ "description": "no name" }),
        ],
        associations: vec![serde_json::json!({ "fileType": "custom-my-dsl", "pattern": "*.dsl" })],
        ..Default::default()
    };
    let info = detect_with(&req, "build/main.dsl");
    assert_eq!(info.id.as_deref(), Some("custom-my-dsl"));
    assert_eq!(info.label, "My DSL");
    let syntax = info.custom.unwrap();
    assert_eq!(syntax.line_comment, "#");
    assert_eq!(syntax.keywords[0], ["else", "if", "then"]);
    assert_eq!(syntax.keywords[1], ["print"]);
    assert!(syntax.keywords[2].is_empty() && syntax.keywords.len() == 4);
    assert!(syntax.ignore_case && syntax.paired_braces && !syntax.paired_brackets);
    assert!(syntax.string_escapes);
    // Bundled types carry no custom syntax.
    assert!(detect_with(&req, "a.php").custom.is_none());

    let rows = FileTypeRegistry::with_builtins().rows(&[], &req.user_file_types());
    let custom: Vec<_> = rows.iter().filter(|r| r.custom).collect();
    assert_eq!(custom.len(), 1);
    assert_eq!(custom[0].description, "Build scripts");
    assert_eq!(custom[0].patterns, ["*.dsl"]);
    assert_eq!(custom_id("  C++ / Legacy!  "), "custom-c-legacy");
    // A stored id wins over the name, so renaming keeps the patterns.
    let renamed = DetectRequest {
        custom_types: vec![serde_json::json!({ "id": "custom-k1", "name": "Renamed" })],
        associations: vec![serde_json::json!({ "fileType": "custom-k1", "pattern": "*.k1" })],
        ..Default::default()
    };
    assert_eq!(detect_with(&renamed, "a.k1").label, "Renamed");
}

#[test]
fn ignored_files_use_the_glob_list() {
    for name in [".git", "__pycache__", "x.pyc", "backup~", ".DS_Store"] {
        assert!(matches_glob_list(name, DEFAULT_IGNORED_FILES), "{name}");
    }
    for name in ["src", ".github", "app.php", ".gitignore"] {
        assert!(!matches_glob_list(name, DEFAULT_IGNORED_FILES), "{name}");
    }
}
