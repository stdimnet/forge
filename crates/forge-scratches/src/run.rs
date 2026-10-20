//! Running scratches: which program runs a scratch (PHP with the project's Composer
//! autoloader, Node for JavaScript/TypeScript, …), and the gutter ▶ markers of runnable
//! scratches and of the requests in `.http` files.

use serde::{Deserialize, Serialize};

use crate::naming::extension;
use crate::{http, Error, Result};

/// How a scratch is executed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Runner {
    /// The project's PHP interpreter.
    Php,
    /// `node`.
    Node,
    /// `node --experimental-strip-types` (Node 22.6+ runs TypeScript directly).
    TypeScript,
    Python,
    Shell,
    Lua,
    /// Requests of an `.http` file, sent by Forge's HTTP client.
    Http,
    /// Statements executed in the attached data source's console.
    Sql,
}

impl Runner {
    /// Scratches run as a process in the Run tool window (not HTTP or SQL).
    pub fn is_process(self) -> bool {
        !matches!(self, Runner::Http | Runner::Sql)
    }
}

/// The runner of a scratch, from its extension.
pub fn runner_for(name: &str) -> Option<Runner> {
    Some(match extension(name).as_str() {
        "php" | "phtml" => Runner::Php,
        "js" | "mjs" | "cjs" => Runner::Node,
        "ts" | "mts" | "cts" => Runner::TypeScript,
        "py" => Runner::Python,
        "sh" | "bash" => Runner::Shell,
        "lua" => Runner::Lua,
        "http" | "rest" => Runner::Http,
        "sql" => Runner::Sql,
        _ => return None,
    })
}

/// What the backend knows when it builds a scratch's command line.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunContext {
    /// Setting `php.interpreter` (`php` when empty).
    #[serde(default)]
    pub php_interpreter: String,
    /// Project root; scratches run in it so relative paths behave as in project scripts.
    #[serde(default)]
    pub project_root: Option<String>,
    /// Absolute path of the project's `vendor/autoload.php`, when it exists. PHP scratches
    /// load it with `-d auto_prepend_file=…`, so project classes are available.
    #[serde(default)]
    pub autoloader: Option<String>,
}

/// A command line to start; the app turns it into a run of the Run tool window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunCommand {
    pub program: String,
    pub args: Vec<String>,
    /// Working directory (absolute).
    pub cwd: String,
}

/// Command line that runs the scratch at `file` (absolute path).
pub fn run_command(file: &str, ctx: &RunContext) -> Result<RunCommand> {
    let name = file.rsplit(['/', '\\']).next().unwrap_or(file);
    let runner = runner_for(name)
        .ok_or_else(|| Error::Invalid(format!("'{name}' cannot be run: unknown language")))?;
    let (program, mut args): (String, Vec<String>) = match runner {
        Runner::Php => {
            let php = match ctx.php_interpreter.trim() {
                "" => "php".to_string(),
                p => p.to_string(),
            };
            let mut args = Vec::new();
            if let Some(autoload) = ctx.autoloader.as_deref().filter(|a| !a.is_empty()) {
                args.push("-d".into());
                args.push(format!("auto_prepend_file={autoload}"));
            }
            (php, args)
        }
        Runner::Node => ("node".into(), vec![]),
        Runner::TypeScript => ("node".into(), vec!["--experimental-strip-types".into()]),
        Runner::Python => (
            if cfg!(windows) { "python" } else { "python3" }.into(),
            vec![],
        ),
        Runner::Shell => ("sh".into(), vec![]),
        Runner::Lua => ("lua".into(), vec![]),
        Runner::Http => {
            return Err(Error::Invalid(
                "HTTP request scratches are sent with the HTTP client".into(),
            ))
        }
        Runner::Sql => {
            return Err(Error::Invalid(
                "SQL scratches run in the console of their data source".into(),
            ))
        }
    };
    args.push(file.to_string());
    let cwd = ctx
        .project_root
        .clone()
        .filter(|r| !r.is_empty())
        .unwrap_or_else(|| {
            file.rsplit_once(['/', '\\'])
                .map(|(dir, _)| dir.to_string())
                .unwrap_or_default()
        });
    Ok(RunCommand { program, args, cwd })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MarkerKind {
    /// Run the whole scratch.
    Run,
    /// Send the request starting on this line.
    Http,
}

/// A gutter ▶ icon.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Marker {
    /// 0-based line.
    pub line: usize,
    pub kind: MarkerKind,
    /// Tooltip.
    pub title: String,
    /// Name of the run (Run tool window tab): the scratch's name, or for a request
    /// `rest-api | Show order` (`rest-api | #2` when the request has no name), as in PHPStorm.
    pub run_name: String,
}

