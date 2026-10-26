//! Which web frameworks a project uses and where their applications are:
//!
//! - a framework is used when a dependency file (`requirements*.txt`, `pyproject.toml`,
//!   `Pipfile`) declares it or a Python file imports it; Django also needs a `manage.py`;
//! - `FLASK_APP`: the module that creates `Flask(...)` (with `:app` unless the variable is
//!   `app` or `application`), the uvicorn app (`shop_api.main:app`, `--app-dir src` for a
//!   `src/` layout) and the Celery `-A` app (`proj` for `proj/celery.py`).

use serde::{Deserialize, Serialize};

use pyforge_lang_python::deps;

use crate::{Location, Project};

/// Frameworks in detection order.
pub const FRAMEWORKS: [&str; 4] = ["django", "flask", "fastapi", "celery"];

/// An application object: what a run configuration passes to the framework.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    /// `FLASK_APP`, `module:attribute` for uvicorn, the module for `celery -A`; `None` when
    /// no application was found.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
    /// uvicorn's `--app-dir` (`src` for a src layout).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_dir: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<Location>,
}

/// Normalized names of the dependencies declared in a project's files.
pub fn declared(project: &Project) -> Vec<String> {
    let mut out = Vec::new();
    for (path, text) in project.files() {
        let name = path.rsplit('/').next().unwrap_or(path);
        let file = if name == "pyproject.toml" {
            deps::pyproject(text)
        } else if deps::is_requirements_file(path) {
            deps::requirements_txt(text)
        } else if name == "Pipfile" {
            out.extend(pipfile(text));
            continue;
        } else {
            continue;
        };
        out.extend(file.deps.into_iter().map(|d| d.normalized));
    }
    out.sort();
    out.dedup();
    out
}

/// Package names of a Pipfile's `[packages]` and `[dev-packages]`.
fn pipfile(text: &str) -> Vec<String> {
    let Ok(value) = text.parse::<toml::Table>() else {
        return Vec::new();
    };
    ["packages", "dev-packages"]
        .iter()
        .filter_map(|s| value.get(*s).and_then(|v| v.as_table()))
        .flat_map(|t| t.keys().map(|k| pyforge_lang_python::normalize(k)))
        .collect()
}

