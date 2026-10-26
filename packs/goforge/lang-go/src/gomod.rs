//! `go.mod` and `go.work` files: directives with their positions (for highlighting,
//! navigation, completion and checks), as the `golang.org/x/mod/modfile` grammar has them.
//!
//! - `go.mod`: `module`, `go`, `toolchain`, `godebug`, `require`, `replace`, `exclude`,
//!   `retract`, `tool`, `ignore`; each may be a single line or a `( … )` block. A
//!   `// indirect` comment marks indirect requirements.
//! - `go.work`: `go`, `toolchain`, `godebug`, `use`, `replace`.

use serde::{Deserialize, Serialize};

use crate::scan::utf16_len;
use crate::version::is_valid_version;

/// A word of a directive line with its position (0-based line, UTF-16 columns).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Word {
    pub text: String,
    pub line: u32,
    pub start: u32,
    pub end: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Require {
    pub path: Word,
    pub version: Option<Word>,
    pub indirect: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Replace {
    pub old: Word,
    pub old_version: Option<Word>,
    pub new: Word,
    pub new_version: Option<Word>,
}

impl Replace {
    /// Whether the replacement is a local directory (`./fork`, `../lib`, `/abs`).
    pub fn is_local(&self) -> bool {
        let p = &self.new.text;
        p.starts_with("./")
            || p.starts_with("../")
            || p.starts_with('/')
            || p == "."
            || p == ".."
            || p.get(1..3) == Some(":\\")
    }
}

/// A problem of a `go.mod` / `go.work` line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModProblem {
    pub line: u32,
    pub start: u32,
    pub end: u32,
    pub message: String,
    /// `error` or `warning`.
    pub severity: String,
}

/// A parsed `go.mod`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoMod {
    pub module: Option<Word>,
    pub go: Option<Word>,
    pub toolchain: Option<Word>,
    pub requires: Vec<Require>,
    pub replaces: Vec<Replace>,
    pub excludes: Vec<Require>,
    pub retracts: Vec<Word>,
    pub tools: Vec<Word>,
    pub problems: Vec<ModProblem>,
}

/// A parsed `go.work`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoWork {
    pub go: Option<Word>,
    pub toolchain: Option<Word>,
    /// Module directories, relative to the `go.work` folder.
    pub uses: Vec<Word>,
    pub replaces: Vec<Replace>,
    pub problems: Vec<ModProblem>,
}

/// One logical line: its words and whether it ends with `// indirect`.
struct Line {
    number: u32,
    words: Vec<Word>,
    comment: Option<String>,
}

/// Split a line into words: quoted strings (`"a b"`, `` `a` ``) are one word, `//` starts a
/// comment, `(`, `)` and `=>` are words of their own.
fn split(number: u32, text: &str) -> Line {
    let mut words = Vec::new();
    let mut comment = None;
    let mut col: u32 = 0;
    let mut current = String::new();
    let mut start = 0u32;
    let mut chars = text.chars().peekable();
    let flush = |current: &mut String, start: u32, col: u32, words: &mut Vec<Word>| {
        if !current.is_empty() {
            words.push(Word {
                text: std::mem::take(current),
                line: number,
                start,
                end: col,
            });
        }
    };
    while let Some(c) = chars.next() {
        let w = c.len_utf16() as u32;
        match c {
            '/' if chars.peek() == Some(&'/') => {
                flush(&mut current, start, col, &mut words);
                let rest: String = chars.collect();
                comment = Some(rest[1..].trim().to_string());
                break;
            }
            '"' | '`' if current.is_empty() => {
                start = col;
                col += w;
                let mut value = String::new();
                let mut escaped = false;
                for d in chars.by_ref() {
                    col += d.len_utf16() as u32;
                    if c == '"' && d == '\\' && !escaped {
                        escaped = true;
                        continue;
                    }
                    if d == c && !escaped {
                        break;
                    }
                    escaped = false;
                    value.push(d);
                }
                words.push(Word {
                    text: value,
                    line: number,
                    start,
                    end: col,
                });
                continue;
            }
            c if c.is_whitespace() => flush(&mut current, start, col, &mut words),
            '(' | ')' => {
                flush(&mut current, start, col, &mut words);
                words.push(Word {
                    text: c.to_string(),
                    line: number,
                    start: col,
                    end: col + 1,
                });
            }
            '=' if chars.peek() == Some(&'>') => {
                flush(&mut current, start, col, &mut words);
                chars.next();
                words.push(Word {
                    text: "=>".into(),
                    line: number,
                    start: col,
                    end: col + 2,
                });
                col += 2;
                continue;
            }
            c => {
                if current.is_empty() {
                    start = col;
                }
                current.push(c);
            }
        }
        col += w;
    }
    flush(&mut current, start, col, &mut words);
    Line {
        number,
        words,
        comment,
    }
}

