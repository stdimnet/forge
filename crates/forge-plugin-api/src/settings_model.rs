//! The model behind the Settings dialog (Ctrl+Alt+S): the page tree built from registered
//! pages, search over pages, settings and plugins, and highlighting of search hits. The UI
//! sends what it shows and renders the answer; this module decides what matches.
//!
//! Pure (no file system or state) so the browser-mode UI runs it as WebAssembly.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::contributions::SettingSpec;

/// PHPStorm's top-level order; unknown groups follow alphabetically.
pub const GROUP_ORDER: &[&str] = &[
    "Appearance & Behavior",
    "Keymap",
    "Editor",
    "Plugins",
    "Version Control",
    // PyCharm's "Project: <name>" (Python Interpreter).
    "Project",
    "Directories",
    "Build, Execution, Deployment",
    "Languages & Frameworks",
    "Tools",
    "AI",
    "Advanced Settings",
];

/// Catalog key of a top-level group's title: `settings.group.<slug>`.
pub fn group_title_key(group: &str) -> String {
    format!("settings.group.{}", forge_i18n::slug(group))
}

/// The title of a top-level group in the IDE language (the group name is its English title
/// and stays the id).
pub fn group_title(group: &str) -> String {
    forge_i18n::t_or(&group_title_key(group), group)
}

/// Node id of a group without a page of its own.
pub fn group_node_id(group: &str) -> String {
    format!("group:{group}")
}

/// A settings page as registered by the UI (`registerSettingsPage`).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageInput {
    pub id: String,
    pub title: String,
    pub group: String,
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
    /// Searchable text beyond title, keywords and the page's schema settings (e.g. the
    /// plugin list of Settings > Plugins).
    #[serde(default)]
    pub text: String,
}

/// A node of the page tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeNode {
    /// Page id, or `group:<name>` for a group without a page of its own.
    pub id: String,
    pub title: String,
    /// Whether `id` is a page (otherwise a group node).
    pub page: bool,
    /// Ids of the pages in this subtree, including this node's own.
    pub pages: Vec<String>,
    pub children: Vec<TreeNode>,
}

fn group_rank(g: &str) -> usize {
    GROUP_ORDER
        .iter()
        .position(|x| *x == g)
        .unwrap_or(GROUP_ORDER.len())
}

/// Pages become a tree: group > page > child pages (via `parent`). A page whose title
/// equals its group (or the group's translated title) and that has no parent (e.g. "Plugins",
/// "Keymap") is a top-level leaf.
pub fn build_tree(pages: &[PageInput]) -> Vec<TreeNode> {
    // Children by parent page id, in registration order.
    let mut children: BTreeMap<&str, Vec<&PageInput>> = BTreeMap::new();
    let known = |id: &str| pages.iter().any(|p| p.id == id);
    // Top-level entries: (group, Some(page)) for leaf pages, (group, None) for group nodes.
    let mut roots: Vec<(&str, Option<&PageInput>)> = Vec::new();
    let mut grouped: BTreeMap<&str, Vec<&PageInput>> = BTreeMap::new();
    for page in pages {
        match page.parent.as_deref() {
            Some(parent) if parent != page.id && known(parent) => {
                children.entry(parent).or_default().push(page);
            }
            None if page.title == page.group || page.title == group_title(&page.group) => {
                roots.push((&page.group, Some(page)))
            }
            _ => {
                if !grouped.contains_key(page.group.as_str()) {
                    roots.push((&page.group, None));
                }
                grouped.entry(&page.group).or_default().push(page);
            }
        }
    }

    fn node(page: &PageInput, children: &BTreeMap<&str, Vec<&PageInput>>) -> TreeNode {
        let kids: Vec<TreeNode> = children
            .get(page.id.as_str())
            .map(|v| v.iter().map(|p| node(p, children)).collect())
            .unwrap_or_default();
        let mut pages = vec![page.id.clone()];
        pages.extend(kids.iter().flat_map(|k| k.pages.iter().cloned()));
        TreeNode {
            id: page.id.clone(),
            title: page.title.clone(),
            page: true,
            pages,
            children: kids,
        }
    }

    let mut out: Vec<(&str, TreeNode)> = roots
        .into_iter()
        .map(|(group, page)| match page {
            Some(p) => (group, node(p, &children)),
            None => {
                let kids: Vec<TreeNode> =
                    grouped[group].iter().map(|p| node(p, &children)).collect();
                let pages = kids.iter().flat_map(|k| k.pages.iter().cloned()).collect();
                let n = TreeNode {
                    id: group_node_id(group),
                    title: group_title(group),
                    page: false,
                    pages,
                    children: kids,
                };
                (group, n)
            }
        })
        .collect();
    out.sort_by(|(a, _), (b, _)| group_rank(a).cmp(&group_rank(b)).then_with(|| a.cmp(b)));
    out.into_iter().map(|(_, n)| n).collect()
}

