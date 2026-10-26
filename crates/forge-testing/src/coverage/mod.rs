//! Code coverage: reports of the common formats read into one model, the per-directory and
//! per-file summaries of the Coverage tool window and the line marks of the editor gutter.
//!
//! | Format | Written by | Module |
//! | --- | --- | --- |
//! | LCOV | c8, Istanbul/nyc, Jest/Vitest `lcov`, luacov-reporter-lcov, grcov, cargo-llvm-cov | [`lcov`] |
//! | Cobertura XML | coverage.py `xml`, Istanbul `cobertura`, cargo-llvm-cov, gcovr | [`xml`] |
//! | Clover XML | PHPUnit `--coverage-clover` (Xdebug, PCOV), Istanbul `clover` | [`xml`] |
//! | JaCoCo XML | JaCoCo, Kover | [`xml`] |
//! | Go coverprofile | `go test -coverprofile` | [`gocover`] |
//!
//! A line is **covered** when it ran and all its branches were taken, **partly covered**
//! when it ran but some branches (or, in Go, some blocks on it) did not, and **uncovered**
//! when it never ran.

pub mod gocover;
pub mod lcov;
pub mod xml;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CoverageFormat {
    #[default]
    Lcov,
    Cobertura,
    Clover,
    Jacoco,
    GoCover,
}

impl CoverageFormat {
    pub fn label(self) -> &'static str {
        match self {
            CoverageFormat::Lcov => "LCOV",
            CoverageFormat::Cobertura => "Cobertura",
            CoverageFormat::Clover => "Clover",
            CoverageFormat::Jacoco => "JaCoCo",
            CoverageFormat::GoCover => "Go coverprofile",
        }
    }
}

/// Coverage of one line (1-based).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineHits {
    pub line: u32,
    /// How often the line ran (JaCoCo: covered instructions).
    pub hits: u64,
    /// Branches taken and branches in total, for lines with branches.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branches: Option<(u32, u32)>,
    /// Only some code on the line ran (Go blocks).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub partial: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LineStatus {
    Covered,
    Partial,
    Uncovered,
}

impl LineHits {
    pub fn status(&self) -> LineStatus {
        if self.hits == 0 {
            LineStatus::Uncovered
        } else if self.partial || self.branches.is_some_and(|(c, t)| c < t) {
            LineStatus::Partial
        } else {
            LineStatus::Covered
        }
    }

    /// Combine two reports of the same line (several runs, duplicate entries).
    fn merge(&mut self, other: &LineHits) {
        // Fully run in one of them: fully run (branches: the better of the two).
        let full = (self.hits > 0 && !self.partial) || (other.hits > 0 && !other.partial);
        self.hits = self.hits.saturating_add(other.hits);
        self.branches = match (self.branches, other.branches) {
            (Some((c1, t1)), Some((c2, t2))) => Some((c1.max(c2).min(t1.max(t2)), t1.max(t2))),
            (a, b) => a.or(b),
        };
        self.partial = !full && (self.partial || other.partial);
    }
}

/// The lines of one source file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileCoverage {
    /// As the report names it; after [`CoverageReport::resolve`] the project-relative path.
    pub path: String,
    /// Sorted by line, one entry per line.
    pub lines: Vec<LineHits>,
}

/// A coverage report: the files in the order they were first seen.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoverageReport {
    pub files: Vec<FileCoverage>,
}

/// Collects lines per file while a report is read.
#[derive(Debug, Default)]
pub(crate) struct Builder {
    order: Vec<String>,
    files: BTreeMap<String, BTreeMap<u32, LineHits>>,
}

impl Builder {
    pub(crate) fn file(&mut self, path: &str) {
        if !self.files.contains_key(path) {
            self.order.push(path.to_string());
            self.files.insert(path.to_string(), BTreeMap::new());
        }
    }

