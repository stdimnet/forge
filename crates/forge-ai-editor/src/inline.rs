//! Inline (ghost text) completion: the context sent to the model, the prompt for
//! fill-in-the-middle (local code models) or chat models (hosted), and the clean-up of the
//! answer before the editor shows it.

use serde::{Deserialize, Serialize};

use crate::lang::{language_of, Language};
use crate::text::{extract_code, indent_of, indent_width, utf16_to_byte};

/// Bytes of code before the caret sent to the model.
pub const MAX_PREFIX: usize = 8000;
/// Bytes of code after the caret.
pub const MAX_SUFFIX: usize = 2500;
/// Other open files shown to the model, and the bytes of each.
pub const MAX_RELATED: usize = 3;
pub const RELATED_BYTES: usize = 1500;
/// Imports from outside the prefix window.
pub const MAX_IMPORTS: usize = 30;
/// Lines of a multi-line suggestion.
pub const MAX_LINES: usize = 16;

/// `ai.completion.multiline`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MultilinePolicy {
    /// Several lines on an empty line or after a block opener, else one line.
    #[default]
    Auto,
    Single,
    Multi,
}

/// `ai.completion.mode`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CompletionMode {
    /// Fill-in-the-middle when the provider supports it (local servers), else chat.
    #[default]
    Auto,
    Fim,
    Chat,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelatedFile {
    pub path: String,
    pub text: String,
}

/// What the editor sends for a suggestion.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InlineRequest {
    /// Project-relative path.
    pub path: String,
    /// Editor language id, when known.
    #[serde(default)]
    pub language: Option<String>,
    pub text: String,
    /// Caret in UTF-16 code units.
    pub offset: usize,
    /// Other open files (most recently used first).
    #[serde(default)]
    pub related: Vec<RelatedFile>,
    #[serde(default)]
    pub multiline: MultilinePolicy,
}

/// The code around the caret, trimmed to the limits above.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InlineContext {
    pub path: String,
    pub language: Language,
    pub prefix: String,
    pub suffix: String,
    /// Text of the caret line before / after the caret.
    pub line_prefix: String,
    pub line_suffix: String,
    /// The last non-blank line before the caret line.
    pub previous_line: String,
    /// Indentation of the caret line.
    pub indent: String,
    /// Import lines of the file that are not in `prefix`.
    pub imports: Vec<String>,
    pub related: Vec<RelatedFile>,
    pub multiline: bool,
}

/// Cut `text` to its last `max` bytes, starting at a line.
fn tail(text: &str, max: usize) -> (&str, bool) {
    if text.len() <= max {
        return (text, false);
    }
    let mut start = text.len() - max;
    while !text.is_char_boundary(start) {
        start += 1;
    }
    let start = text[start..]
        .find('\n')
        .map(|i| start + i + 1)
        .unwrap_or(start);
    (&text[start..], true)
}

/// Cut `text` to its first `max` bytes, ending at a line.
fn head(text: &str, max: usize) -> &str {
    if text.len() <= max {
        return text;
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    match text[..end].rfind('\n') {
        Some(i) if i > 0 => &text[..i],
        _ => &text[..end],
    }
}

fn wants_multiline(policy: MultilinePolicy, line_prefix: &str, line_suffix: &str) -> bool {
    match policy {
        MultilinePolicy::Single => false,
        MultilinePolicy::Multi => true,
        MultilinePolicy::Auto => {
            let after = line_suffix.trim();
            if !after.chars().all(|c| ")]};,".contains(c)) {
                return false;
            }
            let before = line_prefix.trim_end();
            before.is_empty()
                || before.ends_with(['{', '(', '[', ':'])
                || before.ends_with("=>")
                || before.ends_with(" then")
                || before.ends_with(" do")
        }
    }
}

/// The context of a request (see the limits above).
pub fn build_context(req: &InlineRequest) -> InlineContext {
    let language = language_of(&req.path, req.language.as_deref());
    let text = req.text.replace("\r\n", "\n");
    let at = utf16_to_byte(&text, req.offset);
    let (before, after) = text.split_at(at);
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    let line_end = after.find('\n').unwrap_or(after.len());
    let line_prefix = before[line_start..].to_string();
    let line_suffix = after[..line_end].to_string();
    let previous_line = before[..line_start]
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or_default()
        .to_string();
    let (prefix, cut) = tail(before, MAX_PREFIX);
    let imports = if cut {
        before[..before.len() - prefix.len()]
            .lines()
            .filter(|l| language.is_import(l))
            .take(MAX_IMPORTS)
            .map(String::from)
            .collect()
    } else {
        Vec::new()
    };
    let mut related: Vec<&RelatedFile> = req
        .related
        .iter()
        .filter(|r| r.path != req.path && !r.text.trim().is_empty())
        .collect();
    // Files of the same language first; otherwise keep the editor's order.
    related.sort_by_key(|r| language_of(&r.path, None) != language);
    let related = related
        .into_iter()
        .take(MAX_RELATED)
        .map(|r| RelatedFile {
            path: r.path.clone(),
            text: head(&r.text, RELATED_BYTES).to_string(),
        })
        .collect();
    InlineContext {
        path: req.path.clone(),
        language,
        multiline: wants_multiline(req.multiline, &line_prefix, &line_suffix),
        indent: indent_of(&line_prefix).to_string(),
        line_prefix,
        line_suffix,
        previous_line,
        prefix: prefix.to_string(),
        suffix: head(after, MAX_SUFFIX).to_string(),
        imports,
        related,
    }
}

/// Both prompt forms; the app picks one by provider and `ai.completion.mode`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InlinePrompt {
    /// Fill-in-the-middle: everything before the caret (with a commented context header).
    pub fim_prompt: String,
    pub fim_suffix: String,
    /// Chat models: instructions and the code with a `<CURSOR>` marker.
    pub system: String,
    pub user: String,
    /// Stop sequences for fill-in-the-middle.
    pub stop: Vec<String>,
    pub max_tokens: u32,
}

