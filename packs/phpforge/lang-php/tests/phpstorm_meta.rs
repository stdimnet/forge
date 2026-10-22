//! `.phpstorm.meta.php` and JetBrains attributes on the fixture project in
//! `tests/fixtures/meta-project` (with a vendor package and an include path).

use std::path::{Path, PathBuf};

use phpforge_lang_php::navigate::{definition, PhpDefinition};
use phpforge_lang_php::{IndexConfig, ProjectIndex};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

pub fn project() -> (tempfile::TempDir, ProjectIndex) {
    let cache = tempfile::tempdir().unwrap();
    let mut idx = ProjectIndex::new(
        fixtures().join("meta-project"),
        IndexConfig {
            cache_file: Some(cache.path().join("index.json")),
            include_paths: vec![fixtures().join("include-lib")],
            ..Default::default()
        },
    );
    idx.index_workspace(|_, _| {});
    (cache, idx)
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(fixtures().join("meta-project").join(rel)).unwrap()
}

/// Byte offset of the `nth` occurrence of `needle` plus `delta`.
fn at(src: &str, needle: &str, nth: usize, delta: usize) -> usize {
    src.match_indices(needle).nth(nth).expect(needle).0 + delta
}

fn def(idx: &mut ProjectIndex, needle: &str, delta: usize) -> PhpDefinition {
    let src = read("app/Http/Checkout.php");
    let offset = at(&src, needle, 0, delta);
    definition(idx, "app/Http/Checkout.php", &src, offset)
}

fn target(d: &PhpDefinition) -> String {
    let t = d
        .targets
        .first()
        .unwrap_or_else(|| panic!("no target: {d:?}"));
    format!("{}#{}", t.path, t.name)
}

#[test]
fn reads_project_and_vendor_metadata() {
    let (_c, idx) = project();
    let paths = idx.meta_paths();
    assert_eq!(
        paths,
        vec![
            ".phpstorm.meta.php",
            "vendor/acme/toolkit/.phpstorm.meta.php"
        ]
    );
    assert!(idx
        .meta()
        .is_exit_point(|t| t.is_function("Acme\\Toolkit\\halt")));
    assert!(idx
        .meta()
        .is_exit_point(|t| t.is_method_of(&["App\\Http\\Responder"], "fail")));
}

#[test]
fn overrides_drive_type_inference_and_go_to_declaration() {
    let (_c, mut idx) = project();
    // `$container->get(Mailer::class)->send()`: `'' => '@'` on the interface's method.
    let d = def(&mut idx, "send('a@b.c')", 0);
    assert_eq!(target(&d), "app/Services/Mailer.php#Mailer::send()");
    // A mapped service id.
    let d = def(&mut idx, "remember('totals')", 0);
    assert_eq!(target(&d), "app/Services/Cache.php#Cache::remember()");
    // A function override with a class argument.
    let d = def(&mut idx, "send('x@y.z')", 0);
    assert_eq!(target(&d), "app/Services/Mailer.php#Mailer::send()");
    // A variable assigned from an overridden call.
    let d = def(&mut idx, "send('c@d.e')", 0);
    assert_eq!(target(&d), "app/Services/Mailer.php#Mailer::send()");
    // `type(0)`: the argument's type.
    let d = def(&mut idx, "send('f@g.h')", 0);
    assert_eq!(target(&d), "app/Services/Mailer.php#Mailer::send()");
}

#[test]
fn hover_follows_overridden_calls() {
    let (_c, mut idx) = project();
    let src = read("app/Http/Checkout.php");
    let h = phpforge_lang_php::hover::hover(
        &mut idx,
        "app/Http/Checkout.php",
        &src,
        at(&src, "remember('totals')", 0, 2),
    )
    .expect("hover");
    assert!(
        h.markdown.contains("function remember(string $key): mixed"),
        "{}",
        h.markdown
    );
}

#[test]
fn include_paths_are_indexed_as_libraries() {
    let (_c, idx) = project();
    let helper = idx
        .find_class("Legacy\\Helper")
        .expect("include path class");
    assert_eq!(helper.name, "Helper");
    let path = idx.class_path("Legacy\\Helper").unwrap();
    assert!(Path::new(path).is_absolute(), "{path}");
    assert!(ProjectIndex::is_library(path));
    let hits = idx.search("Helper", phpforge_lang_php::index::SearchKind::Class, 10);
    assert!(hits.iter().any(|h| h.fqn == "Legacy\\Helper" && h.vendor));
}

