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
        ("legacy/templates/cart.tpl", "Smarty"),
        ("app/Presenters/templates/Home.latte", "Latte"),
        ("templates/mail.hbs", "Handlebars"),
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
