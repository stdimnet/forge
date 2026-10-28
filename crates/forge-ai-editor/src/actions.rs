//! AI actions in the editor (JetBrains AI Assistant style): "Explain with AI", "Fix with AI"
//! (problems at the caret), "Generate Documentation with AI" and "Generate Tests with AI".
//! [`prompt`] builds the request for the code at the caret; [`proposal`] turns the model's
//! reply into a change with a diff preview that the user applies or discards.

use forge_agent_cli::diff::{text_diff, EditDiff};
use serde::{Deserialize, Serialize};

use crate::lang::{language_of, DocStyle, Language};
use crate::text::{
    byte_to_utf16, extract_code, indent_of, indent_width, line_starts, utf16_to_byte,
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ActionKind {
    #[default]
    Explain,
    Fix,
    Docs,
    Tests,
}

/// A problem at the caret (from a language server or an inspection).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    pub message: String,
    /// 0-based line.
    pub line: usize,
    #[serde(default)]
    pub end_line: Option<usize>,
    #[serde(default)]
    pub source: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionRequest {
    pub kind: ActionKind,
    /// Project-relative path.
    pub path: String,
    #[serde(default)]
    pub language: Option<String>,
    pub text: String,
    /// Selection (caret when equal), UTF-16.
    pub from: usize,
    pub to: usize,
    #[serde(default)]
    pub problems: Vec<Problem>,
    /// Tests: the current content of the test file, when it exists (filled in by the app).
    #[serde(default)]
    pub existing_test: Option<String>,
}

/// The request to send for an action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionPrompt {
    pub title: String,
    pub system: String,
    pub user: String,
    /// 0-based first and last line of the code the action is about.
    pub start_line: usize,
    pub end_line: usize,
    /// Tests: the file the tests go to (project-relative).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub test_path: Option<String>,
}

/// A replacement in the requesting file, UTF-16 offsets into its text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextEdit {
    pub from: usize,
    pub to: usize,
    pub text: String,
}

/// A change proposed by the model, shown as a diff before anything is applied.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Proposal {
    pub kind: ActionKind,
    pub title: String,
    /// The file that changes.
    pub path: String,
    pub original: String,
    pub proposed: String,
    /// Set when `path` is the requesting file: the minimal edit to apply in the editor.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edit: Option<TextEdit>,
    /// `path` does not exist yet.
    pub new_file: bool,
    pub diff: EditDiff,
}

const MAX_REGION_LINES: usize = 150;
const CONTEXT_LINES: usize = 15;

fn lines_of(text: &str) -> Vec<&str> {
    text.split('\n').collect()
}

fn brace_language(lang: Language) -> bool {
    !matches!(
        lang,
        Language::Python
            | Language::Lua
            | Language::Ruby
            | Language::Yaml
            | Language::Markdown
            | Language::Other
    )
}

/// The nearest declaration at or above `line` that encloses it.
fn declaration_at(lines: &[&str], lang: Language, line: usize) -> Option<usize> {
    let line = line.min(lines.len().saturating_sub(1));
    let here = indent_width(indent_of(lines[line]));
    let mut limit = if lines[line].trim().is_empty() {
        usize::MAX
    } else {
        here
    };
    for i in (line.saturating_sub(300)..=line).rev() {
        let l = lines[i];
        if l.trim().is_empty() {
            continue;
        }
        let w = indent_width(indent_of(l));
        if lang.is_declaration(l) && (i == line || w <= limit) && block_end(lines, lang, i) >= line
        {
            return Some(i);
        }
        limit = limit.min(w);
    }
    None
}

/// Last line of the block that starts at `decl`.
fn block_end(lines: &[&str], lang: Language, decl: usize) -> usize {
    let last = (decl + MAX_REGION_LINES).min(lines.len() - 1);
    if brace_language(lang) {
        let mut depth = 0i32;
        let mut opened = false;
        for (i, l) in lines.iter().enumerate().take(last + 1).skip(decl) {
            for c in strip_strings(l).chars() {
                match c {
                    '{' => {
                        depth += 1;
                        opened = true;
                    }
                    '}' => depth -= 1,
                    _ => {}
                }
            }
            if opened && depth <= 0 {
                return i;
            }
            if !opened && l.trim_end().ends_with(';') {
                return i;
            }
        }
        if opened {
            return last;
        }
    }
    let base = indent_width(indent_of(lines[decl]));
    let mut end = decl;
    for (i, l) in lines.iter().enumerate().take(last + 1).skip(decl + 1) {
        if l.trim().is_empty() {
            continue;
        }
        let w = indent_width(indent_of(l));
        if w > base {
            end = i;
        } else {
            let t = l.trim();
            if w == base && (t == "end" || t.starts_with("end ") || t.starts_with('}')) {
                end = i;
            }
            break;
        }
    }
    end
}

