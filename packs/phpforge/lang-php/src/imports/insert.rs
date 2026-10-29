//! The one helper that adds `use` statements to a file. Import class, completion, Auto
//! Import on the fly, Create from usage and the refactorings (Move, Copy, Pull Members Up,
//! Extract Interface) all go through [`insert_imports`], so new imports land where the code
//! style puts them: in their group (classes, functions, constants; or by top-level
//! namespace), in sort order among the existing imports, with the blank lines between
//! groups. Existing imports are never reordered; that is Optimize Imports' job.
//!
//! The edits are pure insertions at line boundaries (bytes), so they combine with other
//! edits of a refactoring.

use crate::generate::TextEdit;
use crate::model::{FileIndex, UseImport, UseKind};

use super::style::ImportStyle;
use super::Clause;

/// An import to add.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewImport {
    pub kind: UseKind,
    /// Without leading backslash.
    pub fqn: String,
    pub alias: Option<String>,
}

impl NewImport {
    pub fn class(fqn: &str) -> Self {
        Self {
            kind: UseKind::Class,
            fqn: fqn.trim_start_matches('\\').to_string(),
            alias: None,
        }
    }

    fn clause(&self) -> Clause {
        Clause {
            kind: self.kind,
            fqn: self.fqn.clone(),
            alias: self.alias.clone(),
        }
    }
}

/// Where a new statement goes relative to an existing one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Anchor {
    /// At the start of the line of the import at this byte (before it).
    Before(usize),
    /// At the end of the line of the import at this byte (after it).
    After(usize),
}

fn line_start(src: &str, at: usize) -> usize {
    src[..at].rfind('\n').map_or(0, |i| i + 1)
}

fn line_end(src: &str, at: usize) -> usize {
    src[at..].find('\n').map_or(src.len(), |i| at + i)
}

fn existing_clause(u: &UseImport) -> Clause {
    Clause {
        kind: u.kind,
        fqn: u.fqn.clone(),
        alias: u.alias.clone(),
    }
}

