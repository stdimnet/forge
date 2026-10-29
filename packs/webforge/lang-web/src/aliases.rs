//! Module aliases of bundler configurations: `resolve.alias` of `vite.config.*` and
//! `webpack.config.*`, read statically (no JavaScript is run), best effort.
//!
//! Understood forms:
//!
//! ```js
//! resolve: {
//!   alias: {
//!     '@': path.resolve(__dirname, './src'),            // also path.join / resolve(…)
//!     '~utils': fileURLToPath(new URL('./src/utils', import.meta.url)),
//!     '@assets': '/src/assets',                          // root-relative (Vite)
//!     vue$: 'vue/dist/vue.esm-bundler.js',              // packages are skipped
//!   },
//!   // or: alias: [{ find: '@', replacement: path.resolve(__dirname, 'src') }]
//! }
//! ```
//!
//! Replacements that are not project folders (packages, regular expressions, computed
//! values) are skipped. `tsconfig.json` `paths` are read by [`crate::source`].

use serde::{Deserialize, Serialize};

use crate::js::{fields, matching, skip_trivia, string_at};
use crate::text::{join_path, parent_dir};

/// Configuration files whose aliases are read, in this order.
pub const CONFIG_FILES: &[&str] = &[
    "vite.config.ts",
    "vite.config.mts",
    "vite.config.js",
    "vite.config.mjs",
    "vite.config.cjs",
    "vitest.config.ts",
    "nuxt.config.ts",
    "webpack.config.js",
    "webpack.config.ts",
    "webpack.config.cjs",
    "webpack.config.mjs",
];

/// One alias: imports starting with `find` resolve into `target` (a project folder or
/// file, workspace relative).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Alias {
    pub find: String,
    pub target: String,
    /// Webpack's `name$`: only the exact specifier matches.
    pub exact: bool,
    /// The configuration file it comes from.
    pub source: String,
}

/// Aliases of one configuration file at workspace path `path`.
pub fn parse(path: &str, text: &str) -> Vec<Alias> {
    let dir = parent_dir(path);
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(i) = text[from..].find("alias").map(|i| from + i) {
        from = i + 5;
        // A key `alias:` (not `aliases`, not `someAlias`).
        let before = text[..i].chars().next_back();
        if before.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '$' || c == '.') {
            continue;
        }
        let mut j = i + 5;
        if text[j..].starts_with(['"', '\'']) {
            j += 1;
        }
        j = skip_trivia(text, j);
        if !text[j..].starts_with(':') {
            continue;
        }
        j = skip_trivia(text, j + 1);
        match text.as_bytes().get(j) {
            Some(b'{') => {
                if let Some(end) = matching(text, j) {
                    object_aliases(text, j + 1, end, dir, path, &mut out);
                    from = end;
                }
            }
            Some(b'[') => {
                if let Some(end) = matching(text, j) {
                    array_aliases(text, j + 1, end, dir, path, &mut out);
                    from = end;
                }
            }
            _ => {}
        }
    }
    out.dedup_by(|a, b| a.find == b.find);
    out
}

fn object_aliases(
    text: &str,
    start: usize,
    end: usize,
    dir: &str,
    source: &str,
    out: &mut Vec<Alias>,
) {
    for f in fields(text, start, end) {
        let Some((vs, ve)) = f.value else { continue };
        let key = f.key.trim_matches(['"', '\'']).to_string();
        let (find, exact) = match key.strip_suffix('$') {
            Some(k) => (k.to_string(), true),
            None => (key, false),
        };
        if let Some(target) = replacement(&text[vs..ve], dir) {
            push(out, find, target, exact, source);
        }
    }
}

fn array_aliases(
    text: &str,
    start: usize,
    end: usize,
    dir: &str,
    source: &str,
    out: &mut Vec<Alias>,
) {
    let mut i = start;
    while i < end {
        let Some(open) = text[i..end].find('{').map(|o| i + o) else {
            break;
        };
        let Some(close) = matching(text, open) else {
            break;
        };
        let members = fields(text, open + 1, close);
        let value = |name: &str| {
            members
                .iter()
                .find(|m| m.key == name)
                .and_then(|m| m.value)
                .map(|(s, e)| &text[s..e])
        };
        let find = value("find").and_then(literal);
        let target = value("replacement").and_then(|r| replacement(r, dir));
        if let (Some(find), Some(target)) = (find, target) {
            push(out, find, target, false, source);
        }
        i = close + 1;
    }
}

fn push(out: &mut Vec<Alias>, find: String, target: String, exact: bool, source: &str) {
    if find.is_empty() {
        return;
    }
    out.push(Alias {
        find,
        target,
        exact,
        source: source.to_string(),
    });
}

/// The contents of a string literal expression (`'x'`, `"x"`, `` `x` `` without `${}`).
fn literal(expr: &str) -> Option<String> {
    let e = expr.trim();
    let (s, end) = string_at(e, 0)?;
    (end + 1 == e.len() && !e[s..end].contains("${")).then(|| e[s..end].to_string())
}