/// Directive name, its argument lines (one per block entry) and the directive's word.
struct Directive {
    verb: Word,
    entries: Vec<Line>,
}

fn directives(text: &str) -> (Vec<Directive>, Vec<ModProblem>) {
    let mut out: Vec<Directive> = Vec::new();
    let mut problems = Vec::new();
    let mut block: Option<Directive> = None;
    for (n, raw) in text.lines().enumerate() {
        let mut line = split(n as u32, raw);
        if line.words.is_empty() {
            continue;
        }
        if let Some(b) = block.as_mut() {
            if line.words[0].text == ")" {
                out.push(block.take().expect("open block"));
                continue;
            }
            b.entries.push(line);
            continue;
        }
        let verb = line.words.remove(0);
        if line.words.first().is_some_and(|w| w.text == "(") {
            block = Some(Directive {
                verb,
                entries: Vec::new(),
            });
            continue;
        }
        out.push(Directive {
            verb,
            entries: vec![line],
        });
    }
    if let Some(b) = block {
        problems.push(problem(&b.verb, "missing closing parenthesis", "error"));
        out.push(b);
    }
    (out, problems)
}

fn problem(w: &Word, message: &str, severity: &str) -> ModProblem {
    ModProblem {
        line: w.line,
        start: w.start,
        end: w.end.max(w.start + 1),
        message: message.to_string(),
        severity: severity.to_string(),
    }
}

fn line_problem(l: &Line, message: &str) -> ModProblem {
    let first = l.words.first().cloned().unwrap_or(Word {
        line: l.number,
        ..Word::default()
    });
    let last = l.words.last().cloned().unwrap_or_default();
    ModProblem {
        line: l.number,
        start: first.start,
        end: last.end.max(first.start + 1),
        message: message.to_string(),
        severity: "error".into(),
    }
}

/// `path version` (require, exclude).
fn module_version(l: &Line, problems: &mut Vec<ModProblem>, verb: &str) -> Option<Require> {
    let [path, version] = &l.words[..] else {
        problems.push(line_problem(
            l,
            &format!("usage: {verb} module/path v1.2.3"),
        ));
        return None;
    };
    if !is_valid_version(&version.text) {
        problems.push(problem(
            version,
            &format!(
                "invalid version '{}': expected a semantic version like v1.2.3",
                version.text
            ),
            "error",
        ));
    }
    Some(Require {
        path: path.clone(),
        version: Some(version.clone()),
        indirect: l
            .comment
            .as_deref()
            .is_some_and(|c| c == "indirect" || c.starts_with("indirect;")),
    })
}

/// `old [v] => new [v]`.
fn replace(l: &Line, problems: &mut Vec<ModProblem>) -> Option<Replace> {
    let arrow = l.words.iter().position(|w| w.text == "=>");
    let Some(arrow) = arrow else {
        problems.push(line_problem(
            l,
            "usage: replace module/path [v1.2.3] => other/path [v1.4.5]",
        ));
        return None;
    };
    let (left, right) = (&l.words[..arrow], &l.words[arrow + 1..]);
    if left.is_empty() || left.len() > 2 || right.is_empty() || right.len() > 2 {
        problems.push(line_problem(
            l,
            "usage: replace module/path [v1.2.3] => other/path [v1.4.5]",
        ));
        return None;
    }
    let r = Replace {
        old: left[0].clone(),
        old_version: left.get(1).cloned(),
        new: right[0].clone(),
        new_version: right.get(1).cloned(),
    };
    if !r.is_local() && r.new_version.is_none() {
        problems.push(problem(
            &r.new,
            "a replacement module needs a version (or use a local directory: ./path)",
            "error",
        ));
    }
    Some(r)
}

