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
