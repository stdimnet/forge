//! Flask routes: `@app.route("/p", methods=[...])`, `@bp.get("/p")` (Flask 2),
//! `add_url_rule`, blueprints (`Blueprint("admin", __name__, url_prefix="/admin")`) and
//! their registration (`app.register_blueprint(bp, url_prefix="/x")`), plus the template
//! folders of the app and of each blueprint (`template_folder=`).

use std::collections::BTreeMap;

use crate::modules::{join, parent};
use crate::source::{self, Line};
use crate::{sample_path, Location, Project, Route};

const VERBS: [&str; 5] = ["get", "post", "put", "patch", "delete"];

/// A blueprint object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Blueprint {
    pub path: String,
    pub var: String,
    pub name: String,
    pub url_prefix: String,
    /// Workspace-relative template folder (`template_folder=` next to the blueprint's file).
    pub template_folder: Option<String>,
}

/// Every `Blueprint(...)` of the project, keyed by (file, variable).
pub fn blueprints(project: &Project) -> BTreeMap<(String, String), Blueprint> {
    let mut out = BTreeMap::new();
    for (path, _) in project.python() {
        for (var, line, open) in project.assigned_calls(path, &["Blueprint"]) {
            let Some((args, _)) = source::call_args(&line.tokens, open) else {
                continue;
            };
            let name = source::positional(&args, 0)
                .or_else(|| source::keyword(&args, "name"))
                .and_then(|a| a.string())
                .unwrap_or_else(|| var.to_string());
            let url_prefix = source::keyword(&args, "url_prefix")
                .and_then(|a| a.string())
                .unwrap_or_default();
            let template_folder = source::keyword(&args, "template_folder")
                .and_then(|a| a.string())
                .map(|f| join(parent(path), &f));
            out.insert(
                (path.to_string(), var.to_string()),
                Blueprint {
                    path: path.to_string(),
                    var: var.to_string(),
                    name,
                    url_prefix,
                    template_folder,
                },
            );
        }
    }
    out
}

/// Prefixes given at registration: (blueprint file, variable) → `url_prefix`.
fn registrations(project: &Project) -> BTreeMap<(String, String), String> {
    let mut out = BTreeMap::new();
    for (path, lines) in project.python() {
        for l in lines {
            let t = &l.tokens;
            for (i, tok) in t.iter().enumerate() {
                if !tok.is_name("register_blueprint") || !t.get(i + 1).is_some_and(|x| x.is_op("("))
                {
                    continue;
                }
                let Some((args, _)) = source::call_args(t, i + 1) else {
                    continue;
                };
                let Some(target) = source::positional(&args, 0).and_then(|a| a.dotted()) else {
                    continue;
                };
                let Some(key) = project.resolve_variable(path, &target) else {
                    continue;
                };
                if let Some(prefix) = source::keyword(&args, "url_prefix").and_then(|a| a.string())
                {
                    out.insert(key, prefix);
                } else {
                    out.entry(key).or_insert_with(String::new);
                }
            }
        }
    }
    out
}

fn join_url(prefix: &str, path: &str) -> String {
    let p = prefix.trim_end_matches('/');
    if p.is_empty() {
        return if path.starts_with('/') {
            path.to_string()
        } else {
            format!("/{path}")
        };
    }
    if path.is_empty() || path == "/" {
        return if path == "/" {
            format!("{p}/")
        } else {
            p.to_string()
        };
    }
    format!("{p}/{}", path.trim_start_matches('/'))
}

/// The routes of every Flask app and blueprint.
pub fn routes(project: &Project) -> Vec<Route> {
    let bps = blueprints(project);
    let registered = registrations(project);
    let mut out = Vec::new();
    for (path, lines) in project.python() {
        let apps: Vec<&str> = project
            .assigned_calls(path, &["Flask"])
            .into_iter()
            .map(|(v, _, _)| v)
            .collect();
        for (i, l) in lines.iter().enumerate() {
            let Some(("def", func)) = source::definition(l) else {
                if let Some(r) = add_url_rule(project, path, l, &apps, &bps, &registered) {
                    out.push(r);
                }
                continue;
            };
            for d in source::decorators(lines, i) {
                if let Some(r) = decorated(project, path, d, func, &apps, &bps, &registered) {
                    out.push(r);
                }
            }
        }
    }
    out
}

