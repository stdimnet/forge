//! `#[Language('SQL')]` on parameters (JetBrains PhpStorm attributes): the places the
//! language injections (`forge-injections`) treat as injected, found in the index.

use serde::{Deserialize, Serialize};

use crate::attributes::language;
use crate::index::ProjectIndex;
use crate::model::{Param, SymbolKind};

/// Arguments of calls to `name` at position `argument` carry `language`; `method` tells a
/// method from a function or constructor (`new Name(…)`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguagePlace {
    pub name: String,
    pub argument: usize,
    pub method: bool,
    pub language: String,
}

fn push(out: &mut Vec<LanguagePlace>, name: &str, method: bool, params: &[Param]) {
    for (argument, p) in params.iter().enumerate() {
        if let Some(language) = language(&p.attributes) {
            let place = LanguagePlace {
                name: name.to_string(),
                argument,
                method,
                language,
            };
            if !out.contains(&place) {
                out.push(place);
            }
        }
    }
}

/// Every `#[Language]` parameter of the indexed functions, methods and constructors.
pub fn language_places(index: &ProjectIndex) -> Vec<LanguagePlace> {
    let mut out = Vec::new();
    for f in index.files() {
        for func in &f.functions {
            push(&mut out, &func.fqn, false, &func.params);
        }
        for c in &f.classes {
            for m in c.members.iter().filter(|m| m.kind == SymbolKind::Method) {
                if m.name.eq_ignore_ascii_case("__construct") {
                    push(&mut out, &c.name, false, &m.params);
                } else {
                    push(&mut out, &m.name, true, &m.params);
                }
            }
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name).then(a.argument.cmp(&b.argument)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::IndexConfig;

    #[test]
    fn finds_language_parameters() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Repo.php"),
            "<?php\nnamespace App;\nuse JetBrains\\PhpStorm\\Language;\nclass Repo {\n  public function __construct(#[Language('SQL')] string $base) {}\n  public function filter(string $field, #[Language('SQL')] string $where) {}\n}\nfunction matches(#[Language('RegExp')] string $p, string $s) {}\n",
        )
        .unwrap();
        let mut idx = ProjectIndex::new(
            dir.path(),
            IndexConfig {
                cache_file: Some(dir.path().join("c.json")),
                ..Default::default()
            },
        );
        idx.index_workspace(|_, _| {});
        let places = language_places(&idx);
        let got: Vec<(&str, usize, bool, &str)> = places
            .iter()
            .map(|p| (p.name.as_str(), p.argument, p.method, p.language.as_str()))
            .collect();
        assert_eq!(
            got,
            vec![
                ("App\\matches", 0, false, "RegExp"),
                ("Repo", 0, false, "SQL"),
                ("filter", 1, true, "SQL"),
            ]
        );
    }
}
