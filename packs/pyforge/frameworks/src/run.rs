//! The web framework run configurations as Python runs (`python [options] <script |
//! -m module> <arguments>`), so they share the Python configuration's interpreters
//! (local, Docker, SSH, WSL), run targets and debugpy:
//!
//! - `django-server`: `manage.py runserver <host>:<port> [--noreload]` with
//!   `DJANGO_SETTINGS_MODULE`, in the folder of `manage.py`;
//! - `django-manage`: `manage.py <task>` (Tools | Run manage.py Task…);
//! - `flask-server`: `-m flask run --host --port [--debug] [--no-reload]` with `FLASK_APP`;
//! - `fastapi`: `-m uvicorn <module:app> --host --port [--reload] [--app-dir]`;
//! - `celery-worker` / `celery-beat`: `-m celery -A <app> worker|beat --loglevel …`.
//!
//! Options left empty take what the project analysis found (`WebProject`).

use std::collections::BTreeMap;

use forge_runtools::args::split_args;
use forge_runtools::Configuration;
use serde::{Deserialize, Serialize};

use crate::WebProject;

pub const DJANGO_SERVER: &str = "django-server";
pub const DJANGO_MANAGE: &str = "django-manage";
pub const FLASK_SERVER: &str = "flask-server";
pub const FASTAPI: &str = "fastapi";
pub const CELERY_WORKER: &str = "celery-worker";
pub const CELERY_BEAT: &str = "celery-beat";

/// Every configuration type built here.
pub const TYPES: [&str; 6] = [
    DJANGO_SERVER,
    DJANGO_MANAGE,
    FLASK_SERVER,
    FASTAPI,
    CELERY_WORKER,
    CELERY_BEAT,
];

pub fn handles(type_id: &str) -> bool {
    TYPES.contains(&type_id)
}

/// A web configuration as the options of a `python` configuration.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PythonRun {
    /// `script` or `module`.
    pub target: String,
    pub script: String,
    /// Arguments before the configuration's own `arguments`.
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    /// Working directory when the configuration has none (the folder of `manage.py`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<String>,
    /// The port the server listens on (published by container interpreters).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    /// The page to open once the server runs (`http://127.0.0.1:8000/docs` for FastAPI).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

fn opt<'a>(cfg: &'a Configuration, key: &str, default: &'a str) -> &'a str {
    match cfg.option(key).trim() {
        "" => default,
        v => v,
    }
}

fn flag(cfg: &Configuration, key: &str, default: bool) -> bool {
    match cfg.option(key).trim() {
        "true" => true,
        "false" => false,
        _ => default,
    }
}

fn port(cfg: &Configuration, default: u16) -> Result<u16, String> {
    let p = opt(cfg, "port", "");
    if p.is_empty() {
        return Ok(default);
    }
    p.parse()
        .map_err(|_| format!("'{}': the port '{p}' is not a number", cfg.name))
}

/// Inside a container `127.0.0.1` is not reachable from the host.
fn browser_host(host: &str) -> &str {
    match host {
        "0.0.0.0" | "::" | "" => "127.0.0.1",
        h => h,
    }
}

fn manage(cfg: &Configuration, project: &WebProject) -> Result<(String, Option<String>), String> {
    let django = project.django.as_ref();
    let script = match cfg.option("manage").trim() {
        "" => django.map(|d| d.manage.clone()).ok_or_else(|| {
            "No manage.py found: set it in Settings | Languages & Frameworks | Django".to_string()
        })?,
        s => s.to_string(),
    };
    let dir = script.rsplit_once('/').map(|(d, _)| d.to_string());
    // Run manage.py from its folder, as `python manage.py` does.
    let file = match &dir {
        Some(d) => script[d.len() + 1..].to_string(),
        None => script,
    };
    Ok((file, dir))
}

fn settings_env(cfg: &Configuration, project: &WebProject, env: &mut BTreeMap<String, String>) {
    let settings = match cfg.option("settings").trim() {
        "" => project
            .django
            .as_ref()
            .and_then(|d| d.settings_module.clone()),
        s => Some(s.to_string()),
    };
    if let Some(s) = settings {
        env.insert("DJANGO_SETTINGS_MODULE".into(), s);
    }
}

