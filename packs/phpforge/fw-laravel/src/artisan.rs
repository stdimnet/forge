//! Artisan commands: the list from `php artisan list --format=json` (with a built-in
//! fallback when PHP is not available), command line splitting, and running commands with
//! streamed output.

use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtisanCommand {
    pub name: String,
    pub description: String,
    /// `make:model [options] [--] <name>`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<String>,
    #[serde(default)]
    pub arguments: Vec<String>,
    #[serde(default)]
    pub options: Vec<String>,
}

/// Parse `php artisan list --format=json`. Hidden commands are skipped.
pub fn parse_list_json(json: &str) -> Option<Vec<ArtisanCommand>> {
    // Artisan may print warnings before the JSON.
    let start = json.find('{')?;
    let v: serde_json::Value = serde_json::from_str(json[start..].trim()).ok()?;
    let mut out: Vec<ArtisanCommand> = v["commands"]
        .as_array()?
        .iter()
        .filter(|c| !c["hidden"].as_bool().unwrap_or(false))
        .filter_map(|c| {
            let name = c["name"].as_str()?.to_string();
            let keys = |section: &str| -> Vec<String> {
                let mut k: Vec<String> = c["definition"][section]
                    .as_object()
                    .map(|m| m.keys().cloned().collect())
                    .unwrap_or_default();
                k.sort();
                k
            };
            Some(ArtisanCommand {
                description: c["description"].as_str().unwrap_or_default().to_string(),
                usage: c["usage"]
                    .as_array()
                    .and_then(|u| u.first())
                    .and_then(|u| u.as_str())
                    .map(String::from),
                arguments: keys("arguments")
                    .into_iter()
                    .filter(|a| a != "command")
                    .collect(),
                options: keys("options")
                    .into_iter()
                    .filter(|o| {
                        !matches!(
                            o.as_str(),
                            "help"
                                | "quiet"
                                | "verbose"
                                | "version"
                                | "ansi"
                                | "no-ansi"
                                | "no-interaction"
                                | "env"
                                | "silent"
                        )
                    })
                    .collect(),
                name,
            })
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Some(out)
}

/// Common commands of a fresh Laravel 11/12 application.
pub fn builtin_commands() -> Vec<ArtisanCommand> {
    const LIST: &[(&str, &str, &str)] = &[
        (
            "about",
            "Display basic information about your application",
            "",
        ),
        ("cache:clear", "Flush the application cache", ""),
        (
            "config:cache",
            "Create a cache file for faster configuration loading",
            "",
        ),
        ("config:clear", "Remove the configuration cache file", ""),
        (
            "config:show",
            "Display all of the values for a given configuration file or key",
            "config",
        ),
        ("db:seed", "Seed the database with records", "class"),
        (
            "db:show",
            "Display information about the given database",
            "",
        ),
        ("db:wipe", "Drop all tables, views, and types", ""),
        (
            "down",
            "Put the application into maintenance / demo mode",
            "",
        ),
        ("env", "Display the current framework environment", ""),
        (
            "event:list",
            "List the application's events and listeners",
            "",
        ),
        ("inspire", "Display an inspiring quote", ""),
        ("key:generate", "Set the application key", ""),
        ("make:command", "Create a new Artisan command", "name"),
        (
            "make:component",
            "Create a new view component class",
            "name",
        ),
        ("make:controller", "Create a new controller class", "name"),
        ("make:event", "Create a new event class", "name"),
        ("make:factory", "Create a new model factory", "name"),
        ("make:job", "Create a new job class", "name"),
        ("make:listener", "Create a new event listener class", "name"),
        ("make:mail", "Create a new email class", "name"),
        (
            "make:middleware",
            "Create a new HTTP middleware class",
            "name",
        ),
        ("make:migration", "Create a new migration file", "name"),
        ("make:model", "Create a new Eloquent model class", "name"),
        (
            "make:notification",
            "Create a new notification class",
            "name",
        ),
        ("make:policy", "Create a new policy class", "name"),
        ("make:request", "Create a new form request class", "name"),
        ("make:resource", "Create a new resource", "name"),
        ("make:seeder", "Create a new seeder class", "name"),
        ("make:test", "Create a new test class", "name"),
        ("make:view", "Create a new view", "name"),
        ("migrate", "Run the database migrations", ""),
        (
            "migrate:fresh",
            "Drop all tables and re-run all migrations",
            "",
        ),
        ("migrate:refresh", "Reset and re-run all migrations", ""),
        (
            "migrate:rollback",
            "Rollback the last database migration",
            "",
        ),
        ("migrate:status", "Show the status of each migration", ""),
        (
            "model:show",
            "Show information about an Eloquent model",
            "model",
        ),
        (
            "optimize",
            "Cache framework bootstrap, configuration, and metadata to increase performance",
            "",
        ),
        ("optimize:clear", "Remove the cached bootstrap files", ""),
        (
            "queue:work",
            "Start processing jobs on the queue as a daemon",
            "connection",
        ),
        (
            "route:cache",
            "Create a route cache file for faster route registration",
            "",
        ),
        ("route:clear", "Remove the route cache file", ""),
        ("route:list", "List all registered routes", ""),
        ("schedule:list", "List all scheduled tasks", ""),
        ("schedule:run", "Run the scheduled commands", ""),
        (
            "serve",
            "Serve the application on the PHP development server",
            "",
        ),
        (
            "storage:link",
            "Create the symbolic links configured for the application",
            "",
        ),
        ("test", "Run the application tests", ""),
        ("tinker", "Interact with your application", ""),
        ("up", "Bring the application out of maintenance mode", ""),
        (
            "vendor:publish",
            "Publish any publishable assets from vendor packages",
            "",
        ),
        (
            "view:cache",
            "Compile all of the application's Blade templates",
            "",
        ),
        ("view:clear", "Clear all compiled view files", ""),
    ];
    LIST.iter()
        .map(|(name, description, arg)| ArtisanCommand {
            name: (*name).into(),
            description: (*description).into(),
            usage: Some(if arg.is_empty() {
                format!("{name} [options]")
            } else {
                format!("{name} [options] [--] <{arg}>")
            }),
            arguments: if arg.is_empty() {
                Vec::new()
            } else {
                vec![(*arg).into()]
            },
            options: Vec::new(),
        })
        .collect()
}

/// Split a command line like a POSIX shell (single/double quotes, backslash escapes).
/// A leading `php artisan` or `artisan` is removed.
pub fn split_command_line(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut has = false;
    let mut quote: Option<char> = None;
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some('"'), '\\') => {
                if let Some(n) = chars.next() {
                    cur.push(n);
                }
            }
            (Some(_), c) => cur.push(c),
            (None, '\'' | '"') => {
                quote = Some(c);
                has = true;
            }
            (None, '\\') => {
                if let Some(n) = chars.next() {
                    cur.push(n);
                    has = true;
                }
            }
            (None, c) if c.is_whitespace() => {
                if has || !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                    has = false;
                }
            }
            (None, c) => cur.push(c),
        }
    }
    if has || !cur.is_empty() {
        out.push(cur);
    }
    if out.first().map(String::as_str) == Some("php")
        && out.get(1).map(String::as_str) == Some("artisan")
    {
        out.drain(..2);
    } else if out.first().map(String::as_str) == Some("artisan") {
        out.remove(0);
    }
    out
}

