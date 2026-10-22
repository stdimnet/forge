//! The Project view's rows for one folder, like PHPStorm's Project tool window options
//! (⋮): Show Excluded Files, Compact Middle Packages, Folders Always on Top, Sort by Type
//! and File Nesting (`.ts` → `.js; .js.map; .d.ts`).

use serde::{Deserialize, Serialize};

/// A folder entry as the backend lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewEntry {
    pub name: String,
    pub path: String,
    /// `file` or `dir`.
    pub kind: String,
    #[serde(default)]
    pub ignored: bool,
    #[serde(default)]
    pub excluded: bool,
}

impl ViewEntry {
    pub fn is_dir(&self) -> bool {
        self.kind == "dir"
    }
}

/// A File Nesting rule: files ending in `parent` take files with the same base name and one
/// of the `children` suffixes (`;`-separated, like PHPStorm's dialog).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NestingRule {
    pub parent: String,
    pub children: String,
}

impl NestingRule {
    fn child_suffixes(&self) -> impl Iterator<Item = &str> {
        self.children
            .split(';')
            .map(str::trim)
            .filter(|c| !c.is_empty())
    }
}

/// PHPStorm's predefined nesting rules.
pub fn default_nesting_rules() -> Vec<NestingRule> {
    [
        (".ts", ".js; .js.map; .d.ts; .min.js"),
        (".tsx", ".js; .jsx; .js.map"),
        (".mts", ".mjs; .mjs.map; .d.mts"),
        (".cts", ".cjs; .cjs.map; .d.cts"),
        (".js", ".js.map; .min.js; .min.js.map; .d.ts; .js.flow"),
        (".mjs", ".mjs.map; .d.mts"),
        (".cjs", ".cjs.map; .d.cts"),
        (".coffee", ".js; .js.map"),
        (".css", ".css.map; .min.css; .min.css.map"),
        (".scss", ".css; .css.map; .min.css"),
        (".sass", ".css; .css.map; .min.css"),
        (".less", ".css; .css.map; .min.css"),
        (".styl", ".css; .css.map; .min.css"),
        (".pug", ".html"),
        (".dart", ".g.dart; .freezed.dart"),
        (".component.ts", ".component.html; .component.css; .component.scss; .component.sass; .component.less; .component.spec.ts"),
        ("composer.json", "composer.lock"),
        ("package.json", "package-lock.json; npm-shrinkwrap.json; yarn.lock; pnpm-lock.yaml; .pnp.cjs; .pnp.loader.mjs"),
        ("Cargo.toml", "Cargo.lock"),
        ("Gemfile", "Gemfile.lock"),
        ("go.mod", "go.sum"),
    ]
    .into_iter()
    .map(|(parent, children)| NestingRule {
        parent: parent.into(),
        children: children.into(),
    })
    .collect()
}

/// The Project view options that change the rows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ViewOptions {
    pub show_excluded: bool,
    pub folders_on_top: bool,
    pub sort_by_type: bool,
    pub compact_middle_packages: bool,
    /// File Nesting is on.
    pub file_nesting: bool,
    /// The File Nesting rules; empty uses [`default_nesting_rules`].
    pub nesting_rules: Vec<NestingRule>,
}

impl Default for ViewOptions {
    fn default() -> Self {
        Self {
            show_excluded: true,
            folders_on_top: true,
            sort_by_type: false,
            compact_middle_packages: false,
            file_nesting: true,
            nesting_rules: Vec::new(),
        }
    }
}

/// One Project view row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeRow {
    /// The label: a compacted chain of folders reads `a/b/c`.
    pub name: String,
    /// The path the row stands for (the last folder of a compacted chain).
    pub path: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub ignored: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub excluded: bool,
    /// The folders a compacted row stands for, outermost first (empty otherwise).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub compacted: Vec<String>,
    /// Files nested under this file (File Nesting).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nested: Vec<TreeRow>,
}

impl TreeRow {
    fn from_entry(e: ViewEntry) -> Self {
        Self {
            name: e.name,
            path: e.path,
            kind: e.kind,
            ignored: e.ignored,
            excluded: e.excluded,
            compacted: Vec::new(),
            nested: Vec::new(),
        }
    }
}

/// Deepest chain of single folders compacted into one row.
const MAX_COMPACT: usize = 32;

