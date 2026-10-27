//! Compiler and linker messages of a build, for the Problems tool window's Build tab and the
//! editor: GCC and Clang (`file:line:col: error: …`, with their source excerpts and notes),
//! MSVC (`file(line,col): error C2065: …`), and linker errors of `ld`, `lld`, Apple's `ld`
//! and `link.exe`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    Error,
    Warning,
    Note,
}

/// One message of a build.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildProblem {
    /// The file as the tool printed it (absolute, relative to the build folder, or an
    /// object file); empty for messages without one.
    pub file: String,
    /// 1-based; 0 when unknown.
    pub line: u32,
    /// 1-based; 0 when unknown.
    pub column: u32,
    pub severity: Severity,
    pub message: String,
    /// `-Wunused-variable`, `C2065`, `LNK2019`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// `compiler` or `linker`.
    pub tool: String,
    /// Notes that explain it (candidates, the place of a declaration).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<BuildProblem>,
}

impl BuildProblem {
    fn new(
        file: &str,
        line: u32,
        column: u32,
        severity: Severity,
        message: &str,
        tool: &str,
    ) -> Self {
        Self {
            file: file.to_string(),
            line,
            column,
            severity,
            message: message.trim().to_string(),
            code: None,
            tool: tool.to_string(),
            notes: Vec::new(),
        }
    }
}

fn severity(word: &str) -> Option<Severity> {
    match word {
        "error" | "fatal error" => Some(Severity::Error),
        "warning" => Some(Severity::Warning),
        "note" => Some(Severity::Note),
        _ => None,
    }
}

fn number(s: &str) -> Option<u32> {
    (!s.is_empty() && s.chars().all(|c| c.is_ascii_digit())).then(|| s.parse().ok())?
}

/// `file:line:col: severity: message` or `file:line: severity: message` (GCC, Clang).
fn gnu(line: &str) -> Option<BuildProblem> {
    for sev in ["fatal error", "error", "warning", "note"] {
        let marker = format!(": {sev}: ");
        let Some(i) = line.find(&marker) else {
            continue;
        };
        let place = &line[..i];
        let message = &line[i + marker.len()..];
        // `C:\x.cpp:3:4` keeps its drive letter: split from the right.
        let mut parts = place.rsplitn(3, ':');
        let a = parts.next()?;
        let b = parts.next();
        let c = parts.next();
        let (file, line_no, col) = match (number(a), b.and_then(number), c) {
            (Some(col), Some(l), Some(f)) => (f, l, col),
            (Some(l), _, _) => (place.rsplit_once(':')?.0, l, 0),
            _ => continue,
        };
        let mut p = BuildProblem::new(file, line_no, col, severity(sev)?, message, "compiler");
        // ` [-Wunused-variable]` / ` [-Werror,-Wunused]`.
        if p.message.ends_with(']') {
            if let Some(start) = p.message.rfind(" [-W") {
                p.code = Some(p.message[start + 2..p.message.len() - 1].to_string());
                p.message.truncate(start);
            }
        }
        return Some(p);
    }
    None
}

/// `file(line,col): severity CODE: message` (MSVC).
fn msvc(line: &str) -> Option<BuildProblem> {
    let open = line.find('(')?;
    let close = open + line[open..].find("): ")?;
    let file = &line[..open];
    let place = &line[open + 1..close];
    let (l, c) = match place.split_once(',') {
        Some((l, c)) => (number(l)?, number(c)?),
        None => (number(place)?, 0),
    };
    let rest = &line[close + 3..];
    let (sev, rest) = ["fatal error", "error", "warning", "note"]
        .iter()
        .find_map(|s| rest.strip_prefix(s).map(|r| (*s, r)))?;
    let rest = rest.trim_start();
    let (code, message) = match rest.split_once(": ") {
        Some((code, m)) if code.chars().all(|c| c.is_ascii_alphanumeric()) => (Some(code), m),
        _ => (None, rest.trim_start_matches(':').trim_start()),
    };
    let mut p = BuildProblem::new(file, l, c, severity(sev)?, message, "compiler");
    p.code = code.map(str::to_string);
    Some(p)
}

/// `main.obj : error LNK2019: …` (link.exe).
fn msvc_linker(line: &str) -> Option<BuildProblem> {
    let (file, rest) = line.split_once(" : ")?;
    let (sev, rest) = ["fatal error", "error", "warning"]
        .iter()
        .find_map(|s| rest.strip_prefix(s).map(|r| (*s, r)))?;
    let (code, message) = rest.trim_start().split_once(": ")?;
    if !code.starts_with("LNK") {
        return None;
    }
    let mut p = BuildProblem::new(file, 0, 0, severity(sev)?, message, "linker");
    p.code = Some(code.to_string());
    Some(p)
}

