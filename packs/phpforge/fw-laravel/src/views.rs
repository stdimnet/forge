//! Views and Blade components: `view('orders.show')` names, anonymous components in
//! `resources/views/components`, class components in `app/View/Components`, Livewire
//! components and custom directives registered with `Blade::directive` / `Blade::if`.

use std::path::Path;

use phpforge_lang_php::parser::index_source;
use phpforge_lang_php::SymbolKind;
use serde::{Deserialize, Serialize};

use crate::project::{files_under, read, Location, Psr4};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    /// Dotted name (`orders.show`).
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Component {
    /// Tag as used in Blade (`x-alert`, `x-forms.input`, `livewire:counter`).
    pub tag: String,
    /// Blade template of the component.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view: Option<String>,
    /// Component class (FQN) for class-based and Livewire components.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class_path: Option<String>,
    /// Attributes the component accepts (`@props` or public constructor properties).
    #[serde(default)]
    pub props: Vec<String>,
}

impl Component {
    /// Where Ctrl+B on the tag goes: the class if there is one, else the template.
    pub fn location(&self) -> Option<Location> {
        self.class_path
            .as_deref()
            .or(self.view.as_deref())
            .map(Location::file)
    }
}

/// A Blade directive contributed by the application.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomDirective {
    pub name: String,
    /// `directive` or `if` (which also defines `else<name>`, `unless<name>` and `end<name>`).
    pub kind: String,
    pub location: Location,
}

const VIEW_DIR: &str = "resources/views";

fn view_name(rel_in_views: &str) -> Option<String> {
    let stem = rel_in_views
        .strip_suffix(".blade.php")
        .or_else(|| rel_in_views.strip_suffix(".php"))?;
    Some(stem.replace('/', "."))
}

pub fn list_views(root: &Path) -> Vec<View> {
    files_under(root, VIEW_DIR, ".php")
        .into_iter()
        .filter_map(|path| {
            let inner = path.strip_prefix(VIEW_DIR)?.trim_start_matches('/');
            Some(View {
                name: view_name(inner)?,
                path,
            })
        })
        .collect()
}

/// `UserProfile` → `user-profile`.
pub fn kebab(name: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = name.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        if c.is_uppercase() {
            let prev_lower =
                i > 0 && (chars[i - 1].is_lowercase() || chars[i - 1].is_ascii_digit());
            let next_lower = chars.get(i + 1).is_some_and(|n| n.is_lowercase());
            if i > 0 && (prev_lower || (next_lower && chars[i - 1].is_uppercase())) {
                out.push('-');
            }
            out.extend(c.to_lowercase());
        } else {
            out.push(*c);
        }
    }
    out
}

/// Names from `@props(['type' => 'info', 'message'])`.
fn blade_props(src: &str) -> Vec<String> {
    let Some(start) = src.find("@props(") else {
        return Vec::new();
    };
    let rest = &src[start + 7..];
    let end = rest.find("])").map_or(rest.len(), |e| e + 1);
    let mut props = Vec::new();
    let body = &rest[..end];
    let bytes = body.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let q = bytes[i];
        if q == b'\'' || q == b'"' {
            if let Some(len) = body[i + 1..].find(q as char) {
                let s = &body[i + 1..i + 1 + len];
                let after = body[i + 1 + len + 1..].trim_start();
                // Keys (`'type' =>`) and list values (`'message',` / `'message']`) are props.
                let prev = body[..i].trim_end();
                let is_value = prev.ends_with("=>");
                if !is_value
                    && (after.starts_with("=>") || after.starts_with(',') || after.starts_with(']'))
                {
                    props.push(s.to_string());
                }
                i += len + 2;
                continue;
            }
        }
        i += 1;
    }
    props
}