pub const CURSOR: &str = "<CURSOR>";

/// The prompts for `ctx`.
pub fn build_prompt(ctx: &InlineContext) -> InlinePrompt {
    let mut header = String::new();
    if let Some(c) = ctx.language.line_comment() {
        header.push_str(&format!("{c} Path: {}\n", ctx.path));
        for r in &ctx.related {
            header.push_str(&format!("{c} Compare this snippet from {}:\n", r.path));
            for l in r.text.lines() {
                header.push_str(&format!("{c} {l}\n"));
            }
        }
    }
    let imports = if ctx.imports.is_empty() {
        String::new()
    } else {
        format!("{}\n", ctx.imports.join("\n"))
    };
    let fim_prompt = format!("{header}{imports}{}", ctx.prefix);

    let lines_rule = if ctx.multiline {
        "You may insert several lines; stop at a natural end such as the end of the statement or block."
    } else {
        "Insert at most one line."
    };
    let system = format!(
        "You are an inline code completion engine inside an IDE. Complete the code at the {CURSOR} marker. \
Reply with only the text to insert at the cursor: no explanations, no Markdown fences, and do not repeat \
the code before or after the cursor. {lines_rule} Reply with nothing if no completion makes sense."
    );
    let mut user = format!("File: {} ({})\n", ctx.path, ctx.language.name());
    for r in &ctx.related {
        user.push_str(&format!(
            "\nOther open file {}:\n```\n{}\n```\n",
            r.path, r.text
        ));
    }
    user.push_str(&format!(
        "\n```{}\n{imports}{}{CURSOR}{}\n```",
        ctx.language.id(),
        ctx.prefix,
        ctx.suffix
    ));
    InlinePrompt {
        fim_prompt,
        fim_suffix: ctx.suffix.clone(),
        system,
        user,
        stop: if ctx.multiline {
            vec!["\n\n\n".into()]
        } else {
            vec!["\n".into()]
        },
        max_tokens: if ctx.multiline { 256 } else { 64 },
    }
}

/// Tokens that end a fill-in-the-middle answer.
const END_TOKENS: [&str; 7] = [
    "<|endoftext|>",
    "<|file_separator|>",
    "<|fim_pad|>",
    "<EOT>",
    "<|eot_id|>",
    "</s>",
    "<|im_end|>",
];
/// Markers some models echo.
const STRIP_TOKENS: [&str; 4] = [CURSOR, "<|fim_middle|>", "<fim_middle>", "<MID>"];

fn is_closer(t: &str) -> bool {
    t.starts_with(['}', ')', ']'])
        || t == "end"
        || t.starts_with("end ")
        || t == "fi"
        || t == "esac"
}

fn closers_only(t: &str) -> bool {
    t.chars().all(|c| ")]}>;,'\"` \t".contains(c))
}

/// Drop text the model repeated from before the caret.
fn strip_echo(s: &str, ctx: &InlineContext) -> String {
    let lp = ctx.line_prefix.trim_start();
    if !lp.trim().is_empty() {
        let t = s.trim_start_matches([' ', '\t']);
        if let Some(rest) = t.strip_prefix(lp) {
            return rest.to_string();
        }
    } else {
        let mut s = s;
        // On an empty line: a first line that repeats the line above.
        if !ctx.previous_line.trim().is_empty() {
            if let Some((first, rest)) = s.split_once('\n') {
                if first.trim() == ctx.previous_line.trim() {
                    s = rest;
                }
            }
        }
        // The caret is already indented; chat models indent the first line again.
        if !ctx.indent.is_empty() {
            if let Some(rest) = s.strip_prefix(ctx.indent.as_str()) {
                return rest.to_string();
            }
        }
        return s.to_string();
    }
    s.to_string()
}

