use std::fs;
use std::path::Path;

use tempfile::TempDir;

use crate::{Query, SearchService};

fn write(root: &Path, rel: &str, body: impl AsRef<[u8]>) {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, body).unwrap();
}

fn project() -> (TempDir, SearchService) {
    let dir = TempDir::new().unwrap();
    let r = dir.path();
    write(
        r,
        "src/User.php",
        "<?php\nclass UserRepository {\n    function findUser() {}\n}\n",
    );
    write(
        r,
        "src/Order.php",
        "<?php\nclass Order { function total() {} }\n",
    );
    write(
        r,
        "web/app.js",
        "const userRepository = new UserRepository();\r\nfoo();\r\n",
    );
    write(r, "node_modules/lib/x.js", "class UserRepository {}");
    write(r, "ignored/secret.txt", "UserRepository");
    write(r, ".gitignore", "ignored/\n");
    write(r, "bin.dat", b"UserRepository\0\0binary");
    let svc = SearchService::open(r, None);
    svc.refresh();
    (dir, svc)
}

fn q(pattern: &str) -> Query {
    Query {
        pattern: pattern.into(),
        ..Query::default()
    }
}

fn paths(svc: &SearchService, query: &Query) -> Vec<String> {
    svc.find_in_files(query)
        .unwrap()
        .files
        .into_iter()
        .map(|f| f.path)
        .collect()
}

#[test]
fn literal_search_is_case_insensitive_by_default_and_skips_excluded_files() {
    let (_d, svc) = project();
    assert_eq!(
        paths(&svc, &q("userrepository")),
        ["src/User.php", "web/app.js"]
    );
}

#[test]
fn case_sensitive_whole_words_and_positions() {
    let (_d, svc) = project();
    let mut query = q("UserRepository");
    query.case_sensitive = true;
    assert_eq!(paths(&svc, &query), ["src/User.php", "web/app.js"]);

    let mut query = q("User");
    query.whole_words = true;
    query.case_sensitive = true;
    assert!(paths(&svc, &query).is_empty());

    let out = svc.find_in_files(&q("findUser")).unwrap();
    let m = &out.files[0].matches[0];
    assert_eq!((m.line, m.column, m.start, m.end), (3, 13, 13, 21));
    assert_eq!(m.text, "    function findUser() {}");
}

#[test]
fn crlf_lines_are_trimmed() {
    let (_d, svc) = project();
    let out = svc.find_in_files(&q("foo")).unwrap();
    assert_eq!(out.files[0].matches[0].text, "foo();");
}

#[test]
fn regex_search_with_and_without_a_usable_prefilter() {
    let (_d, svc) = project();
    let mut query = q(r"class \w+Repository");
    query.regex = true;
    assert_eq!(paths(&svc, &query), ["src/User.php"]);

    let mut query = q(r"\w+Repository\(\)");
    query.regex = true;
    assert_eq!(paths(&svc, &query), ["web/app.js"]);

    let mut query = q(r"^\s*function");
    query.regex = true;
    assert_eq!(paths(&svc, &query), ["src/User.php"]);

    let mut query = q("(");
    query.regex = true;
    assert!(svc.find_in_files(&query).is_err());
}

#[test]
fn file_mask_and_directory_scope() {
    let (_d, svc) = project();
    let mut query = q("class");
    query.file_mask = "*.php, !Order.php".into();
    assert_eq!(paths(&svc, &query), ["src/User.php"]);

    let mut query = q("UserRepository");
    query.dir = Some("web".into());
    assert_eq!(paths(&svc, &query), ["web/app.js"]);
}

#[test]
fn match_limit_truncates() {
    let dir = TempDir::new().unwrap();
    write(dir.path(), "a.txt", "x1\nx2\nx3\nx4\n");
    let svc = SearchService::open(dir.path(), None);
    svc.refresh();
    let mut query = q("x");
    query.max_matches = 2;
    let out = svc.find_in_files(&query).unwrap();
    assert_eq!(out.total_matches, 2);
    assert!(out.truncated);
}

