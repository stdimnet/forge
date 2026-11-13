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
fn excluded_folders_and_patterns_leave_the_index_on_refresh() {
    use forge_core::excludes::{FolderKind, FolderMark, StructureConfig};
    let dir = TempDir::new().unwrap();
    let r = dir.path();
    write(r, "src/User.php", "<?php class UserRepository {}");
    write(r, "web/app.js", "new UserRepository();");
    write(r, "web/app.min.js", "new UserRepository();");
    let excludes = forge_core::Excludes::default();
    let svc = SearchService::open(r, None).with_excludes(excludes.clone());
    svc.refresh();
    assert_eq!(svc.file_count(), 3);
    excludes.set(StructureConfig {
        folders: vec![FolderMark {
            path: "src".into(),
            kind: FolderKind::Excluded,
        }],
        exclude_patterns: "*.min.js".into(),
    });
    svc.refresh();
    assert_eq!(paths(&svc, &q("userrepository")), ["web/app.js"]);
    // The watcher's changes inside an excluded folder are dropped too.
    write(r, "src/New.php", "<?php class UserRepository {}");
    svc.update_paths(&[r.join("src/New.php")]);
    assert_eq!(svc.file_count(), 1);
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
    let hits = svc.find_files("usrphp", 5, false);
    assert_eq!(hits[0].path, "src/User.php");
    assert!(!hits[0].matched.is_empty());
    assert!(svc.find_files("", 5, false).is_empty());
    assert!(svc
        .find_files("bin.dat", 5, false)
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

    let classes = svc.find_symbols("order", true, 20, &[], false);
    let mut names: Vec<&str> = classes.iter().map(|h| h.name.as_str()).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        ["OrderBook", "OrderController", "OrderDto", "OrderTool"]
    );
    assert!(classes
        .iter()
        .all(|h| ["class", "interface"].contains(&h.kind.as_str())));

    let show = &svc.find_symbols("show", false, 5, &[], false)[0];
    assert_eq!(
        (show.name.as_str(), show.kind.as_str(), show.line),
        ("show", "method", 7)
    );
    assert_eq!(show.container.as_deref(), Some("OrderController"));
    assert_eq!(show.path, "app/Order.php");

    let kinds = |q: &str| {
        svc.find_symbols(q, false, 5, &[], false)
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
        svc.find_symbols("run", false, 5, &[], false)[0]
            .container
            .as_deref(),
        Some("OrderTool")
    );
    assert!(svc.find_symbols("zzzzqq", false, 5, &[], false).is_empty());

    let skip = ["php".to_string(), "PY".to_string()];
    let names = |q: &str| {
        svc.find_symbols(q, true, 20, &skip, false)
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
    write(d.path(), "src/Crlf.txt", "alpha end\r\nsecond\r\n");
    write(d.path(), "src/Wide.php", "<?php // $user and $userId\n");
    write(d.path(), "src/Mixed.txt", "one\rtwo\n\nthree  \n");
    write(
        d.path(),
        "src/Uni.txt",
        "Ünïcode straße ÄÖÜ\nplain line\n日本語 テキスト needle\n",
    );
    write(
        d.path(),
        "src/Long.txt",
        format!(
            "{}needle{}\nshort needle\n",
            "a".repeat(20_000),
            "b".repeat(20_000)
        ),
    );
    write(d.path(), "src/Bin.dat", b"needle\0binary");
    write(d.path(), "src/Latin1.txt", b"caf\xe9 needle\nok\n");
    write(d.path(), "src/Empty.txt", "");
    write(d.path(), "src/NoEol.txt", "needle at the end");
    write(d.path(), "src/Tabs.txt", "\tneedle\t\tneedle\n");
    let mut big = vec![b'q'; 4 * 1024 * 1024 + 1];
    big.extend_from_slice(b" needle\n");
    write(d.path(), "src/Big.txt", big);
    let mut huge = vec![b'x'; forge_find::MAX_FILE_SIZE as usize];
    huge.extend_from_slice(b" needle\n");
    write(d.path(), "src/Huge.txt", huge);
    write(
        d.path(),
        "src/Utf16Le.txt",
        utf16("caf\u{e9} needle\r\nstra\u{df}e\n", true),
    );
    write(
        d.path(),
        "src/Utf16Be.txt",
        utf16("first\nneedle \u{65e5}\u{672c} end\n", false),
    );
    write(d.path(), "src/Mixed.txt", "one\rtwo\n\nthree  \n");
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
    for query in [
        "needle",
        "straße",
        "ÜNÏ",
        "日本",
        "caf",
        "end",
        "needle needle",
    ] {
        cases.push(req(query));
    }
    for pattern in [
        r"\bneedle\b",
        "ß",
        "(?i)ä",
        r"\p{Han}+",
        r"[^\x00-\x7f]+",
        "b+n",
        "^$",
        "e.*e",
    ] {
        let mut regex = req(pattern);
        regex.regex = true;
        cases.push(regex);
    }
    let mut sensitive_unicode = req("ä");
    sensitive_unicode.case_sensitive = true;
    cases.push(sensitive_unicode);
    let mut regex = req(r"class \w+Repository");
    regex.regex = true;
    cases.push(regex);
    for pattern in [
        r"end\s",
        r"^\s*function",
        r"\w+Repository",
        r"[^a-z]x",
        "x|e",
        ".*",
    ] {
        let mut regex = req(pattern);
        regex.regex = true;
        cases.push(regex);
    }
    let mut eol = req(r"end$");
    eol.regex = true;
    cases.push(eol);
    let mut mask = req("class");
    mask.file_mask = "*.php".into();
    cases.push(mask);
    let mut dir = req("UserRepository");
    dir.directory = "web".into();
    cases.push(dir);
    let mut with_context = req("function");
    with_context.context = 2;
    cases.push(with_context);
    let mut only = req("user");
    only.paths = Some(vec![
        "src/User.php".into(),
        "web/app.js".into(),
        "missing.php".into(),
    ]);
    cases.push(only);
    let mut nothing = req("user");
    nothing.paths = Some(Vec::new());
    cases.push(nothing);
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

fn utf16(text: &str, little_endian: bool) -> Vec<u8> {
    let mut out = if little_endian {
        vec![0xFF, 0xFE]
    } else {
        vec![0xFE, 0xFF]
    };
    for unit in text.encode_utf16() {
        out.extend(if little_endian {
            unit.to_le_bytes()
        } else {
            unit.to_be_bytes()
        });
    }
    out
}

#[test]
fn large_and_utf16_files_are_searched_by_content() {
    assert_eq!(
        forge_find::MAX_FILE_SIZE,
        forge_search_core::MAX_CONTENT_BYTES
    );
    let dir = TempDir::new().unwrap();
    let mut big = vec![b'x'; 4 * 1024 * 1024 + 1];
    big.extend_from_slice(b"\nbig needle\n");
    write(dir.path(), "Big.sql", big);
    let mut huge = vec![b'x'; forge_find::MAX_FILE_SIZE as usize];
    huge.extend_from_slice(b"\nhuge needle\n");
    write(dir.path(), "Huge.sql", huge);
    write(
        dir.path(),
        "Le.txt",
        utf16("one\r\nthe needle \u{e9}\r\n", true),
    );
    write(dir.path(), "Be.txt", utf16("needle\n", false));
    // A byte order mark does not make binary content text.
    write(
        dir.path(),
        "Bin.dat",
        [&[0xFF, 0xFE, 0, 0][..], b"needle"].concat(),
    );
    let cache_dir = TempDir::new().unwrap();
    let cache = cache_dir.path().join("cache.bin");
    let svc = SearchService::open(dir.path(), Some(cache.clone()));
    svc.refresh();

    let (stats, _) = svc.stats();
    assert_eq!((stats.content, stats.binary, stats.oversized), (3, 1, 1));
    let req = forge_find::FindRequest {
        query: "needle".into(),
        ..forge_find::FindRequest::default()
    };
    let rows = |svc: &SearchService| -> Vec<(String, usize, usize, String)> {
        let found = svc
            .find_text_lazy(&req, &std::sync::atomic::AtomicBool::new(false))
            .unwrap();
        found
            .page(0, usize::MAX)
            .into_iter()
            .flat_map(|f| {
                f.matches
                    .into_iter()
                    .map(move |m| (f.path.clone(), m.line, m.column, m.text))
            })
            .collect()
    };
    let expected = [
        ("Be.txt".to_string(), 0, 0, "needle".to_string()),
        ("Big.sql".to_string(), 1, 4, "big needle".to_string()),
        ("Le.txt".to_string(), 1, 4, "the needle \u{e9}".to_string()),
    ];
    assert_eq!(rows(&svc), expected);
    assert_eq!(paths(&svc, &q("needle")), ["Be.txt", "Big.sql", "Le.txt"]);

    // The same from the cache of a later run.
    drop(svc);
    let again = SearchService::open(dir.path(), Some(cache));
    assert_eq!(rows(&again), expected);
}

#[test]
fn a_cached_index_is_brought_up_to_date_by_the_first_refresh() {
    let dir = TempDir::new().unwrap();
    write(dir.path(), "old.txt", "needle\n");
    write(dir.path(), "gone.txt", "needle\n");
    let cache_dir = TempDir::new().unwrap();
    let cache = cache_dir.path().join("cache.bin");
    SearchService::open(dir.path(), Some(cache.clone())).refresh();

    // Changes made while Forge was closed.
    write(dir.path(), "new.txt", "needle\n");
    fs::remove_file(dir.path().join("gone.txt")).unwrap();

    let svc = SearchService::open(dir.path(), Some(cache));
    // The cached listing answers at once and says that it is being checked against the disk.
    assert!(svc.ready());
    assert!(svc.indexing());
    assert_eq!(paths(&svc, &q("needle")), ["old.txt"]);
    assert!(svc.find_in_files(&q("needle")).unwrap().indexing);

    let stats = svc.refresh();
    assert_eq!((stats.added, stats.removed), (1, 1));
    assert!(!svc.indexing());
    assert_eq!(paths(&svc, &q("needle")), ["new.txt", "old.txt"]);
}

#[test]
fn ranked_files_match_the_walk_ranking() {
    use forge_core::Workspace;
    let (d, svc) = project();
    let ws = Workspace::open(d.path()).unwrap();
    for q in ["user", "php", "app", "Ord"] {
        assert_eq!(
            svc.find_ranked(q, 3, false),
            ws.find_files(q, 3).unwrap(),
            "{q}"
        );
    }
}

#[test]
fn indexing_threads_run_below_normal_priority_when_asked() {
    use crate::IndexOptions;
    let dir = TempDir::new().unwrap();
    let priority = |low_priority| {
        SearchService::open_with(dir.path(), None, IndexOptions { low_priority })
            .pool()
            .install(current_priority)
    };
    assert_ne!(priority(true), priority(false));
}

/// Linux lowers a normal thread's priority through its nice value, which
/// `get_current_thread_priority` does not report (it reads the scheduling priority, always 0).
#[cfg(target_os = "linux")]
fn current_priority() -> String {
    let stat = std::fs::read_to_string("/proc/thread-self/stat").unwrap();
    // The fields after the command name (which can hold spaces); nice is the 19th field.
    let rest = &stat[stat.rfind(')').unwrap() + 2..];
    rest.split(' ').nth(16).unwrap().to_string()
}

#[cfg(not(target_os = "linux"))]
fn current_priority() -> String {
    format!(
        "{:?}",
        thread_priority::get_current_thread_priority().unwrap()
    )
}

#[test]
fn exact_matching_needs_a_substring() {
    let (_d, svc) = project();
    assert_eq!(svc.find_files("usrphp", 5, false)[0].path, "src/User.php");
    assert!(svc.find_files("usrphp", 5, true).is_empty());
    assert_eq!(svc.find_files("user.ph", 5, true)[0].path, "src/User.php");
    assert!(!svc.find_ranked("user", 5, true).is_empty());
    assert!(svc.find_ranked("usrp", 5, true).is_empty());
}

#[test]
fn a_cancelled_search_reports_it() {
    let (_d, svc) = project();
    let req = forge_find::FindRequest {
        query: "user".into(),
        ..forge_find::FindRequest::default()
    };
    let flag = std::sync::atomic::AtomicBool::new(true);
    let found = svc.find_text_lazy(&req, &flag).unwrap();
    assert_eq!(found.error.as_deref(), Some(crate::CANCELLED));
    assert_eq!(found.file_count(), 0);
    let flag = std::sync::atomic::AtomicBool::new(false);
    assert!(svc.find_text_lazy(&req, &flag).unwrap().error.is_none());
}

#[test]
fn a_cache_cut_short_is_ignored() {
    let dir = TempDir::new().unwrap();
    write(dir.path(), "a.txt", "alpha\n");
    let cache_dir = TempDir::new().unwrap();
    let cache = cache_dir.path().join("cache.bin");
    let svc = SearchService::open(dir.path(), Some(cache.clone()));
    svc.refresh();
    svc.save();
    assert!(SearchService::open(dir.path(), Some(cache.clone())).ready());

    let bytes = std::fs::read(&cache).unwrap();
    std::fs::write(&cache, &bytes[..bytes.len() - 3]).unwrap();
    assert!(!SearchService::open(dir.path(), Some(cache.clone())).ready());

    std::fs::write(&cache, b"not an index at all").unwrap();
    assert!(!SearchService::open(dir.path(), Some(cache)).ready());
}

#[test]
fn a_rescan_event_triggers_a_full_refresh() {
    let dir = TempDir::new().unwrap();
    write(dir.path(), "a.txt", "alpha\n");
    let svc = std::sync::Arc::new(SearchService::open(dir.path(), None));
    svc.refresh();
    write(dir.path(), "b.txt", "rescantoken\n");
    assert!(paths(&svc, &q("rescantoken")).is_empty());

    let (tx, rx) = std::sync::mpsc::channel();
    let weak = std::sync::Arc::downgrade(&svc);
    std::thread::spawn(move || crate::service::watch_loop(rx, weak));
    let event = notify::Event::new(notify::EventKind::Other).set_flag(notify::event::Flag::Rescan);
    tx.send(Ok(event)).unwrap();
    for _ in 0..50 {
        if !paths(&svc, &q("rescantoken")).is_empty() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    panic!("a rescan event did not refresh the index");
}

#[test]
fn an_unlimited_search_below_the_ceiling_is_not_truncated() {
    let (_d, svc) = project();
    let req = forge_find::FindRequest {
        query: "e".into(),
        limit: usize::MAX,
        ..forge_find::FindRequest::default()
    };
    let found = svc
        .find_text_lazy(&req, &std::sync::atomic::AtomicBool::new(false))
        .unwrap();
    assert!(!found.truncated);
    assert!(found.match_count < crate::service::MAX_RETAINED_MATCHES);
}

#[test]
fn stats_count_what_is_listed_by_name_only() {
    let (_d, svc) = project();
    let (stats, last) = svc.stats();
    assert_eq!(stats.files, stats.content + stats.binary + stats.oversized);
    assert!(stats.binary >= 1);
    assert!(last > 0);
}

#[test]
fn files_can_be_ordered_by_matches_and_by_modification_time() {
    use forge_find::SortBy;
    let dir = TempDir::new().unwrap();
    write(dir.path(), "a.txt", "needle\n");
    write(dir.path(), "b.txt", "needle\nneedle\nneedle\n");
    write(dir.path(), "c.txt", "needle\nneedle\n");
    let old = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
    fs::File::options()
        .write(true)
        .open(dir.path().join("b.txt"))
        .unwrap()
        .set_modified(old)
        .unwrap();
    let svc = SearchService::open(dir.path(), None);
    svc.refresh();
    let order = |sort| {
        let req = forge_find::FindRequest {
            query: "needle".into(),
            sort,
            ..forge_find::FindRequest::default()
        };
        let found = svc
            .find_text_lazy(&req, &std::sync::atomic::AtomicBool::new(false))
            .unwrap();
        found
            .page(0, usize::MAX)
            .into_iter()
            .map(|f| f.path)
            .collect::<Vec<_>>()
    };
    assert_eq!(order(SortBy::Path), ["a.txt", "b.txt", "c.txt"]);
    assert_eq!(order(SortBy::Matches), ["b.txt", "c.txt", "a.txt"]);
    let by_time = order(SortBy::Modified);
    assert_eq!(by_time.last().unwrap(), "b.txt");
}

#[test]
fn lazy_pages_of_a_file_changed_after_the_search_are_searched_again() {
    let dir = TempDir::new().unwrap();
    write(dir.path(), "a.txt", "x foo\nfoo foo\n\n\nlast foo here\n");
    let svc = SearchService::open(dir.path(), None);
    svc.refresh();
    let req = forge_find::FindRequest {
        query: "foo".into(),
        ..forge_find::FindRequest::default()
    };
    let found = svc
        .find_text_lazy(&req, &std::sync::atomic::AtomicBool::new(false))
        .unwrap();
    assert_eq!(found.row_count(), 3);
    // Shorter, with a multi-byte character where the old ranges point.
    write(dir.path(), "a.txt", "€€€€€€€€ foo\n");
    let rows: Vec<_> = found
        .page(0, usize::MAX)
        .into_iter()
        .flat_map(|f| f.matches)
        .collect();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].text, "€€€€€€€€ foo");
    assert_eq!(rows[0].ranges, [(9, 12)]);
    // A later page of the changed file is a page of the new matches (here: none).
    assert!(found.page(1, 2).iter().all(|f| f.matches.is_empty()));
}

#[test]
fn a_steady_stream_of_excluded_events_does_not_hold_back_changes() {
    use forge_core::excludes::{FolderKind, FolderMark, StructureConfig};
    let dir = TempDir::new().unwrap();
    let r = dir.path();
    write(r, "a.txt", "alpha\n");
    let excludes = forge_core::Excludes::default();
    excludes.set(StructureConfig {
        folders: vec![FolderMark {
            path: "target".into(),
            kind: FolderKind::Excluded,
        }],
        exclude_patterns: String::new(),
    });
    let svc = std::sync::Arc::new(SearchService::open(r, None).with_excludes(excludes.clone()));
    svc.refresh();
    let (tx, rx) = std::sync::mpsc::channel();
    let weak = std::sync::Arc::downgrade(&svc);
    std::thread::spawn(move || crate::service::watch_loop(rx, weak));
    let event =
        |path: std::path::PathBuf| Ok(notify::Event::new(notify::EventKind::Any).add_path(path));

    // The user's file changes while a build keeps writing below `target/` (and, later, to a
    // tracked file too): the index catches up anyway.
    write(r, "b.txt", "streamtoken\n");
    tx.send(event(r.join("b.txt"))).unwrap();
    let started = std::time::Instant::now();
    let mut n = 0;
    while started.elapsed() < std::time::Duration::from_secs(8) {
        if !paths(&svc, &q("streamtoken")).is_empty() {
            break;
        }
        n += 1;
        tx.send(event(r.join(format!("target/out{n}.o")))).unwrap();
        if n % 10 == 0 {
            tx.send(event(r.join("a.txt"))).unwrap();
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert_eq!(paths(&svc, &q("streamtoken")), ["b.txt"]);
    assert!(
        started.elapsed() < crate::service::WATCH_MAX_WAIT + std::time::Duration::from_secs(2),
        "{:?}",
        started.elapsed()
    );
}
