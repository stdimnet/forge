//! Tests of the whole engine: one per keyword, completion, hover and Go to Declaration, and
//! samples of composer.json, package.json, Compose, GitHub Actions and GitLab CI files
//! against the bundled schemas.

use std::sync::Arc;

use super::*;

const URI: &str = "https://test.forge/s.json";

fn reg(schema: &str) -> Registry {
    let mut r = Registry::new();
    r.insert(Arc::new(
        SchemaDoc::parse(URI, "schemas/s.json", schema.to_string()).unwrap(),
    ));
    r
}

fn json_problems(schema: &str, text: &str) -> Vec<String> {
    validate(&reg(schema), URI, DocKind::Json, text)
        .into_iter()
        .map(|p| p.message)
        .collect()
}

fn yaml_problems(schema: &str, text: &str) -> Vec<String> {
    validate(&reg(schema), URI, DocKind::Yaml, text)
        .into_iter()
        .map(|p| p.message)
        .collect()
}

fn ranged(schema: &str, kind: DocKind, text: &str) -> Vec<(String, String)> {
    validate(&reg(schema), URI, kind, text)
        .into_iter()
        .map(|p| (text[p.from..p.to].to_string(), p.message))
        .collect()
}

// ---- Keywords -----------------------------------------------------------------------