#[test]
fn refresh_picks_up_edits_additions_and_removals() {
    let (d, svc) = project();
    write(
        d.path(),
        "src/Order.php",
        "<?php\nclass Order { /* UserRepository */ }\n",
    );
    write(d.path(), "src/New.php", "<?php // brandnewtoken\n");
    fs::remove_file(d.path().join("web/app.js")).unwrap();
    let stats = svc.refresh();
    assert_eq!((stats.added, stats.changed, stats.removed), (1, 1, 1));

    assert_eq!(
        paths(&svc, &q("userrepository")),
        ["src/Order.php", "src/User.php"]
    );
    assert_eq!(paths(&svc, &q("brandnewtoken")), ["src/New.php"]);
    assert_eq!(svc.refresh(), Default::default());
}

#[test]
fn index_survives_a_restart() {
    let (d, svc) = project();
    let cache_dir = TempDir::new().unwrap();
    let cache = cache_dir.path().join("cache.bin");
    let first = SearchService::open(d.path(), Some(cache.clone()));
    first.refresh();
    assert!(cache.exists());
    drop(svc);

    let second = SearchService::open(d.path(), Some(cache));
    assert_eq!(second.refresh(), Default::default());
    assert_eq!(
        paths(&second, &q("userrepository")),
        ["src/User.php", "web/app.js"]
    );
}

#[test]
fn fuzzy_file_names() {
    let (_d, svc) = project();
    let hits = svc.find_files("usrphp", 5);
    assert_eq!(hits[0].path, "src/User.php");
    assert!(!hits[0].matched.is_empty());
    assert!(svc.find_files("", 5).is_empty());
    assert!(svc
        .find_files("bin.dat", 5)
        .iter()
        .any(|h| h.path == "bin.dat"));
}

#[test]
fn update_paths_patches_only_what_changed() {
    let (d, svc) = project();
    let r = d.path();
    write(r, "src/Fresh.php", "<?php // patchedtoken\n");
    write(r, "ignored/other.txt", "patchedtoken");
    fs::remove_dir_all(r.join("web")).unwrap();
    svc.update_paths(&[
        r.join("src/Fresh.php"),
        r.join("ignored/other.txt"),
        r.join("web"),
    ]);

    assert_eq!(paths(&svc, &q("patchedtoken")), ["src/Fresh.php"]);
    assert!(paths(&svc, &q("userrepository"))
        .iter()
        .all(|p| p != "web/app.js"));
    assert_eq!(svc.refresh(), Default::default());
}

