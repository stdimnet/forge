//! What Extract Component (Vue and React, `webforge-fw-vue` / `webforge-fw-react`) shares:
//! its answers, import lines in the file's own style, module paths relative to the new file
//! and re-indenting the extracted markup.

use serde::{Deserialize, Serialize};

use crate::imports::{Import, Imported};
use crate::text::{join_path, parent_dir};

/// What the Extract Component dialog shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractComponentInfo {
    /// `vue` or `react`.
    pub framework: String,
    /// A suggested name.
    pub name: String,
    /// The props the new component gets (names the markup takes from the parent).
    pub props: Vec<String>,
    /// Names the new component imports itself (components and helpers the parent imports).
    pub imports: Vec<String>,
    /// The new file's extension (`vue`, `tsx`, `jsx`, `js`).
    pub extension: String,
    /// The extracted range (bytes, trimmed).
    pub start: usize,
    pub end: usize,
}

/// A replacement in the parent file (bytes).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Replacement {
    pub start: usize,
    pub end: usize,
    pub text: String,
}

/// The result: the new file and the parent's edits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractedComponent {
    pub path: String,
    pub text: String,
    pub edits: Vec<Replacement>,
}

/// A component name: PascalCase identifier.
pub fn check_component_name(name: &str) -> Result<(), String> {
    let ok = name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if ok {
        Ok(())
    } else {
        Err(format!(
            "'{name}' is not a valid component name (use PascalCase)"
        ))
    }
}

/// How a file writes imports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImportStyle {
    pub quote: char,
    pub semicolon: bool,
}

/// The style of the first import in `code` (`default` without imports).
pub fn import_style(code: &str, default: ImportStyle) -> ImportStyle {
    for line in code.lines() {
        let t = line.trim();
        if !t.starts_with("import ") {
            continue;
        }
        let quote = if t.contains('"') { '"' } else { '\'' };
        return ImportStyle {
            quote,
            semicolon: t.ends_with(';'),
        };
    }
    default
}

/// `import Local from 'spec'` (or the named / namespace form) in `style`.
pub fn import_line(import: &Import, spec: &str, style: ImportStyle) -> String {
    let q = style.quote;
    let what = match &import.imported {
        Imported::Default => import.local.clone(),
        Imported::Namespace => format!("* as {}", import.local),
        Imported::Named(n) if *n == import.local => format!("{{ {n} }}"),
        Imported::Named(n) => format!("{{ {n} as {} }}", import.local),
    };
    format!(
        "import {what} from {q}{spec}{q}{}",
        if style.semicolon { ";" } else { "" }
    )
}

/// `import Name from './Name.vue'`.
pub fn default_import(name: &str, spec: &str, style: ImportStyle) -> String {
    import_line(
        &Import {
            local: name.into(),
            imported: Imported::Default,
            source: spec.into(),
        },
        spec,
        style,
    )
}

/// The module path from the file `from` to the file `to` (`./X`, `../a/X`).
pub fn relative_spec(from: &str, to: &str) -> String {
    let a: Vec<&str> = parent_dir(from)
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();
    let b: Vec<&str> = to.split('/').filter(|s| !s.is_empty()).collect();
    let common = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let ups = a.len() - common;
    let rest = b[common..].join("/");
    if ups == 0 {
        format!("./{rest}")
    } else {
        format!("{}{rest}", "../".repeat(ups))
    }
}

/// A relative import of `old_file` as `new_file` (in another folder) writes it.
pub fn rebase_spec(spec: &str, old_file: &str, new_file: &str) -> String {
    if !(spec.starts_with("./") || spec.starts_with("../")) {
        return spec.to_string();
    }
    if parent_dir(old_file) == parent_dir(new_file) {
        return spec.to_string();
    }
    let target = join_path(parent_dir(old_file), spec);
    relative_spec(new_file, &target)
}