/// Depth-first list of nodes.
pub fn flatten(nodes: &[TreeNode]) -> Vec<&TreeNode> {
    let mut out = Vec::new();
    fn walk<'a>(nodes: &'a [TreeNode], out: &mut Vec<&'a TreeNode>) {
        for n in nodes {
            out.push(n);
            walk(&n.children, out);
        }
    }
    walk(nodes, &mut out);
    out
}

/// Ids from the root to every node, e.g. `editor.code-style.php` →
/// `["group:Editor", "editor.code-style", "editor.code-style.php"]`.
pub fn breadcrumbs(nodes: &[TreeNode]) -> BTreeMap<String, Vec<String>> {
    let mut out = BTreeMap::new();
    fn walk(nodes: &[TreeNode], path: &mut Vec<String>, out: &mut BTreeMap<String, Vec<String>>) {
        for n in nodes {
            path.push(n.id.clone());
            out.insert(n.id.clone(), path.clone());
            walk(&n.children, path, out);
            path.pop();
        }
    }
    walk(nodes, &mut vec![], &mut out);
    out
}

fn words(query: &str) -> Vec<String> {
    query
        .to_lowercase()
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

/// Every word of `query` occurs in `text` (case-insensitive). An empty query matches.
pub fn matches_query(text: &str, query: &str) -> bool {
    let hay = text.to_lowercase();
    words(query).iter().all(|w| hay.contains(w.as_str()))
}

/// A run of text, marked when it is a search hit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Segment {
    pub text: String,
    pub hit: bool,
}

/// Split `text` into segments, marking occurrences of any query word as hits.
pub fn highlight(text: &str, query: &str) -> Vec<Segment> {
    let ws = words(query);
    let chars: Vec<char> = text.chars().collect();
    if ws.is_empty() {
        return vec![Segment {
            text: text.to_string(),
            hit: false,
        }];
    }
    // Compare per char (lower-cased one to one) so indices stay aligned with `text`.
    let lower: Vec<char> = chars
        .iter()
        .map(|c| c.to_lowercase().next().unwrap_or(*c))
        .collect();
    let mut hits = vec![false; chars.len()];
    for w in &ws {
        let w: Vec<char> = w.chars().collect();
        if w.len() > lower.len() {
            continue;
        }
        for i in 0..=lower.len() - w.len() {
            if lower[i..i + w.len()] == w[..] {
                hits[i..i + w.len()].iter_mut().for_each(|h| *h = true);
            }
        }
    }
    let mut out: Vec<Segment> = Vec::new();
    for (c, hit) in chars.into_iter().zip(hits) {
        match out.last_mut() {
            Some(last) if last.hit == hit => last.text.push(c),
            _ => out.push(Segment {
                text: c.to_string(),
                hit,
            }),
        }
    }
    out
}

/// One generic searchable item, e.g. a plugin in Settings > Plugins.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SearchEntry {
    pub id: String,
    pub text: String,
}

/// Request of the `settings_search` IPC command (and its WebAssembly twin).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchRequest {
    pub query: String,
    /// Settings pages; omitted when only entries are searched.
    #[serde(default)]
    pub pages: Vec<PageInput>,
    /// The settings schema; each setting is searchable on the page of its `group`.
    #[serde(default)]
    pub settings: Vec<SettingSpec>,
    #[serde(default)]
    pub entries: Vec<SearchEntry>,
    /// Texts to split into highlighted segments.
    #[serde(default)]
    pub highlight: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResponse {
    /// The full page tree.
    pub tree: Vec<TreeNode>,
    /// The tree filtered to matching nodes and their ancestors (the full tree for an empty
    /// query).
    pub visible: Vec<TreeNode>,
    /// Nodes that match the query themselves.
    pub matches: Vec<String>,
    /// The page to select for this query: the first matching page, else the first page.
    pub first_match: Option<String>,
    /// Ids from the root to each node.
    pub breadcrumbs: BTreeMap<String, Vec<String>>,
    /// Keys of the settings that match (highlighted rows).
    pub settings: Vec<String>,
    /// Ids of the matching entries (all of them for an empty query).
    pub entries: Vec<String>,
    /// Segments of each `highlight` text, in request order.
    pub highlights: Vec<Vec<Segment>>,
}

