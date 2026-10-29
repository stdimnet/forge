//! Run Anything (double Ctrl): the popup's grouped suggestions, its `?` help, the history
//! of what ran from it, and turning a typed command into a temporary run configuration.
//!
//! The desktop app collects the candidates (npm and Composer scripts, PHP files matching
//! `php …`) through the project backend, the UI adds the run configurations, the history
//! and commands of other plugins (Artisan, Symfony console); [`suggest`] ranks them with
//! `forge-fuzzy`. [`resolve`] maps `php file.php`, `php artisan …`, `composer …`,
//! `npm|pnpm|yarn|bun …` to the matching configuration type (so PHP runs through the
//! project's interpreter) and anything else to a shell command in the project directory.

use serde::{Deserialize, Serialize};

use crate::args::{quote_arg, split_args};
use crate::Configuration;

/// Items per group unless the request says otherwise.
pub const DEFAULT_PER_GROUP: usize = 8;
/// Entries kept in the history.
pub const MAX_HISTORY: usize = 50;

/// Something that ran from Run Anything.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HistoryEntry {
    /// The command, or the configuration name.
    pub command: String,
    /// `command` names a run configuration.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub configuration: bool,
    /// Working directory relative to the project (npm scripts of nested packages).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub cwd: String,
}

/// A run configuration offered by name.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ConfigurationItem {
    pub name: String,
    #[serde(rename = "type")]
    pub type_id: String,
    /// "PHPUnit", shown after the name.
    pub type_name: String,
}

/// The scripts of one `package.json`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NpmScripts {
    /// Directory of the `package.json` relative to the project (`""` for the root).
    pub dir: String,
    /// `npm`, `pnpm`, `yarn` or `bun`.
    pub package_manager: String,
    pub scripts: Vec<String>,
}

