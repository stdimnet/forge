//! The tests that covered each line, from PHPUnit's XML coverage report
//! (`--coverage-xml <dir>`; Codeception `--coverage-phpunit <dir>`), which php-code-coverage
//! writes with the test of every covered line. Clover, Cobertura and the HTML report do not
//! carry them, and `--coverage-php` is a serialized PHP object; so Run with Coverage with
//! "Track per-test coverage" writes Clover for the lines plus this folder for the tests.
//!
//! - `index.xml`: `<project source="/var/www/html/app">` and its `<file href="Services/Cart.php.xml">`
//!   entries (nested in `<directory>` elements);
//! - one `*.xml` file per source file:
//!   `<coverage><line nr="14"><covered by="Tests\CartTest::test_total"/></line></coverage>`.
//!
//! A source file's path is the project's `source` joined with the entry's `href` without
//! `.xml`, like the paths of the Clover report of the same run.

use quick_xml::events::Event;
use quick_xml::{Reader, XmlVersion};

/// Covered lines with the tests that ran them.
pub type LineTests = Vec<(u32, Vec<String>)>;

/// The index of a report folder.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Index {
    /// The folder the report's paths are relative to.
    pub source: String,
    /// The per-file reports, relative to the folder.
    pub files: Vec<String>,
}

fn attr(e: &quick_xml::events::BytesStart<'_>, key: &str) -> Option<String> {
    e.attributes().flatten().find_map(|a| {
        (a.key.as_ref() == key)
            .then(|| a.normalized_value(XmlVersion::Implicit1_0).ok())
            .flatten()
            .map(|v| v.into_owned())
    })
}

/// Read `index.xml`.
pub fn index(text: &str) -> Result<Index, String> {
    let mut reader = Reader::from_str(text);
    let mut out = Index::default();
    let mut project = false;
    loop {
        match reader.read_event().map_err(|e| e.to_string())? {
            Event::Start(e) | Event::Empty(e) => match e.name().as_ref() {
                "project" => {
                    project = true;
                    out.source = attr(&e, "source").unwrap_or_default().replace('\\', "/");
                }
                "file" => {
                    if let Some(href) = attr(&e, "href") {
                        out.files.push(href.replace('\\', "/"));
                    }
                }
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }
    if !project {
        return Err("not a PHPUnit XML coverage index".into());
    }
    Ok(out)
}

/// The covered lines of one file's report with their tests (sorted, unique).
pub fn line_tests(text: &str) -> Result<LineTests, String> {
    let mut reader = Reader::from_str(text);
    let mut out: LineTests = Vec::new();
    let mut in_coverage = false;
    let mut current: Option<(u32, Vec<String>)> = None;
    loop {
        let event = reader.read_event().map_err(|e| e.to_string())?;
        match &event {
            Event::Start(e) | Event::Empty(e) => {
                let empty = matches!(event, Event::Empty(_));
                match e.name().as_ref() {
                    "coverage" if !empty => in_coverage = true,
                    "line" if in_coverage => {
                        let nr = attr(e, "nr").and_then(|n| n.trim().parse().ok());
                        if let Some(nr) = nr.filter(|_| !empty) {
                            current = Some((nr, Vec::new()));
                        }
                    }
                    "covered" => {
                        if let (Some((_, tests)), Some(by)) = (current.as_mut(), attr(e, "by")) {
                            tests.push(by);
                        }
                    }
                    _ => {}
                }
            }
            Event::End(e) => match e.name().as_ref() {
                "coverage" => in_coverage = false,
                "line" => {
                    if let Some((nr, mut tests)) = current.take() {
                        tests.sort();
                        tests.dedup();
                        if !tests.is_empty() {
                            out.push((nr, tests));
                        }
                    }
                }
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(out)
}

/// Every file of a report folder with its covered lines' tests; `read` gives the text of
/// a file of the folder (`index.xml`, `Services/Cart.php.xml`). Paths are as the Clover
/// report of the same run names them.
pub fn collect(read: impl Fn(&str) -> Option<String>) -> Result<Vec<(String, LineTests)>, String> {
    let index = index(&read("index.xml").ok_or("no index.xml in the XML coverage report")?)?;
    let mut out = Vec::new();
    for href in &index.files {
        let Some(text) = read(href) else {
            continue;
        };
        let rel = href.strip_suffix(".xml").unwrap_or(href);
        let path = if index.source.is_empty() {
            rel.to_string()
        } else {
            format!("{}/{rel}", index.source.trim_end_matches('/'))
        };
        out.push((path, line_tests(&text)?));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coverage::tests::fixture;
    use crate::coverage::CoverageReport;

    #[test]
    fn tests_of_covered_lines_join_the_clover_report() {
        let tests = collect(|name| {
            let path = format!("{}/fixtures/phpunit-xml/{name}", env!("CARGO_MANIFEST_DIR"));
            std::fs::read_to_string(path).ok()
        })
        .unwrap();
        assert_eq!(tests.len(), 1, "the missing file is skipped");
        let (path, lines) = &tests[0];
        assert_eq!(path, "/p/app/Services/Cart.php");
        assert_eq!(
            lines[0],
            (
                12,
                vec![
                    "Tests\\Unit\\CartTest::test_adds_items".to_string(),
                    "Tests\\Unit\\CartTest::test_total with data set #0".to_string()
                ]
            )
        );
        assert_eq!(lines.len(), 3);
        let (mut r, _) = CoverageReport::parse_any(&fixture("clover.xml")).unwrap();
        for (path, lines) in &tests {
            r.attach_tests(path, lines);
        }
        assert!(r.has_tests());
        let f = r.file("/p/app/Services/Cart.php").unwrap();
        let marks = f.marks();
        assert_eq!(marks[0].tests.len(), 2);
        assert_eq!(marks[0].hint, "Hits: 3, 2 tests");
        assert!(
            f.lines
                .iter()
                .find(|l| l.line == 16)
                .unwrap()
                .tests
                .is_empty(),
            "line 16 never ran"
        );
        // A line Clover does not list is added as run.
        assert_eq!(f.lines.iter().find(|l| l.line == 13).unwrap().hits, 1);
        assert!(index("<phpunit/>").is_err());
        assert!(collect(|_| None).is_err());
    }
}