/// Top-level modules imported anywhere in the project.
fn imported(project: &Project) -> Vec<String> {
    let mut out = Vec::new();
    for (_, lines) in project.python() {
        for l in lines {
            let t = &l.tokens;
            if t.len() >= 2 && (t[0].is_name("import") || t[0].is_name("from")) {
                out.push(t[1].text.clone());
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// The frameworks of a project.
pub fn frameworks(project: &Project) -> Vec<String> {
    let declared = declared(project);
    let imported = imported(project);
    let uses = |f: &str| declared.iter().any(|d| d == f) || imported.iter().any(|m| m == f);
    let mut out = Vec::new();
    for f in FRAMEWORKS {
        let used = match f {
            "django" => manage_scripts(project)
                .iter()
                .any(|m| uses("django") || project.file(m).is_some_and(|t| t.contains("django"))),
            _ => uses(f),
        };
        if used {
            out.push(f.to_string());
        }
    }
    out
}

/// Every `manage.py` of the project, the shallowest first.
pub fn manage_scripts(project: &Project) -> Vec<String> {
    if !project.options.manage_script.trim().is_empty() {
        return vec![project.options.manage_script.trim().to_string()];
    }
    let mut out: Vec<String> = project
        .files()
        .map(|(p, _)| p)
        .filter(|p| *p == "manage.py" || p.ends_with("/manage.py"))
        .filter(|p| !p.contains("site-packages/") && !p.starts_with(".venv/"))
        .map(str::to_string)
        .collect();
    out.sort_by_key(|p| (p.matches('/').count(), p.clone()));
    out
}

/// Where a module-level `name = Callee(...)` with one of `callees` is, the first one in
/// path order preferring `preferred` file names.
fn find_app(
    project: &Project,
    callees: &[&str],
    preferred: &[&str],
) -> Option<(String, String, usize)> {
    let mut found: Vec<(usize, String, String, usize)> = Vec::new();
    for (path, _) in project.python() {
        if is_test_or_env(path) {
            continue;
        }
        for (name, line, open) in project.assigned_calls(path, callees) {
            let file = path.rsplit('/').next().unwrap_or(path);
            let rank = preferred
                .iter()
                .position(|p| *p == file)
                .unwrap_or(preferred.len());
            found.push((
                rank,
                path.to_string(),
                name.to_string(),
                line.tokens[open].start,
            ));
        }
    }
    found.sort_by_key(|(rank, path, _, _)| (*rank, path.matches('/').count(), path.clone()));
    found.into_iter().next().map(|(_, p, n, o)| (p, n, o))
}

fn is_test_or_env(path: &str) -> bool {
    path.starts_with(".venv/")
        || path.contains("/site-packages/")
        || path.starts_with("tests/")
        || path.contains("/tests/")
        || path
            .rsplit('/')
            .next()
            .is_some_and(|f| f.starts_with("test_"))
}

/// `src/shop_api/main.py` → (`shop_api.main`, `Some("src")`).
fn module_and_dir(project: &Project, path: &str) -> (String, Option<String>) {
    let module = project.module_of(path);
    let dir = path
        .strip_suffix(&format!("{}.py", module.replace('.', "/")))
        .or_else(|| path.strip_suffix(&format!("{}/__init__.py", module.replace('.', "/"))))
        .map(|d| d.trim_end_matches('/').to_string())
        .filter(|d| !d.is_empty());
    (module, dir)
}

/// `FLASK_APP`: the `flask.app` setting or the module that creates `Flask(...)`.
pub fn flask_app(project: &Project) -> AppInfo {
    let found = find_app(project, &["Flask"], &["app.py", "wsgi.py", "__init__.py"]);
    let location = found.as_ref().map(|(p, _, o)| project.locate(p, *o));
    if !project.options.flask_app.trim().is_empty() {
        return AppInfo {
            app: Some(project.options.flask_app.trim().into()),
            app_dir: None,
            location,
        };
    }
    let factory = find_factory(project, "Flask");
    let app = match (&found, factory) {
        (_, Some((path, func))) => Some(format!("{}:{func}()", module_and_dir(project, &path).0)),
        (Some((path, name, _)), None) => {
            let (module, _) = module_and_dir(project, path);
            Some(if name == "app" || name == "application" {
                module
            } else {
                format!("{module}:{name}")
            })
        }
        _ => None,
    };
    AppInfo {
        app,
        app_dir: None,
        location,
    }
}

/// An application factory: `def create_app(...)` that returns a `Flask(...)`.
fn find_factory(project: &Project, callee: &str) -> Option<(String, String)> {
    for (path, lines) in project.python() {
        if is_test_or_env(path) {
            continue;
        }
        for (i, l) in lines.iter().enumerate() {
            let Some((kw, name)) = crate::source::definition(l) else {
                continue;
            };
            if kw != "def" || !(name.text == "create_app" || name.text == "make_app") {
                continue;
            }
            let body = lines[i + 1..].iter().take_while(|b| b.indent > l.indent);
            if body.into_iter().any(|b| crate::mentions(b, callee)) {
                return Some((path.to_string(), name.text.clone()));
            }
        }
    }
    None
}

/// The uvicorn app: the `fastapi.app` setting or `module:attribute` of `FastAPI(...)`.
pub fn fastapi_app(project: &Project) -> AppInfo {
    let found = find_app(project, &["FastAPI"], &["main.py", "app.py", "api.py"]);
    let location = found.as_ref().map(|(p, _, o)| project.locate(p, *o));
    if !project.options.fastapi_app.trim().is_empty() {
        return AppInfo {
            app: Some(project.options.fastapi_app.trim().into()),
            app_dir: None,
            location,
        };
    }
    match found {
        Some((path, name, _)) => {
            let (module, dir) = module_and_dir(project, &path);
            AppInfo {
                app: Some(format!("{module}:{name}")),
                app_dir: dir,
                location,
            }
        }
        None => AppInfo::default(),
    }
}

/// The Celery app: the `celery.app` setting or the module that creates `Celery(...)`
/// (its package for a `celery.py` module, as `celery -A proj` expects).
pub fn celery_app(project: &Project) -> AppInfo {
    let found = find_app(
        project,
        &["Celery"],
        &["celery.py", "celery_app.py", "tasks.py", "worker.py"],
    );
    let location = found.as_ref().map(|(p, _, o)| project.locate(p, *o));
    if !project.options.celery_app.trim().is_empty() {
        return AppInfo {
            app: Some(project.options.celery_app.trim().into()),
            app_dir: None,
            location,
        };
    }
    match found {
        Some((path, name, _)) => {
            let (module, _) = module_and_dir(project, &path);
            let app = match module.strip_suffix(".celery") {
                Some(pkg) if name == "app" || name == "celery" => pkg.to_string(),
                _ if name == "app" || name == "celery" => module,
                _ => format!("{module}:{name}"),
            };
            AppInfo {
                app: Some(app),
                app_dir: None,
                location,
            }
        }
        None => AppInfo::default(),
    }
}

#[cfg(test)]
mod tests {
    use crate::tests::project;

    #[test]
    fn frameworks_from_dependencies_and_imports() {
        let p = project(&[
            ("requirements.txt", "Django==5.1\ncelery[redis]>=5\n"),
            ("manage.py", "import os\n"),
            (
                "api/app.py",
                "from fastapi import FastAPI\napp = FastAPI()\n",
            ),
        ]);
        assert_eq!(p.info.frameworks, ["django", "fastapi", "celery"]);
        let p = project(&[("app.py", "from flask import Flask\n")]);
        assert_eq!(p.info.frameworks, ["flask"]);
        // Django without manage.py is not a Django project.
        let p = project(&[(
            "pyproject.toml",
            "[project]\nname='x'\ndependencies=['django']\n",
        )]);
        assert!(p.info.frameworks.is_empty());
        let p = project(&[("Pipfile", "[packages]\nFlask = \"*\"\n")]);
        assert_eq!(p.info.frameworks, ["flask"]);
    }

    #[test]
    fn application_objects() {
        let p = project(&[
            (
                "pyproject.toml",
                "[project]\nname='x'\ndependencies=['fastapi', 'celery', 'flask']\n",
            ),
            ("src/shop_api/__init__.py", ""),
            (
                "src/shop_api/main.py",
                "from fastapi import FastAPI\napi = FastAPI(title='x')\n",
            ),
            ("proj/__init__.py", ""),
            (
                "proj/celery.py",
                "from celery import Celery\napp = Celery('proj')\n",
            ),
            (
                "web/app.py",
                "from flask import Flask\napp = Flask(__name__)\n",
            ),
        ]);
        let fastapi = p.info.fastapi.clone().unwrap();
        assert_eq!(fastapi.app.as_deref(), Some("shop_api.main:api"));
        assert_eq!(fastapi.app_dir.as_deref(), Some("src"));
        assert_eq!(fastapi.location.unwrap().line, 1);
        assert_eq!(p.info.celery.clone().unwrap().app.as_deref(), Some("proj"));
        assert_eq!(
            p.info.flask.clone().unwrap().app.as_deref(),
            Some("web.app")
        );
    }

    #[test]
    fn flask_factory() {
        let p = project(&[
            ("app/__init__.py", "from flask import Flask\n\ndef create_app():\n    app = Flask(__name__)\n    return app\n"),
        ]);
        assert_eq!(
            p.info.flask.clone().unwrap().app.as_deref(),
            Some("app:create_app()")
        );
    }
}
