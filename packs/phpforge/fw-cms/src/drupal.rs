//! Drupal hooks: completion of `<module>_<hook>` implementations after `function`, the
//! implementations of a file (gutter marks) and Go to the hook's documentation, which is the
//! `hook_…` function in the project's own `*.api.php` files (Drupal core ships them) or the
//! hook's page on api.drupal.org.

use serde::{Deserialize, Serialize};

use crate::drupal_hooks::{hook_for, HOOKS};
use crate::{Location, Snapshot};

/// File extensions that hold Drupal hook implementations.
pub const HOOK_FILE_EXTENSIONS: &[&str] = &[
    "module", "install", "theme", "profile", "inc", "php", "engine",
];

/// A module, theme or profile: the folder with its `<name>.info.yml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Extension {
    pub name: String,
    /// Project-relative folder ("" for the root).
    pub dir: String,
    /// `module`, `theme` or `profile` (the `type:` of the info file).
    pub kind: String,
}

/// The machine name of the extension a file belongs to: the nearest folder with an
/// `.info.yml`, else the file name (`acme.module` → `acme`).
pub fn extension_of<'a>(snap: &'a Snapshot, path: &'a str) -> Option<&'a str> {
    let best = snap
        .extensions
        .iter()
        .filter(|e| e.dir.is_empty() || path.starts_with(&format!("{}/", e.dir)))
        .max_by_key(|e| e.dir.len());
    if let Some(e) = best {
        return Some(&e.name);
    }
    let file = path.rsplit('/').next()?;
    let (stem, ext) = file.split_once('.')?;
    matches!(ext, "module" | "install" | "theme" | "profile").then_some(stem)
}

pub fn is_hook_file(path: &str) -> bool {
    let ext = path.rsplit('.').next().unwrap_or("");
    HOOK_FILE_EXTENSIONS.contains(&ext) && !path.ends_with(".api.php")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookCompletion {
    /// `acme_form_alter`.
    pub label: String,
    /// `hook_form_alter`.
    pub hook: String,
    /// The parameters, inserted after the name.
    pub params: String,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionList {
    pub from: usize,
    pub items: Vec<HookCompletion>,
}

/// Hook implementations to complete after `function ` in a hook file.
pub fn complete(snap: &Snapshot, path: &str, src: &str, offset: usize) -> Option<CompletionList> {
    if !is_hook_file(path) {
        return None;
    }
    let module = extension_of(snap, path)?;
    let offset = offset.min(src.len());
    let line_start = src[..offset].rfind('\n').map(|p| p + 1).unwrap_or(0);
    let before = &src[line_start..offset];
    let word_start = before
        .char_indices()
        .rev()
        .take_while(|&(_, c)| c.is_alphanumeric() || c == '_')
        .last()
        .map(|(i, _)| i)
        .unwrap_or(before.len());
    let head = before[..word_start].trim_end();
    if !(head == "function" || head.ends_with(" function") || head.ends_with("\tfunction")) {
        return None;
    }
    let typed = &before[word_start..];
    let prefix = format!("{module}_");
    if !(prefix.starts_with(typed) || typed.starts_with(&prefix)) {
        return None;
    }
    let items = HOOKS
        .iter()
        .map(|&(hook, params, summary)| HookCompletion {
            label: format!("{module}_{hook}"),
            hook: format!("hook_{hook}"),
            params: params.to_string(),
            summary: summary.to_string(),
        })
        .collect();
    Some(CompletionList {
        from: line_start + word_start,
        items,
    })
}

/// A hook implementation in a file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Implementation {
    pub function: String,
    /// `hook_form_FORM_ID_alter`.
    pub hook: String,
    pub summary: String,
    /// 0-based line and byte column of the function name; byte range of the name.
    pub line: u32,
    pub column: u32,
    pub start: usize,
    pub end: usize,
}

/// The hook implementations of `src`: global functions `<module>_<hook>`.
pub fn implementations(snap: &Snapshot, path: &str, src: &str) -> Vec<Implementation> {
    let Some(module) = extension_of(snap, path).filter(|_| is_hook_file(path)) else {
        return Vec::new();
    };
    let prefix = format!("{module}_");
    let mut out = Vec::new();
    let mut offset = 0;
    for (n, line) in src.split_inclusive('\n').enumerate() {
        let t = line.trim_start();
        // Top-level functions only: hooks are never methods.
        if line.len() == t.len() {
            if let Some(rest) = t.strip_prefix("function ") {
                let name_off = line.len() - rest.len();
                let rest = rest.trim_start_matches('&');
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                if let Some(hook) = name.strip_prefix(&prefix).and_then(hook_for) {
                    let col = name_off + (line[name_off..].len() - rest.len());
                    out.push(Implementation {
                        function: name.clone(),
                        hook: format!("hook_{}", hook.0),
                        summary: hook.2.to_string(),
                        line: n as u32,
                        column: col as u32,
                        start: offset + col,
                        end: offset + col + name.len(),
                    });
                }
            }
        }
        offset += line.len();
    }
    out
}

/// The documentation of a hook: its declaration in an `*.api.php` file of the project, and
/// its page on api.drupal.org.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookDocs {
    pub hook: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<Location>,
    pub url: String,
}

