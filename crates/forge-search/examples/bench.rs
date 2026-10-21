use std::time::{Duration, Instant};

use forge_find::FindRequest;
use forge_search::SearchService;

const RUNS: usize = 3;

fn best<T>(mut run: impl FnMut() -> T) -> (Duration, T) {
    let mut fastest = Duration::MAX;
    let mut last = None;
    for _ in 0..RUNS {
        let started = Instant::now();
        let value = run();
        fastest = fastest.min(started.elapsed());
        last = Some(value);
    }
    (fastest, last.expect("at least one run"))
}

fn text(query: &str, regex: bool) -> FindRequest {
    FindRequest {
        query: query.into(),
        regex,
        limit: 100_000_000,
        ..FindRequest::default()
    }
}

fn row(name: &str, took: Duration, detail: impl std::fmt::Display) {
    println!(
        "| {name} | {:.1} ms | {detail} |",
        took.as_secs_f64() * 1000.0
    );
}

fn main() {
    let root = std::env::args()
        .nth(1)
        .expect("usage: bench <project root>");
    let dir = tempfile::tempdir().expect("temp dir");
    let cache = dir.path().join("bench.idx");

    println!("| Scenario | Best of {RUNS} | Result |");
    println!("| --- | --- | --- |");

    let svc = SearchService::open(&root, Some(cache.clone()));
    let started = Instant::now();
    let stats = svc.refresh();
    row(
        "Cold index",
        started.elapsed(),
        format!("{} files, {} added", svc.file_count(), stats.added),
    );
    svc.save();
    let size = std::fs::metadata(&cache).map(|m| m.len()).unwrap_or(0);

    let started = Instant::now();
    let svc = SearchService::open(&root, Some(cache.clone()));
    row(
        "Load saved index",
        started.elapsed(),
        format!("{} files, cache {} MB", svc.file_count(), size >> 20),
    );

    let (took, stats) = best(|| svc.refresh());
    row(
        "Refresh, nothing changed",
        took,
        format!("{} changed", stats.added + stats.changed + stats.removed),
    );

    for query in ["usr", "ordctl", "index.php"] {
        let (took, hits) = best(|| svc.find_ranked(query, 100, false));
        row(
            &format!("Go to File `{query}`"),
            took,
            format!("{} hits", hits.len()),
        );
    }
    let (took, hits) = best(|| svc.find_ranked("index", 100, true));
    row(
        "Go to File `index`, substring",
        took,
        format!("{} hits", hits.len()),
    );

    for query in ["order", "user"] {
        let (took, hits) = best(|| svc.find_symbols(query, true, 50, &[], false));
        row(
            &format!("Classes `{query}`"),
            took,
            format!("{} hits", hits.len()),
        );
    }

    for (label, query, regex) in [
        ("Find `return`", "return", false),
        ("Find `->get(`", "->get(", false),
        ("Find `function \\w+\\(` regex", "function \\w+\\(", true),
        ("Find `.*` regex", ".*", true),
        ("Find rare word `zzqxv`", "zzqxv", false),
    ] {
        let req = text(query, regex);
        let (took, found) = best(|| svc.find_text_lazy(&req).expect("find"));
        row(
            label,
            took,
            format!(
                "{} matches in {} files",
                found.match_count,
                found.file_count()
            ),
        );
    }
}
