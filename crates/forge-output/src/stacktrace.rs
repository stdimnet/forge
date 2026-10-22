//! Analyze External Stack Trace (Code | Analyze Stack Trace or Thread Dump…): a pasted stack
//! trace becomes console lines whose frames link to project files.
//!
//! Frames are recognized in PHP traces (`#0 /var/www/app/User.php(12): …`, `… in
//! /path/File.php:42`, `on line 7`, Xdebug's `/path/index.php:0`), Laravel and Symfony
//! exception pages and logs, JavaScript / Node (`at fn (file.js:10:5)`, `file:///…`,
//! `webpack:///./src/…`) and Lua (`main.lua:12: in function …`). Paths from containers and
//! servers are mapped into the project with path mappings (PHP servers, Docker Compose bind
//! mounts), the project root, or else the longest trailing part that exists in the project.

use std::sync::OnceLock;

use regex_lite::Regex;
use serde::{Deserialize, Serialize};

use crate::ansi::Style;
use crate::console::{ConsoleLine, Span, SpanLink};
use crate::links::{find_file_links, to_workspace_path};

/// A project folder and where it lives on a server or in a container.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PathMapping {
    /// Project-relative folder; empty for the project root.
    pub local: String,
    /// Absolute path on the server.
    pub remote: String,
}

/// Where paths of a trace are resolved.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TraceContext {
    /// The project root on its machine.
    pub root: Option<String>,
    pub mappings: Vec<PathMapping>,
}

/// The language a trace comes from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TraceKind {
    Php,
    JavaScript,
    Lua,
    #[default]
    Unknown,
}

/// The analyzed trace.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceAnalysis {
    pub lines: Vec<ConsoleLine>,
    pub kind: TraceKind,
    /// File references found.
    pub frames: usize,
    /// Of those, the ones that lead to a project file.
    pub resolved: usize,
}

/// A file reference in a line: byte range of the link text, the path and position.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Ref {
    start: usize,
    end: usize,
    path: String,
    line: Option<u32>,
    column: Option<u32>,
}

fn uri_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?:file://|webpack:///)([^\s:()'\x22]+):([0-9]+)(?::([0-9]+))?")
            .expect("uri regex")
    })
}

fn refs(line: &str) -> Vec<Ref> {
    let mut out: Vec<Ref> = uri_re()
        .captures_iter(line)
        .map(|c| {
            let whole = c.get(0).expect("match");
            Ref {
                start: whole.start(),
                end: whole.end(),
                path: c[1].to_string(),
                line: c[2].parse().ok(),
                column: c.get(3).and_then(|m| m.as_str().parse().ok()),
            }
        })
        .collect();
    for l in find_file_links(line) {
        if out.iter().any(|r| l.start < r.end && r.start < l.end) {
            continue;
        }
        out.push(Ref {
            start: l.start,
            end: l.end,
            path: l.path,
            line: l.line,
            column: l.column,
        });
    }
    out.sort_by_key(|r| r.start);
    out
}

fn is_absolute(p: &str) -> bool {
    let b = p.as_bytes();
    p.starts_with('/') || b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && b[2] == b'/'
}

/// Every trailing part of a path, longest first: `/app/src/a.php` → `app/src/a.php`,
/// `src/a.php`, `a.php`.
fn tails(path: &str) -> Vec<String> {
    let parts: Vec<&str> = path
        .split('/')
        .filter(|p| !p.is_empty() && *p != "." && *p != "..")
        .collect();
    let parts = match parts.first() {
        Some(d) if d.len() == 2 && d.ends_with(':') => &parts[1..],
        _ => &parts[..],
    };
    (0..parts.len()).map(|i| parts[i..].join("/")).collect()
}

/// The project file a path of a trace refers to, if it exists.
pub fn resolve(path: &str, ctx: &TraceContext, exists: &dyn Fn(&str) -> bool) -> Option<String> {
    let mut p = path.replace('\\', "/");
    for prefix in ["file://", "webpack:///"] {
        if let Some(rest) = p.strip_prefix(prefix) {
            p = rest.to_string();
        }
    }
    let p = p.strip_prefix("./").unwrap_or(&p).to_string();
    if is_absolute(&p) {
        let mut mappings: Vec<&PathMapping> = ctx.mappings.iter().collect();
        mappings.sort_by_key(|m| std::cmp::Reverse(m.remote.len()));
        for m in mappings {
            let remote = m.remote.trim_end_matches('/');
            let rest = if remote.is_empty() {
                p.strip_prefix('/')
            } else {
                p.strip_prefix(remote).and_then(|r| r.strip_prefix('/'))
            };
            if let Some(rest) = rest {
                let local = m.local.trim_matches('/');
                let candidate = if local.is_empty() {
                    rest.to_string()
                } else {
                    format!("{local}/{rest}")
                };
                if exists(&candidate) {
                    return Some(candidate);
                }
            }
        }
        if let Some(rel) = to_workspace_path(&p, ctx.root.as_deref(), "") {
            if exists(&rel) {
                return Some(rel);
            }
        }
    } else if let Some(rel) = to_workspace_path(&p, None, "") {
        if exists(&rel) {
            return Some(rel);
        }
    }
    tails(&p).into_iter().find(|t| exists(t))
}