    pub(crate) fn line(&mut self, path: &str, hits: LineHits) {
        if hits.line == 0 {
            return;
        }
        self.file(path);
        let lines = self.files.get_mut(path).expect("file added");
        match lines.get_mut(&hits.line) {
            Some(known) => known.merge(&hits),
            None => {
                lines.insert(hits.line, hits);
            }
        }
    }

    pub(crate) fn finish(mut self) -> CoverageReport {
        CoverageReport {
            files: self
                .order
                .iter()
                .map(|p| FileCoverage {
                    path: p.clone(),
                    lines: self
                        .files
                        .remove(p)
                        .unwrap_or_default()
                        .into_values()
                        .collect(),
                })
                .collect(),
        }
    }
}

impl CoverageReport {
    /// Read a report of a known format.
    pub fn parse(text: &str, format: CoverageFormat) -> Result<CoverageReport, String> {
        match format {
            CoverageFormat::Lcov => lcov::parse(text),
            CoverageFormat::GoCover => gocover::parse(text),
            CoverageFormat::Cobertura | CoverageFormat::Clover | CoverageFormat::Jacoco => {
                xml::parse(text)
            }
        }
    }

    /// The format of a report by its content.
    pub fn sniff(text: &str) -> Option<CoverageFormat> {
        let t = text.trim_start_matches('\u{feff}').trim_start();
        if t.starts_with("mode:") {
            return Some(CoverageFormat::GoCover);
        }
        if t.starts_with('<') {
            return xml::sniff(t);
        }
        t.lines()
            .any(|l| l.starts_with("SF:") || l.starts_with("TN:"))
            .then_some(CoverageFormat::Lcov)
    }

    /// Read a report of any supported format.
    pub fn parse_any(text: &str) -> Result<(CoverageReport, CoverageFormat), String> {
        let format = Self::sniff(text).ok_or("not a coverage report")?;
        Ok((Self::parse(text, format)?, format))
    }

    /// Add another report (several report files of one run).
    pub fn merge(&mut self, other: CoverageReport) {
        let mut b = Builder::default();
        for f in self.files.drain(..).chain(other.files) {
            b.file(&f.path);
            for l in f.lines {
                b.line(&f.path, l);
            }
        }
        *self = b.finish();
    }

    /// Paths as project paths: `resolve` maps a reported path into the project (`None`:
    /// outside it, the file is dropped). Files that resolve to the same path merge.
    pub fn resolve(self, resolve: impl Fn(&str) -> Option<String>) -> CoverageReport {
        let mut b = Builder::default();
        for f in self.files {
            let Some(path) = resolve(&f.path) else {
                continue;
            };
            b.file(&path);
            for l in f.lines {
                b.line(&path, l);
            }
        }
        b.finish()
    }

    pub fn file(&self, path: &str) -> Option<&FileCoverage> {
        self.files.iter().find(|f| f.path == path)
    }
}

/// Covered and total units (lines or branches).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Counter {
    pub covered: u32,
    pub total: u32,
}

impl Counter {
    fn add(&mut self, other: Counter) {
        self.covered += other.covered;
        self.total += other.total;
    }

    /// Whole percent, rounded down like IntelliJ (`None` without units).
    pub fn percent(self) -> Option<u32> {
        (self.total > 0).then(|| (u64::from(self.covered) * 100 / u64::from(self.total)) as u32)
    }

    /// "85% (17/20)" or "n/a".
    pub fn text(self) -> String {
        match self.percent() {
            Some(p) => format!("{p}% ({}/{})", self.covered, self.total),
            None => "n/a".into(),
        }
    }
}

impl FileCoverage {
    pub fn line_counter(&self) -> Counter {
        Counter {
            covered: self.lines.iter().filter(|l| l.hits > 0).count() as u32,
            total: self.lines.len() as u32,
        }
    }

    pub fn branch_counter(&self) -> Counter {
        let mut c = Counter::default();
        for (covered, total) in self.lines.iter().filter_map(|l| l.branches) {
            c.add(Counter { covered, total });
        }
        c
    }