/// `go 1.22` / `go 1.22.3` / `go 1.23rc1`.
pub fn is_go_version(v: &str) -> bool {
    let core = v
        .split_once("rc")
        .or_else(|| v.split_once("beta"))
        .map(|(c, n)| (c, n.chars().all(|c| c.is_ascii_digit()) && !n.is_empty()))
        .map_or(Some(v), |(c, ok)| ok.then_some(c));
    let Some(core) = core else {
        return false;
    };
    let parts: Vec<&str> = core.split('.').collect();
    (2..=3).contains(&parts.len())
        && parts[0] == "1"
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

fn go_directive(d: &Directive, problems: &mut Vec<ModProblem>) -> Option<Word> {
    let l = d.entries.first()?;
    match &l.words[..] {
        [v] => {
            if !is_go_version(&v.text) {
                problems.push(problem(
                    v,
                    &format!("invalid go version '{}': must match format 1.23.0", v.text),
                    "error",
                ));
            }
            Some(v.clone())
        }
        _ => {
            problems.push(line_problem(l, "usage: go 1.23.0"));
            None
        }
    }
}

fn single(d: &Directive, problems: &mut Vec<ModProblem>, usage: &str) -> Option<Word> {
    let l = d.entries.first()?;
    match &l.words[..] {
        [v] => Some(v.clone()),
        _ => {
            problems.push(line_problem(l, usage));
            None
        }
    }
}

/// Parse a `go.mod` file.
pub fn parse_go_mod(text: &str) -> GoMod {
    let (dirs, mut problems) = directives(text);
    let mut m = GoMod::default();
    for d in &dirs {
        match d.verb.text.as_str() {
            "module" => {
                if m.module.is_some() {
                    problems.push(problem(&d.verb, "repeated module statement", "error"));
                }
                m.module = single(d, &mut problems, "usage: module module/path");
            }
            "go" => m.go = go_directive(d, &mut problems),
            "toolchain" => m.toolchain = single(d, &mut problems, "usage: toolchain go1.23.0"),
            "require" => {
                for l in &d.entries {
                    if let Some(r) = module_version(l, &mut problems, "require") {
                        if let Some(prev) = m.requires.iter().find(|p| p.path.text == r.path.text) {
                            if prev.version.as_ref().map(|v| &v.text)
                                == r.version.as_ref().map(|v| &v.text)
                            {
                                problems.push(problem(
                                    &r.path,
                                    &format!("{} is required twice", r.path.text),
                                    "warning",
                                ));
                            }
                        }
                        m.requires.push(r);
                    }
                }
            }
            "exclude" => {
                for l in &d.entries {
                    if let Some(r) = module_version(l, &mut problems, "exclude") {
                        m.excludes.push(r);
                    }
                }
            }
            "replace" => {
                for l in &d.entries {
                    if let Some(r) = replace(l, &mut problems) {
                        m.replaces.push(r);
                    }
                }
            }
            "retract" => {
                for l in &d.entries {
                    m.retracts.extend(l.words.iter().cloned());
                }
            }
            "tool" => {
                for l in &d.entries {
                    m.tools.extend(l.words.first().cloned());
                }
            }
            "godebug" | "ignore" => {}
            other => problems.push(problem(
                &d.verb,
                &format!("unknown directive: {other}"),
                "error",
            )),
        }
    }
    if m.module.is_none() && !text.trim().is_empty() {
        problems.push(ModProblem {
            line: 0,
            start: 0,
            end: 1,
            message: "no module declaration in go.mod".into(),
            severity: "error".into(),
        });
    }
    m.problems = problems;
    m
}

/// Parse a `go.work` file.
pub fn parse_go_work(text: &str) -> GoWork {
    let (dirs, mut problems) = directives(text);
    let mut w = GoWork::default();
    for d in &dirs {
        match d.verb.text.as_str() {
            "go" => w.go = go_directive(d, &mut problems),
            "toolchain" => w.toolchain = single(d, &mut problems, "usage: toolchain go1.23.0"),
            "use" => {
                for l in &d.entries {
                    match &l.words[..] {
                        [p] => w.uses.push(p.clone()),
                        _ => problems.push(line_problem(l, "usage: use ./module/dir")),
                    }
                }
            }
            "replace" => {
                for l in &d.entries {
                    if let Some(r) = replace(l, &mut problems) {
                        w.replaces.push(r);
                    }
                }
            }
            "godebug" => {}
            other => problems.push(problem(
                &d.verb,
                &format!("unknown directive: {other}"),
                "error",
            )),
        }
    }
    w.problems = problems;
    w
}

/// What the caret in a `go.mod` / `go.work` line completes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ModCompletion {
    /// A directive at the start of a line.
    Directive { prefix: String, from: u32 },
    /// A module path (require, exclude, replace, tool).
    ModulePath { prefix: String, from: u32 },
    /// A version of `module`.
    Version {
        module: String,
        prefix: String,
        from: u32,
    },
    /// The `go` / `toolchain` version.
    GoVersion { prefix: String, from: u32 },
}