/// The object a decorator or call is made on: an app (`""` prefix, no endpoint prefix) or a
/// blueprint (its prefix and name).
fn owner(
    project: &Project,
    path: &str,
    object: &str,
    apps: &[&str],
    bps: &BTreeMap<(String, String), Blueprint>,
    registered: &BTreeMap<(String, String), String>,
) -> Option<(String, Option<String>)> {
    if apps.contains(&object) {
        return Some((String::new(), None));
    }
    let key = project.resolve_variable(path, object)?;
    let bp = bps.get(&key)?;
    let prefix = registered
        .get(&key)
        .filter(|p| !p.is_empty())
        .cloned()
        .unwrap_or_else(|| bp.url_prefix.clone());
    Some((prefix, Some(bp.name.clone())))
}

fn decorated(
    project: &Project,
    path: &str,
    d: &Line,
    func: &source::Token,
    apps: &[&str],
    bps: &BTreeMap<(String, String), Blueprint>,
    registered: &BTreeMap<(String, String), String>,
) -> Option<Route> {
    let t = &d.tokens;
    let open = t.iter().position(|x| x.is_op("("))?;
    let callee = source::dotted_exact(&t[1..open])?;
    let (object, method) = callee.rsplit_once('.')?;
    if method != "route" && !VERBS.contains(&method) {
        return None;
    }
    let (prefix, bp) = owner(project, path, object, apps, bps, registered)?;
    let (args, _) = source::call_args(t, open)?;
    let rule_arg = source::positional(&args, 0).or_else(|| source::keyword(&args, "rule"))?;
    let rule = rule_arg.string()?;
    let methods = if method == "route" {
        let m: Vec<String> = source::keyword(&args, "methods")
            .map(|a| a.strings())
            .unwrap_or_default();
        if m.is_empty() {
            vec!["GET".to_string()]
        } else {
            m.iter().map(|x| x.to_uppercase()).collect()
        }
    } else {
        vec![method.to_uppercase()]
    };
    let endpoint = source::keyword(&args, "endpoint")
        .and_then(|a| a.string())
        .unwrap_or_else(|| func.text.clone());
    let full = join_url(&prefix, &rule);
    let module = project.module_of(path);
    Some(Route {
        framework: "flask".into(),
        methods,
        sample_path: sample_path(&full),
        path: full,
        name: Some(match bp {
            Some(b) => format!("{b}.{endpoint}"),
            None => endpoint,
        }),
        handler: format!("{module}.{}", func.text),
        location: project.locate(path, rule_arg.tokens[0].start),
        handler_location: Some(project.locate(path, func.start)),
    })
}

/// `app.add_url_rule("/p", "endpoint", view_func)`.
fn add_url_rule(
    project: &Project,
    path: &str,
    l: &Line,
    apps: &[&str],
    bps: &BTreeMap<(String, String), Blueprint>,
    registered: &BTreeMap<(String, String), String>,
) -> Option<Route> {
    let t = &l.tokens;
    let at = t.iter().position(|x| x.is_name("add_url_rule"))?;
    if !t.get(at + 1).is_some_and(|x| x.is_op("(")) || at < 2 || !t[at - 1].is_op(".") {
        return None;
    }
    let (object, _) = source::dotted_before(t, at - 1)?;
    let (prefix, bp) = owner(project, path, &object, apps, bps, registered)?;
    let (args, _) = source::call_args(t, at + 1)?;
    let rule_arg = source::positional(&args, 0)?;
    let rule = rule_arg.string()?;
    let view = source::positional(&args, 2)
        .or_else(|| source::keyword(&args, "view_func"))
        .and_then(|a| {
            let end = a
                .tokens
                .iter()
                .position(|x| x.is_op("("))
                .unwrap_or(a.tokens.len());
            source::dotted_exact(&a.tokens[..end])
        });
    let endpoint = source::positional(&args, 1)
        .or_else(|| source::keyword(&args, "endpoint"))
        .and_then(|a| a.string())
        .or_else(|| {
            view.as_ref().map(|v| {
                v.rsplit('.')
                    .nth(usize::from(v.ends_with(".as_view")))
                    .unwrap_or(v)
                    .to_string()
            })
        })?;
    let methods = source::keyword(&args, "methods")
        .map(|a| a.strings())
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| vec!["GET".into()]);
    let (handler, handler_location) = match &view {
        Some(v) => project.resolve(path, v),
        None => (endpoint.clone(), None),
    };
    let full = join_url(&prefix, &rule);
    Some(Route {
        framework: "flask".into(),
        methods: methods.iter().map(|m| m.to_uppercase()).collect(),
        sample_path: sample_path(&full),
        path: full,
        name: Some(match bp {
            Some(b) => format!("{b}.{endpoint}"),
            None => endpoint,
        }),
        handler,
        location: project.locate(path, rule_arg.tokens[0].start),
        handler_location,
    })
}