/// The Python run of a web configuration.
pub fn build(cfg: &Configuration, project: &WebProject) -> Result<PythonRun, String> {
    let mut env = BTreeMap::new();
    let run = match cfg.type_id.as_str() {
        DJANGO_SERVER => {
            let (script, dir) = manage(cfg, project)?;
            let host = opt(cfg, "host", "127.0.0.1");
            let port = port(cfg, 8000)?;
            let mut args = vec!["runserver".to_string(), format!("{host}:{port}")];
            if flag(cfg, "noReload", false) {
                args.push("--noreload".into());
            }
            settings_env(cfg, project, &mut env);
            PythonRun {
                target: "script".into(),
                script,
                args,
                env,
                working_directory: dir,
                port: Some(port),
                url: Some(format!("http://{}:{port}/", browser_host(host))),
            }
        }
        DJANGO_MANAGE => {
            let (script, dir) = manage(cfg, project)?;
            let task = split_args(cfg.option("task"));
            if task.is_empty() {
                return Err(format!("'{}': choose the manage.py task", cfg.name));
            }
            settings_env(cfg, project, &mut env);
            PythonRun {
                target: "script".into(),
                script,
                args: task,
                env,
                working_directory: dir,
                port: None,
                url: None,
            }
        }
        FLASK_SERVER => {
            let app = match cfg.option("app").trim() {
                "" => project
                    .flask
                    .as_ref()
                    .and_then(|f| f.app.clone())
                    .ok_or_else(|| {
                        format!("'{}': set the Flask application (FLASK_APP)", cfg.name)
                    })?,
                a => a.to_string(),
            };
            env.insert("FLASK_APP".into(), app);
            let host = opt(cfg, "host", "127.0.0.1");
            let port = port(cfg, 5000)?;
            let mut args = vec![
                "run".into(),
                "--host".into(),
                host.to_string(),
                "--port".into(),
                port.to_string(),
            ];
            if flag(cfg, "debugMode", true) {
                args.push("--debug".into());
                env.insert("FLASK_DEBUG".into(), "1".into());
            }
            if flag(cfg, "noReload", false) {
                args.push("--no-reload".into());
            }
            PythonRun {
                target: "module".into(),
                script: "flask".into(),
                args,
                env,
                working_directory: None,
                port: Some(port),
                url: Some(format!("http://{}:{port}/", browser_host(host))),
            }
        }
        FASTAPI => {
            let info = project.fastapi.clone().unwrap_or_default();
            let (app, app_dir) = match cfg.option("app").trim() {
                "" => (
                    info.app.ok_or_else(|| {
                        format!("'{}': set the application (module:app)", cfg.name)
                    })?,
                    info.app_dir,
                ),
                a => (a.to_string(), None),
            };
            let app_dir = match cfg.option("appDir").trim() {
                "" => app_dir,
                d => Some(d.to_string()),
            };
            let host = opt(cfg, "host", "127.0.0.1");
            let port = port(cfg, 8000)?;
            let mut args = vec![
                app,
                "--host".into(),
                host.to_string(),
                "--port".into(),
                port.to_string(),
            ];
            if flag(cfg, "reload", true) {
                args.push("--reload".into());
            }
            if let Some(d) = app_dir {
                args.extend(["--app-dir".into(), d]);
            }
            PythonRun {
                target: "module".into(),
                script: "uvicorn".into(),
                args,
                env,
                working_directory: None,
                port: Some(port),
                url: Some(format!("http://{}:{port}/docs", browser_host(host))),
            }
        }
        CELERY_WORKER | CELERY_BEAT => {
            let app = match cfg.option("app").trim() {
                "" => project
                    .celery
                    .as_ref()
                    .and_then(|c| c.app.clone())
                    .ok_or_else(|| format!("'{}': set the Celery application (-A)", cfg.name))?,
                a => a.to_string(),
            };
            let worker = cfg.type_id == CELERY_WORKER;
            let mut args = vec![
                "-A".into(),
                app,
                if worker { "worker" } else { "beat" }.into(),
                "--loglevel".into(),
                opt(cfg, "loglevel", "INFO").to_string(),
            ];
            if worker {
                let c = opt(cfg, "concurrency", "");
                if !c.is_empty() {
                    args.extend(["--concurrency".into(), c.to_string()]);
                }
                let pool = opt(cfg, "pool", "");
                if !pool.is_empty() {
                    args.extend(["--pool".into(), pool.to_string()]);
                }
            }
            // Django projects: the worker imports the settings.
            if project.django.is_some() {
                settings_env(cfg, project, &mut env);
            }
            PythonRun {
                target: "module".into(),
                script: "celery".into(),
                args,
                env,
                working_directory: None,
                port: None,
                url: None,
            }
        }
        other => return Err(format!("'{other}' is not a web framework configuration")),
    };
    Ok(run)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::project;

    fn cfg(type_id: &str, options: &[(&str, &str)]) -> Configuration {
        let mut c = Configuration::new("x", type_id);
        for (k, v) in options {
            c = c.with_option(k, *v);
        }
        c
    }

    #[test]
    fn django() {
        let p = project(&[
            ("requirements.txt", "django\n"),
            (
                "backend/manage.py",
                "os.environ.setdefault('DJANGO_SETTINGS_MODULE', 'mysite.settings')\n",
            ),
        ]);
        let r = build(
            &cfg(DJANGO_SERVER, &[("port", "8001"), ("noReload", "true")]),
            &p.info,
        )
        .unwrap();
        assert_eq!(r.script, "manage.py");
        assert_eq!(r.working_directory.as_deref(), Some("backend"));
        assert_eq!(r.args, ["runserver", "127.0.0.1:8001", "--noreload"]);
        assert_eq!(r.env["DJANGO_SETTINGS_MODULE"], "mysite.settings");
        assert_eq!(r.url.as_deref(), Some("http://127.0.0.1:8001/"));
        let r = build(
            &cfg(
                DJANGO_MANAGE,
                &[("task", "migrate shop 0002"), ("settings", "mysite.test")],
            ),
            &p.info,
        )
        .unwrap();
        assert_eq!(r.args, ["migrate", "shop", "0002"]);
        assert_eq!(r.env["DJANGO_SETTINGS_MODULE"], "mysite.test");
        assert!(build(&cfg(DJANGO_MANAGE, &[]), &p.info).is_err());
        assert!(build(&cfg(DJANGO_SERVER, &[]), &WebProject::default()).is_err());
    }

    #[test]
    fn flask_fastapi_celery() {
        let p = project(&[
            ("requirements.txt", "flask\nfastapi\ncelery\n"),
            (
                "web/app.py",
                "from flask import Flask\napp = Flask(__name__)\n",
            ),
            (
                "src/api/main.py",
                "from fastapi import FastAPI\napp = FastAPI()\n",
            ),
            (
                "proj/celery.py",
                "from celery import Celery\napp = Celery('proj')\n",
            ),
        ]);
        let r = build(&cfg(FLASK_SERVER, &[]), &p.info).unwrap();
        assert_eq!((r.target.as_str(), r.script.as_str()), ("module", "flask"));
        assert_eq!(
            r.args,
            ["run", "--host", "127.0.0.1", "--port", "5000", "--debug"]
        );
        assert_eq!(r.env["FLASK_APP"], "web.app");
        let r = build(&cfg(FASTAPI, &[("host", "0.0.0.0")]), &p.info).unwrap();
        assert_eq!(
            r.args,
            [
                "api.main:app",
                "--host",
                "0.0.0.0",
                "--port",
                "8000",
                "--reload",
                "--app-dir",
                "src"
            ]
        );
        assert_eq!(r.url.as_deref(), Some("http://127.0.0.1:8000/docs"));
        let r = build(
            &cfg(CELERY_WORKER, &[("concurrency", "2"), ("pool", "solo")]),
            &p.info,
        )
        .unwrap();
        assert_eq!(
            r.args,
            [
                "-A",
                "proj",
                "worker",
                "--loglevel",
                "INFO",
                "--concurrency",
                "2",
                "--pool",
                "solo"
            ]
        );
        let r = build(&cfg(CELERY_BEAT, &[("loglevel", "DEBUG")]), &p.info).unwrap();
        assert_eq!(r.args, ["-A", "proj", "beat", "--loglevel", "DEBUG"]);
    }
}