/// Searchable text of one setting row.
fn setting_text(s: &SettingSpec) -> String {
    format!(
        "{} {} {}",
        s.title,
        s.description,
        s.section.as_deref().unwrap_or("")
    )
}

/// Search the Settings dialog.
pub fn search(req: &SearchRequest) -> SearchResponse {
    let has_query = !words(&req.query).is_empty();
    let tree = build_tree(&req.pages);

    let page_text: BTreeMap<&str, String> = req
        .pages
        .iter()
        .map(|p| {
            let schema: Vec<String> = req
                .settings
                .iter()
                .filter(|s| s.group == p.id)
                .map(|s| format!("{} {}", setting_text(s), s.key))
                .collect();
            let text = [
                p.title.clone(),
                p.keywords.join(" \n "),
                p.text.clone(),
                schema.join("\n"),
            ]
            .join(" \n ");
            (p.id.as_str(), text)
        })
        .collect();
    let node_matches = |n: &TreeNode| {
        has_query && {
            let text = if n.page {
                page_text.get(n.id.as_str()).cloned().unwrap_or_default()
            } else {
                n.title.clone()
            };
            matches_query(&text, &req.query)
        }
    };

    fn filter(nodes: &[TreeNode], keep: &dyn Fn(&TreeNode) -> bool) -> Vec<TreeNode> {
        nodes
            .iter()
            .filter_map(|n| {
                let children = filter(&n.children, keep);
                (!children.is_empty() || keep(n)).then(|| TreeNode {
                    children,
                    ..n.clone()
                })
            })
            .collect()
    }
    let visible = if has_query {
        filter(&tree, &node_matches)
    } else {
        tree.clone()
    };
    let matches: Vec<String> = flatten(&tree)
        .into_iter()
        .filter(|n| node_matches(n))
        .map(|n| n.id.clone())
        .collect();
    let flat_visible = flatten(&visible);
    let first_match = flat_visible
        .iter()
        .find(|n| n.page && matches.contains(&n.id))
        .or_else(|| flat_visible.iter().find(|n| n.page))
        .map(|n| n.id.clone());

    SearchResponse {
        breadcrumbs: breadcrumbs(&tree),
        tree,
        visible,
        matches,
        first_match,
        settings: req
            .settings
            .iter()
            .filter(|s| has_query && matches_query(&setting_text(s), &req.query))
            .map(|s| s.key.clone())
            .collect(),
        entries: req
            .entries
            .iter()
            .filter(|e| matches_query(&e.text, &req.query))
            .map(|e| e.id.clone())
            .collect(),
        highlights: req
            .highlight
            .iter()
            .map(|t| highlight(t, &req.query))
            .collect(),
    }
}

/// Request of the `settings_validate` IPC command.
#[derive(Debug, Clone, Deserialize)]
pub struct ValidateRequest {
    pub spec: SettingSpec,
    pub value: Value,
}