#[test]
fn type_messages() {
    let s = r#"{"properties": {"name": {"type": "string"}, "n": {"type": ["array", "string"]}, "i": {"type": "integer"}}}"#;
    assert_eq!(
        ranged(s, DocKind::Json, r#"{"name": 5, "n": 1.5, "i": 2.0}"#),
        vec![
            (
                "5".into(),
                "Incompatible types. Required: string. Actual: integer.".into()
            ),
            (
                "1.5".into(),
                "Incompatible types. Required one of: array, string. Actual: number.".into()
            ),
        ]
    );
    assert_eq!(
        json_problems(s, r#"{"i": 2.5}"#),
        vec!["Incompatible types. Required: integer. Actual: number."]
    );
    // YAML: an empty value is null.
    assert_eq!(
        yaml_problems(s, "name:\n"),
        vec!["Incompatible types. Required: string. Actual: null."]
    );
}

#[test]
fn required_properties() {
    let s = r#"{"required": ["name", "description"], "properties": {"a": {"required": ["x"]}}}"#;
    assert_eq!(
        ranged(s, DocKind::Json, r#"{"a": {}}"#),
        vec![
            (
                "{".into(),
                "Missing required properties 'name', 'description'".into()
            ),
            ("\"a\"".into(), "Missing required property 'x'".into()),
        ]
    );
    assert_eq!(
        ranged(s, DocKind::Yaml, "name: x\na:\n  y: 1\n"),
        vec![
            (
                "name".into(),
                "Missing required property 'description'".into()
            ),
            ("a".into(), "Missing required property 'x'".into()),
        ]
    );
}

#[test]
fn additional_and_pattern_properties() {
    let s = r#"{"properties": {"a": {}}, "patternProperties": {"^x-": {"type": "string"}}, "additionalProperties": false}"#;
    assert_eq!(
        ranged(
            s,
            DocKind::Json,
            r#"{"$schema": "s.json", "a": 1, "x-b": 2, "c": 3}"#
        ),
        vec![
            (
                "2".into(),
                "Incompatible types. Required: string. Actual: integer.".into()
            ),
            ("\"c\"".into(), "Property 'c' is not allowed".into()),
        ]
    );
    let s = r#"{"additionalProperties": {"type": "boolean"}}"#;
    assert_eq!(
        json_problems(s, r#"{"a": true, "b": "no"}"#),
        vec!["Incompatible types. Required: boolean. Actual: string."]
    );
    let s = r#"{"properties": {"gone": false}}"#;
    assert_eq!(
        json_problems(s, r#"{"gone": 1}"#),
        vec!["Property 'gone' is not allowed"]
    );
}

#[test]
fn enum_and_const() {
    let s = r#"{"properties": {"e": {"enum": ["a", "b", 1]}, "c": {"const": "x"}}}"#;
    assert_eq!(
        json_problems(s, r#"{"e": "z", "c": "y"}"#),
        vec![
            r#"Value should be one of: "a", "b", 1"#,
            r#"Value should be one of: "x""#
        ]
    );
    assert!(json_problems(s, r#"{"e": 1, "c": "x"}"#).is_empty());
}

#[test]
fn numbers() {
    let s = r#"{"properties": {"a": {"minimum": 1, "maximum": 5}, "b": {"exclusiveMinimum": 0, "exclusiveMaximum": 10}, "c": {"multipleOf": 0.5}}}"#;
    assert_eq!(
        json_problems(s, r#"{"a": 0, "b": 10, "c": 0.75}"#),
        vec![
            "Less than a minimum 1",
            "Greater than an exclusive maximum 10",
            "Is not multiple of 0.5"
        ]
    );
    assert_eq!(
        json_problems(s, r#"{"a": 6, "b": 0}"#),
        vec![
            "Greater than a maximum 5",
            "Less than an exclusive minimum 0"
        ]
    );
    assert!(json_problems(s, r#"{"a": 5, "b": 9.9, "c": 1.5}"#).is_empty());
    // Draft 4: boolean exclusiveMinimum.
    let d4 = r#"{"$schema": "http://json-schema.org/draft-04/schema#", "minimum": 1, "exclusiveMinimum": true}"#;
    assert_eq!(
        json_problems(d4, "1"),
        vec!["Less than an exclusive minimum 1"]
    );
}

#[test]
fn strings() {
    let s = r#"{"properties": {"a": {"minLength": 2, "maxLength": 3}, "p": {"pattern": "^[a-z]+/[a-z]+$"}, "m": {"pattern": "^x", "patternErrorMessage": "Must start with x"}, "f": {"format": "email"}}}"#;
    assert_eq!(
        json_problems(
            s,
            r#"{"a": "é", "p": "Vendor/Name", "m": "y", "f": "nope"}"#
        ),
        vec![
            "String is shorter than 2",
            "String violates the pattern: '^[a-z]+/[a-z]+$'",
            "Must start with x",
            "String violates the format 'email'",
        ]
    );
    assert_eq!(
        json_problems(s, r#"{"a": "abcd"}"#),
        vec!["String is longer than 3"]
    );
    assert!(json_problems(s, r#"{"a": "ab", "p": "a/b", "f": "a@b.org"}"#).is_empty());
}

#[test]
fn arrays() {
    let s = r#"{"properties": {
        "a": {"items": {"type": "string"}, "minItems": 1, "maxItems": 2, "uniqueItems": true},
        "t": {"prefixItems": [{"type": "string"}, {"type": "integer"}], "items": false},
        "old": {"items": [{"type": "string"}], "additionalItems": {"type": "boolean"}},
        "c": {"contains": {"const": 1}}
    }}"#;
    let mut got = json_problems(s, r#"{"a": ["x", "x", 3]}"#);
    got.sort();
    assert_eq!(
        got,
        vec![
            "Array is longer than 2",
            "Incompatible types. Required: string. Actual: integer.",
            "Item is not unique"
        ]
    );
    assert_eq!(
        json_problems(s, r#"{"a": []}"#),
        vec!["Array is shorter than 1"]
    );
    assert_eq!(
        json_problems(s, r#"{"t": ["a", "b", 3]}"#),
        vec![
            "Incompatible types. Required: integer. Actual: string.",
            "Additional items are not allowed"
        ]
    );
    assert_eq!(
        json_problems(s, r#"{"old": ["a", "b"]}"#),
        vec!["Incompatible types. Required: boolean. Actual: string."]
    );
    assert_eq!(
        json_problems(s, r#"{"c": [2]}"#),
        vec!["Array does not contain an item that matches the 'contains' schema"]
    );
}

#[test]
fn combinators() {
    let s = r#"{"properties": {
        "any": {"anyOf": [{"type": "string"}, {"type": "boolean"}]},
        "one": {"oneOf": [{"type": "integer"}, {"minimum": 0}]},
        "all": {"allOf": [{"minLength": 2}, {"maxLength": 3}]},
        "not": {"not": {"type": "null"}},
        "best": {"anyOf": [{"type": "string"}, {"type": "object", "required": ["id"], "properties": {"id": {"type": "integer"}}}]}
    }}"#;
    assert_eq!(
        json_problems(s, r#"{"any": 1}"#),
        vec!["Incompatible types. Required one of: string, boolean. Actual: integer."]
    );
    assert_eq!(
        json_problems(s, r#"{"one": 5}"#),
        vec!["Validates to more than one variant"]
    );
    assert!(json_problems(s, r#"{"one": -5, "any": true}"#).is_empty());
    assert_eq!(
        json_problems(s, r#"{"all": "abcd"}"#),
        vec!["String is longer than 3"]
    );
    assert_eq!(
        json_problems(s, r#"{"not": null}"#),
        vec!["Validates against 'not' schema"]
    );
    // The closest branch reports its own problem.
    assert_eq!(
        json_problems(s, r#"{"best": {"id": "x"}}"#),
        vec!["Incompatible types. Required: integer. Actual: string."]
    );
}

#[test]
fn conditionals_and_dependencies() {
    let s = r#"{
        "if": {"properties": {"kind": {"const": "file"}}},
        "then": {"required": ["path"]},
        "else": {"required": ["url"]},
        "dependentRequired": {"user": ["password"]},
        "dependencies": {"a": {"required": ["b"]}}
    }"#;
    assert_eq!(
        json_problems(s, r#"{"kind": "file"}"#),
        vec!["Missing required property 'path'"]
    );
    assert_eq!(
        json_problems(s, r#"{"kind": "web"}"#),
        vec!["Missing required property 'url'"]
    );
    assert_eq!(
        json_problems(s, r#"{"kind": "web", "url": "u", "user": "x"}"#),
        vec!["Dependency is violated: property 'password' must be specified, since 'user' is specified"]
    );
    assert_eq!(
        json_problems(s, r#"{"kind": "web", "url": "u", "a": 1}"#),
        vec!["Missing required property 'b'"]
    );
    // Without `kind` the `if` holds (its property is absent).
    assert_eq!(
        json_problems(s, r#"{"url": "u"}"#),
        vec!["Missing required property 'path'"]
    );
}

#[test]
fn refs_defs_and_unevaluated() {
    let s = r##"{
        "$defs": {"name": {"type": "string"}, "base": {"properties": {"id": {"$ref": "#/$defs/name"}}}},
        "definitions": {"old": {"type": "boolean"}},
        "allOf": [{"$ref": "#/$defs/base"}],
        "properties": {"flag": {"$ref": "#/definitions/old"}, "self": {"$ref": "#"}},
        "unevaluatedProperties": false
    }"##;
    assert_eq!(
        json_problems(
            s,
            r#"{"id": 1, "flag": "x", "self": {"id": "a", "z": 1}, "other": 1}"#
        ),
        vec![
            "Incompatible types. Required: string. Actual: integer.",
            "Incompatible types. Required: boolean. Actual: string.",
            "Property 'z' is not allowed",
            "Property 'other' is not allowed",
        ]
    );
}

#[test]
fn property_names_counts_and_deprecation() {
    let s = r#"{"propertyNames": {"pattern": "^[a-z]+$"}, "minProperties": 1, "maxProperties": 2,
        "properties": {"old": {"deprecated": true}, "older": {"deprecationMessage": "Use 'new'."}}}"#;
    assert_eq!(
        json_problems(s, "{}"),
        vec!["Number of properties is less than 1"]
    );
    let all = validate(
        &reg(s),
        URI,
        DocKind::Json,
        r#"{"old": 1, "older": 2, "Bad": 3}"#,
    );
    let msgs: Vec<(&str, validate::Severity)> = all
        .iter()
        .map(|p| (p.message.as_str(), p.severity))
        .collect();
    assert_eq!(
        msgs,
        vec![
            (
                "Number of properties is greater than 2",
                validate::Severity::Warning
            ),
            ("Property 'old' is deprecated", validate::Severity::Weak),
            (
                "Property 'older' is deprecated: Use 'new'.",
                validate::Severity::Weak
            ),
            ("Property 'Bad' is not allowed", validate::Severity::Warning),
        ]
    );
}

#[test]
fn external_refs_resolve_once_loaded() {
    let mut r = reg(r#"{"properties": {"a": {"$ref": "other.json#/$defs/x"}}}"#);
    assert_eq!(
        r.missing(),
        vec!["https://test.forge/other.json".to_string()]
    );
    // Unloaded refs accept anything.
    assert!(validate(&r, URI, DocKind::Json, r#"{"a": 1}"#).is_empty());
    r.insert(Arc::new(
        SchemaDoc::parse(
            "https://test.forge/other.json",
            "o.json",
            r#"{"$defs": {"x": {"type": "string"}}}"#.into(),
        )
        .unwrap(),
    ));
    assert_eq!(
        validate(&r, URI, DocKind::Json, r#"{"a": 1}"#)[0].message,
        "Incompatible types. Required: string. Actual: integer."
    );
}

#[test]
fn custom_tags_and_utf16_offsets() {
    let s = r#"{"properties": {"script": {"type": "array", "items": {"type": "string"}}, "n": {"type": "string"}}}"#;
    assert!(yaml_problems(s, "script: !reference [.setup, script]\n").is_empty());
    let text = "{\"é\": 1, \"n\": 2}";
    let p = &validate(&reg(s), URI, DocKind::Json, text)[0];
    // `é` is one UTF-16 unit and two bytes.
    assert_eq!((p.from, p.to), (14, 15));
}

// ---- Completion ---------------------------------------------------------------------

const COMPLETION_SCHEMA: &str = r#"{
    "required": ["name"],
    "properties": {
        "description": {"type": "string", "description": "What it does."},
        "name": {"type": "string", "description": "The **name**."},
        "type": {"enum": ["library", "project"], "markdownEnumDescriptions": ["A library.", "A project."]},
        "private": {"type": "boolean"},
        "config": {"type": "object", "properties": {"sort": {"type": "boolean", "default": true}}},
        "keywords": {"type": "array", "items": {"type": "string", "examples": ["php"]}},
        "old": {"deprecated": true, "type": "string"}
    }
}"#;

fn complete_at(kind: DocKind, marked: &str) -> (CompletionOut, String) {
    let offset = marked.find('|').expect("cursor");
    let text = marked.replacen('|', "", 1);
    let c = complete(&reg(COMPLETION_SCHEMA), URI, kind, &text, offset).expect("completion");
    (c, text)
}

fn labels(c: &CompletionOut) -> Vec<&str> {
    c.items.iter().map(|i| i.label.as_str()).collect()
}

#[test]
fn json_property_names() {
    let (c, _) = complete_at(DocKind::Json, "{\"private\": true, |}");
    assert_eq!(
        labels(&c),
        vec!["name", "description", "type", "config", "keywords", "old"]
    );
    let name = &c.items[0];
    assert!(name.required);
    assert_eq!(name.detail.as_deref(), Some("string"));
    assert_eq!(name.documentation.as_deref(), Some("The **name**."));
    assert_eq!(name.insert, "\"name\": \"$1\"");
    assert!(
        c.items
            .iter()
            .find(|i| i.label == "old")
            .unwrap()
            .deprecated
    );
    let config = c.items.iter().find(|i| i.label == "config").unwrap();
    assert_eq!(config.insert, "\"config\": {$1}");
    let ty = c.items.iter().find(|i| i.label == "type").unwrap();
    assert!(ty.retrigger);

    // Inside a key being typed, with a following property: comma added.
    let (c, text) = complete_at(DocKind::Json, "{\n  \"na|\n  \"private\": true\n}");
    assert_eq!(&text[c.from..c.from + 2], "na");
    assert_eq!(&text[c.replace_from..c.replace_to], "\"na");
    assert_eq!(c.items[0].insert, "\"name\": \"$1\",");
    // Renaming an existing key keeps its value.
    let (c, _) = complete_at(DocKind::Json, "{\"de|sc\": \"x\"}");
    assert_eq!(
        c.items
            .iter()
            .find(|i| i.label == "description")
            .unwrap()
            .insert,
        "\"description\""
    );
    // Nested object.
    let (c, _) = complete_at(DocKind::Json, "{\"config\": {|}}");
    assert_eq!(labels(&c), vec!["sort"]);
    assert_eq!(c.items[0].insert, "\"sort\": ${1:true}");
}

#[test]
fn json_values() {
    let (c, text) = complete_at(DocKind::Json, "{\"type\": \"|\"}");
    assert_eq!(labels(&c), vec!["library", "project"]);
    assert_eq!(c.items[0].insert, "\"library\"");
    assert_eq!(c.items[1].documentation.as_deref(), Some("A project."));
    assert_eq!(&text[c.replace_from..c.replace_to], "\"\"");
    let (c, _) = complete_at(DocKind::Json, "{\"private\": |, \"name\": \"x\"}");
    assert_eq!(labels(&c), vec!["true", "false"]);
    let (c, _) = complete_at(DocKind::Json, "{\"private\": |\n\"name\": \"x\"}");
    assert_eq!(c.items[0].insert, "true,");
    let (c, _) = complete_at(DocKind::Json, "{\"keywords\": [|]}");
    assert_eq!(labels(&c), vec!["php"]);
    let (c, _) = complete_at(DocKind::Json, "{\"config\": |}");
    assert_eq!(labels(&c), vec!["{}"]);
}

#[test]
fn yaml_completion() {
    let (c, _) = complete_at(DocKind::Yaml, "private: true\n|");
    assert_eq!(labels(&c)[0], "name");
    assert_eq!(
        c.items.iter().find(|i| i.label == "config").unwrap().insert,
        "config:\n  $1"
    );
    assert_eq!(
        c.items
            .iter()
            .find(|i| i.label == "keywords")
            .unwrap()
            .insert,
        "keywords:\n  - $1"
    );
    assert_eq!(
        c.items.iter().find(|i| i.label == "name").unwrap().insert,
        "name: $1"
    );
    let (c, text) = complete_at(DocKind::Yaml, "config:\n  so|\n");
    assert_eq!(labels(&c), vec!["sort"]);
    assert_eq!(&text[c.replace_from..c.replace_to], "so");
    let (c, _) = complete_at(DocKind::Yaml, "type: |\n");
    assert_eq!(labels(&c), vec!["library", "project"]);
    let (c, _) = complete_at(DocKind::Yaml, "keywords:\n  - |\n");
    assert_eq!(labels(&c), vec!["php"]);
    let (c, _) = complete_at(DocKind::Yaml, "config:\n  sort: |\n");
    assert_eq!(labels(&c)[..2], ["true", "false"]);
}

// ---- Hover and Go to Declaration ------------------------------------------------------

#[test]
fn hover_and_goto() {
    let r = reg(COMPLETION_SCHEMA);
    let text = "{\"name\": \"x\", \"type\": \"project\"}";
    let h = hover(&r, URI, DocKind::Json, text, 3).unwrap();
    assert_eq!(
        (h.title.as_str(), h.markdown.as_str()),
        ("name: string", "The **name**.")
    );
    assert_eq!(&text[h.from..h.to], "\"name\"");
    let h = hover(&r, URI, DocKind::Json, text, 26).unwrap();
    assert_eq!(h.markdown, "A project.");
    let targets = goto(&r, URI, DocKind::Json, text, 3);
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].path, "schemas/s.json");
    assert_eq!(targets[0].line, 4);
    assert!(targets[0].preview.starts_with("\"name\""));
    let yaml = "config:\n  sort: false\n";
    let targets = goto(&r, URI, DocKind::Yaml, yaml, 11);
    assert_eq!(targets[0].line, 7);
}

// ---- Real files against the bundled schemas ---------------------------------------------

fn bundled_problems(uri: &str, kind: DocKind, text: &str) -> Vec<String> {
    let mut r = Registry::new();
    assert!(load_bundled(&mut r, uri));
    validate(&r, uri, kind, text)
        .into_iter()
        .map(|p| p.message)
        .collect()
}

#[test]
fn composer_json() {
    let ok = r#"{
        "name": "acme/shop",
        "type": "project",
        "require": {"php": "^8.2", "laravel/framework": "^11.0"},
        "autoload": {"psr-4": {"App\\": "app/"}},
        "config": {"sort-packages": true},
        "minimum-stability": "stable"
    }"#;
    assert_eq!(
        bundled_problems(bundled::COMPOSER, DocKind::Json, ok),
        Vec::<String>::new()
    );
    let bad = r#"{"name": "Acme/Shop", "minimum-stability": "nightly", "config": {"sort-packages": "yes"}, "unknown": 1}"#;
    let problems = bundled_problems(bundled::COMPOSER, DocKind::Json, bad);
    assert!(
        problems
            .iter()
            .any(|p| p.starts_with("String violates the pattern")),
        "{problems:?}"
    );
    assert!(
        problems
            .iter()
            .any(|p| p.starts_with("Value should be one of: \"dev\"")),
        "{problems:?}"
    );
    assert!(
        problems.contains(&"Incompatible types. Required: boolean. Actual: string.".to_string()),
        "{problems:?}"
    );

    let mut r = Registry::new();
    load_bundled(&mut r, bundled::COMPOSER);
    let c = complete(&r, bundled::COMPOSER, DocKind::Json, "{\n  \n}", 4).unwrap();
    let names: Vec<&str> = c.items.iter().map(|i| i.label.as_str()).collect();
    assert!(
        names.contains(&"require") && names.contains(&"autoload"),
        "{names:?}"
    );
    let h = hover(
        &r,
        bundled::COMPOSER,
        DocKind::Json,
        r#"{"require": {}}"#,
        3,
    )
    .unwrap();
    assert!(h.markdown.to_lowercase().contains("package"), "{h:?}");
    let t = goto(
        &r,
        bundled::COMPOSER,
        DocKind::Json,
        r#"{"require": {}}"#,
        3,
    );
    assert_eq!(t[0].path, "json-schema://bundled/composer.json");
}

#[test]
fn package_json() {
    let ok = r#"{"name": "shop", "version": "1.0.0", "private": true, "type": "module",
        "scripts": {"dev": "vite"}, "devDependencies": {"vite": "^5.0.0"}, "engines": {"node": ">=18"}}"#;
    assert_eq!(
        bundled_problems(bundled::PACKAGE, DocKind::Json, ok),
        Vec::<String>::new()
    );
    let problems = bundled_problems(
        bundled::PACKAGE,
        DocKind::Json,
        r#"{"name": "shop", "type": "esm", "private": "yes"}"#,
    );
    assert_eq!(
        problems,
        vec![
            r#"Value should be one of: "commonjs", "module""#,
            r#"Value should be one of: "false", "true""#
        ]
    );
    let mut r = Registry::new();
    load_bundled(&mut r, bundled::PACKAGE);
    let c = complete(&r, bundled::PACKAGE, DocKind::Json, r#"{"type": ""}"#, 10).unwrap();
    let names: Vec<&str> = c.items.iter().map(|i| i.label.as_str()).collect();
    assert_eq!(names, vec!["commonjs", "module"]);
}

#[test]
fn compose_file() {
    let ok = "services:\n  web:\n    image: nginx:1.27\n    ports:\n      - \"8080:80\"\n    depends_on: [db]\n    restart: unless-stopped\n  db:\n    image: mysql:8\n    environment:\n      MYSQL_ROOT_PASSWORD: secret\nvolumes:\n  data:\n";
    assert_eq!(
        bundled_problems(bundled::COMPOSE, DocKind::Yaml, ok),
        Vec::<String>::new()
    );
    let problems = bundled_problems(
        bundled::COMPOSE,
        DocKind::Yaml,
        "services:\n  web:\n    imag: nginx\n    restart: sometimes\n    ports: 80\n",
    );
    assert!(
        problems.contains(&"Property 'imag' is not allowed".to_string()),
        "{problems:?}"
    );
    assert!(
        problems
            .iter()
            .any(|p| p.contains("Required: array. Actual: integer")),
        "{problems:?}"
    );
    let mut r = Registry::new();
    load_bundled(&mut r, bundled::COMPOSE);
    let text = "services:\n  web:\n    im\n";
    let c = complete(&r, bundled::COMPOSE, DocKind::Yaml, text, text.len() - 1).unwrap();
    assert!(c.items.iter().any(|i| i.label == "image"));
}

#[test]
fn github_workflow() {
    let ok = "name: CI\non:\n  push:\n    branches: [main]\n  pull_request:\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v4\n      - run: cargo test\n        env:\n          RUST_LOG: debug\n";
    assert_eq!(
        bundled_problems(bundled::GITHUB_WORKFLOW, DocKind::Yaml, ok),
        Vec::<String>::new()
    );
    let problems = bundled_problems(
        bundled::GITHUB_WORKFLOW,
        DocKind::Yaml,
        "on: push\njobs:\n  test:\n    steps:\n      - run: x\n",
    );
    assert!(
        problems.iter().any(|p| p.contains("'runs-on'")),
        "{problems:?}"
    );
    let mut r = Registry::new();
    load_bundled(&mut r, bundled::GITHUB_WORKFLOW);
    let text = "on: push\njobs:\n  test:\n    runs-on: ubuntu-latest\n    \n";
    let c = complete(
        &r,
        bundled::GITHUB_WORKFLOW,
        DocKind::Yaml,
        text,
        text.len() - 1,
    )
    .unwrap();
    let names: Vec<&str> = c.items.iter().map(|i| i.label.as_str()).collect();
    assert!(
        names.contains(&"steps") && names.contains(&"needs"),
        "{names:?}"
    );
    assert!(!names.contains(&"runs-on"));
}

#[test]
fn gitlab_ci() {
    let ok = "stages: [build, test]\n.defaults: &defaults\n  image: php:8.3\nbuild:\n  <<: *defaults\n  stage: build\n  script:\n    - composer install\n  only:\n    - main\n";
    assert_eq!(
        bundled_problems(bundled::GITLAB_CI, DocKind::Yaml, ok),
        Vec::<String>::new()
    );
}

#[test]
fn selects_and_names_builtins() {
    let s = mapping::select(".github/workflows/ci.yml", "", DocKind::Yaml, &[], None).unwrap();
    assert_eq!(s.uri, bundled::GITHUB_WORKFLOW);
    for m in mapping::BUILTINS {
        assert!(bundled::find(m.uri).is_some(), "{} is bundled", m.name);
    }
    assert_eq!(DocKind::from_path("a/.eslintrc"), Some(DocKind::Json));
    assert_eq!(
        DocKind::from_language("docker-compose"),
        Some(DocKind::Yaml)
    );
}

#[test]
fn ecmascript_patterns() {
    use validate::translate_pattern as t;
    assert_eq!(t(r"^[a-z0-9-*~]+$"), r"^[a-z0-9\-*~]+$");
    assert_eq!(t(r"^[-a]\/x$"), r"^[\-a]/x$");
    assert_eq!(t(r"[^a-]"), r"[^a\-]");
    let s = r#"{"pattern": "^(?:@[a-z0-9-*~][a-z0-9-*._~]*/)?[a-z0-9-~][a-z0-9-._~]*$"}"#;
    assert!(json_problems(s, r#""@acme/shop""#).is_empty());
    assert_eq!(json_problems(s, r#""Shop""#).len(), 1);
}

#[test]
fn ref_cycles_terminate() {
    let s = r##"{"$defs": {"a": {"anyOf": [{"$ref": "#/$defs/b"}, {"$ref": "#/$defs/b"}]}, "b": {"anyOf": [{"$ref": "#/$defs/a"}, {"type": "string"}]}}, "$ref": "#/$defs/a"}"##;
    let started = std::time::Instant::now();
    let _ = json_problems(s, "1");
    assert!(json_problems(s, r#""x""#).is_empty());
    assert!(started.elapsed().as_secs() < 2);
}

#[test]
fn json_cursor_reports_keys_and_values() {
    use crate::complete::json_cursor;
    use crate::tree::Seg;
    let text = r#"{"require": {"lara": "", "php": "^8.3"}}"#;
    let at = |needle: &str, delta: usize| text.find(needle).unwrap() + delta;

    let key = json_cursor(text, at("lara", 2)).unwrap();
    assert!(key.key && key.colon);
    assert_eq!(key.path, vec![Seg::Key("require".into())]);
    assert_eq!(&text[key.from..at("lara", 2)], "la");
    assert_eq!(&text[key.replace.0..key.replace.1], "\"lara\"");

    let value = json_cursor(text, at("^8", 1)).unwrap();
    assert!(!value.key);
    assert_eq!(
        value.path,
        vec![Seg::Key("require".into()), Seg::Key("php".into())]
    );
    assert_eq!(&text[value.replace.0..value.replace.1], "\"^8.3\"");

    let empty = json_cursor(text, at("\"\"", 1)).unwrap();
    assert_eq!(
        empty.path,
        vec![Seg::Key("require".into()), Seg::Key("lara".into())]
    );
    assert!(!empty.comma);
}

#[test]
fn openapi_documents_by_content() {
    let ok30 = "openapi: 3.0.3\ninfo:\n  title: Shop\n  version: '1.0'\npaths:\n  /users/{id}:\n    get:\n      operationId: getUser\n      parameters:\n        - name: id\n          in: path\n          required: true\n          schema:\n            type: integer\n      responses:\n        '200':\n          description: The user\n";
    let s = mapping::select("api/shop.yaml", ok30, DocKind::Yaml, &[], None).unwrap();
    assert_eq!(s.uri, bundled::OPENAPI_30);
    assert_eq!(s.name, "OpenAPI 3.0");
    assert_eq!(mapping::name_of(bundled::OPENAPI_30), "OpenAPI 3.0");
    assert_eq!(
        bundled_problems(bundled::OPENAPI_30, DocKind::Yaml, ok30),
        Vec::<String>::new()
    );
    // `info` is required and responses need a description.
    let bad = "openapi: 3.0.3\npaths:\n  /a:\n    get:\n      responses:\n        '200': {}\n";
    let problems = bundled_problems(bundled::OPENAPI_30, DocKind::Yaml, bad);
    assert!(!problems.is_empty(), "{problems:?}");

    let ok31 = r#"{"openapi": "3.1.0", "info": {"title": "T", "version": "1"}, "paths": {}}"#;
    let s = mapping::select("spec.json", ok31, DocKind::Json, &[], None).unwrap();
    assert_eq!(s.uri, bundled::OPENAPI_31);
    assert_eq!(
        bundled_problems(bundled::OPENAPI_31, DocKind::Json, ok31),
        Vec::<String>::new()
    );
    assert!(!bundled_problems(
        bundled::OPENAPI_31,
        DocKind::Json,
        r#"{"openapi": "3.1.0"}"#
    )
    .is_empty());
    assert_eq!(mapping::openapi_version("swagger: '2.0'\n"), None);
    assert!(mapping::select("config.yaml", "name: x\n", DocKind::Yaml, &[], None).is_none());
}

#[test]
fn minecraft_plugin_descriptors() {
    let s = mapping::select(
        "src/main/resources/plugin.yml",
        "",
        DocKind::Yaml,
        &[],
        None,
    )
    .unwrap();
    assert_eq!(s.uri, bundled::MINECRAFT_BUKKIT);
    let s = mapping::select(
        "core/src/main/resources/paper-plugin.yml",
        "",
        DocKind::Yaml,
        &[],
        None,
    )
    .unwrap();
    assert_eq!(s.uri, bundled::MINECRAFT_PAPER);
    // A plugin.yml elsewhere is not a Minecraft descriptor.
    assert!(mapping::select("plugin.yml", "", DocKind::Yaml, &[], None).is_none());

    let ok = "name: BetterChat\nversion: '${version}'\nmain: com.example.betterchat.BetterChat\napi-version: '1.21'\nauthors: ['Alex', 'Sam']\nfolia-supported: true\n\ncommands:\n  hello:\n    description: Says hello\n    usage: /<command>\n    aliases: [hi]\npermissions:\n  betterchat.admin:\n    default: op\n    children:\n      betterchat.use: true\n";
    assert_eq!(
        bundled_problems(bundled::MINECRAFT_BUKKIT, DocKind::Yaml, ok),
        Vec::<String>::new()
    );
    let bad = bundled_problems(
        bundled::MINECRAFT_BUKKIT,
        DocKind::Yaml,
        "name: Better Chat!\nversion: 1\nload: LATER\n",
    );
    assert!(bad.iter().any(|p| p.contains("'main'")), "{bad:?}");
    assert!(
        bad.iter()
            .any(|p| p.contains("LATER") || p.contains("STARTUP")),
        "{bad:?}"
    );
    assert_eq!(bad.len(), 3, "{bad:?}");

    let paper = "name: X\nversion: '1'\nmain: a.b.X\napi-version: '1.21'\ndependencies:\n  server:\n    LuckPerms:\n      load: BEFORE\n      required: false\n";
    assert_eq!(
        bundled_problems(bundled::MINECRAFT_PAPER, DocKind::Yaml, paper),
        Vec::<String>::new()
    );
    let velocity = r#"{"id": "Lobby", "main": "a.b.Lobby", "authors": ["Alex"]}"#;
    let v = bundled_problems(bundled::MINECRAFT_VELOCITY, DocKind::Json, velocity);
    assert_eq!(v.len(), 1, "{v:?}");
    assert_eq!(
        bundled_problems(
            bundled::MINECRAFT_BUNGEE,
            DocKind::Yaml,
            "name: Lobby\nversion: '1.0'\nmain: a.b.Lobby\nauthor: Alex\n"
        ),
        Vec::<String>::new()
    );

    let mut r = Registry::new();
    load_bundled(&mut r, bundled::MINECRAFT_BUKKIT);
    let text = "name: X\nmain: a.b.X\n\n";
    let c = complete(
        &r,
        bundled::MINECRAFT_BUKKIT,
        DocKind::Yaml,
        text,
        text.len(),
    )
    .unwrap();
    let names: Vec<&str> = c.items.iter().map(|i| i.label.as_str()).collect();
    assert!(
        names.contains(&"api-version") && names.contains(&"commands"),
        "{names:?}"
    );
}

#[test]
fn minecraft_mod_metadata() {
    let s = mapping::select(
        "src/main/resources/fabric.mod.json",
        "",
        DocKind::Json,
        &[],
        None,
    )
    .unwrap();
    assert_eq!(s.uri, bundled::MINECRAFT_FABRIC_MOD);
    let s = mapping::select(
        "src/client/resources/demo.client.mixins.json",
        "",
        DocKind::Json,
        &[],
        None,
    )
    .unwrap();
    assert_eq!(s.uri, bundled::MINECRAFT_MIXINS);

    let ok = r#"{
  "schemaVersion": 1,
  "id": "better_chests",
  "version": "${version}",
  "name": "Better Chests",
  "authors": ["Alex", {"name": "Sam", "contact": {"homepage": "https://example.com"}}],
  "environment": "*",
  "entrypoints": {
    "main": ["com.example.BetterChests"],
    "client": [{"adapter": "kotlin", "value": "com.example.Client"}]
  },
  "mixins": ["better_chests.mixins.json", {"config": "x.mixins.json", "environment": "client"}],
  "depends": {"fabricloader": ">=0.20.1", "minecraft": ["~26.3", "~26.2"]}
}"#;
    assert_eq!(
        bundled_problems(bundled::MINECRAFT_FABRIC_MOD, DocKind::Json, ok),
        Vec::<String>::new()
    );
    let bad = bundled_problems(
        bundled::MINECRAFT_FABRIC_MOD,
        DocKind::Json,
        r#"{"schemaVersion": 1, "id": "Better", "version": "1", "environment": "both"}"#,
    );
    assert_eq!(bad.len(), 2, "{bad:?}");
    let mixins = r#"{"required": true, "package": "a.mixin", "compatibilityLevel": "JAVA_25", "mixins": ["A"], "injectors": {"defaultRequire": 1}}"#;
    assert_eq!(
        bundled_problems(bundled::MINECRAFT_MIXINS, DocKind::Json, mixins),
        Vec::<String>::new()
    );

    let mut r = Registry::new();
    load_bundled(&mut r, bundled::MINECRAFT_FABRIC_MOD);
    let text = "{\n  \"entrypoints\": {\n    \n  }\n}";
    let pos = text.find("    \n").unwrap() + 4;
    let c = complete(&r, bundled::MINECRAFT_FABRIC_MOD, DocKind::Json, text, pos).unwrap();
    let names: Vec<&str> = c.items.iter().map(|i| i.label.as_str()).collect();
    assert!(
        names.contains(&"main") && names.contains(&"fabric-datagen"),
        "{names:?}"
    );
}

#[test]
fn unity_assembly_definitions() {
    let s = mapping::select("Assets/Scripts/Game.asmdef", "", DocKind::Json, &[], None).unwrap();
    assert_eq!(s.uri, bundled::UNITY_ASMDEF);
    let s = mapping::select("Assets/Plugins/Shared.asmref", "", DocKind::Json, &[], None).unwrap();
    assert_eq!(s.uri, bundled::UNITY_ASMREF);
    let ok = r#"{"name": "Shooter.Game", "rootNamespace": "Shooter", "references": ["GUID:6055be8ebefd69e48b49212b09b47b2f"], "includePlatforms": [], "excludePlatforms": [], "allowUnsafeCode": false, "autoReferenced": true, "defineConstraints": [], "versionDefines": [{"name": "com.unity.inputsystem", "expression": "1.0.0", "define": "HAS_INPUT_SYSTEM"}], "noEngineReferences": false}"#;
    assert_eq!(
        bundled_problems(bundled::UNITY_ASMDEF, DocKind::Json, ok),
        Vec::<String>::new()
    );
    let bad = bundled_problems(
        bundled::UNITY_ASMDEF,
        DocKind::Json,
        r#"{"includePlatforms": ["Linux"], "autoReferenced": "yes"}"#,
    );
    assert_eq!(bad.len(), 3, "{bad:?}");
    assert!(bad.iter().any(|p| p.contains("'name'")), "{bad:?}");

    let mut r = Registry::new();
    load_bundled(&mut r, bundled::UNITY_ASMDEF);
    let text = "{\n  \"name\": \"Game\",\n  \n}";
    let at = text.find("  \n}").unwrap() + 2;
    let c = complete(&r, bundled::UNITY_ASMDEF, DocKind::Json, text, at).unwrap();
    let names: Vec<&str> = c.items.iter().map(|i| i.label.as_str()).collect();
    assert!(
        names.contains(&"rootNamespace") && names.contains(&"includePlatforms"),
        "{names:?}"
    );
}
