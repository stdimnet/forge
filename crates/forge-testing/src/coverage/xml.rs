//! The XML coverage formats, read in one streaming pass:
//!
//! - **Cobertura** (`<coverage line-rate=…>`): `<source>` folders, `<class filename>` files
//!   and their `<line number hits branch condition-coverage="50% (1/2)">` (the duplicates
//!   under `<methods>` are skipped).
//! - **Clover** (`<coverage><project>`, PHPUnit `--coverage-clover`): `<file name path?>`
//!   and `<line num type count>`; `cond` lines have `truecount` / `falsecount` branches.
//! - **JaCoCo** (`<report>`): `<package name>` / `<sourcefile name>` and
//!   `<line nr mi ci mb cb>` (missed / covered instructions and branches); hits are the
//!   covered instructions.

use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};

use super::{Builder, CoverageFormat, CoverageReport, LineHits};

/// Which XML format a document is (by its first elements).
pub fn sniff(text: &str) -> Option<CoverageFormat> {
    let head: String = text.chars().take(4000).collect();
    if head.contains("<report")
        && (head.contains("JACOCO")
            || head.contains("jacoco")
            || head.contains("<sourcefile")
            || head.contains("<package"))
    {
        return Some(CoverageFormat::Jacoco);
    }
    if head.contains("<coverage") && head.contains("<project") {
        return Some(CoverageFormat::Clover);
    }
    if head.contains("<coverage")
        && (head.contains("line-rate") || head.contains("<packages") || head.contains("<sources"))
    {
        return Some(CoverageFormat::Cobertura);
    }
    None
}

fn attrs(e: &BytesStart<'_>) -> Vec<(String, String)> {
    e.attributes()
        .flatten()
        .filter_map(|a| {
            let v = a
                .normalized_value(XmlVersion::Implicit1_0)
                .ok()?
                .into_owned();
            Some((a.key.as_ref().to_string(), v))
        })
        .collect()
}

fn get<'a>(a: &'a [(String, String)], k: &str) -> Option<&'a str> {
    a.iter().find(|(x, _)| x == k).map(|(_, v)| v.as_str())
}

fn num<T: std::str::FromStr>(a: &[(String, String)], k: &str) -> Option<T> {
    get(a, k).and_then(|v| v.trim().parse().ok())
}

/// `50% (1/2)` → (1, 2).
fn condition(text: &str) -> Option<(u32, u32)> {
    let inner = text.split('(').nth(1)?.trim_end_matches(')');
    let (c, t) = inner.split_once('/')?;
    Some((c.trim().parse().ok()?, t.trim().parse().ok()?))
}

fn join(base: &str, rel: &str) -> String {
    let rel = rel.replace('\\', "/");
    if base.is_empty() || rel.starts_with('/') || rel.as_bytes().get(1) == Some(&b':') {
        return rel;
    }
    format!("{}/{}", base.replace('\\', "/").trim_end_matches('/'), rel)
}