/// Progress of a running artisan command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ArtisanEvent {
    Started { command_line: String },
    Output { text: String, stderr: bool },
    Exit { code: i32 },
    Error { message: String },
}

fn artisan_command(root: &Path, php: &str, args: &[String]) -> Command {
    let mut cmd = Command::new(php);
    cmd.arg("artisan")
        .args(args)
        .current_dir(root)
        .env("NO_COLOR", "1");
    // In Flatpak PHP runs on the host.
    let mut cmd = forge_hostspawn::host_command(cmd);
    cmd.stdin(Stdio::null());
    cmd
}

/// Whether artisan can run at all (`artisan` and `vendor/autoload.php` exist).
pub fn is_runnable(root: &Path) -> bool {
    root.join("artisan").is_file() && root.join("vendor/autoload.php").is_file()
}

/// Run `php artisan <args>` and return stdout, or `None` on failure or timeout.
pub fn capture(root: &Path, php: &str, args: &[&str], timeout: Duration) -> Option<String> {
    let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    let mut child = artisan_command(root, php, &args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stdout.read_to_string(&mut s);
        s
    });
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let out = reader.join().ok()?;
                return status.success().then_some(out);
            }
            Ok(None) if start.elapsed() < timeout => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

/// Commands of the project, or the built-in list when artisan cannot run.
pub fn list_commands(root: &Path, php: &str) -> (Vec<ArtisanCommand>, bool) {
    if is_runnable(root) {
        if let Some(cmds) = capture(
            root,
            php,
            &["list", "--format=json"],
            Duration::from_secs(20),
        )
        .and_then(|json| parse_list_json(&json))
        {
            return (cmds, true);
        }
    }
    (builtin_commands(), false)
}