/// The line without string literals and line comments (for brace counting).
fn strip_strings(line: &str) -> String {
    let mut out = String::new();
    let mut quote: Option<char> = None;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match quote {
            Some(q) => {
                if c == '\\' {
                    chars.next();
                } else if c == q {
                    quote = None;
                }
            }
            None => match c {
                '"' | '\'' | '`' => quote = Some(c),
                '/' if chars.peek() == Some(&'/') => break,
                '#' if out.trim().is_empty() && chars.peek() != Some(&'[') => break,
                _ => out.push(c),
            },
        }
    }
    out
}

/// 0-based first and last line the action is about: the selection, else the declaration
/// around the caret, else a few lines around it.
pub fn focus_region(text: &str, lang: Language, from: usize, to: usize) -> (usize, usize) {
    let lines = lines_of(text);
    let a = utf16_to_byte(text, from.min(to));
    let b = utf16_to_byte(text, from.max(to));
    let la = crate::text::line_of(text, a);
    if a != b {
        let mut lb = crate::text::line_of(text, b);
        // A selection ending at the start of a line does not include that line.
        if lb > la && line_starts(text)[lb] == b {
            lb -= 1;
        }
        return (la, lb);
    }
    match declaration_at(&lines, lang, la) {
        Some(d) => (d, block_end(&lines, lang, d)),
        None => (
            la.saturating_sub(10),
            (la + 10).min(lines.len().saturating_sub(1)),
        ),
    }
}

