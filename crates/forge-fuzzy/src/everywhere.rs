//! Search Everywhere ranking: merges the candidates of all providers of a tab, scores them
//! against the query, dedupes the "All" tab and cuts each group to its page size.
//!
//! Providers only *collect* candidates (project files from `fs_find_files`, PHP index hits,
//! language-server symbols, actions from the keymap, UI-only lists) and send them in one
//! request; every match position and every ordering decision is made here. Candidates come
//! in two shapes:
//!
//! - **text candidates** (`text` set): matched here, strict or not; `score` is a bonus
//!   added to the match score.
//! - **pre-ranked candidates** (`text` unset): already scored by a Rust backend (e.g. the
//!   file walker or the PHP index); `score` and `positions` are used as they are.

use serde::{Deserialize, Serialize};

use crate::{fuzzy_match_with, MatchOptions};

/// Tabs in display order; groups of unknown tabs follow.
pub const TAB_ORDER: &[&str] = &["classes", "files", "symbols", "actions", "text"];

/// Score penalty when only an alternative text (e.g. an action id) matched.
pub const ALT_TEXT_PENALTY: i32 = 10;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RankRequest {
    /// `all` or one tab id.
    pub tab: String,
    pub query: String,
    /// Items per group (the "All" tab shows a few per group, single tabs many).
    pub per_group: usize,
    /// Keep dimmed items (disabled actions) even when undimmed ones match.
    #[serde(default)]
    pub include_dimmed: bool,
    pub sources: Vec<Source>,
}

/// The candidates of one provider.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub provider: String,
    pub tab: String,
    /// Order among the providers of the same tab (lower first).
    #[serde(default = "default_order")]
    pub order: i32,
    pub items: Vec<Candidate>,
}

