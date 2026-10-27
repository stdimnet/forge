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
//! | Istanbul JSON (`coverage-final.json`) | Istanbul / nyc `json`, Jest and Vitest `json` (v8 and istanbul providers), c8 `json` | [`istanbul`] |
//!
//! Besides lines and branches a report may name **functions** (LCOV `FN`/`FNDA`, Clover
//! `method` lines, JaCoCo `<method>`, Istanbul `fnMap`; the Coverage tool window's
//! "Method, %") and the **tests that covered a line** (LCOV test names `TN`, PHPUnit's XML
//! report through [`phpunit_xml`]; "Show tests covering this line"). [`html`] renders a
//! suite as a standalone HTML report (Generate Coverage Report).
//!
//! A line is **covered** when it ran and all its branches were taken, **partly covered**
//! when it ran but some branches (or, in Go, some blocks on it) did not, and **uncovered**
//! when it never ran.

pub mod gocover;
pub mod html;
pub mod istanbul;
pub mod lcov;
pub mod phpunit_xml;
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
    /// Istanbul's `coverage-final.json`.
    IstanbulJson,
}

impl CoverageFormat {
    pub fn label(self) -> &'static str {
        match self {
            CoverageFormat::Lcov => "LCOV",
            CoverageFormat::Cobertura => "Cobertura",
            CoverageFormat::Clover => "Clover",
            CoverageFormat::Jacoco => "JaCoCo",
            CoverageFormat::GoCover => "Go coverprofile",
            CoverageFormat::IstanbulJson => "Istanbul JSON",
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
    /// The tests that ran the line (sorted, unique), when the report names them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tests: Vec<String>,
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
        add_tests(&mut self.tests, &other.tests);
    }
}

/// Add test names to a sorted, unique list.
fn add_tests(list: &mut Vec<String>, more: &[String]) {
    if more.is_empty() {
        return;
    }
    list.extend(more.iter().cloned());
    list.sort();
    list.dedup();
}

/// A function or method of a file: where it starts and how often it was called.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionHits {
    pub name: String,
    /// 1-based line of its declaration.
    pub line: u32,
    pub hits: u64,
}