pub const GO_MOD_DIRECTIVES: &[&str] = &[
    "module",
    "go",
    "toolchain",
    "godebug",
    "require",
    "replace",
    "exclude",
    "retract",
    "tool",
    "ignore",
];
pub const GO_WORK_DIRECTIVES: &[&str] = &["go", "toolchain", "godebug", "use", "replace"];

/// The completion context at `offset` (UTF-16, in the whole text).
pub fn completion_context(text: &str, offset: u32) -> Option<ModCompletion> {
    // Find the line and the column of the offset.
    let mut seen = 0u32;
    let mut block_verb: Option<String> = None;
    for (n, raw) in text.split('\n').enumerate() {
        let len = utf16_len(raw);
        if offset > seen + len {
            seen += len + 1;
            let l = split(n as u32, raw);
            match l.words.first().map(|w| w.text.as_str()) {
                Some(")") => block_verb = None,
                Some(v) if l.words.get(1).is_some_and(|w| w.text == "(") => {
                    block_verb = Some(v.to_string())
                }
                _ => {}
            }
            continue;
        }
        let col = offset - seen;
        let before: String = {
            let mut out = String::new();
            let mut c = 0;
            for ch in raw.chars() {
                if c >= col {
                    break;
                }
                c += ch.len_utf16() as u32;
                out.push(ch);
            }
            out
        };
        if before.contains("//") {
            return None;
        }
        let l = split(n as u32, &before);
        let ends_with_space = before.ends_with(char::is_whitespace) || before.is_empty();
        let mut words: Vec<Word> = l.words;
        let current = if ends_with_space { None } else { words.pop() };
        let prefix = current.as_ref().map(|w| w.text.clone()).unwrap_or_default();
        let from = current.as_ref().map_or(col, |w| w.start);
        // Inside a block, the block's verb leads the line.
        let mut verb = block_verb.clone();
        if verb.is_none() {
            if words.is_empty() {
                return Some(ModCompletion::Directive { prefix, from });
            }
            verb = Some(words.remove(0).text);
        }
        let verb = verb.unwrap_or_default();
        let args: Vec<&str> = words.iter().map(|w| w.text.as_str()).collect();
        return match verb.as_str() {
            "go" | "toolchain" if args.is_empty() => {
                Some(ModCompletion::GoVersion { prefix, from })
            }
            "require" | "exclude" | "tool" if args.is_empty() => {
                Some(ModCompletion::ModulePath { prefix, from })
            }
            "require" | "exclude" if args.len() == 1 => Some(ModCompletion::Version {
                module: args[0].to_string(),
                prefix,
                from,
            }),
            "replace" => match args.iter().position(|a| *a == "=>") {
                None if args.is_empty() => Some(ModCompletion::ModulePath { prefix, from }),
                None if args.len() == 1 => Some(ModCompletion::Version {
                    module: args[0].to_string(),
                    prefix,
                    from,
                }),
                Some(p) if args.len() == p + 1 => Some(ModCompletion::ModulePath { prefix, from }),
                Some(p) if args.len() == p + 2 => Some(ModCompletion::Version {
                    module: args[p + 1].to_string(),
                    prefix,
                    from,
                }),
                _ => None,
            },
            _ => None,
        };
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    pub const GO_MOD: &str = "module example.com/shop\n\ngo 1.23.0\n\ntoolchain go1.24.7\n\nrequire (\n\tgithub.com/google/uuid v1.6.0\n\tgolang.org/x/text v0.21.0 // indirect\n)\n\nrequire github.com/stretchr/testify v1.10.0\n\nreplace example.com/legacy => ./legacy\n\nreplace golang.org/x/net v0.1.0 => golang.org/x/net v0.30.0\n\nexclude golang.org/x/crypto v0.0.1\n\nretract [v1.0.0, v1.0.5] // broken\n";

    #[test]
    fn parses_go_mod_with_positions() {
        let m = parse_go_mod(GO_MOD);
        assert_eq!(m.module.as_ref().unwrap().text, "example.com/shop");
        assert_eq!(m.go.as_ref().unwrap().text, "1.23.0");
        assert_eq!(m.toolchain.as_ref().unwrap().text, "go1.24.7");
        let reqs: Vec<_> = m
            .requires
            .iter()
            .map(|r| {
                (
                    r.path.text.as_str(),
                    r.version.as_ref().unwrap().text.as_str(),
                    r.indirect,
                )
            })
            .collect();
        assert_eq!(
            reqs,
            [
                ("github.com/google/uuid", "v1.6.0", false),
                ("golang.org/x/text", "v0.21.0", true),
                ("github.com/stretchr/testify", "v1.10.0", false),
            ]
        );
        let uuid = &m.requires[0];
        assert_eq!((uuid.path.line, uuid.path.start, uuid.path.end), (7, 1, 23));
        assert_eq!(m.replaces.len(), 2);
        assert!(m.replaces[0].is_local());
        assert_eq!(m.replaces[1].new_version.as_ref().unwrap().text, "v0.30.0");
        assert_eq!(m.excludes[0].path.text, "golang.org/x/crypto");
        assert!(m.problems.is_empty(), "{:?}", m.problems);
    }

    #[test]
    fn reports_problems() {
        let m = parse_go_mod(
            "module a\ngo 1.x\nrequire b v1\nrequire c\nfrob x\nrequire (\n d v1.0.0\n",
        );
        let msgs: Vec<_> = m
            .problems
            .iter()
            .map(|p| (p.line, p.message.as_str()))
            .collect();
        assert!(msgs.contains(&(1, "invalid go version '1.x': must match format 1.23.0")));
        assert!(msgs
            .iter()
            .any(|(l, m)| *l == 2 && m.starts_with("invalid version 'v1'")));
        assert!(msgs.contains(&(3, "usage: require module/path v1.2.3")));
        assert!(msgs.contains(&(4, "unknown directive: frob")));
        assert!(msgs.contains(&(5, "missing closing parenthesis")));
        assert!(parse_go_mod("go 1.22\n").problems[0]
            .message
            .contains("no module"));
        assert!(is_go_version("1.22") && is_go_version("1.23rc1") && !is_go_version("2.0"));
    }

    #[test]
    fn parses_go_work() {
        let w = parse_go_work("go 1.23\n\nuse (\n\t./api\n\t./worker // jobs\n)\nuse ./tools\nreplace x.org/y => ../y\n");
        let uses: Vec<_> = w.uses.iter().map(|u| u.text.as_str()).collect();
        assert_eq!(uses, ["./api", "./worker", "./tools"]);
        assert_eq!(w.go.unwrap().text, "1.23");
        assert_eq!(w.replaces[0].new.text, "../y");
        assert!(w.problems.is_empty());
    }

    #[test]
    fn completion_contexts() {
        let text = "module a\n\ngo 1.2\nrequire (\n\tgithub.com/x/y v1.\n\tgolang.org/\n)\nreq";
        let at = |needle: &str| {
            let i = text.rfind(needle).unwrap() + needle.len();
            completion_context(text, utf16_len(&text[..i])).unwrap()
        };
        assert_eq!(
            at("github.com/x/y v1."),
            ModCompletion::Version {
                module: "github.com/x/y".into(),
                prefix: "v1.".into(),
                from: 16
            }
        );
        assert_eq!(
            at("golang.org/"),
            ModCompletion::ModulePath {
                prefix: "golang.org/".into(),
                from: 1
            }
        );
        assert_eq!(
            at("go 1.2"),
            ModCompletion::GoVersion {
                prefix: "1.2".into(),
                from: 3
            }
        );
        assert_eq!(
            at("\nreq"),
            ModCompletion::Directive {
                prefix: "req".into(),
                from: 0
            }
        );
        let r = "replace a.b/c => a.b/d v";
        assert_eq!(
            completion_context(r, utf16_len(r)),
            Some(ModCompletion::Version {
                module: "a.b/d".into(),
                prefix: "v".into(),
                from: 23
            })
        );
    }
}
