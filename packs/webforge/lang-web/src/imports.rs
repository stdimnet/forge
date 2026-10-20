//! ES module imports and re-exports, read lexically from JavaScript/TypeScript source.

/// What an import binding refers to in the imported module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Imported {
    Default,
    Named(String),
    Namespace,
}

/// `import Local from "source"`, `import { Imported as Local } from "source"`, ...
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Import {
    pub local: String,
    pub imported: Imported,
    pub source: String,
}

/// `export { Imported as Exported } from "source"` or `export * from "source"`
/// (`exported` and `imported` are `*`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReExport {
    pub exported: String,
    pub imported: String,
    pub source: String,
}

/// Every `import … from "…"` declaration in `text`.
pub fn imports(text: &str) -> Vec<Import> {
    let mut out = Vec::new();
    for (clause, source) in statements(text, "import") {
        let clause = clause.trim().trim_start_matches("type ").trim();
        let (head, braces) = match clause.find('{') {
            Some(i) => (&clause[..i], Some(&clause[i + 1..clause.rfind('}').unwrap_or(clause.len())])),
            None => (clause, None),
        };
        for part in head.split(',').map(str::trim).filter(|p| !p.is_empty()) {
            if let Some(ns) = part.strip_prefix('*') {
                let local = ns.trim().trim_start_matches("as").trim();
                if is_ident(local) {
                    out.push(Import {
                        local: local.to_string(),
                        imported: Imported::Namespace,
                        source: source.clone(),
                    });
                }
            } else if is_ident(part) {
                out.push(Import {
                    local: part.to_string(),
                    imported: Imported::Default,
                    source: source.clone(),
                });
            }
        }
        for (imported, local) in braces.map(specifiers).unwrap_or_default() {
            out.push(Import {
                local,
                imported: if imported == "default" {
                    Imported::Default
                } else {
                    Imported::Named(imported)
                },
                source: source.clone(),
            });
        }
    }
    out
}

/// Every `export { … } from "…"` and `export * from "…"`.
pub fn re_exports(text: &str) -> Vec<ReExport> {
    let mut out = Vec::new();
    for (clause, source) in statements(text, "export") {
        let clause = clause.trim().trim_start_matches("type ").trim();
        if clause.starts_with('*') {
            let alias = clause[1..].trim().trim_start_matches("as").trim();
            out.push(ReExport {
                exported: if is_ident(alias) { alias.to_string() } else { "*".into() },
                imported: "*".into(),
                source,
            });
        } else if let Some(inner) = clause.strip_prefix('{') {
            let inner = &inner[..inner.rfind('}').unwrap_or(inner.len())];
            for (imported, exported) in specifiers(inner) {
                out.push(ReExport {
                    exported,
                    imported,
                    source: source.clone(),
                });
            }
        }
    }
    out
}

/// `a, b as c, type d` -> [(a, a), (b, c), (d, d)].
fn specifiers(inner: &str) -> Vec<(String, String)> {
    inner
        .split(',')
        .filter_map(|s| {
            let s = s.trim().trim_start_matches("type ").trim();
            let mut parts = s.split_whitespace();
            let imported = parts.next()?;
            let local = match (parts.next(), parts.next()) {
                (Some("as"), Some(l)) => l,
                _ => imported,
            };
            (is_ident(imported) && is_ident(local)).then(|| (imported.to_string(), local.to_string()))
        })
        .collect()
}

fn is_ident(s: &str) -> bool {
    let mut c = s.chars();
    c.next().is_some_and(crate::text::is_ident_start) && c.all(crate::text::is_ident_char)
}

/// `(clause, source)` of every `<keyword> clause from "source"` statement. The keyword must
/// start a statement (start of a line or after `;`).
fn statements(text: &str, keyword: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut search = 0;
    while let Some(found) = text[search..].find(keyword) {
        let at = search + found;
        search = at + keyword.len();
        let before = text[..at].trim_end_matches([' ', '\t']);
        if !(before.is_empty() || before.ends_with('\n') || before.ends_with(';') || before.ends_with('>')) {
            continue;
        }
        let rest = &text[search..];
        if !rest.starts_with([' ', '\t', '\n', '{', '*']) {
            continue;
        }
        // The statement ends at the module specifier after `from`.
        let window = &rest[..rest.len().min(2000)];
        let Some(from) = find_from(window) else {
            continue;
        };
        let clause = &window[..from];
        if clause.contains(';') || clause.contains('(') || clause.contains('=') {
            continue;
        }
        let after = window[from + 4..].trim_start();
        let Some(q) = after.chars().next().filter(|c| *c == '"' || *c == '\'') else {
            continue;
        };
        let Some(close) = after[1..].find(q) else {
            continue;
        };
        out.push((clause.to_string(), after[1..1 + close].to_string()));
    }
    out
}

/// Offset of the keyword `from` as a whole word.
fn find_from(s: &str) -> Option<usize> {
    let mut start = 0;
    while let Some(i) = s[start..].find("from") {
        let at = start + i;
        let ok_before = at == 0 || s[..at].ends_with([' ', '\n', '\t', '}']);
        let ok_after = s[at + 4..].starts_with([' ', '"', '\'', '\n', '\t']);
        if ok_before && ok_after {
            return Some(at);
        }
        start = at + 4;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_import_forms() {
        let src = r#"import React, { useState } from "react";
import Button from './Button'
import { Card as UiCard, type Props } from "@/components/ui";
import * as icons from 'lucide-react';
import type { User } from './types';
import './styles.css';
const x = "import y from 'z'";
"#;
        let imports = imports(src);
        let short: Vec<_> = imports
            .iter()
            .map(|i| (i.local.as_str(), i.source.as_str()))
            .collect();
        assert_eq!(
            short,
            [
                ("React", "react"),
                ("useState", "react"),
                ("Button", "./Button"),
                ("UiCard", "@/components/ui"),
                ("Props", "@/components/ui"),
                ("icons", "lucide-react"),
                ("User", "./types"),
            ]
        );
        assert_eq!(imports[3].imported, Imported::Named("Card".into()));
        assert_eq!(imports[5].imported, Imported::Namespace);
    }

    #[test]
    fn multi_line_imports_and_re_exports() {
        let src = "import {\n  Header,\n  Footer as Bottom,\n} from './layout';\nexport { default as Button } from './Button';\nexport * from './Card';\nexport { Modal } from \"./Modal\";\n";
        assert_eq!(imports(src).len(), 2);
        let re = re_exports(src);
        assert_eq!(re.len(), 3);
        assert_eq!((re[0].imported.as_str(), re[0].exported.as_str()), ("default", "Button"));
        assert_eq!(re[1].exported, "*");
        assert_eq!(re[2].source, "./Modal");
    }
}