pub fn parse(text: &str) -> Result<CoverageReport, String> {
    let format = sniff(text).ok_or("not a Cobertura, Clover or JaCoCo report")?;
    let mut reader = Reader::from_str(text);
    let mut b = Builder::default();
    let mut sources: Vec<String> = Vec::new();
    let mut in_source = false;
    let mut source_text = String::new();
    let mut methods = 0u32;
    let mut file: Option<String> = None;
    let mut package = String::new();
    loop {
        let event = reader.read_event().map_err(|e| e.to_string())?;
        let (start, empty) = match &event {
            Event::Start(e) => (Some(e.clone()), false),
            Event::Empty(e) => (Some(e.clone()), true),
            _ => (None, false),
        };
        if let Some(e) = start {
            let name = e.name().as_ref().to_string();
            let a = attrs(&e);
            match (format, name.as_str()) {
                (CoverageFormat::Cobertura, "source") if !empty => {
                    in_source = true;
                    source_text.clear();
                }
                (CoverageFormat::Cobertura, "methods") if !empty => methods += 1,
                (CoverageFormat::Cobertura, "class") => {
                    let f = get(&a, "filename").unwrap_or_default();
                    let path = join(sources.first().map(String::as_str).unwrap_or(""), f);
                    b.file(&path);
                    file = (!empty).then_some(path);
                }
                (CoverageFormat::Cobertura, "line") if methods == 0 => {
                    if let (Some(f), Some(line)) = (&file, num::<u32>(&a, "number")) {
                        let branches = (get(&a, "branch") == Some("true"))
                            .then(|| get(&a, "condition-coverage").and_then(condition))
                            .flatten();
                        b.line(
                            f,
                            LineHits {
                                line,
                                hits: num::<f64>(&a, "hits").unwrap_or(0.0).max(0.0) as u64,
                                branches,
                                partial: false,
                            },
                        );
                    }
                }
                (CoverageFormat::Clover, "file") => {
                    let path = get(&a, "path")
                        .or(get(&a, "name"))
                        .unwrap_or_default()
                        .replace('\\', "/");
                    b.file(&path);
                    file = (!empty).then_some(path);
                }
                (CoverageFormat::Clover, "line") => {
                    if let (Some(f), Some(line)) = (&file, num::<u32>(&a, "num")) {
                        let count = num::<f64>(&a, "count").unwrap_or(0.0).max(0.0) as u64;
                        let branches = (get(&a, "type") == Some("cond")).then(|| {
                            let t = num::<u64>(&a, "truecount").unwrap_or(0);
                            let fc = num::<u64>(&a, "falsecount").unwrap_or(0);
                            (u32::from(t > 0) + u32::from(fc > 0), 2)
                        });
                        let hits = if count == 0 {
                            branches.map_or(0, |(c, _)| u64::from(c))
                        } else {
                            count
                        };
                        b.line(
                            f,
                            LineHits {
                                line,
                                hits,
                                branches,
                                partial: false,
                            },
                        );
                    }
                }
                (CoverageFormat::Jacoco, "package") => {
                    package = get(&a, "name").unwrap_or_default().to_string();
                }
                (CoverageFormat::Jacoco, "sourcefile") => {
                    let name = get(&a, "name").unwrap_or_default();
                    let path = if package.is_empty() {
                        name.to_string()
                    } else {
                        format!("{package}/{name}")
                    };
                    b.file(&path);
                    file = (!empty).then_some(path);
                }
                (CoverageFormat::Jacoco, "line") => {
                    if let (Some(f), Some(line)) = (&file, num::<u32>(&a, "nr")) {
                        let mb = num::<u32>(&a, "mb").unwrap_or(0);
                        let cb = num::<u32>(&a, "cb").unwrap_or(0);
                        b.line(
                            f,
                            LineHits {
                                line,
                                hits: num::<u64>(&a, "ci").unwrap_or(0),
                                branches: (mb + cb > 0).then_some((cb, mb + cb)),
                                partial: false,
                            },
                        );
                    }
                }
                _ => {}
            }
            continue;
        }
        match event {
            Event::Text(t) if in_source => source_text.push_str(&t.xml10_content()),
            Event::End(e) => match e.name().as_ref() {
                "source" if in_source => {
                    in_source = false;
                    let s = source_text.trim().to_string();
                    if !s.is_empty() {
                        sources.push(s);
                    }
                }
                "methods" => methods = methods.saturating_sub(1),
                n if n
                    == match format {
                        CoverageFormat::Clover => "file",
                        CoverageFormat::Jacoco => "sourcefile",
                        _ => "class",
                    } =>
                {
                    file = None
                }
                "package" => package.clear(),
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(b.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coverage::tests::{fixture, lines};
    use crate::coverage::LineStatus::*;

    #[test]
    fn cobertura_joins_sources_and_skips_method_duplicates() {
        let r = parse(&fixture("cobertura.xml")).unwrap();
        assert_eq!(
            lines(&r, "/p/shop/cart.py"),
            [(1, 1, Covered), (2, 4, Partial), (3, 0, Uncovered)]
        );
        assert_eq!(r.files[0].branch_counter().text(), "50% (1/2)");
    }

    #[test]
    fn clover_reads_statements_methods_and_conditions() {
        let r = parse(&fixture("clover.xml")).unwrap();
        assert_eq!(
            lines(&r, "/p/app/Services/Cart.php"),
            [
                (12, 3, Covered),
                (14, 3, Covered),
                (15, 1, Partial),
                (16, 0, Uncovered),
                (20, 0, Uncovered)
            ]
        );
        assert_eq!(r.files.len(), 2);
    }

    #[test]
    fn jacoco_counts_instructions_and_branches() {
        let r = parse(&fixture("jacoco.xml")).unwrap();
        assert_eq!(
            lines(&r, "com/acme/shop/Cart.java"),
            [(5, 3, Covered), (7, 2, Partial), (9, 0, Uncovered)]
        );
        assert!(parse("<html/>").is_err());
    }
}