#[test]
fn watcher_applies_changes_without_a_refresh() {
    let dir = TempDir::new().unwrap();
    write(dir.path(), "a.txt", "alpha\n");
    let svc = std::sync::Arc::new(SearchService::open(dir.path(), None));
    svc.watch().unwrap();
    svc.refresh();
    write(dir.path(), "b.txt", "watchedtoken\n");
    for _ in 0..50 {
        if !paths(&svc, &q("watchedtoken")).is_empty() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    panic!("watcher never indexed b.txt");
}

#[test]
fn postings_survive_freezing_into_segments_and_a_restart() {
    let dir = TempDir::new().unwrap();
    for i in 0..16_500 {
        write(
            dir.path(),
            &format!("d{}/f{i}.txt", i % 50),
            format!("shared line\nuniquetoken{i}\n"),
        );
    }
    let cache_dir = TempDir::new().unwrap();
    let cache = cache_dir.path().join("cache.bin");
    let svc = SearchService::open(dir.path(), Some(cache.clone()));
    svc.refresh();
    for svc in [svc, SearchService::open(dir.path(), Some(cache))] {
        let mut word = q("uniquetoken7");
        word.whole_words = true;
        assert_eq!(paths(&svc, &word), ["d7/f7.txt"]);
        assert_eq!(paths(&svc, &q("uniquetoken16499")), ["d49/f16499.txt"]);
        assert_eq!(
            svc.find_in_files(&q("shared line")).unwrap().total_matches,
            16_500
        );
        let mut capped = q("shared line");
        capped.max_matches = 1000;
        assert_eq!(svc.find_in_files(&capped).unwrap().total_matches, 1000);
    }
}

#[test]
fn symbols_are_found_with_kind_container_and_line() {
    let dir = TempDir::new().unwrap();
    write(
        dir.path(),
        "app/Order.php",
        "<?php\nnamespace App;\nfinal class OrderController\n{\n    const LIMIT = 3;\n    private string $currency;\n    public function show(): void {}\n}\nfunction helperFn() {}\n",
    );
    write(dir.path(), "src/lib.rs", "pub struct OrderBook {}\nimpl OrderBook {\n    pub fn total(&self) {}\n}\npub const MAX_ORDERS: u32 = 3;\n");
    write(
        dir.path(),
        "tool.py",
        "class OrderTool:\n    def run(self):\n        pass\n\ndef main():\n    pass\n",
    );
    write(
        dir.path(),
        "web.ts",
        "export interface OrderDto {}\nexport const makeOrder = (id: number) => id;\n",
    );
    let svc = SearchService::open(dir.path(), None);
    svc.refresh();

    let classes = svc.find_symbols("order", true, 20, &[]);
    let mut names: Vec<&str> = classes.iter().map(|h| h.name.as_str()).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        ["OrderBook", "OrderController", "OrderDto", "OrderTool"]
    );
    assert!(classes
        .iter()
        .all(|h| ["class", "interface"].contains(&h.kind.as_str())));

    let show = &svc.find_symbols("show", false, 5, &[])[0];
    assert_eq!(
        (show.name.as_str(), show.kind.as_str(), show.line),
        ("show", "method", 7)
    );
    assert_eq!(show.container.as_deref(), Some("OrderController"));
    assert_eq!(show.path, "app/Order.php");

    let kinds = |q: &str| {
        svc.find_symbols(q, false, 5, &[])
            .iter()
            .map(|h| h.kind.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(kinds("LIMIT")[0], "constant");
    assert_eq!(kinds("$currency")[0], "property");
    assert_eq!(kinds("helperFn")[0], "function");
    assert_eq!(kinds("MAX_ORDERS")[0], "constant");
    assert_eq!(kinds("makeOrder")[0], "function");
    assert_eq!(
        svc.find_symbols("run", false, 5, &[])[0]
            .container
            .as_deref(),
        Some("OrderTool")
    );
    assert!(svc.find_symbols("zzzzqq", false, 5, &[]).is_empty());

    let skip = ["php".to_string(), "PY".to_string()];
    let names = |q: &str| {
        svc.find_symbols(q, true, 20, &skip)
            .into_iter()
            .map(|h| h.name)
            .collect::<Vec<_>>()
    };
    let mut left = names("order");
    left.sort_unstable();
    assert_eq!(left, ["OrderBook", "OrderDto"]);
}

#[test]
fn find_text_matches_a_directory_walk() {
    use forge_core::search::FindRequest;
    use forge_core::Workspace;

    let (d, svc) = project();
    write(d.path(), "src/Many.php", "x\nx\nx\nx\nx\n");
    write(d.path(), "src/Wide.php", "<?php // $user and $userId\n");
    svc.refresh();
    let ws = Workspace::open(d.path()).unwrap();
    let req = |query: &str| FindRequest {
        query: query.into(),
        ..FindRequest::default()
    };
    let mut cases = vec![
        req("userrepository"),
        req("function"),
        req("zzzz"),
        req("("),
        req("x"),
    ];
    let mut sensitive = req("UserRepository");
    sensitive.case_sensitive = true;
    cases.push(sensitive);
    let mut words = req("$user");
    words.whole_words = true;
    cases.push(words);
    let mut regex = req(r"class \w+Repository");
    regex.regex = true;
    cases.push(regex);
    let mut mask = req("class");
    mask.file_mask = "*.php".into();
    cases.push(mask);
    let mut dir = req("UserRepository");
    dir.directory = "web".into();
    cases.push(dir);
    for limit in [1, 2, 3, 5, 6] {
        let mut capped = req("x");
        capped.limit = limit;
        cases.push(capped);
    }
    for case in cases {
        let walked = ws.find_in_files(&case).unwrap();
        let indexed = svc.find_text(&case).unwrap();
        assert_eq!(indexed, walked, "{case:?}");
    }
}

#[test]
fn ranked_files_match_the_walk_ranking() {
    use forge_core::Workspace;
    let (d, svc) = project();
    let ws = Workspace::open(d.path()).unwrap();
    for q in ["user", "php", "app", "Ord"] {
        assert_eq!(svc.find_ranked(q, 3), ws.find_files(q, 3).unwrap(), "{q}");
    }
}