/// The lines of one source file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileCoverage {
    /// As the report names it; after [`CoverageReport::resolve`] the project-relative path.
    pub path: String,
    /// Sorted by line, one entry per line.
    pub lines: Vec<LineHits>,
    /// Functions and methods, sorted by line (empty when the format has none).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub functions: Vec<FunctionHits>,
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
    /// Functions per file by (line, name).
    functions: BTreeMap<String, BTreeMap<(u32, String), u64>>,
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

    /// A function (several entries of the same name and line add up).
    pub(crate) fn function(&mut self, path: &str, name: &str, line: u32, hits: u64) {
        self.file(path);
        let e = self
            .functions
            .entry(path.to_string())
            .or_default()
            .entry((line, name.to_string()))
            .or_default();
        *e = e.saturating_add(hits);
    }

    /// Add a whole file (merging and resolving reports).
    fn add(&mut self, path: &str, f: FileCoverage) {
        self.file(path);
        for l in f.lines {
            self.line(path, l);
        }
        for func in f.functions {
            self.function(path, &func.name, func.line, func.hits);
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
                    functions: self
                        .functions
                        .remove(p)
                        .unwrap_or_default()
                        .into_iter()
                        .map(|((line, name), hits)| FunctionHits { name, line, hits })
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
            CoverageFormat::IstanbulJson => istanbul::parse(text),
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
        if t.starts_with('{') {
            return istanbul::sniff(t).then_some(CoverageFormat::IstanbulJson);
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
            let path = f.path.clone();
            b.add(&path, f);
        }
        *self = b.finish();
    }

    /// Several reports as one (Show Coverage Data with more than one suite selected).
    pub fn merged(reports: impl IntoIterator<Item = CoverageReport>) -> CoverageReport {
        let mut out = CoverageReport::default();
        for r in reports {
            out.merge(r);
        }
        out
    }

    /// Record the tests that ran some lines of a file (`path` as the report names it):
    /// lines the report lacks are added as run.
    pub fn attach_tests(&mut self, path: &str, lines: &[(u32, Vec<String>)]) {
        if lines.is_empty() {
            return;
        }
        let i = match self.files.iter().position(|f| f.path == path) {
            Some(i) => i,
            None => {
                self.files.push(FileCoverage {
                    path: path.to_string(),
                    ..FileCoverage::default()
                });
                self.files.len() - 1
            }
        };
        let f = &mut self.files[i];
        for (line, tests) in lines {
            match f.lines.binary_search_by_key(line, |l| l.line) {
                Ok(at) => add_tests(&mut f.lines[at].tests, tests),
                Err(at) if *line > 0 => {
                    let mut l = LineHits {
                        line: *line,
                        hits: tests.len() as u64,
                        ..LineHits::default()
                    };
                    add_tests(&mut l.tests, tests);
                    f.lines.insert(at, l);
                }
                Err(_) => {}
            }
        }
    }

    /// Whether some line names the tests that ran it.
    pub fn has_tests(&self) -> bool {
        self.files
            .iter()
            .any(|f| f.lines.iter().any(|l| !l.tests.is_empty()))
    }

    /// Lines, branches, methods and files of the whole report.
    pub fn totals(&self) -> Totals {
        let mut t = Totals::default();
        for f in &self.files {
            t.add_file(f);
        }
        t
    }

    /// Paths as project paths: `resolve` maps a reported path into the project (`None`:
    /// outside it, the file is dropped). Files that resolve to the same path merge.
    pub fn resolve(self, resolve: impl Fn(&str) -> Option<String>) -> CoverageReport {
        let mut b = Builder::default();
        for f in self.files {
            let Some(path) = resolve(&f.path) else {
                continue;
            };
            b.add(&path, f);
        }
        b.finish()
    }

    pub fn file(&self, path: &str) -> Option<&FileCoverage> {
        self.files.iter().find(|f| f.path == path)
    }
}

/// Covered and total units (lines, branches, methods or files).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Counter {
    pub covered: u32,
    pub total: u32,
}

/// The counters of a report, a directory or a file.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Totals {
    pub lines: Counter,
    pub branches: Counter,
    pub methods: Counter,
    /// Files with at least one line that ran.
    pub files: Counter,
}

impl Totals {
    fn add(&mut self, other: Totals) {
        self.lines.add(other.lines);
        self.branches.add(other.branches);
        self.methods.add(other.methods);
        self.files.add(other.files);
    }

    fn add_file(&mut self, f: &FileCoverage) {
        self.add(f.totals());
    }
}