/// GNU ld: `main.cpp:(.text+0x9): undefined reference to …`; lld: `ld.lld: error: …`;
/// Apple ld: `ld: symbol(s) not found for architecture arm64`.
fn gnu_linker(line: &str) -> Option<BuildProblem> {
    if let Some(i) = line.find(":(.") {
        let close = i + line[i..].find("): ")?;
        let file = &line[..i];
        if file.contains(' ') && !file.contains('/') && !file.contains('\\') {
            return None;
        }
        let message = &line[close + 3..];
        return Some(BuildProblem::new(
            file,
            0,
            0,
            Severity::Error,
            message,
            "linker",
        ));
    }
    for prefix in ["ld.lld: error: ", "ld: error: ", "lld-link: error: "] {
        if let Some(m) = line.strip_prefix(prefix) {
            return Some(BuildProblem::new("", 0, 0, Severity::Error, m, "linker"));
        }
    }
    if let Some(m) = line.strip_prefix("ld: ") {
        if m.contains("not found") || m.contains("undefined") {
            return Some(BuildProblem::new("", 0, 0, Severity::Error, m, "linker"));
        }
    }
    None
}

/// Lines that only repeat or frame other messages.
fn noise(line: &str) -> bool {
    let t = line.trim_start();
    t.is_empty()
        || t.starts_with('|')
        || t.split_once('|').is_some_and(|(n, _)| {
            n.trim().chars().all(|c| c.is_ascii_digit()) && !n.trim().is_empty()
        })
        || line.starts_with("FAILED: ")
        || line.starts_with("ninja: ")
        || line.starts_with("make: ")
        || line.starts_with("make[")
        || line.starts_with("collect2: ")
        || line.starts_with("gmake")
        || line.starts_with("In file included from ")
        || line.starts_with("                 from ")
        || line.ends_with(" errors generated.")
        || line.ends_with(" error generated.")
        || line.ends_with(" warnings generated.")
        || line.ends_with(" warning generated.")
}

/// Every problem of a build's output, in order; notes are attached to the message before
/// them. Duplicates (the same message printed for several targets) are dropped.
pub fn parse_build_output(text: &str) -> Vec<BuildProblem> {
    let mut out: Vec<BuildProblem> = Vec::new();
    for raw in text.lines() {
        let line = raw.trim_end_matches('\r');
        if noise(line) {
            continue;
        }
        let parsed = gnu(line)
            .or_else(|| msvc(line))
            .or_else(|| msvc_linker(line))
            .or_else(|| gnu_linker(line));
        let Some(p) = parsed else {
            continue;
        };
        if p.severity == Severity::Note {
            if let Some(last) = out.last_mut() {
                last.notes.push(p);
                continue;
            }
        }
        if !out.iter().any(|o| {
            o.file == p.file && o.line == p.line && o.column == p.column && o.message == p.message
        }) {
            out.push(p);
        }
    }
    out
}