    /// The gutter marks of the file.
    pub fn marks(&self) -> Vec<LineMark> {
        self.lines
            .iter()
            .map(|l| LineMark {
                line: l.line,
                hits: l.hits,
                status: l.status(),
                branches: l.branches,
                hint: hint(l),
            })
            .collect()
    }
}

/// A gutter mark: the line (1-based), its status and the hover text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineMark {
    pub line: u32,
    pub hits: u64,
    pub status: LineStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branches: Option<(u32, u32)>,
    /// "Hits: 3", "Hits: 3, branches: 1 of 2 covered", "Line was not executed".
    pub hint: String,
}

fn hint(l: &LineHits) -> String {
    if l.hits == 0 {
        return "Line was not executed".into();
    }
    let mut s = format!("Hits: {}", l.hits);
    if let Some((c, t)) = l.branches {
        s.push_str(&format!(", branches: {c} of {t} covered"));
    } else if l.partial {
        s.push_str(", partly covered");
    }
    s
}

/// A row of the Coverage tool window: the project, a directory or a file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoverageNode {
    /// Project-relative path (`""` for the project).
    pub path: String,
    pub name: String,
    pub kind: CoverageNodeKind,
    pub depth: u32,
    pub lines: Counter,
    pub branches: Counter,
    /// "85% (17/20)", "n/a".
    pub line_text: String,
    pub branch_text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CoverageNodeKind {
    Project,
    Directory,
    File,
}

#[derive(Debug, Default)]
struct Dir {
    dirs: BTreeMap<String, Dir>,
    files: BTreeMap<String, (Counter, Counter)>,
}

impl Dir {
    fn totals(&self) -> (Counter, Counter) {
        let mut lines = Counter::default();
        let mut branches = Counter::default();
        for d in self.dirs.values() {
            let (l, b) = d.totals();
            lines.add(l);
            branches.add(b);
        }
        for (l, b) in self.files.values() {
            lines.add(*l);
            branches.add(*b);
        }
        (lines, branches)
    }
}

fn node(
    path: String,
    name: String,
    kind: CoverageNodeKind,
    depth: u32,
    (lines, branches): (Counter, Counter),
) -> CoverageNode {
    CoverageNode {
        path,
        name,
        kind,
        depth,
        lines,
        branches,
        line_text: lines.text(),
        branch_text: branches.text(),
    }
}

