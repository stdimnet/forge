//! Import specifiers under the caret: Go to Declaration on `'@/components/Nav'` and
//! completion of module paths, both through the project's aliases (`tsconfig` `paths`,
//! Vite and Webpack `resolve.alias`), relative paths and index files.

use serde::{Deserialize, Serialize};

use crate::js::string_at;
use crate::source::{Sources, MODULE_EXTENSIONS};
use crate::text::{byte_to_utf16, join_path, parent_dir, utf16_to_byte, Location};

/// The module specifier string the caret is in, if it is one: `(spec, start, end)` with
/// byte offsets of the string's contents.
pub fn specifier_at(content: &str, byte: usize) -> Option<(String, usize, usize)> {
    let line_start = content[..byte].rfind('\n').map_or(0, |i| i + 1);
    let line_end = content[byte..].find('\n').map_or(content.len(), |i| byte + i);
    let line = &content[line_start..line_end];
    let col = byte - line_start;
    // The string around the caret.
    let mut i = 0;
    while i < line.len() {
        let b = line.as_bytes()[i];
        if matches!(b, b'"' | b'\'' | b'`') {
            let (s, e) = match string_at(line, i) {
                Some(r) => r,
                None if col > i => (i + 1, line.len()),
                None => break,
            };
            if col >= s && col <= e {
                let before = line[..i].trim_end();
                let is_import = before.ends_with("from")
                    || before.ends_with("import")
                    || before.ends_with("import(")
                    || before.ends_with("require(")
                    || before.ends_with("export * from")
                    || line.trim_start().starts_with("import ")
                        && before.ends_with("from");
                if !is_import {
                    return None;
                }
                let spec = line[s..e].to_string();
                return Some((spec, line_start + s, line_start + e));
            }
            i = e + 1;
            continue;
        }
        i += 1;
    }
    None
}

/// Go to Declaration on an import specifier at UTF-16 `offset`: the project file it
/// resolves to.
pub fn goto(sources: &Sources, path: &str, content: &str, offset: usize) -> Option<Location> {
    let (spec, _, _) = specifier_at(content, utf16_to_byte(content, offset))?;
    sources.resolve_module(path, &spec).map(|p| Location::file(&p))
}

/// A completion of a module path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportItem {
    /// The text after the last `/` of the typed prefix.
    pub label: String,
    pub dir: bool,
    /// The project path it stands for.
    pub path: String,
}

/// Completion of an import specifier at UTF-16 `offset`: `(from, items)` where `from` is
/// the UTF-16 offset the item labels replace (after the last `/`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportCompletion {
    pub from: usize,
    pub items: Vec<ImportItem>,
}

/// Folders the typed prefix (up to its last `/`) may mean.
fn prefix_dirs(sources: &Sources, from: &str, typed_dir: &str) -> Vec<String> {
    if typed_dir.starts_with('.') || typed_dir.starts_with('/') {
        return vec![join_path(parent_dir(from), typed_dir)];
    }
    let mut out = Vec::new();
    let with_slash = format!("{typed_dir}/");
    for (prefix, targets) in sources.aliases() {
        if let Some(rest) = with_slash.strip_prefix(prefix.as_str()) {
            for t in targets {
                out.push(join_path(t, rest));
            }
        }
    }
    out
}