/// Run `php artisan <line>`, streaming output line by line until it exits or `cancel` is
/// set. Returns the exit code (`-1` when it could not start or was cancelled).
pub fn run(
    root: &Path,
    php: &str,
    line: &str,
    cancel: &AtomicBool,
    on_event: &mut dyn FnMut(ArtisanEvent),
) -> i32 {
    let args = split_command_line(line);
    on_event(ArtisanEvent::Started {
        command_line: format!("php artisan {}", args.join(" ")),
    });
    let mut child = match artisan_command(root, php, &args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            on_event(ArtisanEvent::Error {
                message: format!("Cannot run {php}: {e}"),
            });
            on_event(ArtisanEvent::Exit { code: -1 });
            return -1;
        }
    };
    let (tx, rx) = mpsc::channel::<(String, bool)>();
    let mut readers = Vec::new();
    if let Some(out) = child.stdout.take() {
        let tx = tx.clone();
        readers.push(std::thread::spawn(move || {
            for l in BufReader::new(out).lines().map_while(|l| l.ok()) {
                let _ = tx.send((l, false));
            }
        }));
    }
    if let Some(err) = child.stderr.take() {
        let tx = tx.clone();
        readers.push(std::thread::spawn(move || {
            for l in BufReader::new(err).lines().map_while(|l| l.ok()) {
                let _ = tx.send((l, true));
            }
        }));
    }
    drop(tx);
    let code = loop {
        while let Ok((text, stderr)) = rx.recv_timeout(Duration::from_millis(30)) {
            on_event(ArtisanEvent::Output { text, stderr });
        }
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            break -1;
        }
        if let Ok(Some(status)) = child.try_wait() {
            for r in readers.drain(..) {
                let _ = r.join();
            }
            for (text, stderr) in rx.try_iter() {
                on_event(ArtisanEvent::Output { text, stderr });
            }
            break status.code().unwrap_or(-1);
        }
    };
    on_event(ArtisanEvent::Exit { code });
    code
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_command_lines() {
        assert_eq!(
            split_command_line("php artisan make:model Order -m"),
            vec!["make:model", "Order", "-m"]
        );
        assert_eq!(
            split_command_line(r#"tinker --execute="echo 'a b';" 'x y' \""#),
            vec!["tinker", "--execute=echo 'a b';", "x y", "\""]
        );
        assert_eq!(split_command_line("  "), Vec::<String>::new());
    }

    #[test]
    fn parses_artisan_list_json() {
        let json = r#"Warning: something
{"application":{"name":"Laravel Framework","version":"12.4.1"},"commands":[
 {"name":"make:model","description":"Create a new Eloquent model class","usage":["make:model [options] [--] <name>"],"hidden":false,
  "definition":{"arguments":{"name":{}},"options":{"help":{},"migration":{},"factory":{}}}},
 {"name":"_complete","description":"","usage":[],"hidden":true,"definition":{"arguments":{},"options":{}}},
 {"name":"about","description":"Display basic information","usage":["about"],"hidden":false,"definition":{"arguments":{"command":{}},"options":{}}}
]}"#;
        let cmds = parse_list_json(json).unwrap();
        assert_eq!(
            cmds.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
            vec!["about", "make:model"]
        );
        assert_eq!(cmds[1].arguments, vec!["name"]);
        assert_eq!(cmds[1].options, vec!["factory", "migration"]);
        assert!(cmds[0].arguments.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn runs_and_streams_output() {
        // A fake "php" that prints its arguments; artisan is just the first argument.
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("fake-php");
        std::fs::write(
            &script,
            "#!/bin/sh\necho \"args: $*\"\necho oops >&2\nexit 3\n",
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut events = Vec::new();
        let code = run(
            dir.path(),
            script.to_str().unwrap(),
            "php artisan route:list --json",
            &AtomicBool::new(false),
            &mut |e| events.push(e),
        );
        assert_eq!(code, 3);
        assert_eq!(
            events[0],
            ArtisanEvent::Started {
                command_line: "php artisan route:list --json".into()
            }
        );
        assert!(events.contains(&ArtisanEvent::Output {
            text: "args: artisan route:list --json".into(),
            stderr: false
        }));
        assert!(events.contains(&ArtisanEvent::Output {
            text: "oops".into(),
            stderr: true
        }));
        assert_eq!(events.last(), Some(&ArtisanEvent::Exit { code: 3 }));
        assert_eq!(
            capture(
                dir.path(),
                script.to_str().unwrap(),
                &["x"],
                Duration::from_secs(5)
            ),
            None
        );
    }

    #[test]
    fn builtin_list_is_sorted_and_complete() {
        let b = builtin_commands();
        assert!(b.windows(2).all(|w| w[0].name < w[1].name));
        assert!(b.iter().any(|c| c.name == "migrate"));
    }
}