/// Lines up to the end of the block the caret is in (no dedent below the caret line).
fn limit_block(s: &str, ctx: &InlineContext) -> String {
    let base = indent_width(&ctx.indent);
    let mut lines = s.split('\n');
    let mut out = vec![lines.next().unwrap_or_default()];
    for line in lines {
        if out.len() >= MAX_LINES {
            break;
        }
        if line.trim().is_empty() {
            out.push(line);
            continue;
        }
        let w = indent_width(indent_of(line));
        if w < base {
            break;
        }
        out.push(line);
        if w == base && is_closer(line.trim_start()) {
            break;
        }
    }
    while out.len() > 1 && out.last().is_some_and(|l| l.trim().is_empty()) {
        out.pop();
    }
    out.join("\n")
}

/// Drop the end of the suggestion that is already after the caret.
fn dedupe_suffix(s: &str, ctx: &InlineContext) -> String {
    let mut lines: Vec<&str> = s.split('\n').collect();
    // Whole lines: `}` the model wrote although the next line already has it.
    if lines.len() > 1 {
        let next: Vec<&str> = ctx
            .suffix
            .split('\n')
            .skip(1)
            .filter(|l| !l.trim().is_empty())
            .take(3)
            .collect();
        for k in (1..=next.len().min(lines.len() - 1)).rev() {
            let tail = &lines[lines.len() - k..];
            if tail
                .iter()
                .zip(&next[..k])
                .all(|(a, b)| a.trim() == b.trim())
            {
                lines.truncate(lines.len() - k);
                break;
            }
        }
    }
    let mut out = lines.join("\n");
    // Characters: `foo(a, b)` typed into `foo(|)` keeps only `a, b`.
    let rest = ctx.line_suffix.trim_end();
    if !rest.is_empty() {
        let mut ends: Vec<usize> = rest.char_indices().map(|(i, c)| i + c.len_utf8()).collect();
        ends.reverse();
        for k in ends {
            let piece = &rest[..k];
            let whole = k == rest.len() && rest.trim().len() >= 2;
            if out.ends_with(piece) && out.len() > piece.len() && (closers_only(piece) || whole) {
                out.truncate(out.len() - piece.len());
                break;
            }
        }
    }
    out
}

/// Clean a raw model answer; `None` when nothing useful is left.
pub fn postprocess(raw: &str, ctx: &InlineContext) -> Option<String> {
    let mut s = raw.replace("\r\n", "\n");
    for t in END_TOKENS {
        if let Some(i) = s.find(t) {
            s.truncate(i);
        }
    }
    for t in STRIP_TOKENS {
        s = s.replace(t, "");
    }
    if s.trim_start().starts_with("```") {
        s = extract_code(&s);
    }
    s = strip_echo(&s, ctx);
    if ctx.multiline {
        s = limit_block(&s, ctx);
    } else {
        let mut lines = s.split('\n');
        let first = lines.next().unwrap_or_default();
        s = if first.trim().is_empty() && ctx.line_suffix.trim().is_empty() {
            // At the end of `if (x) {`: the next line is the suggestion.
            match lines.find(|l| !l.trim().is_empty()) {
                Some(l) => format!("\n{l}"),
                None => String::new(),
            }
        } else {
            first.to_string()
        };
    }
    s = dedupe_suffix(&s, ctx);
    let s = s
        .split('\n')
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n");
    let s = s.trim_end_matches('\n');
    if s.trim().is_empty() || ctx.line_suffix.starts_with(s) {
        return None;
    }
    Some(s.to_string())
}