pub fn list_components(root: &Path, psr4: &Psr4) -> Vec<Component> {
    let mut out: Vec<Component> = Vec::new();
    let comp_dir = format!("{VIEW_DIR}/components");
    // Anonymous components.
    for path in files_under(root, &comp_dir, ".blade.php") {
        let Some(inner) = path.strip_prefix(&comp_dir) else {
            continue;
        };
        let Some(mut name) = view_name(inner.trim_start_matches('/')) else {
            continue;
        };
        if let Some(dir) = name.strip_suffix(".index") {
            name = dir.to_string();
        }
        let props = read(root, &path)
            .map(|s| blade_props(&s))
            .unwrap_or_default();
        out.push(Component {
            tag: format!("x-{name}"),
            view: Some(path),
            class: None,
            class_path: None,
            props,
        });
    }
    // Class components.
    let class_dir = "app/View/Components";
    for path in files_under(root, class_dir, ".php") {
        let Some(inner) = path
            .strip_prefix(class_dir)
            .and_then(|p| p.strip_suffix(".php"))
        else {
            continue;
        };
        let tag_name: Vec<String> = inner
            .trim_start_matches('/')
            .split('/')
            .map(kebab)
            .collect();
        let tag = format!("x-{}", tag_name.join("."));
        let src = read(root, &path).unwrap_or_default();
        let index = index_source(&path, &src);
        let class = index.classes.first();
        let mut props: Vec<String> = Vec::new();
        if let Some(c) = class {
            if let Some(ctor) = c.method("__construct") {
                props.extend(
                    ctor.params
                        .iter()
                        .map(|p| kebab_prop(p.name.trim_start_matches('$'))),
                );
            }
            props.extend(
                c.members
                    .iter()
                    .filter(|m| {
                        m.kind == SymbolKind::Property
                            && !m.promoted
                            && m.visibility.as_str() == "public"
                    })
                    .map(|m| kebab_prop(&m.name)),
            );
        }
        props.dedup();
        let view = rendered_view(&src).and_then(|v| resolve_view_path(root, &v));
        // An anonymous component with the same tag is replaced by the class one.
        let existing_view = out
            .iter()
            .position(|c| c.tag == tag)
            .map(|i| out.remove(i).view);
        out.push(Component {
            tag,
            view: view.or(existing_view.flatten()),
            class: class
                .map(|c| c.fqn.clone())
                .or_else(|| psr4.class_of(&path)),
            class_path: Some(path),
            props,
        });
    }
    // Livewire components.
    for dir in ["app/Livewire", "app/Http/Livewire"] {
        for path in files_under(root, dir, ".php") {
            let Some(inner) = path.strip_prefix(dir).and_then(|p| p.strip_suffix(".php")) else {
                continue;
            };
            let name: Vec<String> = inner
                .trim_start_matches('/')
                .split('/')
                .map(kebab)
                .collect();
            let src = read(root, &path).unwrap_or_default();
            let view = rendered_view(&src).and_then(|v| resolve_view_path(root, &v));
            let index = index_source(&path, &src);
            let props = index
                .classes
                .first()
                .map(|c| {
                    c.members
                        .iter()
                        .filter(|m| {
                            m.kind == SymbolKind::Property && m.visibility.as_str() == "public"
                        })
                        .map(|m| kebab_prop(&m.name))
                        .collect()
                })
                .unwrap_or_default();
            out.push(Component {
                tag: format!("livewire:{}", name.join(".")),
                view,
                class: psr4.class_of(&path),
                class_path: Some(path),
                props,
            });
        }
    }
    out.sort_by(|a, b| a.tag.cmp(&b.tag));
    out
}

/// `userName` → `user-name` (component attributes are kebab-cased).
fn kebab_prop(name: &str) -> String {
    kebab(name)
}

/// First `view('...')` in a component's `render()`.
fn rendered_view(src: &str) -> Option<String> {
    let i = src.find("view(")?;
    let rest = src[i + 5..].trim_start();
    let q = rest.chars().next().filter(|c| *c == '\'' || *c == '"')?;
    let end = rest[1..].find(q)?;
    Some(rest[1..1 + end].to_string())
}

/// File of a view name (`orders.show` → `resources/views/orders/show.blade.php`).
pub fn resolve_view_path(root: &Path, name: &str) -> Option<String> {
    let (namespace, name) = match name.split_once("::") {
        Some((ns, n)) => (Some(ns), n),
        None => (None, name),
    };
    let rel = name.replace('.', "/");
    let bases: Vec<String> = match namespace {
        Some(ns) => vec![format!("{VIEW_DIR}/vendor/{ns}")],
        None => vec![VIEW_DIR.to_string()],
    };
    for base in bases {
        for ext in [".blade.php", ".php"] {
            let p = format!("{base}/{rel}{ext}");
            if root.join(&p).is_file() {
                return Some(p);
            }
        }
    }
    None
}

/// Directives from `Blade::directive('money', ...)` and `Blade::if('admin', ...)` in
/// `app/` (usually `AppServiceProvider::boot`).
pub fn custom_directives(root: &Path) -> Vec<CustomDirective> {
    let mut out = Vec::new();
    for path in files_under(root, "app/Providers", ".php") {
        let Some(src) = read(root, &path) else {
            continue;
        };
        for (needle, kind) in [("Blade::directive(", "directive"), ("Blade::if(", "if")] {
            let mut from = 0;
            while let Some(i) = src[from..].find(needle) {
                let at = from + i + needle.len();
                from = at;
                let rest = src[at..].trim_start();
                let Some(q) = rest.chars().next().filter(|c| *c == '\'' || *c == '"') else {
                    continue;
                };
                let Some(end) = rest[1..].find(q) else {
                    continue;
                };
                let name = &rest[1..1 + end];
                let name_at = src.len() - rest.len() + 1;
                out.push(CustomDirective {
                    name: name.to_string(),
                    kind: kind.to_string(),
                    location: Location::at(&path, &src, name_at),
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kebab_cases_class_names() {
        assert_eq!(kebab("UserProfile"), "user-profile");
        assert_eq!(kebab("Alert"), "alert");
        assert_eq!(kebab("HTMLEditor"), "html-editor");
        assert_eq!(kebab("Step2Form"), "step2-form");
    }

    #[test]
    fn reads_props_directive() {
        let src = "@props(['type' => 'info', 'message', \"dismissible\" => false])\n<div>";
        assert_eq!(blade_props(src), vec!["type", "message", "dismissible"]);
    }
}