fn slice_lines(text: &str, start: usize, end: usize) -> String {
    let lines = lines_of(text);
    let end = end.min(lines.len().saturating_sub(1));
    if start > end {
        return String::new();
    }
    lines[start..=end].join("\n")
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn split_name(path: &str) -> (&str, &str, &str) {
    let (dir, name) = match path.rfind('/') {
        Some(i) => (&path[..i], &path[i + 1..]),
        None => ("", path),
    };
    let (stem, ext) = name.rsplit_once('.').unwrap_or((name, ""));
    (dir, stem, ext)
}

fn join(dir: &str, name: &str) -> String {
    if name.is_empty() {
        dir.to_string()
    } else if dir.is_empty() {
        name.to_string()
    } else {
        format!("{dir}/{name}")
    }
}

fn upper_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

/// Where tests for `path` go, following each ecosystem's convention. Rust keeps unit
/// tests in the same file.
pub fn test_path(path: &str, lang: Language) -> String {
    let (dir, stem, ext) = split_name(path);
    match lang {
        Language::Php => {
            let rest = dir
                .split('/')
                .skip_while(|d| matches!(*d, "src" | "app" | "lib"))
                .collect::<Vec<_>>()
                .join("/");
            join(
                &join("tests/Unit", &rest),
                &format!("{}Test.php", upper_first(stem)),
            )
        }
        Language::JavaScript | Language::TypeScript => {
            let ext = match ext {
                "vue" | "svelte" | "mts" | "cts" => "ts",
                e => e,
            };
            join(dir, &format!("{stem}.test.{ext}"))
        }
        Language::Lua => format!("spec/{stem}_spec.lua"),
        Language::Python => format!("tests/test_{stem}.py"),
        Language::Go => join(dir, &format!("{stem}_test.go")),
        Language::Rust => path.to_string(),
        Language::Java | Language::Kotlin | Language::Scala | Language::Groovy => {
            let dir = if dir.contains("src/main/") {
                dir.replacen("src/main/", "src/test/", 1)
            } else {
                dir.to_string()
            };
            join(&dir, &format!("{stem}Test.{ext}"))
        }
        Language::CSharp => join(dir, &format!("{stem}Tests.cs")),
        Language::C | Language::Cpp => {
            format!("tests/{stem}_test.{}", if ext == "c" { "c" } else { "cpp" })
        }
        Language::Ruby => format!("spec/{stem}_spec.rb"),
        _ => join(dir, &format!("{stem}.test.{ext}")),
    }
}

fn fenced(lang: Language, code: &str) -> String {
    format!("```{}\n{code}\n```", lang.id())
}

/// The request for an action; `Err` with a message for the user when it does not apply.
pub fn prompt(req: &ActionRequest) -> Result<ActionPrompt, String> {
    let lang = language_of(&req.path, req.language.as_deref());
    let text = req.text.replace("\r\n", "\n");
    let lines = lines_of(&text);
    let (mut start, mut end) = focus_region(&text, lang, req.from, req.to);
    let name = file_name(&req.path);
    let mut test_target = None;
    let (title, system, user) = match req.kind {
        ActionKind::Explain => {
            let code = slice_lines(&text, start, end);
            (
                format!("Explain code in {name}:{}-{}", start + 1, end + 1),
                format!(
                    "You are an expert {} developer. Explain code clearly and concisely in Markdown: what it does, \
how it does it, and anything notable such as edge cases, possible bugs or performance issues. Use short \
paragraphs and lists; do not repeat the whole code.",
                    lang.name()
                ),
                format!(
                    "Explain the following code from `{}` (lines {}-{}):\n\n{}",
                    req.path,
                    start + 1,
                    end + 1,
                    fenced(lang, &code)
                ),
            )
        }
        ActionKind::Fix => {
            let Some(first) = req.problems.first() else {
                return Err("No problems at the caret".into());
            };
            for p in &req.problems {
                start = start.min(p.line);
                end = end.max(p.end_line.unwrap_or(p.line)).min(lines.len() - 1);
            }
            if end - start >= MAX_REGION_LINES {
                start = first.line.saturating_sub(10);
                end = (first.line + 10).min(lines.len() - 1);
            }
            let problems: String = req
                .problems
                .iter()
                .map(|p| {
                    let src = p
                        .source
                        .as_deref()
                        .map(|s| format!(" ({s})"))
                        .unwrap_or_default();
                    format!("- line {}: {}{src}\n", p.line + 1, p.message)
                })
                .collect();
            let before = slice_lines(
                &text,
                start.saturating_sub(CONTEXT_LINES),
                start.wrapping_sub(1),
            );
            let after = slice_lines(&text, end + 1, end + CONTEXT_LINES);
            let mut user = format!("File `{}`.\n\nProblems:\n{problems}\n", req.path);
            if start > 0 && !before.is_empty() {
                user.push_str(&format!(
                    "Code before the region (do not repeat it):\n{}\n\n",
                    fenced(lang, &before)
                ));
            }
            user.push_str(&format!(
                "Region to fix (lines {}-{}):\n{}\n",
                start + 1,
                end + 1,
                fenced(lang, &slice_lines(&text, start, end))
            ));
            if !after.is_empty() {
                user.push_str(&format!(
                    "\nCode after the region (do not repeat it):\n{}\n",
                    fenced(lang, &after)
                ));
            }
            (
                format!("Fix with AI: {}", first.message),
                format!(
                    "You fix problems in {} code. Reply with the corrected code of the region only, in one fenced \
code block. Change only what is needed to fix the problems; keep the formatting, indentation, comments and names.",
                    lang.name()
                ),
                user,
            )
        }
        ActionKind::Docs => {
            let caret = crate::text::line_of(&text, utf16_to_byte(&text, req.from));
            let decl = declaration_at(&lines, lang, caret)
                .ok_or_else(|| "No declaration at the caret to document".to_string())?;
            if lang.doc_style().is_none() {
                return Err(format!(
                    "Documentation comments are not supported for {}",
                    lang.name()
                ));
            }
            start = decl;
            end = block_end(&lines, lang, decl).min(decl + 60);
            let only = if lang.doc_style() == Some(DocStyle::Docstring) {
                "Reply with only the docstring (with its triple quotes)"
            } else {
                "Reply with only the documentation comment"
            };
            (
                "Generate Documentation with AI".to_string(),
                format!(
                    "You write {} for {} code. {only}, in one fenced code block, without the declaration itself. \
Describe the purpose, the parameters, the return value and thrown errors, using the tags of the format \
(such as @param, @return and @throws) where it has them. Be concise.",
                    lang.doc_name(),
                    lang.name()
                ),
                format!(
                    "Document the declaration on the first line of this code from `{}`:\n\n{}",
                    req.path,
                    fenced(lang, &slice_lines(&text, start, end))
                ),
            )
        }
        ActionKind::Tests => {
            let target = test_path(&req.path, lang);
            let code = slice_lines(&text, start, end);
            let mut user = format!(
                "Write unit tests for the following code from `{}` (lines {}-{}):\n\n{}\n",
                req.path,
                start + 1,
                end + 1,
                fenced(lang, &code)
            );
            if start > 0 {
                let head: Vec<&str> = lines.iter().take(start.min(40)).copied().collect();
                user.push_str(&format!(
                    "\nThe file starts with (for imports and namespaces):\n{}\n",
                    fenced(lang, &head.join("\n"))
                ));
            }
            let reply_rule = if lang == Language::Rust {
                "Reply with only a `#[cfg(test)] mod tests { ... }` module in one fenced code block; it is appended to the file."
                    .to_string()
            } else {
                match req.existing_test.as_deref() {
                    Some(existing) => {
                        user.push_str(&format!(
                            "\nThe test file `{target}` already exists; keep its tests and add the new ones:\n{}\n",
                            fenced(lang, existing)
                        ));
                        "Reply with the complete updated test file in one fenced code block.".to_string()
                    }
                    None => format!(
                        "Reply with the complete content of the new test file `{target}` in one fenced code block."
                    ),
                }
            };
            test_target = Some(target);
            (
                "Generate Tests with AI".to_string(),
                format!(
                    "You write focused, runnable unit tests for {} code using {}. Cover the main behaviour and \
edge cases. {reply_rule}",
                    lang.name(),
                    lang.test_framework()
                ),
                user,
            )
        }
    };
    Ok(ActionPrompt {
        title,
        system,
        user,
        start_line: start,
        end_line: end,
        test_path: test_target,
    })
}

/// "Explain with AI" (streamed into the AI Chat, no proposal).
pub fn explain_prompt(req: &ActionRequest) -> Result<ActionPrompt, String> {
    prompt(&ActionRequest {
        kind: ActionKind::Explain,
        ..req.clone()
    })
}

/// Byte range of lines `start..=end` (without the last line's newline).
fn line_range(text: &str, start: usize, end: usize) -> (usize, usize) {
    let starts = line_starts(text);
    let a = starts[start.min(starts.len() - 1)];
    let b = starts.get(end + 1).map(|s| s - 1).unwrap_or(text.len());
    (a, b.max(a))
}

fn min_indent(text: &str) -> usize {
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| indent_width(indent_of(l)))
        .min()
        .unwrap_or(0)
}