/// The string arguments of a call such as `path.resolve(__dirname, 'src', "x")`; `None`
/// when an argument is neither a string nor `__dirname` / `process.cwd()` /
/// `import.meta.dirname`.
fn path_call(expr: &str) -> Option<Vec<String>> {
    let open = expr.find('(')?;
    let callee = expr[..open].trim();
    let known = [
        "path.resolve",
        "path.join",
        "resolve",
        "join",
        "path.posix.resolve",
        "path.posix.join",
        "pathResolve",
    ];
    if !known.contains(&callee) {
        return None;
    }
    let close = matching(expr, open)?;
    let args = &expr[open + 1..close];
    let mut out = Vec::new();
    for arg in split_top(args) {
        let a = arg.trim();
        if matches!(
            a,
            "__dirname" | "process.cwd()" | "import.meta.dirname" | "root" | "rootDir"
        ) || a.is_empty()
        {
            continue;
        }
        out.push(literal(a)?);
    }
    Some(out)
}

/// Split call arguments at top-level commas.
fn split_top(args: &str) -> Vec<&str> {
    let bytes = args.as_bytes();
    let mut out = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'(' | b'[' | b'{' => {
                i = matching(args, i).map_or(bytes.len(), |c| c + 1);
                continue;
            }
            b'"' | b'\'' | b'`' => {
                i = string_at(args, i).map_or(i + 1, |(_, e)| e + 1);
                continue;
            }
            b',' => {
                out.push(&args[start..i]);
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    out.push(&args[start..]);
    out
}

/// The project path an alias replacement expression points at, relative to the
/// workspace; `dir` is the configuration file's folder.
fn replacement(expr: &str, dir: &str) -> Option<String> {
    let e = expr.trim().trim_end_matches(',').trim();
    // `fileURLToPath(new URL('./src', import.meta.url))`
    if let Some(i) = e.find("new URL(") {
        let inner = &e[i + "new URL(".len()..];
        let first = split_top(inner.split_once(')')?.0).into_iter().next()?;
        let rel = literal(first)?;
        return Some(join_path(dir, &rel));
    }
    // `` `${__dirname}/src` ``
    if let Some(rest) = e
        .strip_prefix("`${__dirname}")
        .or_else(|| e.strip_prefix("`${process.cwd()}"))
    {
        let rel = rest.strip_suffix('`')?;
        return (!rel.contains("${")).then(|| join_path(dir, rel.trim_start_matches('/')));
    }
    if let Some(parts) = path_call(e) {
        let mut p = dir.to_string();
        for part in parts {
            p = if part.starts_with('/') {
                // An absolute segment resets the path; outside the project: give up.
                return None;
            } else {
                join_path(&p, &part)
            };
        }
        return Some(p);
    }
    let lit = literal(e)?;
    if let Some(rooted) = lit.strip_prefix('/') {
        // Vite: `/src/x` is relative to the project root.
        return Some(join_path(dir, rooted));
    }
    if lit.starts_with("./") || lit.starts_with("../") {
        return Some(join_path(dir, &lit));
    }
    // A package (`vue/dist/vue.esm-bundler.js`) or an absolute OS path.
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(aliases: &[Alias]) -> Vec<(&str, &str, bool)> {
        aliases
            .iter()
            .map(|a| (a.find.as_str(), a.target.as_str(), a.exact))
            .collect()
    }

    #[test]
    fn vite_object_aliases() {
        let text = r#"
import { defineConfig } from 'vite'
import { fileURLToPath, URL } from 'node:url'
import path from 'node:path'

export default defineConfig({
  plugins: [vue()],
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
      '@components': path.resolve(__dirname, 'src', 'components'),
      "~assets": '/src/assets',
      utils: `${__dirname}/src/utils`,
      vue: 'vue/dist/vue.esm-bundler.js',
      dynamic: someFunction(),
    },
  },
})
"#;
        assert_eq!(
            pairs(&parse("web/vite.config.ts", text)),
            vec![
                ("@", "web/src", false),
                ("@components", "web/src/components", false),
                ("~assets", "web/src/assets", false),
                ("utils", "web/src/utils", false),
            ]
        );
    }

    #[test]
    fn vite_array_and_webpack_aliases() {
        let vite = r#"export default {
  resolve: {
    alias: [
      { find: '@', replacement: path.resolve(__dirname, './src') },
      { find: /^~/, replacement: '' },
      { find: 'lib', replacement: resolve(__dirname, "packages/lib/src") },
    ],
  },
}"#;
        assert_eq!(
            pairs(&parse("vite.config.js", vite)),
            vec![("@", "src", false), ("lib", "packages/lib/src", false)]
        );
        let webpack = r#"module.exports = {
  entry: './src/index.js',
  resolve: {
    extensions: ['.js', '.ts'],
    alias: {
      Utilities: path.resolve(__dirname, 'src/utilities/'),
      Templates$: path.resolve(__dirname, 'src/templates/index.js'),
    },
  },
};"#;
        assert_eq!(
            pairs(&parse("webpack.config.js", webpack)),
            vec![
                ("Utilities", "src/utilities", false),
                ("Templates", "src/templates/index.js", true),
            ]
        );
        // Not an alias block.
        assert!(parse(
            "vite.config.ts",
            "const aliases = { a: './x' }; myalias: {}"
        )
        .is_empty());
    }
}