/// Commands another plugin offers (Artisan, Symfony console, Composer commands).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProviderGroup {
    pub id: String,
    pub title: String,
    pub commands: Vec<ProviderCommand>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProviderCommand {
    /// The full command line, e.g. `php artisan migrate`.
    pub command: String,
    pub description: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SuggestRequest {
    pub query: String,
    pub recent: Vec<HistoryEntry>,
    pub configurations: Vec<ConfigurationItem>,
    pub npm: Vec<NpmScripts>,
    pub composer_scripts: Vec<String>,
    /// PHP files for `php …` (workspace-relative).
    pub php_files: Vec<String>,
    pub providers: Vec<ProviderGroup>,
    /// 0 = [`DEFAULT_PER_GROUP`].
    pub per_group: usize,
    /// The project uses Laravel: `php artisan` is offered in the help.
    pub laravel: bool,
}

/// What picking an item does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Action {
    /// Run (or debug) a run configuration.
    Configuration { name: String },
    /// Run a command line (see [`resolve`]).
    Command {
        command: String,
        #[serde(skip_serializing_if = "String::is_empty")]
        cwd: String,
    },
    /// Put text into the input (help items).
    Insert { text: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// `recent`, `configuration`, `npm`, `composer`, `php`, `command`, `help` or a
    /// provider id.
    pub icon: String,
    /// Configuration type id of configuration items (for its icon).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_id: Option<String>,
    /// Matched character positions in `label`.
    pub matches: Vec<usize>,
    pub action: Action,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub id: String,
    pub title: String,
    pub items: Vec<Item>,
    /// More items matched than are shown.
    pub more: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestResponse {
    pub groups: Vec<Group>,
    /// The query starts with `?`: the groups are help.
    pub help: bool,
}

/// A candidate before ranking.
struct Candidate {
    label: String,
    detail: Option<String>,
    icon: String,
    type_id: Option<String>,
    action: Action,
}

fn command(command: impl Into<String>, cwd: impl Into<String>) -> Action {
    Action::Command {
        command: command.into(),
        cwd: cwd.into(),
    }
}

/// Rank `candidates` against `query`; an empty query keeps their order.
fn rank(
    id: &str,
    title: &str,
    query: &str,
    candidates: Vec<Candidate>,
    per_group: usize,
) -> Option<Group> {
    let q = query.trim();
    let mut scored: Vec<(i32, usize, Item)> = candidates
        .into_iter()
        .enumerate()
        .filter_map(|(i, c)| {
            let (score, matches) = if q.is_empty() {
                (0, Vec::new())
            } else {
                let m = forge_fuzzy::fuzzy_match(q, &c.label)?;
                (m.score, m.positions)
            };
            Some((
                score,
                i,
                Item {
                    label: c.label,
                    detail: c.detail,
                    icon: c.icon,
                    type_id: c.type_id,
                    matches,
                    action: c.action,
                },
            ))
        })
        .collect();
    if scored.is_empty() {
        return None;
    }
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    let more = scored.len() > per_group;
    Some(Group {
        id: id.into(),
        title: title.into(),
        items: scored.into_iter().take(per_group).map(|s| s.2).collect(),
        more,
    })
}

/// Help for `?`: what can be typed, each item inserts its prefix.
fn help(req: &SuggestRequest) -> Vec<Group> {
    let mut items = vec![
        ("Run configurations", "<configuration name>", ""),
        ("Recent", "", ""),
        (
            "php",
            "<file>",
            "Runs a PHP script with the project interpreter",
        ),
        ("composer", "<command>", "Runs a Composer command"),
        ("npm", "<command>", "Runs an npm command"),
        ("yarn", "<command>", "Runs a Yarn command"),
        ("pnpm", "<command>", "Runs a pnpm command"),
        ("bun", "<command>", "Runs a Bun command"),
    ];
    if req.laravel {
        items.insert(3, ("php artisan", "<command>", "Runs an Artisan command"));
    }
    let mut out: Vec<Item> = items
        .into_iter()
        .map(|(prefix, arg, description)| {
            // "Run configurations" and "Recent" describe groups; the others are prefixes.
            let is_group = prefix.starts_with(char::is_uppercase);
            Item {
                label: if arg.is_empty() {
                    prefix.to_string()
                } else {
                    format!("{prefix} {arg}")
                },
                detail: (!description.is_empty()).then(|| description.to_string()),
                icon: "help".into(),
                type_id: None,
                matches: Vec::new(),
                action: Action::Insert {
                    text: if is_group {
                        String::new()
                    } else {
                        format!("{prefix} ")
                    },
                },
            }
        })
        .collect();
    for p in &req.providers {
        if let Some(first) = p.commands.first() {
            let prefix = first
                .command
                .rsplit_once(' ')
                .map(|(head, _)| head)
                .unwrap_or(&first.command);
            if out
                .iter()
                .any(|i| i.label.starts_with(&format!("{prefix} ")))
            {
                continue;
            }
            out.push(Item {
                label: format!("{prefix} <command>"),
                detail: Some(p.title.clone()),
                icon: "help".into(),
                type_id: None,
                matches: Vec::new(),
                action: Action::Insert {
                    text: format!("{prefix} "),
                },
            });
        }
    }
    out.push(Item {
        label: "<command>".into(),
        detail: Some("Runs any command in the project directory".into()),
        icon: "help".into(),
        type_id: None,
        matches: Vec::new(),
        action: Action::Insert {
            text: String::new(),
        },
    });
    vec![Group {
        id: "help".into(),
        title: "Help".into(),
        items: out,
        more: false,
    }]
}

/// The popup's groups for a query.
pub fn suggest(req: &SuggestRequest) -> SuggestResponse {
    let query = req.query.trim_start();
    if let Some(rest) = query.strip_prefix('?') {
        let mut groups = help(req);
        let q = rest.trim();
        if !q.is_empty() {
            for g in &mut groups {
                g.items
                    .retain(|i| i.label.to_lowercase().contains(&q.to_lowercase()));
            }
        }
        return SuggestResponse { groups, help: true };
    }
    let per = if req.per_group == 0 {
        DEFAULT_PER_GROUP
    } else {
        req.per_group
    };
    let q = query.trim_end();
    let mut groups: Vec<Group> = Vec::new();

    let recent = req
        .recent
        .iter()
        .map(|h| Candidate {
            label: h.command.clone(),
            detail: (!h.cwd.is_empty()).then(|| h.cwd.clone()),
            icon: if h.configuration {
                "configuration".into()
            } else {
                "recent".into()
            },
            type_id: h
                .configuration
                .then(|| {
                    req.configurations
                        .iter()
                        .find(|c| c.name == h.command)
                        .map(|c| c.type_id.clone())
                })
                .flatten(),
            action: if h.configuration {
                Action::Configuration {
                    name: h.command.clone(),
                }
            } else {
                command(h.command.clone(), h.cwd.clone())
            },
        })
        .collect();
    groups.extend(rank("recent", "Recent", q, recent, per));

    let configurations = req
        .configurations
        .iter()
        .map(|c| Candidate {
            label: c.name.clone(),
            detail: (!c.type_name.is_empty()).then(|| c.type_name.clone()),
            icon: "configuration".into(),
            type_id: Some(c.type_id.clone()),
            action: Action::Configuration {
                name: c.name.clone(),
            },
        })
        .collect();
    groups.extend(rank(
        "configurations",
        "Run configurations",
        q,
        configurations,
        per,
    ));

    let mut npm = Vec::new();
    for pkg in &req.npm {
        let manager = match pkg.package_manager.trim() {
            "" => "npm",
            m => m,
        };
        for script in &pkg.scripts {
            npm.push(Candidate {
                label: format!("{manager} run {script}"),
                detail: (!pkg.dir.is_empty()).then(|| pkg.dir.clone()),
                icon: "npm".into(),
                type_id: None,
                action: command(
                    format!("{manager} run {}", quote_arg(script)),
                    pkg.dir.clone(),
                ),
            });
        }
    }
    groups.extend(rank("npm", "npm scripts", q, npm, per));

    let composer = req
        .composer_scripts
        .iter()
        .map(|s| Candidate {
            label: format!("composer {s}"),
            detail: None,
            icon: "composer".into(),
            type_id: None,
            action: command(format!("composer {}", quote_arg(s)), ""),
        })
        .collect();
    groups.extend(rank("composer", "Composer scripts", q, composer, per));

    // PHP files only once the query asks for them.
    if q.starts_with("php") {
        let files = req
            .php_files
            .iter()
            .map(|f| Candidate {
                label: format!("php {f}"),
                detail: None,
                icon: "php".into(),
                type_id: None,
                action: command(format!("php {}", quote_arg(f)), ""),
            })
            .collect();
        groups.extend(rank("php", "PHP files", q, files, per));
    }

    for p in &req.providers {
        let candidates = p
            .commands
            .iter()
            .map(|c| Candidate {
                label: c.command.clone(),
                detail: (!c.description.is_empty()).then(|| c.description.clone()),
                icon: p.id.clone(),
                type_id: None,
                action: command(c.command.clone(), ""),
            })
            .collect();
        // Only once the query is typed: these lists are long.
        if !q.is_empty() {
            groups.extend(rank(&p.id, &p.title, q, candidates, per));
        }
    }

    if !q.is_empty() {
        let typed = Group {
            id: "command".into(),
            title: "Commands".into(),
            items: vec![Item {
                label: q.to_string(),
                detail: Some("Run command".into()),
                icon: "command".into(),
                type_id: None,
                matches: Vec::new(),
                action: command(q, ""),
            }],
            more: false,
        };
        // The typed command comes first unless a suggestion starts with it.
        let lower = q.to_lowercase();
        let prefixed = groups
            .iter()
            .flat_map(|g| &g.items)
            .any(|i| i.label.to_lowercase().starts_with(&lower));
        if prefixed {
            groups.push(typed);
        } else {
            groups.insert(0, typed);
        }
    }
    SuggestResponse {
        groups,
        help: false,
    }
}

/// Composer script names of a `composer.json` (sorted).
pub fn composer_scripts(composer_json: &str) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(composer_json) else {
        return Vec::new();
    };
    value
        .get("scripts")
        .and_then(|s| s.as_object())
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default()
}

/// Remember something that ran: newest first, without duplicates, at most `MAX_HISTORY`.
pub fn push_history(history: &[HistoryEntry], entry: HistoryEntry) -> Vec<HistoryEntry> {
    let mut out = vec![entry.clone()];
    out.extend(
        history
            .iter()
            .filter(|h| !(h.command == entry.command && h.configuration == entry.configuration))
            .cloned(),
    );
    out.truncate(MAX_HISTORY);
    out
}

const PACKAGE_MANAGERS: &[&str] = &["npm", "pnpm", "yarn", "bun"];
/// PHP options that take the next argument as their value.
const PHP_VALUE_OPTIONS: &[&str] = &["-c", "-d", "-z"];

fn join(args: &[String]) -> String {
    args.iter()
        .map(|a| quote_arg(a))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The temporary configuration that runs a typed command. `cwd` is relative to the project
/// (empty = the project directory).
pub fn resolve(command: &str, cwd: &str) -> Result<Configuration, String> {
    let line = command.trim();
    let args = split_args(line);
    let Some(program) = args.first() else {
        return Err("Nothing to run".into());
    };
    let rest = &args[1..];
    let mut cfg = match program.as_str() {
        "php" => resolve_php(line, rest),
        "artisan" => Configuration::new(line, "artisan").with_option("command", join(rest)),
        "composer" => Configuration::new(line, "composer-command").with_option(
            "command",
            if rest.is_empty() {
                "list".into()
            } else {
                join(rest)
            },
        ),
        m if PACKAGE_MANAGERS.contains(&m) => resolve_npm(line, m, rest),
        _ => shell(line),
    };
    cfg.working_directory = cwd.trim().to_string();
    Ok(cfg)
}

fn shell(line: &str) -> Configuration {
    Configuration::new(line, "shell")
        .with_option("mode", "text")
        .with_option("scriptText", line)
}

/// `php [options] file [args]` → PHP Script; `php artisan …` → Artisan.
fn resolve_php(line: &str, rest: &[String]) -> Configuration {
    let mut options = Vec::new();
    let mut i = 0;
    while i < rest.len() && rest[i].starts_with('-') {
        options.push(rest[i].clone());
        if PHP_VALUE_OPTIONS.contains(&rest[i].as_str()) && i + 1 < rest.len() {
            options.push(rest[i + 1].clone());
            i += 1;
        }
        i += 1;
    }
    let Some(file) = rest.get(i) else {
        // `php -v`: nothing to run as a script.
        return shell(line);
    };
    let tail = &rest[i + 1..];
    let mut cfg = if file == "artisan" {
        Configuration::new(line, "artisan").with_option("command", join(tail))
    } else {
        let mut c = Configuration::new(line, "php-script").with_option("file", file.clone());
        c.arguments = join(tail);
        c
    };
    cfg.interpreter_options = join(&options);
    cfg
}

/// `npm run x -- args`, `pnpm x`, `yarn add y`.
fn resolve_npm(line: &str, manager: &str, rest: &[String]) -> Configuration {
    let base = Configuration::new(line, "npm").with_option("packageManager", manager);
    match rest.first().map(String::as_str) {
        None => base.with_option("command", "install"),
        Some("run" | "run-script") if rest.len() > 1 => {
            let mut extra = &rest[2..];
            if extra.first().map(String::as_str) == Some("--") {
                extra = &extra[1..];
            }
            let mut c = base
                .with_option("command", "run")
                .with_option("script", rest[1].clone());
            c.arguments = join(extra);
            c
        }
        Some(cmd) => {
            let mut c = base.with_option("command", cmd);
            c.arguments = join(&rest[1..]);
            c
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(query: &str) -> SuggestRequest {
        SuggestRequest {
            query: query.into(),
            recent: vec![
                HistoryEntry {
                    command: "php artisan migrate".into(),
                    ..HistoryEntry::default()
                },
                HistoryEntry {
                    command: "UserTest".into(),
                    configuration: true,
                    ..HistoryEntry::default()
                },
            ],
            configurations: vec![
                ConfigurationItem {
                    name: "UserTest".into(),
                    type_id: "phpunit".into(),
                    type_name: "PHPUnit".into(),
                },
                ConfigurationItem {
                    name: "artisan serve".into(),
                    type_id: "artisan".into(),
                    type_name: "Artisan".into(),
                },
            ],
            npm: vec![
                NpmScripts {
                    dir: String::new(),
                    package_manager: "pnpm".into(),
                    scripts: vec!["dev".into(), "build".into()],
                },
                NpmScripts {
                    dir: "admin".into(),
                    package_manager: String::new(),
                    scripts: vec!["build".into()],
                },
            ],
            composer_scripts: vec!["test".into(), "post-autoload-dump".into()],
            php_files: vec!["artisan".into(), "app/Models/User.php".into()],
            providers: vec![ProviderGroup {
                id: "artisan".into(),
                title: "Artisan commands".into(),
                commands: vec![ProviderCommand {
                    command: "php artisan queue:work".into(),
                    description: "Start processing jobs".into(),
                }],
            }],
            per_group: 0,
            laravel: true,
        }
    }

    fn ids(r: &SuggestResponse) -> Vec<&str> {
        r.groups.iter().map(|g| g.id.as_str()).collect()
    }

    #[test]
    fn an_empty_query_shows_recent_configurations_and_scripts() {
        let r = suggest(&request(""));
        assert_eq!(ids(&r), ["recent", "configurations", "npm", "composer"]);
        assert_eq!(r.groups[0].items[1].icon, "configuration");
        assert_eq!(r.groups[0].items[1].type_id.as_deref(), Some("phpunit"));
        assert_eq!(r.groups[1].items[0].detail.as_deref(), Some("PHPUnit"));
        assert_eq!(
            r.groups[2].items[2].action,
            Action::Command {
                command: "npm run build".into(),
                cwd: "admin".into()
            }
        );
    }

    #[test]
    fn queries_rank_matches_and_offer_the_typed_command() {
        // Nothing starts with the query: the typed command comes first.
        let r = suggest(&request("build"));
        assert_eq!(ids(&r), ["command", "npm"]);
        assert_eq!(r.groups[0].items[0].label, "build");
        assert_eq!(r.groups[1].items[0].label, "pnpm run build");
        assert_eq!(r.groups[1].items[0].matches, [9, 10, 11, 12, 13]);

        let r = suggest(&request("ls -la"));
        assert_eq!(r.groups[0].id, "command");
        assert_eq!(
            r.groups[0].items[0].action,
            Action::Command {
                command: "ls -la".into(),
                cwd: String::new()
            }
        );

        // A query that a suggestion starts with keeps the command last.
        let r = suggest(&request("php art"));
        assert_eq!(r.groups[0].id, "recent");
        assert_eq!(r.groups.last().unwrap().id, "command");
        assert!(ids(&r).contains(&"php"));
        assert!(ids(&r).contains(&"artisan"));
    }

    #[test]
    fn php_files_only_show_for_php_queries_and_groups_are_cut() {
        assert!(!ids(&suggest(&request("User"))).contains(&"php"));
        let r = suggest(&request("php User"));
        let php = r.groups.iter().find(|g| g.id == "php").unwrap();
        assert_eq!(php.items[0].label, "php app/Models/User.php");
        let mut many = request("");
        many.configurations = (0..12)
            .map(|i| ConfigurationItem {
                name: format!("c{i}"),
                ..ConfigurationItem::default()
            })
            .collect();
        let r = suggest(&many);
        let g = r.groups.iter().find(|g| g.id == "configurations").unwrap();
        assert_eq!(g.items.len(), DEFAULT_PER_GROUP);
        assert!(g.more);
    }

    #[test]
    fn question_mark_shows_help() {
        let r = suggest(&request("?"));
        assert!(r.help);
        let labels: Vec<&str> = r.groups[0].items.iter().map(|i| i.label.as_str()).collect();
        assert!(labels.contains(&"php <file>"));
        assert!(labels.contains(&"php artisan <command>"));
        assert!(labels.contains(&"composer <command>"));
        let php = r.groups[0]
            .items
            .iter()
            .find(|i| i.label == "php <file>")
            .unwrap();
        assert_eq!(
            php.action,
            Action::Insert {
                text: "php ".into()
            }
        );
        let r = suggest(&request("? comp"));
        assert_eq!(r.groups[0].items.len(), 1);
        let mut plain = request("?");
        plain.laravel = false;
        plain.providers.clear();
        let r = suggest(&plain);
        assert!(!r.groups[0]
            .items
            .iter()
            .any(|i| i.label.contains("artisan")));
    }

    #[test]
    fn resolves_commands_to_configuration_types() {
        let c = resolve("php -d memory_limit=-1 bin/import.php --dry-run 'a b'", "").unwrap();
        assert_eq!(c.type_id, "php-script");
        assert_eq!(c.option("file"), "bin/import.php");
        assert_eq!(c.interpreter_options, "-d memory_limit=-1");
        assert_eq!(c.arguments, "--dry-run 'a b'");
        assert_eq!(
            c.name,
            "php -d memory_limit=-1 bin/import.php --dry-run 'a b'"
        );

        let c = resolve("php artisan migrate --seed", "").unwrap();
        assert_eq!(
            (c.type_id.as_str(), c.option("command")),
            ("artisan", "migrate --seed")
        );
        let c = resolve("composer require laravel/pint --dev", "").unwrap();
        assert_eq!(
            (c.type_id.as_str(), c.option("command")),
            ("composer-command", "require laravel/pint --dev")
        );
        let c = resolve("npm run build -- --watch", "admin").unwrap();
        assert_eq!(c.type_id, "npm");
        assert_eq!(c.option("script"), "build");
        assert_eq!(c.arguments, "--watch");
        assert_eq!(c.working_directory, "admin");
        let c = resolve("yarn add lodash", "").unwrap();
        assert_eq!(
            (c.option("packageManager"), c.option("command")),
            ("yarn", "add")
        );
        assert_eq!(c.arguments, "lodash");
        assert_eq!(resolve("pnpm", "").unwrap().option("command"), "install");

        let c = resolve("  ls -la | grep x ", "").unwrap();
        assert_eq!(c.type_id, "shell");
        assert_eq!(c.option("scriptText"), "ls -la | grep x");
        assert_eq!(resolve("php -v", "").unwrap().type_id, "shell");
        assert!(resolve("   ", "").is_err());
    }

    #[test]
    fn history_is_newest_first_without_duplicates() {
        let e = |c: &str| HistoryEntry {
            command: c.into(),
            ..HistoryEntry::default()
        };
        let h = push_history(&[e("a"), e("b")], e("b"));
        assert_eq!(h, [e("b"), e("a")]);
        let long: Vec<HistoryEntry> = (0..60).map(|i| e(&i.to_string())).collect();
        assert_eq!(push_history(&long, e("x")).len(), MAX_HISTORY);
        // A configuration and a command with the same text are different entries.
        let cfg = HistoryEntry {
            configuration: true,
            ..e("a")
        };
        assert_eq!(push_history(&[e("a")], cfg.clone()), [cfg, e("a")]);
    }

    #[test]
    fn reads_composer_scripts() {
        let json = r#"{"name":"acme/shop","scripts":{"test":"phpunit","post-autoload-dump":["@php artisan package:discover"]}}"#;
        assert_eq!(composer_scripts(json), ["post-autoload-dump", "test"]);
        assert!(composer_scripts("{").is_empty());
        assert!(composer_scripts("{}").is_empty());
    }
}