/// Edits adding `imports` to `src` (whose index is `file`), skipping the ones the file
/// already has. The imports of the first namespace (or of the file) are the reference;
/// without any, the statements go after the namespace declaration, else after `<?php`.
pub fn insert_imports(
    file: &FileIndex,
    src: &str,
    imports: &[NewImport],
    style: &ImportStyle,
) -> Vec<TextEdit> {
    // Existing imports of the first namespace, in file order.
    let scope_end = file.namespaces.get(1).map_or(usize::MAX, |n| n.range.start);
    let mut existing: Vec<&UseImport> = file
        .uses
        .iter()
        .filter(|u| u.range.start < scope_end)
        .collect();
    existing.sort_by_key(|u| u.range.start);

    let mut todo: Vec<NewImport> = Vec::new();
    for n in imports {
        let n = NewImport {
            fqn: n.fqn.trim_start_matches('\\').to_string(),
            ..n.clone()
        };
        let has = |kind: UseKind, fqn: &str, alias: &Option<String>| {
            kind == n.kind && fqn.eq_ignore_ascii_case(&n.fqn) && *alias == n.alias
        };
        if existing.iter().any(|u| has(u.kind, &u.fqn, &u.alias))
            || todo.iter().any(|t| has(t.kind, &t.fqn, &t.alias))
        {
            continue;
        }
        todo.push(n);
    }
    if todo.is_empty() {
        return vec![];
    }
    let group = |c: &Clause| style.group_of(c.kind, &c.fqn);
    let order = |a: &Clause, b: &Clause| {
        group(a)
            .cmp(&group(b))
            .then_with(|| style.compare(&a.fqn, &a.render(), &b.fqn, &b.render()))
    };
    // New imports in the order they are written.
    let mut new: Vec<Clause> = todo.iter().map(NewImport::clause).collect();
    new.sort_by(order);

    if existing.is_empty() {
        let text = render_groups(&new, style, &group);
        if let Some(ns) = file.namespaces.first() {
            if let Some(semi) = src[ns.range.start..].find(';') {
                let at = ns.range.start + semi + 1;
                return vec![insert(at, format!("\n\n{text}"))];
            }
        }
        let after_open_tag = src.find('\n').unwrap_or(src.len());
        return vec![insert(after_open_tag, format!("\n\n{text}"))];
    }

    // Each new import's anchor: before the first existing import of its group that sorts
    // after it, else after the last one of its group, else next to the neighbouring group.
    let mut placed: Vec<(Anchor, Clause)> = Vec::new();
    for c in new {
        let g = group(&c);
        let same: Vec<&&UseImport> = existing
            .iter()
            .filter(|u| group(&existing_clause(u)) == g)
            .collect();
        let anchor = if let Some(next) = same.iter().find(|u| {
            let e = existing_clause(u);
            style.compare(&c.fqn, &c.render(), &e.fqn, &e.render()) == std::cmp::Ordering::Less
        }) {
            Anchor::Before(next.range.start)
        } else if let Some(last) = same.last() {
            Anchor::After(last.range.start)
        } else if let Some(prev) = existing
            .iter()
            .filter(|u| group(&existing_clause(u)) < g)
            .max_by(|a, b| {
                group(&existing_clause(a))
                    .cmp(&group(&existing_clause(b)))
                    .then(a.range.start.cmp(&b.range.start))
            })
        {
            Anchor::After(prev.range.start)
        } else {
            Anchor::Before(existing[0].range.start)
        };
        placed.push((anchor, c));
    }

    let mut edits: Vec<TextEdit> = Vec::new();
    let mut anchors: Vec<Anchor> = Vec::new();
    for (a, _) in &placed {
        if !anchors.contains(a) {
            anchors.push(*a);
        }
    }
    for anchor in anchors {
        let items: Vec<&Clause> = placed
            .iter()
            .filter(|(a, _)| *a == anchor)
            .map(|(_, c)| c)
            .collect();
        let neighbour = existing
            .iter()
            .find(|u| match anchor {
                Anchor::Before(at) | Anchor::After(at) => u.range.start == at,
            })
            .map(|u| group(&existing_clause(u)));
        let sep = style.group_separator();
        let mut text = String::new();
        match anchor {
            Anchor::After(at) => {
                let mut prev = neighbour;
                for c in &items {
                    let g = Some(group(c));
                    text.push_str(if g == prev { "\n" } else { &sep });
                    text.push_str(&c.render());
                    prev = g;
                }
                edits.push(insert(line_end(src, at), text));
            }
            Anchor::Before(at) => {
                for (i, c) in items.iter().enumerate() {
                    text.push_str(&c.render());
                    let next = items.get(i + 1).map(|n| group(n)).or(neighbour.clone());
                    text.push_str(if next == Some(group(c)) { "\n" } else { &sep });
                }
                edits.push(insert(line_start(src, at), text));
            }
        }
    }
    edits
}

fn insert(at: usize, text: String) -> TextEdit {
    TextEdit {
        start: at,
        end: at,
        new_text: text,
    }
}

/// Statements of new imports, groups separated by the style's blank lines.
fn render_groups(
    clauses: &[Clause],
    style: &ImportStyle,
    group: &dyn Fn(&Clause) -> (u8, String),
) -> String {
    let sep = style.group_separator();
    let mut text = String::new();
    let mut prev = None;
    for c in clauses {
        let g = Some(group(c));
        if prev.is_some() {
            text.push_str(if g == prev { "\n" } else { &sep });
        }
        text.push_str(&c.render());
        prev = g;
    }
    text
}

#[cfg(test)]
mod tests {
    use super::super::style::{ImportGrouping, ImportSort};
    use super::*;
    use crate::generate::apply_edits;
    use crate::parser::index_source;

    fn add(src: &str, fqns: &[&str], style: &ImportStyle) -> String {
        let file = index_source("a.php", src);
        let imports: Vec<NewImport> = fqns.iter().map(|f| NewImport::class(f)).collect();
        apply_edits(src, &insert_imports(&file, src, &imports, style))
    }