/// The corrected region from a "Fix with AI" reply, re-indented to the region's level when
/// the model dropped the common indentation.
pub fn region_reply(region: &str, reply: &str) -> String {
    let code = extract_code(reply);
    let want = min_indent(region);
    let have = min_indent(&code);
    if have >= want {
        return code;
    }
    let unit = region
        .lines()
        .find(|l| !l.trim().is_empty())
        .map(|l| {
            if indent_of(l).starts_with('\t') {
                "\t"
            } else {
                " "
            }
        })
        .unwrap_or(" ");
    let pad = if unit == "\t" {
        "\t".repeat((want - have) / 4)
    } else {
        " ".repeat(want - have)
    };
    code.split('\n')
        .map(|l| {
            if l.trim().is_empty() {
                l.to_string()
            } else {
                format!("{pad}{l}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn is_comment_line(lang: Language, t: &str) -> bool {
    match lang.doc_style() {
        Some(DocStyle::Block) => t.starts_with("/**") || t.starts_with('*') || t.starts_with("*/"),
        Some(DocStyle::TripleSlash) => t.starts_with("///"),
        Some(DocStyle::TripleDash) => t.starts_with("---"),
        Some(DocStyle::LineSlash) => t.starts_with("//"),
        Some(DocStyle::Hash) => t.starts_with('#') && !t.starts_with("#!"),
        _ => false,
    }
}

fn is_attribute_line(lang: Language, t: &str) -> bool {
    match lang {
        Language::Rust | Language::Php => t.starts_with("#["),
        Language::CSharp => t.starts_with('['),
        Language::Java
        | Language::Kotlin
        | Language::Scala
        | Language::Groovy
        | Language::TypeScript
        | Language::JavaScript => t.starts_with('@'),
        _ => false,
    }
}

/// The doc comment from the reply, in the language's style, indented like the declaration.
fn doc_comment(lang: Language, reply: &str, indent: &str) -> String {
    let code = extract_code(reply);
    let lines: Vec<&str> = code.lines().map(str::trim).collect();
    let (prefix, wrap) = match lang.doc_style() {
        Some(DocStyle::Block) => {
            if lines.first().is_some_and(|l| l.starts_with("/**")) {
                let out: Vec<String> = lines
                    .iter()
                    .map(|l| {
                        if l.starts_with("/**") {
                            format!("{indent}{l}")
                        } else if l.starts_with('*') {
                            format!("{indent} {l}")
                        } else {
                            format!("{indent} * {l}")
                        }
                    })
                    .collect();
                return out.join("\n");
            }
            ("", true)
        }
        Some(DocStyle::TripleSlash) => ("///", false),
        Some(DocStyle::TripleDash) => ("---", false),
        Some(DocStyle::LineSlash) => ("//", false),
        Some(DocStyle::Hash) => ("#", false),
        _ => ("", false),
    };
    if wrap {
        let mut out = vec![format!("{indent}/**")];
        out.extend(lines.iter().map(|l| {
            let l = l.trim_start_matches('*').trim();
            if l.is_empty() {
                format!("{indent} *")
            } else {
                format!("{indent} * {l}")
            }
        }));
        out.push(format!("{indent} */"));
        return out.join("\n");
    }
    lines
        .iter()
        .map(|l| {
            if !prefix.is_empty() && l.starts_with(prefix) {
                return format!("{indent}{l}");
            }
            let body = l.trim();
            if body.is_empty() {
                format!("{indent}{prefix}")
            } else {
                format!("{indent}{prefix} {body}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The docstring from the reply, indented for the body.
fn docstring(reply: &str, indent: &str) -> String {
    let code = extract_code(reply);
    let code = code.trim();
    let quoted = if code.starts_with("\"\"\"") || code.starts_with("'''") {
        code.to_string()
    } else {
        format!("\"\"\"{code}\"\"\"")
    };
    let lines: Vec<&str> = quoted.lines().collect();
    let base = lines
        .iter()
        .skip(1)
        .filter(|l| !l.trim().is_empty())
        .map(|l| indent_of(l).len())
        .min()
        .unwrap_or(0);
    lines
        .iter()
        .enumerate()
        .map(|(i, l)| {
            if l.trim().is_empty() {
                String::new()
            } else if i == 0 {
                format!("{indent}{}", l.trim())
            } else {
                format!("{indent}{}", &l[base.min(indent_of(l).len())..])
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Byte range to replace and the new text for a documentation reply.
fn docs_edit(text: &str, lang: Language, decl: usize, reply: &str) -> (usize, usize, String) {
    let lines = lines_of(text);
    let starts = line_starts(text);
    let indent = indent_of(lines[decl]);
    if lang.doc_style() == Some(DocStyle::Docstring) {
        // After the signature (which may span lines), as the first statement of the body.
        let sig_end = (decl..lines.len())
            .find(|&i| strip_strings(lines[i]).trim_end().ends_with(':'))
            .unwrap_or(decl);
        let body = (sig_end + 1..lines.len()).find(|&i| !lines[i].trim().is_empty());
        let body_indent = body
            .map(|i| indent_of(lines[i]).to_string())
            .filter(|b| b.len() > indent.len())
            .unwrap_or_else(|| format!("{indent}    "));
        let doc = docstring(reply, &body_indent);
        let at = starts.get(sig_end + 1).copied().unwrap_or(text.len());
        if let Some(b) = body {
            let t = lines[b].trim_start();
            if t.starts_with("\"\"\"") || t.starts_with("'''") {
                let q = &t[..3];
                let close = if t.len() > 3 && t[3..].contains(q) {
                    b
                } else {
                    (b + 1..lines.len())
                        .find(|&i| lines[i].contains(q))
                        .unwrap_or(b)
                };
                let (a, z) = line_range(text, b, close);
                return (a, z, doc);
            }
        }
        let sep = if at == text.len() && !text.ends_with('\n') {
            "\n"
        } else {
            ""
        };
        return (at, at, format!("{sep}{doc}\n"));
    }
    // Above the declaration and its attributes; an existing doc comment is replaced.
    let mut top = decl;
    while top > 0 && is_attribute_line(lang, lines[top - 1].trim()) {
        top -= 1;
    }
    let mut doc_top = top;
    while doc_top > 0 && is_comment_line(lang, lines[doc_top - 1].trim()) {
        doc_top -= 1;
    }
    let doc = doc_comment(lang, reply, indent);
    if doc_top < top {
        let (a, z) = line_range(text, doc_top, top - 1);
        (a, z, doc)
    } else {
        let at = starts[top];
        (at, at, format!("{doc}\n"))
    }
}

fn same_file_proposal(
    req: &ActionRequest,
    title: String,
    text: &str,
    a: usize,
    z: usize,
    new: String,
) -> Proposal {
    let mut proposed = String::with_capacity(text.len() + new.len());
    proposed.push_str(&text[..a]);
    proposed.push_str(&new);
    proposed.push_str(&text[z..]);
    Proposal {
        kind: req.kind,
        title,
        path: req.path.clone(),
        diff: text_diff(text, &proposed),
        original: text.to_string(),
        proposed,
        edit: Some(TextEdit {
            from: byte_to_utf16(text, a),
            to: byte_to_utf16(text, z),
            text: new,
        }),
        new_file: false,
    }
}

/// The change a reply proposes. `Err` with a message when the reply is unusable.
pub fn proposal(req: &ActionRequest, reply: &str) -> Result<Proposal, String> {
    let lang = language_of(&req.path, req.language.as_deref());
    let p = prompt(req)?;
    let text = req.text.replace("\r\n", "\n");
    if extract_code(reply).trim().is_empty() {
        return Err("The AI reply contained no code".into());
    }
    match req.kind {
        ActionKind::Explain => Err("Explain with AI shows its answer in the AI Chat".into()),
        ActionKind::Fix => {
            let (a, z) = line_range(&text, p.start_line, p.end_line);
            let region = &text[a..z];
            let fixed = region_reply(region, reply);
            if fixed.trim_end() == region.trim_end() {
                return Err("The AI proposed no changes".into());
            }
            Ok(same_file_proposal(req, p.title, &text, a, z, fixed))
        }
        ActionKind::Docs => {
            let (a, z, doc) = docs_edit(&text, lang, p.start_line, reply);
            Ok(same_file_proposal(req, p.title, &text, a, z, doc))
        }
        ActionKind::Tests => {
            let code = extract_code(reply);
            if lang == Language::Rust {
                let trimmed = text.trim_end().len();
                let new = format!("\n\n{}\n", code.trim_end());
                return Ok(same_file_proposal(
                    req,
                    p.title,
                    &text,
                    trimmed,
                    text.len(),
                    new,
                ));
            }
            let original = req.existing_test.clone().unwrap_or_default();
            let proposed = format!("{}\n", code.trim_end());
            Ok(Proposal {
                kind: req.kind,
                title: p.title,
                path: p.test_path.unwrap_or_default(),
                diff: text_diff(&original, &proposed),
                new_file: req.existing_test.is_none(),
                original,
                proposed,
                edit: None,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PHP: &str = "<?php\n\nnamespace App\\Services;\n\nclass Checkout\n{\n    #[Pure]\n    public function total(array $items): int\n    {\n        $sum = 0;\n        foreach ($items as $i) {\n            $sum += $i->price;\n        }\n        return $sum\n    }\n}\n";

    fn at(text: &str, needle: &str) -> usize {
        text[..text.find(needle).unwrap()].encode_utf16().count()
    }

    fn req(kind: ActionKind, path: &str, text: &str, caret: usize) -> ActionRequest {
        ActionRequest {
            kind,
            path: path.into(),
            text: text.into(),
            from: caret,
            to: caret,
            ..ActionRequest::default()
        }
    }

    #[test]
    fn region_is_the_declaration_around_the_caret_or_the_selection() {
        let c = at(PHP, "$sum += ");
        assert_eq!(focus_region(PHP, Language::Php, c, c), (7, 14));
        let a = at(PHP, "        $sum = 0;");
        let b = at(PHP, "        foreach");
        assert_eq!(focus_region(PHP, Language::Php, a, b), (9, 9));
        let py = "class A:\n    def f(self, x):\n        y = x * 2\n        return y\n\n    def g(self):\n        pass\n";
        let c = at(py, "return y");
        assert_eq!(focus_region(py, Language::Python, c, c), (1, 3));
        let lua = "local function add(a, b)\n  return a + b\nend\nprint(add(1, 2))\n";
        let c = at(lua, "return");
        assert_eq!(focus_region(lua, Language::Lua, c, c), (0, 2));
    }

    #[test]
    fn explain_prompt_names_file_and_lines() {
        let p = explain_prompt(&req(
            ActionKind::Fix,
            "app/Services/Checkout.php",
            PHP,
            at(PHP, "$sum += "),
        ))
        .unwrap();
        assert_eq!(p.title, "Explain code in Checkout.php:8-15");
        assert!(p.user.contains("```php\n    public function total"));
        assert!(p.system.contains("PHP developer"));
    }

    #[test]
    fn fix_replaces_the_region_and_keeps_indentation() {
        let mut r = req(
            ActionKind::Fix,
            "app/Services/Checkout.php",
            PHP,
            at(PHP, "return $sum"),
        );
        assert_eq!(prompt(&r).unwrap_err(), "No problems at the caret");
        r.problems = vec![Problem {
            message: "Syntax error: missing ';'".into(),
            line: 13,
            end_line: None,
            source: Some("intelephense".into()),
        }];
        let p = prompt(&r).unwrap();
        assert_eq!(p.title, "Fix with AI: Syntax error: missing ';'");
        assert!(p
            .user
            .contains("- line 14: Syntax error: missing ';' (intelephense)"));
        assert!(p.user.contains("Region to fix (lines 8-15)"));
        // The model dropped the class-level indentation.
        let reply = "Here you go:\n```php\npublic function total(array $items): int\n{\n    $sum = 0;\n    foreach ($items as $i) {\n        $sum += $i->price;\n    }\n    return $sum;\n}\n```";
        let prop = proposal(&r, reply).unwrap();
        assert!(
            prop.proposed.contains("        return $sum;\n    }\n}\n"),
            "{}",
            prop.proposed
        );
        assert_eq!((prop.diff.added, prop.diff.removed), (1, 1));
        let e = prop.edit.unwrap();
        let mut applied = PHP.to_string();
        applied.replace_range(
            utf16_to_byte(PHP, e.from)..utf16_to_byte(PHP, e.to),
            &e.text,
        );
        assert_eq!(applied, prop.proposed);
        let same = format!("```php\n{}\n```", slice_lines(PHP, 7, 14));
        assert_eq!(
            proposal(&r, &same).unwrap_err(),
            "The AI proposed no changes"
        );
    }

    #[test]
    fn docs_go_above_attributes_and_replace_existing_comments() {
        let r = req(
            ActionKind::Docs,
            "app/Services/Checkout.php",
            PHP,
            at(PHP, "total("),
        );
        let p = prompt(&r).unwrap();
        assert!(p.system.contains("PHPDoc"));
        let prop = proposal(&r, "```php\n/**\n * Sum of the item prices.\n *\n * @param array $items\n * @return int\n */\n```").unwrap();
        assert!(prop.proposed.contains("{\n    /**\n     * Sum of the item prices.\n     *\n     * @param array $items\n     * @return int\n     */\n    #[Pure]\n    public function total"), "{}", prop.proposed);
        // Running it again replaces the comment instead of adding a second one.
        let again = req(
            ActionKind::Docs,
            "a.php",
            &prop.proposed,
            at(&prop.proposed, "total("),
        );
        let twice = proposal(&again, "Sum.").unwrap();
        assert!(
            twice
                .proposed
                .contains("{\n    /**\n     * Sum.\n     */\n    #[Pure]"),
            "{}",
            twice.proposed
        );
        assert_eq!(twice.proposed.matches("/**").count(), 1);
    }

    #[test]
    fn docs_in_other_styles() {
        let rs = "impl A {\n    pub fn add(a: i32) -> i32 {\n        a\n    }\n}\n";
        let r = req(ActionKind::Docs, "src/lib.rs", rs, at(rs, "a\n"));
        let p = proposal(&r, "Adds one.\n\n# Panics\nNever.").unwrap();
        assert!(
            p.proposed.contains(
                "    /// Adds one.\n    ///\n    /// # Panics\n    /// Never.\n    pub fn add"
            ),
            "{}",
            p.proposed
        );
        let py = "def area(r):\n    return 3.14 * r * r\n";
        let r = req(ActionKind::Docs, "geo.py", py, at(py, "return"));
        let p = proposal(
            &r,
            "```python\n\"\"\"Area of a circle.\n\nArgs:\n    r: radius\n\"\"\"\n```",
        )
        .unwrap();
        assert_eq!(p.proposed, "def area(r):\n    \"\"\"Area of a circle.\n\n    Args:\n        r: radius\n    \"\"\"\n    return 3.14 * r * r\n");
        let lua = "local function add(a, b)\n  return a + b\nend\n";
        let r = req(ActionKind::Docs, "m.lua", lua, at(lua, "return"));
        let p = proposal(&r, "--- Adds.\n---@param a number").unwrap();
        assert!(
            p.proposed
                .starts_with("--- Adds.\n---@param a number\nlocal function"),
            "{}",
            p.proposed
        );
        let none = req(ActionKind::Docs, "m.lua", "print(1)\n", 0);
        assert_eq!(
            prompt(&none).unwrap_err(),
            "No declaration at the caret to document"
        );
    }

    #[test]
    fn test_paths_follow_conventions() {
        assert_eq!(
            test_path("app/Services/Checkout.php", Language::Php),
            "tests/Unit/Services/CheckoutTest.php"
        );
        assert_eq!(
            test_path("app/Cart.php", Language::Php),
            "tests/Unit/CartTest.php"
        );
        assert_eq!(
            test_path("src/utils/math.ts", Language::TypeScript),
            "src/utils/math.test.ts"
        );
        assert_eq!(
            test_path("src/App.vue", Language::TypeScript),
            "src/App.test.ts"
        );
        assert_eq!(
            test_path("lib/geo.py", Language::Python),
            "tests/test_geo.py"
        );
        assert_eq!(
            test_path("pkg/cart/cart.go", Language::Go),
            "pkg/cart/cart_test.go"
        );
        assert_eq!(
            test_path("src/main/java/com/acme/Cart.java", Language::Java),
            "src/test/java/com/acme/CartTest.java"
        );
        assert_eq!(test_path("src/lib.rs", Language::Rust), "src/lib.rs");
        assert_eq!(
            test_path("inventory.lua", Language::Lua),
            "spec/inventory_spec.lua"
        );
    }

    #[test]
    fn tests_create_or_update_the_test_file() {
        let r = req(
            ActionKind::Tests,
            "app/Services/Checkout.php",
            PHP,
            at(PHP, "total("),
        );
        let p = prompt(&r).unwrap();
        assert_eq!(
            p.test_path.as_deref(),
            Some("tests/Unit/Services/CheckoutTest.php")
        );
        assert!(p.system.contains("PHPUnit"));
        assert!(p.user.contains("The file starts with"));
        let prop = proposal(&r, "```php\n<?php\nclass CheckoutTest {}\n```").unwrap();
        assert!(prop.new_file && prop.edit.is_none());
        assert_eq!(prop.proposed, "<?php\nclass CheckoutTest {}\n");
        // Two lines plus the empty one after the final newline.
        assert_eq!(prop.diff.added, 3);

        let mut r2 = r.clone();
        r2.existing_test = Some("<?php\nclass CheckoutTest {}\n".into());
        assert!(prompt(&r2).unwrap().user.contains("already exists"));
        let prop = proposal(
            &r2,
            "```php\n<?php\nclass CheckoutTest { function testA() {} }\n```",
        )
        .unwrap();
        assert!(!prop.new_file);

        let rs = "pub fn one() -> i32 {\n    1\n}\n";
        let r = req(ActionKind::Tests, "src/lib.rs", rs, at(rs, "1\n"));
        let prop = proposal(&r, "```rust\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn one_is_one() { assert_eq!(super::one(), 1); }\n}\n```").unwrap();
        assert!(prop.proposed.starts_with(rs) && prop.proposed.ends_with("}\n"));
        assert!(prop.proposed.contains("}\n\n#[cfg(test)]"));
        assert_eq!(
            proposal(&r, "Sorry.\n```\n```").unwrap_err(),
            "The AI reply contained no code"
        );
    }
}
