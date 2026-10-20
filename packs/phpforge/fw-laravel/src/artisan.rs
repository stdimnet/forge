//! Artisan commands: the list from `php artisan list --format=json` (with a built-in
//! fallback when PHP is not available) and command line splitting for running them.

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

    #[test]
    fn builtin_list_is_sorted_and_complete() {
        let b = builtin_commands();
        assert!(b.windows(2).all(|w| w[0].name < w[1].name));
        assert!(b.iter().any(|c| c.name == "migrate"));
    }
}