/// Gutter markers of a scratch: one ▶ on the first line of runnable scratches and one per
/// request of an `.http` file.
pub fn markers(name: &str, content: &str) -> Vec<Marker> {
    match runner_for(name) {
        Some(Runner::Http) => http::parse(content)
            .requests
            .iter()
            .map(|r| Marker {
                line: r.line,
                kind: MarkerKind::Http,
                title: format!("Run '{}'", r.title()),
                run_name: format!(
                    "{} | {}",
                    stem(name),
                    r.name
                        .clone()
                        .unwrap_or_else(|| format!("#{}", r.index + 1))
                ),
            })
            .collect(),
        Some(r) if r.is_process() => vec![Marker {
            line: 0,
            kind: MarkerKind::Run,
            title: format!("Run '{name}'"),
            run_name: name.to_string(),
        }],
        _ => vec![],
    }
}

/// `rest-api.http` → `rest-api`.
fn stem(name: &str) -> &str {
    name.rsplit_once('.')
        .map(|(stem, _)| stem)
        .filter(|s| !s.is_empty())
        .unwrap_or(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn php_scratches_load_the_project_autoloader() {
        let ctx = RunContext {
            php_interpreter: "/usr/bin/php8.3".into(),
            project_root: Some("/home/me/shop".into()),
            autoloader: Some("/home/me/shop/vendor/autoload.php".into()),
        };
        let cmd = run_command("/home/me/.config/forge/scratches/scratch.php", &ctx).unwrap();
        assert_eq!(cmd.program, "/usr/bin/php8.3");
        assert_eq!(
            cmd.args,
            [
                "-d",
                "auto_prepend_file=/home/me/shop/vendor/autoload.php",
                "/home/me/.config/forge/scratches/scratch.php"
            ]
        );
        assert_eq!(cmd.cwd, "/home/me/shop");

        // Without a project (or without vendor/) the plain interpreter runs it in place.
        let cmd = run_command("/s/scratch_1.php", &RunContext::default()).unwrap();
        assert_eq!(cmd.program, "php");
        assert_eq!(cmd.args, ["/s/scratch_1.php"]);
        assert_eq!(cmd.cwd, "/s");
    }

    #[test]
    fn javascript_and_typescript_run_with_node() {
        let ctx = RunContext::default();
        let js = run_command("/s/scratch.js", &ctx).unwrap();
        assert_eq!((js.program.as_str(), js.args.len()), ("node", 1));
        let ts = run_command("/s/scratch.ts", &ctx).unwrap();
        assert_eq!(ts.args, ["--experimental-strip-types", "/s/scratch.ts"]);
    }

    #[test]
    fn http_sql_and_unknown_scratches_are_not_processes() {
        let ctx = RunContext::default();
        assert!(run_command("/s/rest-api.http", &ctx).is_err());
        assert!(run_command("/s/scratch.sql", &ctx).is_err());
        assert!(run_command("/s/buffer1.txt", &ctx).is_err());
        assert_eq!(runner_for("x.mjs"), Some(Runner::Node));
        assert_eq!(runner_for("x.txt"), None);
    }

    #[test]
    fn gutter_markers() {
        assert_eq!(
            markers("scratch.php", "<?php\necho 1;\n"),
            [Marker {
                line: 0,
                kind: MarkerKind::Run,
                title: "Run 'scratch.php'".into(),
                run_name: "scratch.php".into(),
            }]
        );
        let http = markers(
            "rest-api.http",
            "### Users\nGET http://localhost/users\n\n###\n# @name create\nPOST http://localhost/users\n",
        );
        assert_eq!(http.len(), 2);
        assert_eq!((http[0].line, http[0].kind), (1, MarkerKind::Http));
        assert_eq!(http[0].title, "Run 'Users'");
        assert_eq!((http[1].line, http[1].title.as_str()), (5, "Run 'create'"));
        assert_eq!(http[0].run_name, "rest-api | Users");
        let unnamed = markers("a.http", "GET http://x\n###\nGET http://y\n");
        assert_eq!(unnamed[1].run_name, "a | #2");
        assert!(markers("scratch.sql", "select 1").is_empty());
        assert!(markers("buffer1.txt", "x").is_empty());
    }
}
