//! Languages of the "New Scratch File" popup (Ctrl+Alt+Shift+Insert): every file type of
//! `forge-filetypes` with an extension, the extension a new scratch gets, and the popup's
//! ordering and speed search.

use forge_filetypes::{FileTypeRegistry, FileTypeSpec};
use forge_fuzzy::{FilterCandidate, FilterRequest};
use serde::{Deserialize, Serialize};

/// Id of the "Plain text" entry (no file type).
pub const PLAIN_TEXT_ID: &str = "text";

/// A language a scratch can be created in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScratchLanguage {
    /// File type id (`php`, `http`, …) or [`PLAIN_TEXT_ID`].
    pub id: String,
    pub name: String,
    /// Extension of new scratches, without the dot (`php`, `blade.php`).
    pub extension: String,
}

/// Every language of the registry that has an extension, plus "Plain text", sorted by name.
/// A type registered twice (a plugin overriding a bundled type) appears once, as its newest
/// registration.
pub fn languages(registry: &FileTypeRegistry) -> Vec<ScratchLanguage> {
    let specs: Vec<&FileTypeSpec> = registry.specs().collect();
    let mut out: Vec<ScratchLanguage> = specs
        .iter()
        .enumerate()
        .filter(|(i, s)| !specs[i + 1..].iter().any(|later| later.id == s.id))
        .filter_map(|(_, s)| {
            let extension = s.extensions.first()?.clone();
            Some(ScratchLanguage {
                id: s.id.clone(),
                name: s.name.clone(),
                extension,
            })
        })
        .collect();
    out.push(ScratchLanguage {
        id: PLAIN_TEXT_ID.into(),
        name: "Plain text".into(),
        extension: "txt".into(),
    });
    out.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then(a.id.cmp(&b.id))
    });
    out
}

/// Extension for new scratches of `language` (a file type id); `None` when unknown.
pub fn extension_for(registry: &FileTypeRegistry, language: &str) -> Option<String> {
    languages(registry)
        .into_iter()
        .find(|l| l.id == language)
        .map(|l| l.extension)
}

/// Request of the popup list (`scratches_languages`).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguageRequest {
    #[serde(default)]
    pub query: String,
    /// File type of the active editor; listed first, as in PHPStorm.
    #[serde(default)]
    pub current: Option<String>,
    /// Recently used languages, most recent first; listed after the current one.
    #[serde(default)]
    pub recent: Vec<String>,
}

/// One row of the popup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguageItem {
    #[serde(flatten)]
    pub language: ScratchLanguage,
    /// Matched character positions in `name`, for highlighting.
    pub positions: Vec<usize>,
}