/// The offset after the last `import` statement in `code[start..end]` (its line end), or
/// `None` without imports.
pub fn after_imports(code: &str, start: usize, end: usize) -> Option<usize> {
    let mut found = None;
    let mut i = start;
    while i < end {
        let line_end = code[i..end].find('\n').map_or(end, |e| i + e);
        let line = code[i..line_end].trim_start();
        if line.starts_with("import ") || line.starts_with("import{") {
            // A multi-line import ends at the line with its module string.
            let mut stmt_end = line_end;
            if !(line.contains(" from ") || line.contains('"') || line.contains('\'')) {
                while stmt_end < end {
                    let next_end = code[stmt_end + 1..end]
                        .find('\n')
                        .map_or(end, |e| stmt_end + 1 + e);
                    let l = &code[stmt_end + 1..next_end.max(stmt_end + 1)];
                    stmt_end = next_end;
                    if l.contains(" from ") || l.trim_start().starts_with("from") {
                        break;
                    }
                }
            }
            found = Some(stmt_end);
            i = stmt_end + 1;
            continue;
        }
        i = line_end + 1;
    }
    found
}

/// The indentation of the line holding `at`.
pub fn line_indent(text: &str, at: usize) -> String {
    let ls = text[..at].rfind('\n').map_or(0, |i| i + 1);
    text[ls..]
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect()
}

/// `code`'s lines with the indentation `from` replaced by `to` (the first line gets `to`).
pub fn reindent(code: &str, from: &str, to: &str) -> String {
    code.split('\n')
        .map(|line| {
            if line.trim().is_empty() {
                String::new()
            } else {
                format!(
                    "{to}{}",
                    line.strip_prefix(from).unwrap_or(line.trim_start())
                )
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// One indentation level of a file (two or four spaces, or a tab).
pub fn indent_unit(text: &str) -> String {
    let mut min = usize::MAX;
    for line in text.lines() {
        if line.starts_with('\t') {
            return "\t".into();
        }
        let n = line.chars().take_while(|c| *c == ' ').count();
        if n > 0 && n < line.len() {
            min = min.min(n);
        }
    }
    if min == usize::MAX || min > 8 {
        "  ".into()
    } else {
        " ".repeat(min)
    }
}

/// `start..end` without surrounding whitespace.
pub fn trimmed(text: &str, start: usize, end: usize) -> (usize, usize) {
    let s = &text[start..end];
    let lead = s.len() - s.trim_start().len();
    (start + lead, start + lead + s.trim().len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_paths() {
        assert_eq!(
            relative_spec("src/App.vue", "src/components/Card.vue"),
            "./components/Card.vue"
        );
        assert_eq!(relative_spec("src/a/App.tsx", "src/b/Card"), "../b/Card");
        assert_eq!(
            rebase_spec("./util", "src/App.tsx", "src/parts/X.tsx"),
            "../util"
        );
        assert_eq!(
            rebase_spec("react", "src/App.tsx", "src/parts/X.tsx"),
            "react"
        );
        assert_eq!(rebase_spec("./util", "src/App.tsx", "src/X.tsx"), "./util");
    }

    #[test]
    fn imports_in_the_files_style() {
        let style = import_style(
            "import a from \"b\";\n",
            ImportStyle {
                quote: '\'',
                semicolon: false,
            },
        );
        assert_eq!(
            style,
            ImportStyle {
                quote: '"',
                semicolon: true
            }
        );
        let i = Import {
            local: "money".into(),
            imported: Imported::Named("formatMoney".into()),
            source: "./m".into(),
        };
        assert_eq!(
            import_line(&i, "../m", style),
            "import { formatMoney as money } from \"../m\";"
        );
        let code = "x\nimport a from 'a'\nimport {\n  b,\n} from 'b'\nconst c = 1\n";
        let at = after_imports(code, 0, code.len()).unwrap();
        assert_eq!(&code[at..], "\nconst c = 1\n");
        assert_eq!(after_imports("const a = 1", 0, 11), None);
    }

    #[test]
    fn indentation() {
        assert_eq!(
            reindent("<a>\n      <b/>\n    </a>", "    ", "  "),
            "  <a>\n    <b/>\n  </a>"
        );
        assert_eq!(indent_unit("a\n    b\n        c\n"), "    ");
        assert_eq!(trimmed("  ab \n", 0, 6), (2, 4));
        assert!(check_component_name("UserCard").is_ok());
        assert!(check_component_name("userCard").is_err());
    }
}
