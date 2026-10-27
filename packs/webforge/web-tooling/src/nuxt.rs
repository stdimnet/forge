//! Nuxt: file-based routes of `pages/` and the auto-imported components of
//! `components/` (with their path prefixes and `Lazy` variants), for the Endpoints view
//! and Go to Declaration on component tags. Nuxt 3 / 4 layouts (`app/pages`,
//! `app/components`) are understood too.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Route {
    /// `/users/:id`.
    pub route: String,
    /// The route name Nuxt generates (`users-id`).
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Component {
    /// `BaseButton`.
    pub name: String,
    pub path: String,
}

const PAGE_EXTS: &[&str] = &["vue", "tsx", "jsx", "ts", "js"];
const COMPONENT_EXTS: &[&str] = &["vue", "tsx", "jsx", "ts", "js"];

/// The source folders of a Nuxt project at `root` (`""` or a subfolder): `app/` (Nuxt 4)
/// or the root.
fn source_dirs(root: &str) -> Vec<String> {
    let base = if root.is_empty() {
        String::new()
    } else {
        format!("{}/", root.trim_end_matches('/'))
    };
    vec![format!("{base}app/"), base]
}

fn split_ext<'a>(name: &'a str, exts: &[&str]) -> Option<&'a str> {
    let (stem, ext) = name.rsplit_once('.')?;
    exts.contains(&ext).then_some(stem)
}

fn segment(seg: &str) -> (String, String) {
    // `[...slug]` catch-all, `[[id]]` optional, `[id]` dynamic.
    if let Some(inner) = seg.strip_prefix("[...").and_then(|s| s.strip_suffix(']')) {
        return (format!(":{inner}(.*)*"), inner.to_string());
    }
    if let Some(inner) = seg.strip_prefix("[[").and_then(|s| s.strip_suffix("]]")) {
        return (format!(":{inner}?"), inner.to_string());
    }
    let mut route = String::new();
    let mut rest = seg;
    while let Some(i) = rest.find('[') {
        route.push_str(&rest[..i]);
        let Some(j) = rest[i..].find(']') else { break };
        route.push(':');
        route.push_str(&rest[i + 1..i + j]);
        rest = &rest[i + j + 1..];
    }
    route.push_str(rest);
    let name = seg.replace(['[', ']'], "").replace("...", "");
    (route, name)
}

/// The routes of the pages of a Nuxt project, sorted by route.
pub fn routes<'a>(root: &str, paths: impl IntoIterator<Item = &'a str>) -> Vec<Route> {
    let dirs = source_dirs(root);
    let mut out = Vec::new();
    for p in paths {
        let Some(rel) = dirs
            .iter()
            .find_map(|d| p.strip_prefix(&format!("{d}pages/")))
        else {
            continue;
        };
        let (dir, file) = rel.rsplit_once('/').unwrap_or(("", rel));
        let Some(stem) = split_ext(file, PAGE_EXTS) else { continue };
        if stem.starts_with('_') || rel.split('/').any(|s| s.starts_with('.')) {
            continue;
        }
        let mut segs: Vec<&str> = dir.split('/').filter(|s| !s.is_empty()).collect();
        // Route groups `(admin)/` do not appear in the URL.
        segs.retain(|s| !(s.starts_with('(') && s.ends_with(')')));
        if stem != "index" {
            segs.push(stem);
        }
        let parts: Vec<(String, String)> = segs.iter().map(|s| segment(s)).collect();
        let route = format!("/{}", parts.iter().map(|(r, _)| r.as_str()).collect::<Vec<_>>().join("/"));
        let name = if parts.is_empty() {
            "index".to_string()
        } else {
            parts.iter().map(|(_, n)| n.as_str()).collect::<Vec<_>>().join("-")
        };
        out.push(Route {
            route,
            name,
            path: p.to_string(),
        });
    }
    out.sort_by(|a, b| a.route.cmp(&b.route));
    out
}