/// The hook's page on api.drupal.org.
pub fn docs_url(hook: &str) -> String {
    format!("https://api.drupal.org/api/search/11.x/{hook}")
}

/// `hook_…` declarations in an `*.api.php` file.
pub fn api_hooks(path: &str, src: &str) -> Vec<(String, Location)> {
    src.lines()
        .enumerate()
        .filter_map(|(n, line)| {
            let rest = line.strip_prefix("function ")?;
            let name: String = rest
                .trim_start_matches('&')
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            name.starts_with("hook_").then(|| {
                let col = line.find(&name).unwrap_or(9) as u32;
                (name, Location::new(path, n as u32, col))
            })
        })
        .collect()
}

/// Go to the documentation of the hook implemented by the function at `offset`.
pub fn goto(snap: &Snapshot, path: &str, src: &str, offset: usize) -> Option<HookDocs> {
    let imp = implementations(snap, path, src)
        .into_iter()
        .find(|i| i.start <= offset && offset <= i.end)?;
    Some(docs(snap, &imp.hook))
}

pub fn docs(snap: &Snapshot, hook: &str) -> HookDocs {
    HookDocs {
        hook: hook.to_string(),
        location: snap
            .api_hooks
            .iter()
            .find(|(n, _)| n == hook)
            .map(|(_, l)| l.clone()),
        url: docs_url(hook),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GutterMark {
    pub line: u32,
    pub title: String,
    pub docs: HookDocs,
}

/// "Implements hook_form_alter()" next to each implementation.
pub fn marks(snap: &Snapshot, path: &str, src: &str) -> Vec<GutterMark> {
    implementations(snap, path, src)
        .into_iter()
        .map(|i| GutterMark {
            line: i.line,
            title: format!("Implements {}(): {}", i.hook, i.summary),
            docs: docs(snap, &i.hook),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap() -> Snapshot {
        Snapshot {
            extensions: vec![Extension {
                name: "acme".into(),
                dir: "web/modules/custom/acme".into(),
                kind: "module".into(),
            }],
            api_hooks: api_hooks(
                "web/core/lib/Drupal/Core/Form/form.api.php",
                "<?php\n/**\n * Docs.\n */\nfunction hook_form_alter(&$form, $form_state, $form_id) {\n}\nfunction hook_form_FORM_ID_alter(&$form) {}\n",
            ),
            ..Snapshot::default()
        }
    }

    const MODULE: &str = "web/modules/custom/acme/acme.module";

    #[test]
    fn finds_the_module_of_a_file() {
        let s = snap();
        assert_eq!(extension_of(&s, MODULE), Some("acme"));
        assert_eq!(
            extension_of(&s, "web/modules/custom/acme/src/Hooks.inc"),
            Some("acme")
        );
        assert_eq!(extension_of(&s, "other/blog.module"), Some("blog"));
        assert_eq!(extension_of(&s, "other/blog.php"), None);
    }

    #[test]
    fn completes_hook_implementations_after_function() {
        let s = snap();
        let src = "<?php\n\nfunction acme_fo";
        let list = complete(&s, MODULE, src, src.len()).unwrap();
        assert_eq!(list.from, src.len() - "acme_fo".len());
        let alter = list
            .items
            .iter()
            .find(|i| i.hook == "hook_form_alter")
            .unwrap();
        assert_eq!(alter.label, "acme_form_alter");
        assert!(alter.params.starts_with("&$form"));
        assert!(complete(&s, MODULE, "<?php\nfunction other_", 21).is_none());
        assert!(complete(&s, MODULE, "<?php\n$x = acme_", 16).is_none());
        assert!(complete(
            &s,
            "web/modules/custom/acme/acme.api.php",
            "<?php\nfunction ",
            15
        )
        .is_none());
        let empty = "<?php\nfunction ";
        assert!(complete(&s, MODULE, empty, empty.len()).is_some());
    }

    #[test]
    fn finds_implementations_and_their_docs() {
        let s = snap();
        let src = "<?php\n\nfunction acme_form_alter(&$form, $form_state, $form_id) {}\nfunction acme_helper() {}\nfunction acme_form_user_login_form_alter(&$form) {}\nfunction acme_node_presave($node) {}\n";
        let imps = implementations(&s, MODULE, src);
        let hooks: Vec<&str> = imps.iter().map(|i| i.hook.as_str()).collect();
        assert_eq!(
            hooks,
            vec![
                "hook_form_alter",
                "hook_form_FORM_ID_alter",
                "hook_ENTITY_TYPE_presave"
            ]
        );
        assert_eq!((imps[0].line, imps[0].column), (2, 9));
        assert_eq!(&src[imps[0].start..imps[0].end], "acme_form_alter");
        let docs = goto(&s, MODULE, src, imps[0].start + 3).unwrap();
        assert_eq!(
            docs.location,
            Some(Location::new(
                "web/core/lib/Drupal/Core/Form/form.api.php",
                4,
                9
            ))
        );
        assert_eq!(
            docs.url,
            "https://api.drupal.org/api/search/11.x/hook_form_alter"
        );
        let presave = goto(&s, MODULE, src, imps[2].start).unwrap();
        assert!(presave.location.is_none());
        let marks = marks(&s, MODULE, src);
        assert!(marks[0].title.starts_with("Implements hook_form_alter(): "));
    }
}