/// The rows of one folder from its listing; `list` lists another folder (for Compact Middle
/// Packages, which looks into single child folders).
pub fn arrange(
    entries: Vec<ViewEntry>,
    list: &mut dyn FnMut(&str) -> Vec<ViewEntry>,
    options: &ViewOptions,
) -> Vec<TreeRow> {
    let visible = |e: &ViewEntry| options.show_excluded || !e.excluded;
    let entries: Vec<ViewEntry> = entries.into_iter().filter(visible).collect();
    let mut rows: Vec<TreeRow> = Vec::with_capacity(entries.len());
    for e in entries {
        let mut row = TreeRow::from_entry(e);
        if row.kind == "dir" && options.compact_middle_packages {
            let mut chain = vec![row.path.clone()];
            while chain.len() < MAX_COMPACT {
                let kids: Vec<ViewEntry> = list(&row.path).into_iter().filter(visible).collect();
                match kids.as_slice() {
                    [only] if only.is_dir() && only.excluded == row.excluded => {
                        row.name = format!("{}/{}", row.name, only.name);
                        row.path = only.path.clone();
                        row.ignored = only.ignored;
                        chain.push(only.path.clone());
                    }
                    _ => break,
                }
            }
            if chain.len() > 1 {
                row.compacted = chain;
            }
        }
        rows.push(row);
    }
    if options.file_nesting {
        if options.nesting_rules.is_empty() {
            rows = nest(rows, &default_nesting_rules());
        } else {
            rows = nest(rows, &options.nesting_rules);
        }
    }
    sort(&mut rows, options);
    rows
}

/// Move files under their File Nesting parents (one level: a chain `a.ts` → `a.js` →
/// `a.js.map` puts both under `a.ts`).
pub fn nest(rows: Vec<TreeRow>, rules: &[NestingRule]) -> Vec<TreeRow> {
    let names: Vec<String> = rows.iter().map(|r| r.name.clone()).collect();
    let is_file = |i: usize| rows[i].kind != "dir";
    let index_of = |name: &str| names.iter().position(|n| n == name);
    // For each file the best parent: the longest matching child suffix wins.
    let mut parent: Vec<Option<usize>> = vec![None; rows.len()];
    for (i, name) in names.iter().enumerate() {
        if !is_file(i) {
            continue;
        }
        let mut best: Option<(usize, usize)> = None;
        for rule in rules {
            for suffix in rule.child_suffixes() {
                let Some(base) = name.strip_suffix(suffix) else {
                    continue;
                };
                // Whole-name rules (`composer.json` → `composer.lock`) need an exact name.
                let whole = !suffix.starts_with('.') || !rule.parent.starts_with('.');
                if whole && !base.is_empty() {
                    continue;
                }
                let candidate = format!("{base}{}", rule.parent);
                if let Some(p) = index_of(&candidate).filter(|&p| p != i && is_file(p)) {
                    if best.is_none_or(|(len, _)| suffix.len() > len) {
                        best = Some((suffix.len(), p));
                    }
                }
            }
        }
        parent[i] = best.map(|(_, p)| p);
    }
    // Follow chains to the top parent; a cycle keeps its files on the top level.
    let top: Vec<Option<usize>> = (0..rows.len())
        .map(|i| {
            let mut p = parent[i]?;
            for _ in 0..rows.len() {
                match parent[p] {
                    Some(next) if next != i => p = next,
                    Some(_) => return None,
                    None => return Some(p),
                }
            }
            None
        })
        .collect();
    let mut nested: Vec<Vec<TreeRow>> = vec![Vec::new(); rows.len()];
    let mut out: Vec<Option<TreeRow>> = rows.into_iter().map(Some).collect();
    for i in 0..out.len() {
        if let Some(p) = top[i] {
            if top[p].is_none() {
                nested[p].push(out[i].take().expect("row moved once"));
            }
        }
    }
    out.into_iter()
        .zip(nested)
        .filter_map(|(row, mut kids)| {
            let mut row = row?;
            kids.sort_by_key(|k| k.name.to_lowercase());
            row.nested = kids;
            Some(row)
        })
        .collect()
}

/// The file type sort key: the extension (`php`, `blade.php` sorts by `php`).
fn extension(name: &str) -> String {
    name.rsplit_once('.')
        .filter(|(base, _)| !base.is_empty())
        .map(|(_, ext)| ext.to_lowercase())
        .unwrap_or_default()
}