impl Counter {
    pub fn add(&mut self, other: Counter) {
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

    /// Functions and methods that were called.
    pub fn method_counter(&self) -> Counter {
        Counter {
            covered: self.functions.iter().filter(|f| f.hits > 0).count() as u32,
            total: self.functions.len() as u32,
        }
    }

    pub fn totals(&self) -> Totals {
        let lines = self.line_counter();
        Totals {
            lines,
            branches: self.branch_counter(),
            methods: self.method_counter(),
            files: Counter {
                covered: u32::from(lines.covered > 0),
                total: 1,
            },
        }
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
                tests: l.tests.clone(),
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
    /// The tests that ran the line (Show tests covering this line).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tests: Vec<String>,
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
    match l.tests.len() {
        0 => {}
        1 => s.push_str(", 1 test"),
        n => s.push_str(&format!(", {n} tests")),
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
    /// Functions and methods (Method, %).
    pub methods: Counter,
    /// Files with lines that ran (directories: "x% files" in the Project view).
    pub files: Counter,
    /// "85% (17/20)", "n/a".
    pub line_text: String,
    pub branch_text: String,
    pub method_text: String,
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
    files: BTreeMap<String, Totals>,
}

impl Dir {
    fn totals(&self) -> Totals {
        let mut t = Totals::default();
        for d in self.dirs.values() {
            t.add(d.totals());
        }
        for f in self.files.values() {
            t.add(*f);
        }
        t
    }

    fn of(report: &CoverageReport) -> Dir {
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
            d.files.insert(file.to_string(), f.totals());
        }
        root
    }
}

fn node(path: String, name: String, kind: CoverageNodeKind, depth: u32, t: Totals) -> CoverageNode {
    CoverageNode {
        path,
        name,
        kind,
        depth,
        lines: t.lines,
        branches: t.branches,
        methods: t.methods,
        files: t.files,
        line_text: t.lines.text(),
        branch_text: t.branches.text(),
        method_text: t.methods.text(),
    }
}

/// Every folder of the report (not compacted) with its folded counters, by project path,
/// for the Project view ("50% files, 62% lines covered"); `""` is the project.
pub fn folders(report: &CoverageReport) -> BTreeMap<String, Totals> {
    fn walk(d: &Dir, prefix: &str, out: &mut BTreeMap<String, Totals>) {
        out.insert(prefix.to_string(), d.totals());
        for (name, sub) in &d.dirs {
            let path = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            walk(sub, &path, out);
        }
    }
    let mut out = BTreeMap::new();
    walk(&Dir::of(report), "", &mut out);
    out
}

/// The Project view's hint of a file or folder: "62% lines covered", for folders
/// "50% files, 62% lines covered".
pub fn project_hint(t: &Totals, folder: bool) -> Option<String> {
    let lines = t.lines.percent()?;
    Some(match t.files.percent().filter(|_| folder) {
        Some(files) => format!("{files}% files, {lines}% lines covered"),
        None => format!("{lines}% lines covered"),
    })
}

/// The rows of the Coverage tool window: the project, then directories (folders with a
/// single subfolder and no files are compacted, `src/main/java`) and files, depth first,
/// directories before files, by name.
pub fn summary(report: &CoverageReport, project: &str) -> Vec<CoverageNode> {
    let root = Dir::of(report);
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
        let rows = summary(&r, "shop");
        assert_eq!(rows[0].method_text, "100% (1/1)");
        assert_eq!(rows[0].files.text(), "50% (1/2)");
    }

    #[test]
    fn folders_fold_every_level_for_the_project_view() {
        let mut b = Builder::default();
        for (path, hits) in [
            ("src/main/java/a/A.java", 1),
            ("src/main/java/a/B.java", 0),
            ("src/main/kotlin/C.kt", 2),
        ] {
            b.line(
                path,
                LineHits {
                    line: 1,
                    hits,
                    ..LineHits::default()
                },
            );
        }
        let r = b.finish();
        let f = folders(&r);
        assert_eq!(
            f.keys().collect::<Vec<_>>(),
            [
                "",
                "src",
                "src/main",
                "src/main/java",
                "src/main/java/a",
                "src/main/kotlin"
            ]
        );
        assert_eq!(
            project_hint(&f["src/main"], true).as_deref(),
            Some("66% files, 66% lines covered")
        );
        assert_eq!(
            project_hint(&f["src/main/java"], true).as_deref(),
            Some("50% files, 50% lines covered")
        );
        let file = r.file("src/main/kotlin/C.kt").unwrap().totals();
        assert_eq!(
            project_hint(&file, false).as_deref(),
            Some("100% lines covered")
        );
        assert_eq!(project_hint(&Totals::default(), true), None);
        assert_eq!(r.totals().files.text(), "66% (2/3)");
    }

    #[test]
    fn merging_suites_keeps_functions_and_tests() {
        let (a, _) = CoverageReport::parse_any(&fixture("lcov.info")).unwrap();
        let (b, _) = CoverageReport::parse_any(&fixture("lcov-tests.info")).unwrap();
        let m = CoverageReport::merged([a.clone(), b, a]);
        assert_eq!(m.files.len(), 3);
        let cart = m.file("/p/src/cart.js").unwrap();
        assert_eq!(cart.lines[0].hits, 6);
        assert_eq!(cart.functions[0].hits, 6);
        assert!(m.file("src/cart.js").unwrap().lines[0].tests.len() == 2);
        assert!(m.has_tests());
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
        assert_eq!(rows[0].files.text(), "100% (1/1)");
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