/// Mappings from Docker Compose bind mounts (`./:/var/www/html`, `./src:/app/src:cached`) of
/// a compose file in `compose_dir` (project-relative).
pub fn compose_mappings(compose_dir: &str, volumes: &[String]) -> Vec<PathMapping> {
    let mut out = Vec::new();
    for v in volumes {
        let mut parts = v.splitn(3, ':');
        let (Some(src), Some(dst)) = (parts.next(), parts.next()) else {
            continue;
        };
        if !(src == "." || src.starts_with("./") || src.starts_with("../")) || !dst.starts_with('/')
        {
            continue;
        }
        let joined = if compose_dir.is_empty() {
            src.to_string()
        } else {
            format!("{compose_dir}/{src}")
        };
        let Some(local) = to_workspace_path(&joined, None, "") else {
            continue;
        };
        out.push(PathMapping {
            local: local
                .trim_end_matches('/')
                .trim_end_matches("/.")
                .to_string(),
            remote: dst.trim_end_matches('/').to_string(),
        });
    }
    out
}

/// Text copied from a log as one line with escaped line breaks (`…\n#1 …`) gets them back.
pub fn normalize(text: &str) -> String {
    let text = text.replace("\r\n", "\n");
    if !text.trim_end().contains('\n') && text.contains("\\n") {
        return text
            .replace("\\n", "\n")
            .replace("\\/", "/")
            .replace("\\\\", "\\");
    }
    text
}

fn detect(text: &str) -> TraceKind {
    let php = Regex::new(r"(?m)^\s*#\d+ |\.php[:(]|\.php on line ").expect("php regex");
    let js = Regex::new(r"(?m)^\s*at .*\.(?:[cm]?js|jsx|tsx?|vue)(?::\d+)").expect("js regex");
    let lua = Regex::new(r"\.lua:\d+:").expect("lua regex");
    if php.is_match(text) {
        TraceKind::Php
    } else if js.is_match(text) || text.contains("file:///") {
        TraceKind::JavaScript
    } else if lua.is_match(text) {
        TraceKind::Lua
    } else {
        TraceKind::Unknown
    }
}

fn span(text: &str, style: &Style, link: Option<SpanLink>) -> Span {
    Span {
        text: text.to_string(),
        style: style.clone(),
        link,
    }
}