/// Check a value against its setting's type and constraints; `None` when valid.
pub fn validate(req: &ValidateRequest) -> Option<String> {
    req.spec
        .kind
        .validate(&req.value)
        .err()
        .map(|e| format!("{}: {e}", req.spec.title))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contributions::SettingType;
    use serde_json::json;

    fn page(id: &str, title: &str, group: &str, parent: Option<&str>) -> PageInput {
        PageInput {
            id: id.into(),
            title: title.into(),
            group: group.into(),
            parent: parent.map(str::to_string),
            ..Default::default()
        }
    }

    fn pages() -> Vec<PageInput> {
        let mut plugins = page("plugins", "Plugins", "Plugins", None);
        plugins.keywords = vec!["marketplace".into()];
        vec![
            page("tools.terminal", "Terminal", "Tools", None),
            page(
                "editor.code-style.php",
                "PHP",
                "Editor",
                Some("editor.code-style"),
            ),
            page("editor.code-style", "Code Style", "Editor", None),
            plugins,
            page("appearance", "Appearance", "Appearance & Behavior", None),
            page("custom", "Custom", "Zeta", None),
        ]
    }

    fn spec(key: &str, title: &str, group: &str) -> SettingSpec {
        SettingSpec {
            key: key.into(),
            title: title.into(),
            description: String::new(),
            kind: SettingType::Bool,
            default: json!(true),
            group: group.into(),
            section: None,
            project: false,
            hidden: false,
            order: 0,
        }
    }

    fn ids(nodes: &[TreeNode]) -> Vec<&str> {
        flatten(nodes).iter().map(|n| n.id.as_str()).collect()
    }

    #[test]
    fn orders_groups_like_phpstorm_and_keeps_title_eq_group_pages_top_level() {
        let tree = build_tree(&pages());
        let titles: Vec<_> = tree.iter().map(|n| n.title.as_str()).collect();
        assert_eq!(
            titles,
            vec![
                "Appearance & Behavior",
                "Editor",
                "Plugins",
                "Tools",
                "Zeta"
            ]
        );
        assert_eq!(tree[2].id, "plugins");
        assert!(tree[2].page);
        assert_eq!(tree[0].id, group_node_id("Appearance & Behavior"));
        assert!(!tree[0].page);
    }

    #[test]
    fn nests_pages_under_their_parent_page() {
        let tree = build_tree(&pages());
        assert_eq!(
            breadcrumbs(&tree)["editor.code-style.php"],
            vec!["group:Editor", "editor.code-style", "editor.code-style.php"]
        );
        assert_eq!(flatten(&tree).len(), pages().len() + 4);
        assert_eq!(
            tree[1].pages,
            vec!["editor.code-style", "editor.code-style.php"]
        );
    }

    #[test]
    fn filters_by_title_keywords_page_text_and_settings_keeping_ancestors() {
        let req = SearchRequest {
            query: "braces".into(),
            pages: pages(),
            settings: vec![spec(
                "php.braces",
                "Braces placement",
                "editor.code-style.php",
            )],
            ..Default::default()
        };
        let res = search(&req);
        assert_eq!(
            ids(&res.visible),
            vec!["group:Editor", "editor.code-style", "editor.code-style.php"]
        );
        assert_eq!(res.matches, vec!["editor.code-style.php"]);
        assert_eq!(res.first_match.as_deref(), Some("editor.code-style.php"));
        assert_eq!(res.settings, vec!["php.braces"]);

        let res = search(&SearchRequest {
            query: "marketplace".into(),
            ..req.clone()
        });
        assert_eq!(
            res.visible
                .iter()
                .map(|n| n.id.as_str())
                .collect::<Vec<_>>(),
            vec!["plugins"]
        );

        let res = search(&SearchRequest {
            query: "  ".into(),
            ..req
        });
        assert_eq!(res.visible, res.tree);
        assert!(res.matches.is_empty() && res.settings.is_empty());
        assert_eq!(res.first_match.as_deref(), Some("appearance"));
    }

    #[test]
    fn matches_all_words_case_insensitively() {
        assert!(matches_query("Show tool window bars", "TOOL bars"));
        assert!(!matches_query("Show tool window bars", "tool font"));
        assert!(matches_query("anything", ""));
    }

    #[test]
    fn splits_text_into_highlighted_segments() {
        let seg = |t: &str, hit| Segment {
            text: t.into(),
            hit,
        };
        assert_eq!(
            highlight("Editor Font", "font"),
            vec![seg("Editor ", false), seg("Font", true)]
        );
        assert_eq!(highlight("abc", ""), vec![seg("abc", false)]);
        assert_eq!(
            highlight("Größe größer", "GRÖ"),
            vec![
                seg("Grö", true),
                seg("ße ", false),
                seg("grö", true),
                seg("ßer", false)
            ]
        );
    }

    #[test]
    fn searches_entries_and_highlights_texts() {
        let res = search(&SearchRequest {
            query: "php".into(),
            entries: vec![
                SearchEntry {
                    id: "forge.php".into(),
                    text: "PHP forge.php".into(),
                },
                SearchEntry {
                    id: "forge.git".into(),
                    text: "Git".into(),
                },
            ],
            highlight: vec!["PHP language".into()],
            ..Default::default()
        });
        assert_eq!(res.entries, vec!["forge.php"]);
        assert!(res.highlights[0][0].hit);
        assert!(res.tree.is_empty());
    }

    #[test]
    fn validates_values_with_the_setting_title() {
        let mut s = spec("editor.font.size", "Font size", "editor.font");
        s.kind = SettingType::Int {
            min: Some(6),
            max: Some(72),
        };
        let check = |v| {
            validate(&ValidateRequest {
                spec: s.clone(),
                value: v,
            })
        };
        assert_eq!(check(json!(13)), None);
        assert_eq!(
            check(json!(100)).as_deref(),
            Some("Font size: must be at most 72")
        );
        assert!(check(json!("13")).is_some());
    }
}