    #[test]
    fn inserts_alphabetically_into_the_existing_imports() {
        let src = "<?php\nnamespace App;\n\nuse App\\B;\nuse App\\D;\n\nclass X {}\n";
        assert_eq!(
            add(src, &["App\\C", "App\\A", "App\\E"], &ImportStyle::default()),
            "<?php\nnamespace App;\n\nuse App\\A;\nuse App\\B;\nuse App\\C;\nuse App\\D;\nuse App\\E;\n\nclass X {}\n"
        );
    }

    #[test]
    fn without_imports_they_follow_the_namespace_or_the_open_tag() {
        let style = ImportStyle::default();
        assert_eq!(
            add(
                "<?php\nnamespace App;\n\nclass X {}\n",
                &["B\\Y", "A\\Z"],
                &style
            ),
            "<?php\nnamespace App;\n\nuse A\\Z;\nuse B\\Y;\n\nclass X {}\n"
        );
        assert_eq!(
            add("<?php\n\nnew Y();\n", &["B\\Y"], &style),
            "<?php\n\nuse B\\Y;\n\nnew Y();\n"
        );
    }

    #[test]
    fn existing_imports_are_skipped() {
        let src = "<?php\nuse A\\B;\n";
        let file = index_source("a.php", src);
        assert!(insert_imports(
            &file,
            src,
            &[NewImport::class("\\a\\b")],
            &ImportStyle::default()
        )
        .is_empty());
    }

    #[test]
    fn groups_by_top_level_namespace_with_blank_lines() {
        let style = ImportStyle {
            group_by: ImportGrouping::Namespace,
            blank_lines_between_groups: 1,
            ..Default::default()
        };
        let src = "<?php\nnamespace App;\n\nuse App\\Models\\User;\n\nuse Symfony\\Component\\Yaml;\n\nclass X {}\n";
        assert_eq!(
            add(src, &["Illuminate\\Support\\Str", "App\\Models\\Order", "Zend\\Z", "Carbon"], &style),
            "<?php\nnamespace App;\n\nuse Carbon;\n\nuse App\\Models\\Order;\nuse App\\Models\\User;\n\nuse Illuminate\\Support\\Str;\n\nuse Symfony\\Component\\Yaml;\n\nuse Zend\\Z;\n\nclass X {}\n"
        );
    }

    #[test]
    fn functions_and_constants_get_their_own_groups() {
        let style = ImportStyle {
            blank_lines_between_groups: 1,
            ..Default::default()
        };
        let src = "<?php\nuse A\\B;\n\nf();\n";
        let file = index_source("a.php", src);
        let imports = [
            NewImport {
                kind: UseKind::Const,
                fqn: "A\\MAX".into(),
                alias: None,
            },
            NewImport {
                kind: UseKind::Function,
                fqn: "A\\f".into(),
                alias: None,
            },
        ];
        assert_eq!(
            apply_edits(src, &insert_imports(&file, src, &imports, &style)),
            "<?php\nuse A\\B;\n\nuse function A\\f;\n\nuse const A\\MAX;\n\nf();\n"
        );
    }

    #[test]
    fn by_length_and_unsorted() {
        let src = "<?php\nuse Aaaa\\Bbbb;\nuse Z\\Y;\n";
        let by_length = ImportStyle {
            sort: ImportSort::ByLength,
            ..Default::default()
        };
        assert_eq!(
            add(src, &["Mm\\Nn"], &by_length),
            "<?php\nuse Mm\\Nn;\nuse Aaaa\\Bbbb;\nuse Z\\Y;\n"
        );
        let unsorted = ImportStyle {
            sort: ImportSort::None,
            ..Default::default()
        };
        assert_eq!(
            add(src, &["A\\A"], &unsorted),
            "<?php\nuse Aaaa\\Bbbb;\nuse Z\\Y;\nuse A\\A;\n"
        );
    }
}