/// Context + clean-up in one step (what the app and the browser mock run on an answer).
pub fn finish(req: &InlineRequest, raw: &str) -> Option<String> {
    postprocess(raw, &build_context(req))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(text: &str, multiline: MultilinePolicy) -> InlineRequest {
        let offset = text.find('|').expect("caret marker");
        let text = text.replacen('|', "", 1);
        InlineRequest {
            path: "src/math.ts".into(),
            offset: text[..offset].encode_utf16().count(),
            text,
            multiline,
            ..InlineRequest::default()
        }
    }

    fn done(text: &str, raw: &str) -> Option<String> {
        finish(&req(text, MultilinePolicy::Auto), raw)
    }

    #[test]
    fn context_splits_at_caret_and_trims_long_files() {
        let mut text = String::from("import { a } from './a';\n");
        for i in 0..2000 {
            text.push_str(&format!("const v{i} = {i};\n"));
        }
        text.push_str("const total = |;\n}\n");
        let mut r = req(&text, MultilinePolicy::Auto);
        r.related = vec![
            RelatedFile {
                path: "src/math.ts".into(),
                text: "self".into(),
            },
            RelatedFile {
                path: "README.md".into(),
                text: "# Readme".into(),
            },
            RelatedFile {
                path: "src/a.ts".into(),
                text: "export const a = 1;".into(),
            },
        ];
        let ctx = build_context(&r);
        assert!(ctx.prefix.len() <= MAX_PREFIX && ctx.prefix.starts_with("const v"));
        assert_eq!(ctx.imports, ["import { a } from './a';"]);
        assert_eq!(ctx.line_prefix, "const total = ");
        assert_eq!(ctx.line_suffix, ";");
        assert_eq!(ctx.previous_line, "const v1999 = 1999;");
        assert!(!ctx.multiline);
        let paths: Vec<_> = ctx.related.iter().map(|r| r.path.as_str()).collect();
        assert_eq!(paths, ["src/a.ts", "README.md"]);

        let p = build_prompt(&ctx);
        assert!(p.fim_prompt.starts_with("// Path: src/math.ts\n// Compare this snippet from src/a.ts:\n// export const a = 1;\n"));
        assert!(p.fim_prompt.contains("import { a } from './a';\nconst v"));
        assert!(p.fim_prompt.ends_with("const total = "));
        assert_eq!(p.fim_suffix, ";\n}\n");
        assert!(p.user.contains("const total = <CURSOR>;\n}"));
        assert_eq!(
            (p.stop.as_slice(), p.max_tokens),
            (&["\n".to_string()][..], 64)
        );
    }

    #[test]
    fn multiline_policy() {
        assert!(build_context(&req("function f() {|\n}", MultilinePolicy::Auto)).multiline);
        assert!(build_context(&req("  |\n", MultilinePolicy::Auto)).multiline);
        assert!(!build_context(&req("let x = |foo", MultilinePolicy::Auto)).multiline);
        assert!(!build_context(&req("  |\n", MultilinePolicy::Single)).multiline);
        assert!(build_context(&req("let x = |", MultilinePolicy::Multi)).multiline);
        let p = build_prompt(&build_context(&req("  |\n", MultilinePolicy::Auto)));
        assert_eq!(p.max_tokens, 256);
    }

    #[test]
    fn strips_fences_tokens_and_echoed_prefix() {
        assert_eq!(
            done("const total = |", "```ts\nconst total = a + b;\n```").as_deref(),
            Some("a + b;")
        );
        assert_eq!(
            done("const total = |", "a + b;<|endoftext|>garbage").as_deref(),
            Some("a + b;")
        );
        assert_eq!(
            done("const total = |", "<CURSOR>a + b;").as_deref(),
            Some("a + b;")
        );
        assert_eq!(done("const total = |", "   ").as_deref(), None);
    }

    #[test]
    fn single_line_cuts_after_the_first_line() {
        assert_eq!(
            done("const total = |", "a + b;\nconsole.log(total);").as_deref(),
            Some("a + b;")
        );
        // At the end of a block opener the next line is offered.
        let r = req("if (ok) {|", MultilinePolicy::Single);
        assert_eq!(finish(&r, "\n  run();\n}").as_deref(), Some("\n  run();"));
    }

    #[test]
    fn multi_line_stops_at_the_end_of_the_block() {
        let text = "function sum(xs) {\n  |\n}\n";
        let raw =
            "let s = 0;\n  for (const x of xs) s += x;\n  return s;\n}\n\nfunction other() {}";
        assert_eq!(
            done(text, raw).as_deref(),
            Some("let s = 0;\n  for (const x of xs) s += x;\n  return s;")
        );
        let text = "function sum(xs) {|\n}\n";
        let raw = "\n  return xs.reduce((a, b) => a + b, 0);\n}";
        assert_eq!(
            done(text, raw).as_deref(),
            Some("\n  return xs.reduce((a, b) => a + b, 0);")
        );
        let long: String = (0..40).map(|i| format!("  f{i}();\n")).collect();
        let got = done("function f() {\n  |\n}", &long).unwrap();
        assert_eq!(got.lines().count(), MAX_LINES);
    }

    #[test]
    fn removes_overlap_with_the_text_after_the_caret() {
        assert_eq!(done("sum(|)", "a, b)").as_deref(), Some("a, b"));
        assert_eq!(done("const x = load(|);", "id);").as_deref(), Some("id"));
        // Letters are not cut: "ba" + "r()" would not be what the model meant.
        assert_eq!(done("foo|r()", "ba").as_deref(), Some("ba"));
        // Already there.
        assert_eq!(done("return |total;", "total;").as_deref(), None);
        // A repeated previous line on an empty line.
        assert_eq!(
            done("  let a = 1;\n  |\n", "let a = 1;\n  let b = 2;").as_deref(),
            Some("let b = 2;")
        );
    }
}