/// The rows of the Coverage tool window: the project, then directories (folders with a
/// single subfolder and no files are compacted, `src/main/java`) and files, depth first,
/// directories before files, by name.
pub fn summary(report: &CoverageReport, project: &str) -> Vec<CoverageNode> {
    let mut root = Dir::default();
    for f in &report.files {
        let parts: Vec<&str> = f.path.split('/').filter(|p| !p.is_empty()).collect();
        let Some((file, dirs)) = parts.split_last() else {
            continue;
        };
        let mut d = &mut root;
        for p in dirs {
            d = d.dirs.entry(p.to_string()).or_default();
        }
        d.files
            .insert(file.to_string(), (f.line_counter(), f.branch_counter()));
    }
    let mut out = vec![node(
        String::new(),
        project.to_string(),
        CoverageNodeKind::Project,
        0,
        root.totals(),
    )];
    fn walk(d: &Dir, prefix: &str, depth: u32, out: &mut Vec<CoverageNode>) {
        for (name, sub) in &d.dirs {
            let mut name = name.clone();
            let mut sub = sub;
            while sub.files.is_empty() && sub.dirs.len() == 1 {
                let (n, s) = sub.dirs.iter().next().expect("one");
                name = format!("{name}/{n}");
                sub = s;
            }
            let path = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            out.push(node(
                path.clone(),
                name,
                CoverageNodeKind::Directory,
                depth,
                sub.totals(),
            ));
            walk(sub, &path, depth + 1, out);
        }
        for (name, counters) in &d.files {
            let path = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            out.push(node(
                path,
                name.clone(),
                CoverageNodeKind::File,
                depth,
                *counters,
            ));
        }
    }
    walk(&root, "", 1, &mut out);
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn fixture(name: &str) -> String {
        let path = format!("{}/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    /// (line, hits, status) of a file.
    pub fn lines(r: &CoverageReport, path: &str) -> Vec<(u32, u64, LineStatus)> {
        r.file(path)
            .unwrap_or_else(|| {
                panic!(
                    "no {path} in {:?}",
                    r.files.iter().map(|f| &f.path).collect::<Vec<_>>()
                )
            })
            .lines
            .iter()
            .map(|l| (l.line, l.hits, l.status()))
            .collect()
    }

    #[test]
    fn sniffs_every_format() {
        assert_eq!(
            CoverageReport::sniff(&fixture("lcov.info")),
            Some(CoverageFormat::Lcov)
        );
        assert_eq!(
            CoverageReport::sniff(&fixture("cobertura.xml")),
            Some(CoverageFormat::Cobertura)
        );
        assert_eq!(
            CoverageReport::sniff(&fixture("clover.xml")),
            Some(CoverageFormat::Clover)
        );
        assert_eq!(
            CoverageReport::sniff(&fixture("jacoco.xml")),
            Some(CoverageFormat::Jacoco)
        );
        assert_eq!(
            CoverageReport::sniff(&fixture("cover.out")),
            Some(CoverageFormat::GoCover)
        );
        assert_eq!(CoverageReport::sniff("hello"), None);
        assert!(CoverageReport::parse_any("hello").is_err());
    }

    #[test]
    fn summarizes_directories_and_files() {
        let (r, _) = CoverageReport::parse_any(&fixture("lcov.info")).unwrap();
        let r = r.resolve(|p| p.strip_prefix("/p/").map(str::to_string));
        let rows: Vec<_> = summary(&r, "shop")
            .iter()
            .map(|n| {
                (
                    n.name.clone(),
                    n.depth,
                    n.line_text.clone(),
                    n.branch_text.clone(),
                )
            })
            .collect();
        assert_eq!(
            rows,
            [
                (
                    "shop".to_string(),
                    0,
                    "62% (5/8)".to_string(),
                    "50% (1/2)".to_string()
                ),
                ("src".into(), 1, "62% (5/8)".into(), "50% (1/2)".into()),
                ("util".into(), 2, "0% (0/2)".into(), "n/a".into()),
                ("format.js".into(), 3, "0% (0/2)".into(), "n/a".into()),
                ("cart.js".into(), 2, "83% (5/6)".into(), "50% (1/2)".into()),
            ]
        );
        let marks = r.file("src/cart.js").unwrap().marks();
        assert_eq!(marks[0].hint, "Hits: 3");
        assert_eq!(marks[2].status, LineStatus::Partial);
        assert_eq!(marks[2].hint, "Hits: 2, branches: 1 of 2 covered");
        assert_eq!(marks[5].hint, "Line was not executed");
    }

    #[test]
    fn merges_runs_and_compacts_single_folders() {
        let mut a = CoverageReport::default();
        let mut b = Builder::default();
        b.line(
            "src/main/java/Cart.java",
            LineHits {
                line: 1,
                hits: 0,
                ..LineHits::default()
            },
        );
        a.merge(b.finish());
        let mut c = Builder::default();
        c.line(
            "src/main/java/Cart.java",
            LineHits {
                line: 1,
                hits: 2,
                ..LineHits::default()
            },
        );
        a.merge(c.finish());
        assert_eq!(
            lines(&a, "src/main/java/Cart.java"),
            [(1, 2, LineStatus::Covered)]
        );
        let rows = summary(&a, "p");
        assert_eq!(rows[1].name, "src/main/java");
        assert_eq!(rows[1].path, "src/main/java");
        assert_eq!(rows[2].path, "src/main/java/Cart.java");
        assert_eq!(
            Counter {
                covered: 2,
                total: 3
            }
            .percent(),
            Some(66)
        );
    }
}
