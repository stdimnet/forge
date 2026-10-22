//! Output filters of External Tools: a regular expression with the macros `$FILE_PATH$`,
//! `$LINE$` and `$COLUMN$` (PHPStorm's "Output filters", e.g. `$FILE_PATH$:$LINE$`) whose
//! matches become clickable file links in the tool's console.
//!
//! Like PHPStorm, everything outside the macros is a regular expression, so `(`, `[` or `.`
//! keep their regex meaning; `$FILE_PATH$` is required.

use regex_lite::Regex;

/// The macros an output filter may contain, with their descriptions (the Edit Tool dialog
/// lists them).
pub const FILTER_MACROS: &[(&str, &str)] = &[
    ("$FILE_PATH$", "File path"),
    ("$LINE$", "Line number"),
    ("$COLUMN$", "Column number"),
];

/// What `$FILE_PATH$` matches: an optional drive letter and path characters (no spaces, so
/// `Error in app/User.php:3` links only the path).
const FILE_PATH_RE: &str = r"(?P<file>(?:[A-Za-z]:)?[0-9A-Za-z_\-\\./~@+]+)";

/// A compiled output filter.
#[derive(Debug, Clone)]
pub struct OutputFilter {
    pattern: String,
    re: Regex,
}

impl PartialEq for OutputFilter {
    fn eq(&self, other: &Self) -> bool {
        self.pattern == other.pattern
    }
}

impl Eq for OutputFilter {}

/// One match of a filter in a line: byte offsets of the linked text (from the path to the
/// end of the line / column number), the path as written and the 1-based position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterMatch {
    pub start: usize,
    pub end: usize,
    pub path: String,
    pub line: Option<u32>,
    pub column: Option<u32>,
}

impl OutputFilter {
    /// Compile a filter such as `$FILE_PATH$:$LINE$:$COLUMN$` or `in $FILE_PATH$ on line
    /// $LINE$`. Errors are user-facing ("Output filter … is not a valid regular expression").
    pub fn compile(pattern: &str) -> Result<Self, String> {
        let trimmed = pattern.trim();
        if trimmed.is_empty() {
            return Err("Output filter is empty".into());
        }
        if !trimmed.contains("$FILE_PATH$") {
            return Err(format!(
                "Output filter `{trimmed}` must contain $FILE_PATH$"
            ));
        }
        let mut seen = [false; 3];
        let mut out = String::new();
        let mut rest = trimmed;
        while let Some(i) = rest.find('$') {
            out.push_str(&rest[..i]);
            let tail = &rest[i..];
            let found = FILTER_MACROS
                .iter()
                .enumerate()
                .find(|(_, (name, _))| tail.starts_with(name));
            match found {
                Some((idx, (name, _))) => {
                    if seen[idx] {
                        return Err(format!("Output filter `{trimmed}` uses {name} twice"));
                    }
                    seen[idx] = true;
                    out.push_str(match idx {
                        0 => FILE_PATH_RE,
                        1 => r"(?P<line>[0-9]+)",
                        _ => r"(?P<column>[0-9]+)",
                    });
                    rest = &tail[name.len()..];
                }
                None => {
                    // A plain `$` is the regex end anchor.
                    out.push('$');
                    rest = &tail[1..];
                }
            }
        }
        out.push_str(rest);
        let re = Regex::new(&out).map_err(|e| {
            format!("Output filter `{trimmed}` is not a valid regular expression: {e}")
        })?;
        Ok(Self {
            pattern: trimmed.to_string(),
            re,
        })
    }

    pub fn pattern(&self) -> &str {
        &self.pattern
    }

    /// All matches in a line of output.
    pub fn find(&self, text: &str) -> Vec<FilterMatch> {
        let mut out = Vec::new();
        for caps in self.re.captures_iter(text) {
            let Some(file) = caps.name("file") else {
                continue;
            };
            if file.as_str().is_empty() {
                continue;
            }
            let line = caps.name("line");
            let column = caps.name("column");
            let end = [Some(file), line, column]
                .into_iter()
                .flatten()
                .map(|m| m.end())
                .max()
                .unwrap_or(file.end());
            out.push(FilterMatch {
                start: file.start(),
                end,
                path: file.as_str().to_string(),
                line: line.and_then(|m| m.as_str().parse().ok()),
                column: column.and_then(|m| m.as_str().parse().ok()),
            });
        }
        out
    }
}

/// Compile several filters; invalid and empty ones are reported, the rest is kept.
pub fn compile_filters<S: AsRef<str>>(patterns: &[S]) -> (Vec<OutputFilter>, Vec<String>) {
    let mut ok = Vec::new();
    let mut errors = Vec::new();
    for p in patterns {
        if p.as_ref().trim().is_empty() {
            continue;
        }
        match OutputFilter::compile(p.as_ref()) {
            Ok(f) => ok.push(f),
            Err(e) => errors.push(e),
        }
    }
    (ok, errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiles_macros_into_named_groups() {
        let f = OutputFilter::compile("$FILE_PATH$:$LINE$:$COLUMN$").unwrap();
        let m = f.find("phpcs: src/Order.php:12:5 Missing doc comment");
        assert_eq!(
            m,
            vec![FilterMatch {
                start: 7,
                end: 25,
                path: "src/Order.php".into(),
                line: Some(12),
                column: Some(5),
            }]
        );
    }

    #[test]
    fn the_rest_of_the_pattern_is_a_regular_expression() {
        let f = OutputFilter::compile(r"in $FILE_PATH$ on line $LINE$\.?$").unwrap();
        let m = f.find("PHP Parse error: syntax error in /p/app/User.php on line 7.");
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].path, "/p/app/User.php");
        assert_eq!(m[0].line, Some(7));
        assert_eq!(m[0].column, None);
        assert!(f.find("in app/User.php on line 7 and more").is_empty());
        // Windows paths keep their drive letter.
        let f = OutputFilter::compile(r"$FILE_PATH$\($LINE$\)").unwrap();
        let m = f.find(r"C:\work\shop\app\User.php(31): error");
        assert_eq!(m[0].path, r"C:\work\shop\app\User.php");
        assert_eq!(m[0].line, Some(31));
    }

    #[test]
    fn rejects_filters_without_a_path_or_with_bad_regex() {
        assert!(OutputFilter::compile("  ").is_err());
        assert!(OutputFilter::compile("$LINE$")
            .unwrap_err()
            .contains("$FILE_PATH$"));
        assert!(OutputFilter::compile("$FILE_PATH$:$LINE$:$LINE$")
            .unwrap_err()
            .contains("twice"));
        let err = OutputFilter::compile("$FILE_PATH$ (unclosed").unwrap_err();
        assert!(err.contains("not a valid regular expression"), "{err}");
        let (ok, errors) = compile_filters(&["$FILE_PATH$:$LINE$", "", "x("]);
        assert_eq!(ok.len(), 1);
        assert_eq!(errors.len(), 1);
    }

    #[test]
    fn finds_every_match_in_a_line() {
        let f = OutputFilter::compile("$FILE_PATH$:$LINE$").unwrap();
        let m = f.find("a.php:1 b/c.php:22");
        assert_eq!(
            m.iter()
                .map(|m| (m.path.as_str(), m.line))
                .collect::<Vec<_>>(),
            vec![("a.php", Some(1)), ("b/c.php", Some(22))]
        );
    }
}