/// The popup list: without a query the current and recent languages come first, then the
/// rest by name; with a query, the fuzzy matches of name, id or extension, best first.
pub fn list(all: Vec<ScratchLanguage>, req: &LanguageRequest) -> Vec<LanguageItem> {
    let query = req.query.trim();
    if query.is_empty() {
        let mut first: Vec<&str> = Vec::new();
        for id in req.current.iter().chain(req.recent.iter()) {
            if !first.contains(&id.as_str()) {
                first.push(id);
            }
        }
        let rank = |l: &ScratchLanguage| {
            first
                .iter()
                .position(|id| *id == l.id)
                .unwrap_or(first.len())
        };
        let mut all = all;
        // Stable: the rest keeps the alphabetical order.
        all.sort_by_key(rank);
        return all
            .into_iter()
            .map(|language| LanguageItem {
                language,
                positions: vec![],
            })
            .collect();
    }
    let request = FilterRequest {
        query: query.to_string(),
        candidates: all
            .iter()
            .map(|l| FilterCandidate::Many(vec![l.name.clone(), l.id.clone(), l.extension.clone()]))
            .collect(),
        strict: false,
        prefer_shorter: true,
        limit: None,
    };
    let mut slots: Vec<Option<ScratchLanguage>> = all.into_iter().map(Some).collect();
    let mut items: Vec<LanguageItem> = forge_fuzzy::filter(&request)
        .into_iter()
        .filter_map(|hit| {
            let language = slots[hit.index].take()?;
            Some(LanguageItem {
                language,
                positions: if hit.text == 0 { hit.positions } else { vec![] },
            })
        })
        .collect();
    // Names containing the query as typed come first: prefixes, then other substrings, each
    // shortest first ("typescr" lists TypeScript before TypeScript React, "scr" JavaScript
    // before JavaScript React). The rest keeps the fuzzy order.
    let lower = query.to_lowercase();
    let query_len = lower.chars().count();
    let tier = |item: &LanguageItem| -> (u8, usize) {
        let name = item.language.name.to_lowercase();
        match name.find(&lower) {
            Some(0) => (0, name.len()),
            Some(_) => (1, name.len()),
            None => (2, 0),
        }
    };
    for item in &mut items {
        let name = item.language.name.to_lowercase();
        if let Some(at) = name.find(&lower) {
            let start = name[..at].chars().count();
            item.positions = (start..start + query_len).collect();
        }
    }
    items.sort_by_key(|i| tier(i));
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(items: &[LanguageItem]) -> Vec<&str> {
        items.iter().map(|i| i.language.id.as_str()).collect()
    }

    #[test]
    fn maps_languages_to_extensions() {
        let r = FileTypeRegistry::with_builtins();
        assert_eq!(extension_for(&r, "php").as_deref(), Some("php"));
        assert_eq!(extension_for(&r, "javascript").as_deref(), Some("js"));
        assert_eq!(extension_for(&r, "typescript").as_deref(), Some("ts"));
        assert_eq!(extension_for(&r, "http").as_deref(), Some("http"));
        assert_eq!(extension_for(&r, "sql").as_deref(), Some("sql"));
        assert_eq!(extension_for(&r, "text").as_deref(), Some("txt"));
        assert_eq!(extension_for(&r, "nope"), None);
        // Types without extensions (Apache config) cannot be scratches.
        assert_eq!(extension_for(&r, "htaccess"), None);
    }

    #[test]
    fn plugin_overrides_appear_once() {
        let mut r = FileTypeRegistry::with_builtins();
        r.register(
            FileTypeSpec {
                id: "yaml".into(),
                name: "YAML".into(),
                extensions: vec!["yaml".into(), "yml".into()],
                ..Default::default()
            },
            Some("forge.yaml"),
        );
        let all = languages(&r);
        let yaml: Vec<_> = all.iter().filter(|l| l.id == "yaml").collect();
        assert_eq!(yaml.len(), 1);
        assert_eq!(yaml[0].extension, "yaml");
        // Sorted by name.
        let names: Vec<String> = all.iter().map(|l| l.name.to_lowercase()).collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted);
    }

    #[test]
    fn current_and_recent_come_first() {
        let r = FileTypeRegistry::with_builtins();
        let req = LanguageRequest {
            query: String::new(),
            current: Some("php".into()),
            recent: vec!["http".into(), "php".into(), "sql".into()],
        };
        let items = list(languages(&r), &req);
        assert_eq!(&ids(&items)[..3], ["php", "http", "sql"]);
        assert_eq!(items.len(), languages(&r).len());
    }

    #[test]
    fn speed_search_matches_names_ids_and_extensions() {
        let r = FileTypeRegistry::with_builtins();
        let find = |q: &str| {
            list(
                languages(&r),
                &LanguageRequest {
                    query: q.into(),
                    ..Default::default()
                },
            )
        };
        let items = find("typescr");
        assert_eq!(items[0].language.id, "typescript");
        assert_eq!(items[0].positions, vec![0, 1, 2, 3, 4, 5, 6]);
        assert_eq!(find("HTTP")[0].language.id, "http");
        // By extension: no highlight in the name.
        let js = find("js");
        assert!(js.iter().any(|i| i.language.id == "javascript"));
        assert!(find("zzzz").is_empty());
        // Substrings beat scattered matches.
        let scr = find("scr");
        let rank = |id: &str| scr.iter().position(|i| i.language.id == id).unwrap();
        assert!(rank("javascript") < rank("jsx"));
        assert_eq!(scr[rank("javascript")].positions, vec![4, 5, 6]);
    }
}