/// Template folders: `templates/` next to the module that creates the app (or the
/// `template_folder=` given to `Flask(...)`), then each blueprint's.
pub fn template_folders(project: &Project) -> Vec<(String, Location)> {
    let mut out = Vec::new();
    for (path, _) in project.python() {
        for (_, line, open) in project.assigned_calls(path, &["Flask"]) {
            let folder = source::call_args(&line.tokens, open)
                .and_then(|(args, _)| {
                    source::keyword(&args, "template_folder").and_then(|a| a.string())
                })
                .unwrap_or_else(|| "templates".into());
            out.push((
                join(parent(path), &folder),
                project.locate(path, line.tokens[open].start),
            ));
        }
    }
    for bp in blueprints(project).values() {
        if let Some(f) = &bp.template_folder {
            out.push((f.clone(), project.locate(&bp.path, 0)));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::tests::project;

    #[test]
    fn routes_and_blueprints() {
        let p = project(&[
            ("requirements.txt", "flask\n"),
            ("blog/__init__.py", "from flask import Flask\nfrom .admin import bp as admin_bp\n\napp = Flask(__name__)\napp.register_blueprint(admin_bp, url_prefix='/backoffice')\n\n@app.route('/')\ndef index():\n    return render_template('index.html')\n\n@app.route('/posts/<int:post_id>', methods=['GET', 'post'])\ndef post(post_id):\n    pass\n\napp.add_url_rule('/about', 'about', views.about)\n"),
            ("blog/admin.py", "from flask import Blueprint\n\nbp = Blueprint('admin', __name__, url_prefix='/admin', template_folder='admin_templates')\n\n@bp.get('/')\ndef dashboard():\n    pass\n\n@bp.post('/users', endpoint='create')\ndef create_user():\n    pass\n"),
        ]);
        let r: Vec<(String, String, String)> = p
            .info
            .routes
            .iter()
            .map(|r| {
                (
                    r.methods.join(","),
                    r.path.clone(),
                    r.name.clone().unwrap_or_default(),
                )
            })
            .collect();
        assert_eq!(
            r,
            [
                ("GET".into(), "/".into(), "index".into()),
                (
                    "GET,POST".into(),
                    "/posts/<int:post_id>".into(),
                    "post".into()
                ),
                ("GET".into(), "/about".into(), "about".into()),
                (
                    "GET".into(),
                    "/backoffice/".into(),
                    "admin.dashboard".into()
                ),
                (
                    "POST".into(),
                    "/backoffice/users".into(),
                    "admin.create".into()
                ),
            ]
        );
        assert_eq!(p.info.routes[1].sample_path, "/posts/1");
        assert_eq!(p.info.routes[3].handler, "blog.admin.dashboard");
        let roots: Vec<&str> = p
            .info
            .template_roots
            .iter()
            .map(|r| r.dir.as_str())
            .collect();
        assert_eq!(roots[..2], ["blog/templates", "blog/admin_templates"]);
    }
}