/// Path completion inside an import string.
pub fn complete(sources: &Sources, path: &str, content: &str, offset: usize) -> Option<ImportCompletion> {
    let byte = utf16_to_byte(content, offset);
    let (spec, start, _) = specifier_at(content, byte)?;
    let typed = &spec[..(byte - start).min(spec.len())];
    let (typed_dir, from_byte) = match typed.rfind('/') {
        Some(i) => (&typed[..i], start + i + 1),
        None => ("", start),
    };
    let mut items: Vec<ImportItem> = Vec::new();
    if typed_dir.is_empty() && !typed.starts_with('.') {
        // Alias names themselves: `@/`, `@components/`.
        for (prefix, targets) in sources.aliases() {
            let label = prefix.trim_end_matches('/');
            if label.is_empty() || !prefix.ends_with('/') {
                continue;
            }
            let target = targets.first().cloned().unwrap_or_default();
            if !items.iter().any(|i| i.label == label) {
                items.push(ImportItem {
                    label: label.to_string(),
                    dir: true,
                    path: target.trim_end_matches('/').to_string(),
                });
            }
        }
    }
    for dir in prefix_dirs(sources, path, typed_dir) {
        let prefix = if dir.is_empty() {
            String::new()
        } else {
            format!("{dir}/")
        };
        for p in sources.paths() {
            let Some(rest) = p.strip_prefix(prefix.as_str()) else {
                continue;
            };
            if p.contains("node_modules/") {
                continue;
            }
            let (label, is_dir) = match rest.split_once('/') {
                Some((d, _)) => (d.to_string(), true),
                None => {
                    // Script modules without their extension, other files as they are.
                    let ext = rest.rsplit('.').next().unwrap_or("");
                    let stem = rest.strip_suffix(&format!(".{ext}")).unwrap_or(rest);
                    if MODULE_EXTENSIONS.contains(&ext) && ext != "vue" {
                        (stem.to_string(), false)
                    } else {
                        (rest.to_string(), false)
                    }
                }
            };
            if label.is_empty() || label.starts_with('.') && !typed.ends_with('.') {
                continue;
            }
            if !items.iter().any(|i| i.label == label && i.dir == is_dir) {
                items.push(ImportItem {
                    path: if is_dir {
                        format!("{prefix}{label}")
                    } else {
                        p.to_string()
                    },
                    label,
                    dir: is_dir,
                });
            }
        }
    }
    items.sort_by(|a, b| b.dir.cmp(&a.dir).then(a.label.cmp(&b.label)));
    Some(ImportCompletion {
        from: byte_to_utf16(content, from_byte),
        items,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SourceFile;

    fn sources() -> Sources {
        let file = |p: &str, c: &str| SourceFile {
            path: p.into(),
            content: c.into(),
        };
        Sources::new(
            vec![
                file(
                    "vite.config.ts",
                    "export default defineConfig({ resolve: { alias: { '@': fileURLToPath(new URL('./src', import.meta.url)), '#lib': path.resolve(__dirname, 'packages/lib') } } })",
                ),
                file("src/components/Nav.vue", ""),
                file("src/components/Footer.tsx", ""),
                file("src/main.ts", ""),
                file("packages/lib/index.ts", ""),
            ],
            ["src/assets/logo.svg".to_string()],
        )
    }

    #[test]
    fn resolves_vite_aliases_for_navigation() {
        let s = sources();
        assert_eq!(
            s.resolve_module("src/main.ts", "@/components/Footer").as_deref(),
            Some("src/components/Footer.tsx")
        );
        assert_eq!(s.resolve_module("src/main.ts", "#lib").as_deref(), Some("packages/lib/index.ts"));
        let text = "import Nav from '@/components/Nav.vue'\nconst x = 'not an import'\n";
        let at = text.find("Nav.vue").unwrap();
        assert_eq!(
            goto(&s, "src/main.ts", text, at),
            Some(Location::file("src/components/Nav.vue"))
        );
        assert_eq!(goto(&s, "src/main.ts", text, text.find("not an").unwrap()), None);
        assert_eq!(s.bundler_aliases().len(), 2);
    }

    #[test]
    fn completes_paths_through_aliases() {
        let s = sources();
        let text = "import Nav from '@/components/'";
        let c = complete(&s, "src/main.ts", text, text.len() - 1).unwrap();
        assert_eq!(c.from, text.len() - 1);
        let labels: Vec<_> = c.items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels, vec!["Footer", "Nav.vue"]);

        let text = "import x from '@/'";
        let c = complete(&s, "src/main.ts", text, text.len() - 1).unwrap();
        let labels: Vec<_> = c.items.iter().map(|i| (i.label.as_str(), i.dir)).collect();
        assert_eq!(labels, vec![("assets", true), ("components", true), ("main", false)]);

        let text = "import x from '";
        let c = complete(&s, "src/main.ts", text, text.len()).unwrap();
        assert!(c.items.iter().any(|i| i.label == "@" && i.dir));
        assert!(c.items.iter().any(|i| i.label == "#lib"));
    }
}