fn pascal(part: &str) -> String {
    part.split(['-', '_', '.'])
        .filter(|s| !s.is_empty())
        .map(|s| {
            let mut c = s.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

/// The auto-imported components: `components/base/Button.vue` → `BaseButton`
/// (directory prefix, duplicated prefixes collapsed like Nuxt: `base/BaseButton.vue` →
/// `BaseButton`), `index` files named after their folder.
pub fn components<'a>(root: &str, paths: impl IntoIterator<Item = &'a str>) -> Vec<Component> {
    let dirs = source_dirs(root);
    let mut out: Vec<Component> = Vec::new();
    for p in paths {
        let Some(rel) = dirs
            .iter()
            .find_map(|d| p.strip_prefix(&format!("{d}components/")))
        else {
            continue;
        };
        let (dir, file) = rel.rsplit_once('/').unwrap_or(("", rel));
        let Some(stem) = split_ext(file, COMPONENT_EXTS) else { continue };
        // `Button.client.vue` / `.server.vue` keep the base name.
        let stem = stem
            .strip_suffix(".client")
            .or_else(|| stem.strip_suffix(".server"))
            .unwrap_or(stem);
        let prefix: String = dir.split('/').filter(|s| !s.is_empty()).map(pascal).collect();
        let base = if stem == "index" { String::new() } else { pascal(stem) };
        let name = if !prefix.is_empty() && base.starts_with(&prefix) {
            base
        } else {
            format!("{prefix}{base}")
        };
        if name.is_empty() || out.iter().any(|c| c.name == name) {
            continue;
        }
        out.push(Component {
            name,
            path: p.to_string(),
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// The component a template tag refers to: `<BaseButton>`, `<base-button>`,
/// `<LazyBaseButton>`.
pub fn component_for_tag<'a>(components: &'a [Component], tag: &str) -> Option<&'a Component> {
    let name = if tag.contains('-') { pascal(tag) } else { tag.to_string() };
    let name = name.strip_prefix("Lazy").filter(|n| !n.is_empty()).unwrap_or(&name);
    components.iter().find(|c| c.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILES: &[&str] = &[
        "pages/index.vue",
        "pages/about.vue",
        "pages/users/index.vue",
        "pages/users/[id].vue",
        "pages/users/[id]/edit.vue",
        "pages/docs/[...slug].vue",
        "pages/(marketing)/pricing.vue",
        "pages/[[lang]]/help.vue",
        "pages/_partial.vue",
        "components/AppHeader.vue",
        "components/base/Button.vue",
        "components/base/BaseInput.vue",
        "components/form/index.vue",
        "components/Chart.client.vue",
        "components/readme.md",
        "server/api/hello.ts",
    ];

    #[test]
    fn routes_from_pages() {
        let r = routes("", FILES.iter().copied());
        let got: Vec<_> = r.iter().map(|r| (r.route.as_str(), r.name.as_str())).collect();
        assert_eq!(
            got,
            vec![
                ("/", "index"),
                ("/:lang?/help", "lang-help"),
                ("/about", "about"),
                ("/docs/:slug(.*)*", "docs-slug"),
                ("/pricing", "pricing"),
                ("/users", "users"),
                ("/users/:id", "users-id"),
                ("/users/:id/edit", "users-id-edit"),
            ]
        );
        // Nuxt 4 layout.
        let r = routes("web", ["web/app/pages/index.vue"]);
        assert_eq!(r[0].route, "/");
    }

    #[test]
    fn auto_imported_components() {
        let c = components("", FILES.iter().copied());
        let names: Vec<_> = c.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["AppHeader", "BaseButton", "BaseInput", "Chart", "Form"]);
        assert_eq!(component_for_tag(&c, "base-button").unwrap().path, "components/base/Button.vue");
        assert_eq!(component_for_tag(&c, "LazyAppHeader").unwrap().name, "AppHeader");
        assert!(component_for_tag(&c, "NuxtLink").is_none());
    }
}
