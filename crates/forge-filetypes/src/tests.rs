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
    assert_eq!(id("#!/usr/bin/php"), None);
    assert_eq!(id("echo hi"), None);
    assert_eq!(r.detect("bin/run", None, &[]), None);
    // A known name beats the shebang.
    assert_eq!(
        r.detect("x.php", Some("#!/bin/bash"), &[]).unwrap().id,
        "php"
    );
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
