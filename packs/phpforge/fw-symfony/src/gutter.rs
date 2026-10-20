//! Editor gutter marks, like the Symfony plugin's line markers: routes on controller
//! actions, "go to template" next to `render()` calls, "rendered by" on Twig templates and
//! a run marker on console commands.

use phpforge_lang_php::parser;
use serde::{Deserialize, Serialize};

use crate::{console, routes, twig, Location, PhpFile, Snapshot};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GutterKind {
    Route,
    Template,
    Controller,
    Command,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GutterMark {
    /// 0-based line.
    pub line: u32,
    pub kind: GutterKind,
    pub title: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<Location>,
    /// Console command to run for [`GutterKind::Command`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
}

fn short_controller(c: &str) -> String {
    let (class, method) = c.split_once("::").unwrap_or((c, ""));
    let short = class.rsplit('\\').next().unwrap_or(class);
    if method.is_empty() {
        short.to_string()
    } else {
        format!("{short}::{method}")
    }
}

/// Marks for one file, computed from its current (possibly unsaved) content.
pub fn marks(snap: &Snapshot, path: &str, content: &str) -> Vec<GutterMark> {
    let mut out = Vec::new();
    if path.ends_with(".php") {
        let file = PhpFile {
            path: path.to_string(),
            source: content.to_string(),
            index: parser::index_source(path, content),
        };
        let files = std::slice::from_ref(&file);
        for r in routes::from_php(files) {
            // Prefer the project-wide route (YAML import prefixes applied).
            let known = r.controller.as_deref().and_then(|c| {
                snap.routes.iter().find(|x| {
                    x.controller.as_deref() == Some(c)
                        && x.source == r.source
                        && x.name.ends_with(&r.name)
                })
            });
            let shown = known.unwrap_or(&r);
            let methods = if shown.methods.is_empty() {
                "ANY".to_string()
            } else {
                shown.methods.join("|")
            };
            out.push(GutterMark {
                line: r.location.as_ref().map(|l| l.line).unwrap_or(0),
                kind: GutterKind::Route,
                title: format!("{methods} {} ({})", shown.path, shown.name),
                targets: Vec::new(),
                command: None,
            });
        }
        for rf in twig::render_refs_in(&file) {
            let target = snap
                .template(&rf.template)
                .map(|t| Location::new(&t.path, 0, 0));
            out.push(GutterMark {
                line: rf.location.line,
                kind: GutterKind::Template,
                title: format!("Go to template {}", rf.template),
                targets: target.into_iter().collect(),
                command: None,
            });
        }
        for c in console::commands(files) {
            out.push(GutterMark {
                line: c.location.as_ref().map(|l| l.line).unwrap_or(0),
                kind: GutterKind::Command,
                title: format!("Run '{}'", c.name),
                targets: Vec::new(),
                command: Some(c.name),
            });
        }
    } else if path.ends_with(".twig") {
        if let Some(t) = snap.templates.iter().find(|t| t.path == path) {
            let refs: Vec<&twig::RenderRef> = snap
                .renders
                .iter()
                .filter(|r| r.template == t.name)
                .collect();
            if !refs.is_empty() {
                let names: Vec<String> = refs
                    .iter()
                    .map(|r| short_controller(&r.controller))
                    .collect();
                out.push(GutterMark {
                    line: 0,
                    kind: GutterKind::Controller,
                    title: format!("Rendered by {}", names.join(", ")),
                    targets: refs
                        .iter()
                        .map(|r| {
                            snap.method_location(&r.controller)
                                .unwrap_or_else(|| r.location.clone())
                        })
                        .collect(),
                    command: None,
                });
            }
        }
    }
    out.sort_by_key(|m| m.line);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{fixture, scan, Config};

    #[test]
    fn controller_marks() {
        let dir = fixture::project();
        let snap = scan(dir.path(), &Config::default());
        let m = marks(
            &snap,
            "src/Controller/OrderController.php",
            fixture::CONTROLLER,
        );
        let route = m
            .iter()
            .find(|m| m.kind == GutterKind::Route && m.line == 20)
            .unwrap();
        assert_eq!(route.title, "GET /orders/{id} (order_show)");
        let tpl = m
            .iter()
            .find(|m| m.kind == GutterKind::Template && m.line == 22)
            .unwrap();
        assert_eq!(tpl.targets[0].path, "templates/order/show.html.twig");
    }

    #[test]
    fn template_and_command_marks() {
        let dir = fixture::project();
        let snap = scan(dir.path(), &Config::default());
        let m = marks(&snap, "templates/order/show.html.twig", "");
        assert_eq!(m[0].kind, GutterKind::Controller);
        assert_eq!(m[0].title, "Rendered by OrderController::show");
        assert_eq!(m[0].targets[0].line, 20);

        let m = marks(
            &snap,
            "src/Command/PurgeOrdersCommand.php",
            fixture::COMMAND,
        );
        assert_eq!(m[0].command.as_deref(), Some("app:orders:purge"));
    }
}