fn default_order() -> i32 {
    100
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    /// Unique within its provider.
    pub id: String,
    /// Item kind (`class`, `file`, ...); with `id` the identity used to dedupe "All".
    #[serde(default)]
    pub kind: Option<String>,
    /// Text the query is matched against; highlight positions index into it.
    #[serde(default)]
    pub text: Option<String>,
    /// Matched when `text` does not (e.g. an action id), with [`ALT_TEXT_PENALTY`] and
    /// without highlighting.
    #[serde(default)]
    pub alt_texts: Vec<String>,
    /// Word-start matching (PHPStorm's behaviour for actions, classes and symbols).
    #[serde(default)]
    pub strict: bool,
    /// Keep the item when the query does not match its text (the provider already
    /// filtered it, e.g. a language server's `workspace/symbol` result).
    #[serde(default)]
    pub keep_unmatched: bool,
    /// Pre-computed score, or a bonus added to the match score for text candidates.
    #[serde(default)]
    pub score: i32,
    /// Pre-computed highlight positions (pre-ranked candidates only).
    #[serde(default)]
    pub positions: Vec<usize>,
    /// Shown dimmed; dropped while undimmed items of the group match, unless
    /// `include_dimmed`.
    #[serde(default)]
    pub dimmed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RankResponse {
    pub groups: Vec<RankedGroup>,
}

/// One group of results: all providers of one tab, merged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RankedGroup {
    pub tab: String,
    /// The tab's first provider; the UI takes the group title from it.
    pub provider: String,
    pub items: Vec<RankedItem>,
    /// More results exist than shown.
    pub more: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RankedItem {
    pub provider: String,
    /// Index into that provider's `items`.
    pub index: usize,
    pub score: i32,
    /// Highlight positions (char indices into the candidate's text / label).
    pub positions: Vec<usize>,
}

fn tab_rank(tab: &str) -> usize {
    TAB_ORDER
        .iter()
        .position(|t| *t == tab)
        .unwrap_or(TAB_ORDER.len())
}

/// Score one candidate, or `None` if it does not match.
fn score(query: &str, c: &Candidate) -> Option<(i32, Vec<usize>)> {
    let Some(text) = &c.text else {
        return Some((c.score, c.positions.clone()));
    };
    let opts = MatchOptions { strict: c.strict };
    if let Some(m) = fuzzy_match_with(query, text, opts) {
        return Some((m.score + c.score, m.positions));
    }
    if let Some(m) = c
        .alt_texts
        .iter()
        .find_map(|t| fuzzy_match_with(query, t, opts))
    {
        return Some((m.score - ALT_TEXT_PENALTY + c.score, vec![]));
    }
    c.keep_unmatched.then(|| (c.score, vec![]))
}

/// Merge, score, dedupe and cut the candidates of a Search Everywhere request.
pub fn rank(req: &RankRequest) -> RankResponse {
    let mut sources: Vec<&Source> = req
        .sources
        .iter()
        .filter(|s| req.tab == "all" || s.tab == req.tab)
        .collect();
    // Stable: equal tab and order keep the registration order.
    sources.sort_by_key(|s| (tab_rank(&s.tab), s.order));

    // Providers serving the same tab are merged into one group (e.g. PHP + LSP classes).
    struct Group<'a> {
        tab: &'a str,
        provider: &'a str,
        items: Vec<(RankedItem, &'a Candidate)>,
    }
    let mut groups: Vec<Group> = Vec::new();
    for s in sources {
        let idx = match groups.iter().position(|g| g.tab == s.tab) {
            Some(i) => i,
            None => {
                groups.push(Group {
                    tab: &s.tab,
                    provider: &s.provider,
                    items: vec![],
                });
                groups.len() - 1
            }
        };
        for (index, c) in s.items.iter().enumerate() {
            if let Some((score, positions)) = score(&req.query, c) {
                let item = RankedItem {
                    provider: s.provider.clone(),
                    index,
                    score,
                    positions,
                };
                groups[idx].items.push((item, c));
            }
        }
    }

    // In "All", an item already listed by an earlier group (e.g. a class that is also a
    // symbol) is not repeated.
    let mut seen = std::collections::HashSet::new();
    let out = groups
        .into_iter()
        .filter_map(|mut g| {
            if req.tab == "all" {
                g.items.retain(|(_, c)| {
                    seen.insert(format!("{}:{}", c.kind.as_deref().unwrap_or(""), c.id))
                });
            }
            // Like PHPStorm, dimmed items (disabled actions) only show when asked for or
            // when nothing else matches.
            if !req.include_dimmed && g.items.iter().any(|(_, c)| !c.dimmed) {
                g.items.retain(|(_, c)| !c.dimmed);
            }
            g.items.sort_by_key(|(i, _)| std::cmp::Reverse(i.score));
            let more = g.items.len() > req.per_group;
            let items: Vec<RankedItem> = g
                .items
                .into_iter()
                .take(req.per_group)
                .map(|(i, _)| i)
                .collect();
            (!items.is_empty()).then(|| RankedGroup {
                tab: g.tab.to_string(),
                provider: g.provider.to_string(),
                items,
                more,
            })
        })
        .collect();
    RankResponse { groups: out }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pre(id: &str, score: i32, kind: &str) -> Candidate {
        Candidate {
            id: id.into(),
            kind: Some(kind.into()),
            score,
            ..Default::default()
        }
    }

    fn source(provider: &str, tab: &str, items: Vec<Candidate>) -> Source {
        Source {
            provider: provider.into(),
            tab: tab.into(),
            order: 100,
            items,
        }
    }

    fn ids(req: &RankRequest, g: &RankedGroup) -> Vec<String> {
        g.items
            .iter()
            .map(|i| {
                let s = req
                    .sources
                    .iter()
                    .find(|s| s.provider == i.provider)
                    .unwrap();
                s.items[i.index].id.clone()
            })
            .collect()
    }

    fn request(tab: &str, per_group: usize) -> RankRequest {
        RankRequest {
            tab: tab.into(),
            query: "x".into(),
            per_group,
            include_dimmed: false,
            sources: vec![
                source(
                    "t.symbols",
                    "symbols",
                    vec![pre("B", 9, "class"), pre("m", 2, "method")],
                ),
                source(
                    "t.classes",
                    "classes",
                    vec![pre("A", 1, "class"), pre("B", 5, "class")],
                ),
                source("t.classes2", "classes", vec![pre("C", 3, "class")]),
            ],
        }
    }

    #[test]
    fn merges_providers_of_one_tab_and_sorts_by_score() {
        let req = request("classes", 10);
        let res = rank(&req);
        assert_eq!(res.groups.len(), 1);
        assert_eq!(ids(&req, &res.groups[0]), vec!["B", "C", "A"]);
        assert_eq!(res.groups[0].provider, "t.classes");
    }

    #[test]
    fn orders_groups_by_tab_dedupes_and_truncates_in_all() {
        let req = request("all", 2);
        let res = rank(&req);
        let tabs: Vec<_> = res.groups.iter().map(|g| g.tab.as_str()).collect();
        assert_eq!(tabs, vec!["classes", "symbols"]);
        assert_eq!(ids(&req, &res.groups[0]), vec!["B", "C"]);
        assert!(res.groups[0].more);
        // "B" was already listed as a class.
        assert_eq!(ids(&req, &res.groups[1]), vec!["m"]);
        assert!(!res.groups[1].more);
    }

    #[test]
    fn matches_text_candidates_strictly_with_alternatives() {
        let action = |id: &str, title: &str, dimmed: bool| Candidate {
            id: id.into(),
            kind: Some("action".into()),
            text: Some(title.into()),
            alt_texts: vec![id.into()],
            strict: true,
            score: if dimmed { 0 } else { 5 },
            dimmed,
            ..Default::default()
        };
        let mut req = RankRequest {
            tab: "actions".into(),
            query: "line".into(),
            per_group: 10,
            include_dimmed: false,
            sources: vec![source(
                "core.actions",
                "actions",
                vec![
                    action("MoveLineUp", "Move Line Up", false),
                    action("CollapseRecursively", "Collapse Recursively", false),
                    action("EditorDuplicateLines", "Duplicate Line or Selection", true),
                    action("GotoLine", "Go to Line:Column", false),
                ],
            )],
        };
        let res = rank(&req);
        let got = ids(&req, &res.groups[0]);
        assert_eq!(got.len(), 2, "{got:?}");
        assert!(got.contains(&"MoveLineUp".to_string()));
        assert_eq!(res.groups[0].items[0].positions.len(), 4);

        // Disabled actions show when asked for, or when nothing else matches.
        req.include_dimmed = true;
        assert_eq!(ids(&req, &rank(&req).groups[0]).len(), 3);
        req.include_dimmed = false;
        req.query = "dupl".into();
        assert_eq!(
            ids(&req, &rank(&req).groups[0]),
            vec!["EditorDuplicateLines"]
        );

        // Only the id matches: lower score, no highlight.
        req.query = "EditorDup".into();
        let res = rank(&req);
        assert_eq!(ids(&req, &res.groups[0]), vec!["EditorDuplicateLines"]);
        assert!(res.groups[0].items[0].positions.is_empty());
    }

    #[test]
    fn keeps_unmatched_items_of_pre_filtered_sources() {
        let sym = |name: &str| Candidate {
            id: name.into(),
            text: Some(name.into()),
            keep_unmatched: true,
            score: -1,
            ..Default::default()
        };
        let req = RankRequest {
            tab: "symbols".into(),
            query: "usr".into(),
            per_group: 10,
            include_dimmed: false,
            sources: vec![source("lsp", "symbols", vec![sym("handle"), sym("User")])],
        };
        let res = rank(&req);
        assert_eq!(ids(&req, &res.groups[0]), vec!["User", "handle"]);
        assert_eq!(res.groups[0].items[1].positions, Vec::<usize>::new());
    }

    #[test]
    fn deserializes_ui_requests() {
        let req: RankRequest = serde_json::from_str(
            r#"{"tab":"all","query":"a","perGroup":5,"sources":[{"provider":"p","tab":"files","items":[{"id":"a.php","score":3,"positions":[0]}]}]}"#,
        )
        .unwrap();
        let res = rank(&req);
        assert_eq!(res.groups[0].items[0].positions, vec![0]);
    }
}