/// A problem's file as a project path: absolute paths inside `root` become relative,
/// relative ones are resolved against the build folder `build_dir_rel` (project-relative)
/// when they do not exist from the root. `exists` checks project-relative paths.
pub fn project_path(
    file: &str,
    root: &str,
    build_dir_rel: &str,
    exists: &dyn Fn(&str) -> bool,
) -> Option<String> {
    if file.is_empty() {
        return None;
    }
    let f = file.replace('\\', "/");
    let root = root.replace('\\', "/");
    let root = root.trim_end_matches('/');
    if let Some(rel) = f.strip_prefix(root).and_then(|r| r.strip_prefix('/')) {
        return Some(rel.to_string());
    }
    if f.starts_with('/') || f.get(1..2) == Some(":") {
        // Outside the project (system headers): keep absolute.
        return Some(f);
    }
    if exists(&f) {
        return Some(f);
    }
    let mut parts: Vec<&str> = build_dir_rel.split('/').filter(|p| !p.is_empty()).collect();
    for p in f.split('/') {
        match p {
            ".." => {
                parts.pop();
            }
            "." | "" => {}
            other => parts.push(other),
        }
    }
    let joined = parts.join("/");
    exists(&joined).then_some(joined)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures")
                .join(name),
        )
        .unwrap()
    }

    fn summary(p: &[BuildProblem]) -> Vec<(String, u32, u32, Severity, String)> {
        p.iter()
            .map(|p| {
                (
                    p.file.clone(),
                    p.line,
                    p.column,
                    p.severity,
                    p.message.clone(),
                )
            })
            .collect()
    }

    #[test]
    fn gcc_errors_and_warnings() {
        let p = parse_build_output(&fixture("gcc-errors.txt"));
        assert_eq!(
            summary(&p),
            [
                ("broken.cpp".into(), 6, 18, Severity::Error, "'undefined_name' was not declared in this scope".into()),
                ("broken.cpp".into(), 4, 7, Severity::Warning, "unused variable 'unused'".into()),
                (
                    "broken.cpp".into(),
                    8,
                    28,
                    Severity::Error,
                    "conversion from 'int' to non-scalar type 'std::string' {aka 'std::__cxx11::basic_string<char>'} requested".into()
                ),
            ]
        );
        assert_eq!(p[1].code.as_deref(), Some("-Wunused-variable"));
        assert_eq!(p[0].tool, "compiler");
    }

    #[test]
    fn clang_errors_with_notes() {
        let p = parse_build_output(&fixture("clang-errors.txt"));
        assert_eq!(p.len(), 2);
        assert_eq!((p[0].line, p[0].column), (6, 18));
        assert_eq!(
            p[0].message,
            "use of undeclared identifier 'undefined_name'"
        );
        assert_eq!(
            p[1].message,
            "no viable conversion from 'int' to 'std::string' (aka 'basic_string<char>')"
        );
        assert_eq!(p[1].notes.len(), 6);
        assert!(p[1].notes[0].file.ends_with("bits/basic_string.h"));
        assert_eq!(p[1].notes[0].line, 547);
    }

    #[test]
    fn ninja_build_with_absolute_paths() {
        let p = parse_build_output(&fixture("ninja-build-failed.txt"));
        assert_eq!(
            summary(&p),
            [(
                "/home/dev/shapes/src/shapes.cpp".into(),
                11,
                28,
                Severity::Error,
                "'missing' was not declared in this scope".into()
            )]
        );
        let exists = |_: &str| false;
        assert_eq!(
            project_path(&p[0].file, "/home/dev/shapes", "build/debug", &exists).as_deref(),
            Some("src/shapes.cpp")
        );
    }

    #[test]
    fn linker_errors() {
        let p = parse_build_output(&fixture("gcc-link-error.txt"));
        assert_eq!(
            summary(&p),
            [(
                "link.cpp".into(),
                0,
                0,
                Severity::Error,
                "undefined reference to `helper()'".into()
            )]
        );
        assert_eq!(p[0].tool, "linker");
        let lld = parse_build_output(
            "ld.lld: error: undefined symbol: helper()\n>>> referenced by link.cpp\n",
        );
        assert_eq!(lld[0].message, "undefined symbol: helper()");
        let apple = parse_build_output("ld: symbol(s) not found for architecture arm64\nclang: error: linker command failed with exit code 1 (use -v to see invocation)\n");
        assert_eq!(apple.len(), 1);
    }

    #[test]
    fn msvc_messages() {
        let p = parse_build_output(&fixture("msvc-errors.txt"));
        assert_eq!(
            summary(&p),
            [
                ("C:\\dev\\shapes\\src\\shapes.cpp".into(), 11, 28, Severity::Error, "'missing': undeclared identifier".into()),
                ("C:\\dev\\shapes\\src\\shapes.cpp".into(), 4, 7, Severity::Warning, "'unused': unreferenced local variable".into()),
                (
                    "C:\\dev\\shapes\\src\\main.cpp".into(),
                    8,
                    0,
                    Severity::Error,
                    "'initializing': cannot convert from 'int' to 'std::basic_string<char,std::char_traits<char>,std::allocator<char>>'".into()
                ),
                ("main.obj".into(), 0, 0, Severity::Error, "unresolved external symbol \"int __cdecl helper(void)\" (?helper@@YAHXZ) referenced in function main".into()),
                ("C:\\dev\\shapes\\build\\shapes_app.exe".into(), 0, 0, Severity::Error, "1 unresolved externals".into()),
            ]
        );
        assert_eq!(p[0].code.as_deref(), Some("C2065"));
        assert_eq!(p[2].notes.len(), 1);
        assert_eq!(p[3].code.as_deref(), Some("LNK2019"));
        assert_eq!(
            project_path(&p[0].file, "C:\\dev\\shapes", "build", &|_| false).as_deref(),
            Some("src/shapes.cpp")
        );
    }

    #[test]
    fn relative_files_resolve_against_the_build_folder() {
        let exists = |p: &str| p == "src/a.cpp";
        assert_eq!(
            project_path("../src/a.cpp", "/p", "build", &exists).as_deref(),
            Some("src/a.cpp")
        );
        assert_eq!(
            project_path("src/a.cpp", "/p", "build", &exists).as_deref(),
            Some("src/a.cpp")
        );
        assert_eq!(
            project_path("/usr/include/x.h", "/p", "build", &exists).as_deref(),
            Some("/usr/include/x.h")
        );
        assert_eq!(project_path("", "/p", "build", &exists), None);
    }
}