#[test]
fn edited_metadata_is_read_again() {
    let (_c, mut idx) = project();
    let meta = "<?php\nnamespace PHPSTORM_META {\n    exitPoint(\\stop_here());\n}\n";
    idx.update_file(".phpstorm.meta.php", meta);
    assert!(idx.meta().is_exit_point(|t| t.is_function("stop_here")));
    assert!(idx
        .meta()
        .overrides
        .iter()
        .all(|o| o.target.class.as_deref() != Some("Psr\\Container\\ContainerInterface")));
    idx.remove_file(".phpstorm.meta.php");
    assert!(!idx.meta().is_exit_point(|t| t.is_function("stop_here")));
    // The vendor package's metadata stays.
    assert!(idx
        .meta()
        .is_exit_point(|t| t.is_function("Acme\\Toolkit\\halt")));
}

fn complete_at(idx: &mut ProjectIndex, src_with_caret: &str) -> Vec<String> {
    let offset = src_with_caret.find('|').expect("caret");
    let src = src_with_caret.replacen('|', "", 1);
    phpforge_lang_php::complete::complete(idx, "app/Http/Draft.php", &src, offset)
        .map(|c| c.items.into_iter().map(|i| i.insert).collect())
        .unwrap_or_default()
}

const DRAFT_HEAD: &str = "<?php\nnamespace App\\Http;\nuse App\\Container;\nuse App\\Models\\Order;\nuse App\\Services\\Mailer;\nclass Draft {\n  public function run(Container $c, Order $order) {\n";

#[test]
fn completes_expected_arguments_and_map_keys() {
    let (_c, mut idx) = project();
    let src = format!("{DRAFT_HEAD}    $order->setStatus('|');\n  }}\n}}\n");
    assert_eq!(
        complete_at(&mut idx, &src),
        vec!["draft", "paid", "shipped"]
    );
    // Outside a string: literals with quotes and constants.
    let src = format!("{DRAFT_HEAD}    $order->sortBy('total', |);\n  }}\n}}\n");
    assert_eq!(
        complete_at(&mut idx, &src),
        vec!["\\App\\Models\\Order::ASC", "\\App\\Models\\Order::DESC"]
    );
    // `#[ExpectedValues]` on the parameter.
    let src = format!("{DRAFT_HEAD}    $order->priority('|');\n  }}\n}}\n");
    assert_eq!(complete_at(&mut idx, &src), vec!["low", "normal", "high"]);
    // Flags after `|`.
    let src = format!("{DRAFT_HEAD}    (new Mailer())->send('a', Mailer::HTML | |);\n  }}\n}}\n");
    assert_eq!(
        complete_at(&mut idx, &src),
        vec![
            "\\App\\Services\\Mailer::HTML",
            "\\App\\Services\\Mailer::QUEUED"
        ]
    );
    // Service ids of an override map.
    let src = format!("{DRAFT_HEAD}    $c->get('|');\n  }}\n}}\n");
    assert_eq!(complete_at(&mut idx, &src), vec!["cache", "mailer"]);
    let src = format!("{DRAFT_HEAD}    app('ca|');\n  }}\n}}\n");
    let c = phpforge_lang_php::complete::complete(
        &mut idx,
        "app/Http/Draft.php",
        &src.replacen('|', "", 1),
        src.find('|').unwrap(),
    )
    .unwrap();
    assert_eq!(&src[c.from..c.to + 1], "ca|");
    // Nothing for other arguments.
    let src = format!("{DRAFT_HEAD}    $order->sortBy(|);\n  }}\n}}\n");
    assert!(complete_at(&mut idx, &src).is_empty());
}

#[test]
fn completes_return_values_and_array_shape_keys() {
    let (_c, mut idx) = project();
    let src = format!("{DRAFT_HEAD}    if ($order->status() === '|') {{}}\n  }}\n}}\n");
    assert_eq!(
        complete_at(&mut idx, &src),
        vec!["draft", "paid", "shipped"]
    );
    let src = format!("{DRAFT_HEAD}    $data = $order->toArray();\n    $data['|'];\n  }}\n}}\n");
    assert_eq!(complete_at(&mut idx, &src), vec!["id", "mailer", "tags"]);
    let src = format!("{DRAFT_HEAD}    $order->toArray()[|];\n  }}\n}}\n");
    assert_eq!(
        complete_at(&mut idx, &src),
        vec!["'id'", "'mailer'", "'tags'"]
    );
}