/// Analyze a pasted trace; `exists` says whether a project-relative file exists.
pub fn analyze(text: &str, ctx: &TraceContext, exists: &dyn Fn(&str) -> bool) -> TraceAnalysis {
    let text = normalize(text);
    let kind = detect(&text);
    let mut out = TraceAnalysis {
        kind,
        ..TraceAnalysis::default()
    };
    // The exception message comes before the first frame or "Stack trace:" line.
    let frame_start = Regex::new(
        r"^\s*(?:#\d+ |at |Stack trace:|stack traceback:|\[stacktrace\]|\d+\.\d+\s+\d+\s+\d+\. )",
    )
    .expect("frame regex");
    let mut in_message = true;
    for raw in text.trim_matches('\n').split('\n') {
        let line = raw.trim_end();
        let found = refs(line);
        let resolved: Vec<(Ref, Option<String>)> = found
            .into_iter()
            .map(|r| {
                let target = resolve(&r.path, ctx, exists);
                (r, target)
            })
            .collect();
        out.frames += resolved.len();
        out.resolved += resolved.iter().filter(|(_, t)| t.is_some()).count();
        let library = resolved.iter().any(|(r, t)| {
            let p = t.as_deref().unwrap_or(&r.path);
            p.contains("vendor/") || p.contains("node_modules/") || p.starts_with("internal/")
        });
        if frame_start.is_match(line) {
            in_message = false;
        }
        // The message is the exception; library frames and unknown files are dimmed.
        let base = if in_message && !line.trim().is_empty() {
            Style {
                fg: Some("var(--danger)".into()),
                ..Style::default()
            }
        } else if library || (!resolved.is_empty() && resolved.iter().all(|(_, t)| t.is_none())) {
            Style {
                fg: Some("var(--fg-muted)".into()),
                ..Style::default()
            }
        } else {
            Style::default()
        };
        let mut segments = Vec::new();
        let mut at = 0;
        for (r, target) in resolved {
            if r.start < at {
                continue;
            }
            if r.start > at {
                segments.push(span(&line[at..r.start], &base, None));
            }
            let link = target.map(|target| SpanLink {
                url: false,
                target,
                line: r.line.filter(|l| *l > 0),
                column: r.column,
            });
            segments.push(span(&line[r.start..r.end], &base, link));
            at = r.end;
        }
        if at < line.len() {
            segments.push(span(&line[at..], &base, None));
        }
        out.lines.push(ConsoleLine { segments });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILES: &[&str] = &[
        "app/Http/Controllers/OrderController.php",
        "app/Models/Order.php",
        "vendor/laravel/framework/src/Illuminate/Routing/Controller.php",
        "public/index.php",
        "src/server.js",
        "src/cart.ts",
        "scripts/main.lua",
        "lib/util.lua",
    ];

    fn exists(p: &str) -> bool {
        FILES.contains(&p)
    }

    fn links(a: &TraceAnalysis) -> Vec<(String, Option<u32>)> {
        a.lines
            .iter()
            .flat_map(|l| &l.segments)
            .filter_map(|s| s.link.as_ref())
            .map(|l| (l.target.clone(), l.line))
            .collect()
    }

    #[test]
    fn php_traces_map_container_paths() {
        let trace = "RuntimeException: Order 12 not found in /var/www/html/app/Models/Order.php:42\n\
Stack trace:\n\
#0 /var/www/html/app/Http/Controllers/OrderController.php(31): App\\Models\\Order::findOrFail(12)\n\
#1 /var/www/html/vendor/laravel/framework/src/Illuminate/Routing/Controller.php(54): App\\Http\\Controllers\\OrderController->show('12')\n\
#2 /usr/share/php/Other.php(1): x()\n\
#3 {main}\n";
        let ctx = TraceContext {
            root: Some("/home/me/shop".into()),
            mappings: vec![PathMapping {
                local: String::new(),
                remote: "/var/www/html".into(),
            }],
        };
        let a = analyze(trace, &ctx, &exists);
        assert_eq!(a.kind, TraceKind::Php);
        assert_eq!(
            links(&a),
            [
                ("app/Models/Order.php".into(), Some(42)),
                ("app/Http/Controllers/OrderController.php".into(), Some(31)),
                (
                    "vendor/laravel/framework/src/Illuminate/Routing/Controller.php".into(),
                    Some(54)
                ),
            ]
        );
        assert_eq!((a.frames, a.resolved), (4, 3));
        // The message is red, library frames are dimmed, unresolved ones have no link.
        assert_eq!(
            a.lines[0].segments[0].style.fg.as_deref(),
            Some("var(--danger)")
        );
        assert_eq!(
            a.lines[3].segments[0].style.fg.as_deref(),
            Some("var(--fg-muted)")
        );
        assert!(a.lines[4].segments.iter().all(|s| s.link.is_none()));
        assert_eq!(a.lines[2].text(), "#0 /var/www/html/app/Http/Controllers/OrderController.php(31): App\\Models\\Order::findOrFail(12)");
    }

    #[test]
    fn without_mappings_the_longest_existing_tail_wins() {
        let ctx = TraceContext::default();
        let a = analyze(
            "PHP Fatal error:  Uncaught Error in /srv/app/releases/42/app/Models/Order.php on line 7\n\
             0.0012  402456   1. {main}() /srv/app/releases/42/public/index.php:0",
            &ctx,
            &exists,
        );
        assert_eq!(
            links(&a),
            [
                ("app/Models/Order.php".into(), Some(7)),
                ("public/index.php".into(), None)
            ]
        );
    }

    #[test]
    fn node_and_lua_traces() {
        let ctx = TraceContext {
            root: Some("/home/me/web".into()),
            mappings: vec![],
        };
        let a = analyze(
            "TypeError: Cannot read properties of undefined (reading 'id')\n    at Cart.add (/home/me/web/src/cart.ts:12:9)\n    at file:///app/src/server.js:40:3\n    at webpack:///./src/cart.ts:3:1\n    at node:internal/main:1:2",
            &ctx,
            &exists,
        );
        assert_eq!(a.kind, TraceKind::JavaScript);
        assert_eq!(
            links(&a),
            [
                ("src/cart.ts".into(), Some(12)),
                ("src/server.js".into(), Some(40)),
                ("src/cart.ts".into(), Some(3)),
            ]
        );
        let a = analyze(
            "lua: scripts/main.lua:12: attempt to index a nil value\nstack traceback:\n\t[C]: in ?\n\tlib/util.lua:4: in function 'load'",
            &ctx,
            &exists,
        );
        assert_eq!(a.kind, TraceKind::Lua);
        assert_eq!(
            links(&a),
            [
                ("scripts/main.lua".into(), Some(12)),
                ("lib/util.lua".into(), Some(4))
            ]
        );
    }

    #[test]
    fn escaped_log_lines_and_compose_mounts() {
        let one_line = r"Error in \/var\/www\/html\/app\/Models\/Order.php:9\n#0 \/var\/www\/html\/public\/index.php(3): run()";
        let n = normalize(one_line);
        assert_eq!(n.lines().count(), 2);
        assert!(n.contains("/var/www/html/public/index.php(3)"));
        let m = compose_mappings(
            "docker",
            &[
                "../:/var/www/html:cached".into(),
                "db-data:/var/lib/mysql".into(),
                "./nginx.conf:/etc/nginx/nginx.conf".into(),
            ],
        );
        assert_eq!(
            m,
            [
                PathMapping {
                    local: String::new(),
                    remote: "/var/www/html".into()
                },
                PathMapping {
                    local: "docker/nginx.conf".into(),
                    remote: "/etc/nginx/nginx.conf".into()
                },
            ]
        );
    }
}