fn sort(rows: &mut [TreeRow], options: &ViewOptions) {
    rows.sort_by(|a, b| {
        let dir_rank = |r: &TreeRow| options.folders_on_top && r.kind != "dir";
        let type_key = |r: &TreeRow| {
            if options.sort_by_type && r.kind != "dir" {
                extension(&r.name)
            } else {
                String::new()
            }
        };
        (dir_rank(a), type_key(a), a.name.to_lowercase(), &a.name).cmp(&(
            dir_rank(b),
            type_key(b),
            b.name.to_lowercase(),
            &b.name,
        ))
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn e(path: &str, dir: bool) -> ViewEntry {
        ViewEntry {
            name: path.rsplit('/').next().unwrap().into(),
            path: path.into(),
            kind: if dir { "dir" } else { "file" }.into(),
            ignored: false,
            excluded: false,
        }
    }

    fn names(rows: &[TreeRow]) -> Vec<&str> {
        rows.iter().map(|r| r.name.as_str()).collect()
    }

    #[test]
    fn nests_compiled_files_under_their_sources() {
        let entries = vec![
            e("app.ts", false),
            e("app.js", false),
            e("app.js.map", false),
            e("app.d.ts", false),
            e("main.scss", false),
            e("main.css", false),
            e("other.js", false),
            e("other.js.map", false),
            e("composer.json", false),
            e("composer.lock", false),
            e("my-composer.lock", false),
        ];
        let rows = arrange(entries, &mut |_| vec![], &ViewOptions::default());
        assert_eq!(
            names(&rows),
            vec![
                "app.ts",
                "composer.json",
                "main.scss",
                "my-composer.lock",
                "other.js"
            ]
        );
        assert_eq!(
            names(&rows[0].nested),
            vec!["app.d.ts", "app.js", "app.js.map"]
        );
        assert_eq!(names(&rows[1].nested), vec!["composer.lock"]);
        assert_eq!(names(&rows[2].nested), vec!["main.css"]);
        assert_eq!(names(&rows[4].nested), vec!["other.js.map"]);
    }

    #[test]
    fn nesting_off_keeps_files_flat_and_folders_are_never_nested() {
        let entries = vec![e("a.ts", false), e("a.js", true), e("a.js.map", false)];
        let off = ViewOptions {
            file_nesting: false,
            ..ViewOptions::default()
        };
        assert_eq!(
            names(&arrange(entries.clone(), &mut |_| vec![], &off)),
            vec!["a.js", "a.js.map", "a.ts"]
        );
        // The folder `a.js` is no parent; the map nests under the TypeScript file.
        let on = arrange(entries, &mut |_| vec![], &ViewOptions::default());
        assert_eq!(names(&on), vec!["a.js", "a.ts"]);
        assert_eq!(names(&on[1].nested), vec!["a.js.map"]);
    }

    #[test]
    fn custom_rules_and_cycles() {
        let rules = vec![
            NestingRule {
                parent: ".a".into(),
                children: ".b".into(),
            },
            NestingRule {
                parent: ".b".into(),
                children: ".a".into(),
            },
        ];
        let rows: Vec<TreeRow> = vec![e("x.a", false), e("x.b", false)]
            .into_iter()
            .map(TreeRow::from_entry)
            .collect();
        let out = nest(rows, &rules);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn compacts_middle_folders_and_hides_excluded() {
        let tree: HashMap<&str, Vec<ViewEntry>> = HashMap::from([
            ("src", vec![e("src/Acme", true)]),
            ("src/Acme", vec![e("src/Acme/Shop", true)]),
            (
                "src/Acme/Shop",
                vec![
                    e("src/Acme/Shop/A.php", false),
                    e("src/Acme/Shop/B.php", false),
                ],
            ),
            ("lib", vec![e("lib/x.php", false)]),
        ]);
        let mut list = |p: &str| tree.get(p).cloned().unwrap_or_default();
        let mut cache = e("cache", true);
        cache.excluded = true;
        let entries = vec![e("src", true), e("lib", true), cache, e("README.md", false)];
        let opts = ViewOptions {
            compact_middle_packages: true,
            show_excluded: false,
            ..ViewOptions::default()
        };
        let rows = arrange(entries.clone(), &mut list, &opts);
        assert_eq!(names(&rows), vec!["lib", "src/Acme/Shop", "README.md"]);
        assert_eq!(rows[1].path, "src/Acme/Shop");
        assert_eq!(rows[1].compacted, vec!["src", "src/Acme", "src/Acme/Shop"]);
        let shown = arrange(entries, &mut list, &ViewOptions::default());
        assert_eq!(names(&shown), vec!["cache", "lib", "src", "README.md"]);
        assert!(shown[0].excluded);
    }

    #[test]
    fn sorts_by_type_and_mixes_folders_when_asked() {
        let entries = vec![
            e("b.php", false),
            e("a.js", false),
            e("c", true),
            e("A.php", false),
            e("Makefile", false),
        ];
        let by_type = ViewOptions {
            sort_by_type: true,
            file_nesting: false,
            ..ViewOptions::default()
        };
        assert_eq!(
            names(&arrange(entries.clone(), &mut |_| vec![], &by_type)),
            vec!["c", "Makefile", "a.js", "A.php", "b.php"]
        );
        let mixed = ViewOptions {
            folders_on_top: false,
            file_nesting: false,
            ..ViewOptions::default()
        };
        assert_eq!(
            names(&arrange(entries, &mut |_| vec![], &mixed)),
            vec!["a.js", "A.php", "b.php", "c", "Makefile"]
        );
    }
}