#[test]
fn array_shape_keys_have_types() {
    let (_c, mut idx) = project();
    let d = def(&mut idx, "send('i@j.k')", 0);
    assert_eq!(target(&d), "app/Services/Mailer.php#Mailer::send()");
}

fn problems(idx: &mut ProjectIndex, rel: &str) -> Vec<(String, String, String)> {
    let src = read(rel);
    phpforge_lang_php::inspections::inspect(idx, rel, &src)
        .into_iter()
        .map(|p| (p.code, src[p.start..p.end].to_string(), p.message))
        .collect()
}

#[test]
fn inspections_use_attributes_and_metadata() {
    let (_c, mut idx) = project();
    let found = problems(&mut idx, "app/Http/Checkout.php");
    let has = |code: &str, text: &str| found.iter().any(|(c, t, _)| c == code && t == text);
    assert!(has("deprecated", "settle"), "{found:#?}");
    let msg = &found.iter().find(|p| p.0 == "deprecated").unwrap().2;
    assert_eq!(msg, "Method 'settle' is deprecated: Use pay() instead");
    assert!(has("pureFunction", "$order->total()"), "{found:#?}");
    assert!(has("expectedValues", "'cancelled'"), "{found:#?}");
    assert!(has("expectedValues", "'urgent'"), "{found:#?}");
    assert!(has("immutable", "number"), "{found:#?}");
    assert!(has("unreachable", "$order->pay(1);"), "{found:#?}");
    let urgent = found.iter().find(|p| p.1 == "'urgent'").unwrap();
    assert_eq!(urgent.2, "Value should be one of: 'low', 'normal', 'high'");
    // Valid values, the constructor's write and other calls are fine.
    assert!(!found.iter().any(|p| p.1 == "'a@b.c'"));
    assert_eq!(found.iter().filter(|p| p.0 == "immutable").count(), 1);
    // The constructor writes `number` in its own class.
    assert!(problems(&mut idx, "app/Models/Order.php")
        .iter()
        .all(|p| p.0 != "immutable"));
}

#[test]
fn deprecated_calls_offer_their_replacement() {
    let (_c, mut idx) = project();
    let src = read("app/Http/Checkout.php");
    let offset = at(&src, "settle(10)", 0, 2);
    let fixes =
        phpforge_lang_php::inspections::fixes_at(&mut idx, "app/Http/Checkout.php", &src, offset);
    assert_eq!(fixes.len(), 1);
    assert_eq!(fixes[0].title, "Replace with '$order->pay(10)'");
    let e = &fixes[0].edits[0];
    assert_eq!(&src[e.start..e.end], "$order->settle(10)");
}

#[test]
fn namespace_must_match_source_roots() {
    let (_c, mut idx) = project();
    let composer = read("composer.json");
    let roots = forge_file_templates::psr4::roots(&composer, "");
    idx.set_source_roots(roots.clone());
    let found = problems(&mut idx, "app/Wrong/Thing.php");
    assert_eq!(
        found,
        vec![(
            "psr4Namespace".to_string(),
            "App\\Other".to_string(),
            "Namespace name doesn't match PSR-0/PSR-4 project structure".to_string()
        )]
    );
    let src = read("app/Wrong/Thing.php");
    let fixes = phpforge_lang_php::inspections::fixes_at(
        &mut idx,
        "app/Wrong/Thing.php",
        &src,
        at(&src, "Other", 0, 1),
    );
    assert_eq!(fixes[0].title, "Set namespace to 'App\\Wrong'");
    // A user's package prefix for the folder wins.
    let marked = forge_file_templates::psr4::apply_overrides(
        &roots,
        &[forge_file_templates::psr4::SourceRootSetting {
            path: "app/Wrong".into(),
            prefix: "App\\Other".into(),
            kind: "source".into(),
        }],
    );
    idx.set_source_roots(marked);
    assert!(problems(&mut idx, "app/Wrong/Thing.php").is_empty());
    // Correct files and files outside the roots are fine.
    assert!(problems(&mut idx, "app/Services/Mailer.php").is_empty());
}
