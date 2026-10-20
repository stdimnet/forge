//! Console commands: the application's own (`#[AsCommand]`, `$defaultName`, `setName()`)
//! plus the common framework and bundle commands, and the `bin/console` command line.

use std::sync::OnceLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::routes::{list_items, unquote};
use crate::{Location, PhpFile};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsoleCommand {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<Location>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hidden: bool,
    /// Provided by Symfony or a bundle rather than the application.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub builtin: bool,
}

const BUILTIN: &[(&str, &str)] = &[
    ("about", "Display information about the current project"),
    ("assets:install", "Install bundle's web assets under a public directory"),
    ("asset-map:compile", "Compile all mapped assets and writes them to the final public output directory"),
    ("cache:clear", "Clear the cache"),
    ("cache:pool:clear", "Clear cache pools"),
    ("cache:warmup", "Warm up an empty cache"),
    ("config:dump-reference", "Dump the default configuration for an extension"),
    ("debug:autowiring", "List classes/interfaces you can use for autowiring"),
    ("debug:config", "Dump the current configuration for an extension"),
    ("debug:container", "Display current services for an application"),
    ("debug:dotenv", "List all dotenv files with variables and values"),
    ("debug:event-dispatcher", "Display configured listeners for an application"),
    ("debug:router", "Display current routes for an application"),
    ("debug:twig", "Show a list of twig functions, filters, globals and tests"),
    ("doctrine:database:create", "Creates the configured database"),
    ("doctrine:database:drop", "Drops the configured database"),
    ("doctrine:fixtures:load", "Load data fixtures to your database"),
    ("doctrine:migrations:diff", "Generate a migration by comparing your current database to your mapping information"),
    ("doctrine:migrations:migrate", "Execute a migration to a specified version or the latest available version"),
    ("doctrine:migrations:status", "View the status of a set of migrations"),
    ("doctrine:query:dql", "Executes arbitrary DQL directly from the command line"),
    ("doctrine:schema:update", "Executes (or dumps) the SQL needed to update the database schema to match the current mapping metadata"),
    ("doctrine:schema:validate", "Validate the mapping files"),
    ("importmap:require", "Require JavaScript packages"),
    ("lint:container", "Ensure that arguments injected into services match type declarations"),
    ("lint:twig", "Lint a Twig template and outputs encountered errors"),
    ("lint:yaml", "Lint a YAML file and outputs encountered errors"),
    ("make:command", "Create a new console command class"),
    ("make:controller", "Create a new controller class"),
    ("make:entity", "Create or update a Doctrine entity class, and optionally an API Platform resource"),
    ("make:form", "Create a new form class"),
    ("make:migration", "Create a new migration based on database changes"),
    ("make:test", "Create a new test class"),
    ("messenger:consume", "Consume messages"),
    ("router:match", "Help debug routes by simulating a path info match"),
    ("secrets:list", "List all secrets"),
    ("secrets:set", "Set a secret in the vault"),
    ("translation:extract", "Extract missing translations keys from code to translation files"),
];

/// Commands every Symfony application (with the usual bundles) has.
pub fn builtin_commands() -> Vec<ConsoleCommand> {
    BUILTIN
        .iter()
        .map(|(name, description)| ConsoleCommand {
            name: (*name).into(),
            description: (*description).into(),
            aliases: Vec::new(),
            class: None,
            location: None,
            hidden: false,
            builtin: true,
        })
        .collect()
}

fn re(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).expect("valid regex"))
}

/// The application's commands.
pub fn commands(php: &[PhpFile]) -> Vec<ConsoleCommand> {
    static DEFAULT_NAME: OnceLock<Regex> = OnceLock::new();
    static DEFAULT_DESC: OnceLock<Regex> = OnceLock::new();
    static SET_NAME: OnceLock<Regex> = OnceLock::new();
    static SET_DESC: OnceLock<Regex> = OnceLock::new();
    static SET_ALIASES: OnceLock<Regex> = OnceLock::new();
    let mut out = Vec::new();
    for f in php {
        for c in &f.index.classes {
            let attr = c
                .attributes
                .iter()
                .find(|a| a.name == "AsCommand" || a.fqn.ends_with("\\AsCommand"));
            let is_command = attr.is_some() || c.extends.iter().any(|e| e.ends_with("Command"));
            if !is_command || c.modifiers.is_abstract {
                continue;
            }
            let body = &f.source[c.range.start..c.range.end.min(f.source.len())];
            let mut cmd = ConsoleCommand {
                name: String::new(),
                description: String::new(),
                aliases: Vec::new(),
                class: Some(c.fqn.clone()),
                location: Some(Location::new(
                    &f.path,
                    c.name_range.line,
                    c.name_range.column,
                )),
                hidden: false,
                builtin: false,
            };
            if let Some(a) = attr {
                let positional: Vec<&str> = a
                    .args
                    .iter()
                    .filter(|x| x.name.is_none())
                    .map(|x| x.value.as_str())
                    .collect();
                let named = |n: &str| {
                    a.args
                        .iter()
                        .find(|x| x.name.as_deref() == Some(n))
                        .map(|x| x.value.as_str())
                };
                if let Some(n) = named("name").or(positional.first().copied()) {
                    let n = unquote(n);
                    // `name: 'app:a|app:b'` declares aliases with pipes.
                    let mut parts = n.split('|');
                    cmd.name = parts.next().unwrap_or_default().to_string();
                    cmd.aliases
                        .extend(parts.filter(|p| !p.is_empty()).map(str::to_string));
                }
                if let Some(d) = named("description").or(positional.get(1).copied()) {
                    cmd.description = unquote(d);
                }
                if let Some(al) = named("aliases") {
                    cmd.aliases.extend(list_items(al));
                }
                cmd.hidden = named("hidden").is_some_and(|h| h.trim() == "true");
            }
            if cmd.name.is_empty() {
                if let Some(m) =
                    re(&DEFAULT_NAME, r#"\$defaultName\s*=\s*['"]([^'"]+)['"]"#).captures(body)
                {
                    cmd.name = m[1].to_string();
                } else if let Some(m) =
                    re(&SET_NAME, r#"->setName\(\s*['"]([^'"]+)['"]"#).captures(body)
                {
                    cmd.name = m[1].to_string();
                }
            }
            if cmd.description.is_empty() {
                if let Some(m) = re(
                    &DEFAULT_DESC,
                    r#"\$defaultDescription\s*=\s*['"]([^'"]+)['"]"#,
                )
                .captures(body)
                {
                    cmd.description = m[1].to_string();
                } else if let Some(m) =
                    re(&SET_DESC, r#"->setDescription\(\s*['"]([^'"]+)['"]"#).captures(body)
                {
                    cmd.description = m[1].to_string();
                }
            }
            if let Some(m) = re(&SET_ALIASES, r#"->setAliases\(\s*(\[[^\]]*\])"#).captures(body) {
                cmd.aliases.extend(list_items(&m[1]));
            }
            if !cmd.name.is_empty() {
                out.push(cmd);
            }
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Application commands followed by the built-in ones they do not override.
pub fn all_commands(app: &[ConsoleCommand]) -> Vec<ConsoleCommand> {
    let mut out = app.to_vec();
    for b in builtin_commands() {
        if !out.iter().any(|c| c.name == b.name) {
            out.push(b);
        }
    }
    out.sort_by(|a, b| a.builtin.cmp(&b.builtin).then(a.name.cmp(&b.name)));
    out
}

/// Split an argument string like a POSIX shell (quotes and backslash escapes).
pub fn shell_split(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut has = false;
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some('"'), '\\') | (None, '\\') => {
                if let Some(n) = chars.next() {
                    cur.push(n);
                }
            }
            (Some(_), c) => cur.push(c),
            (None, '\'' | '"') => {
                quote = Some(c);
                has = true;
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
    out
}

/// `php bin/console <command> <args...>`; a command name containing spaces is split too.
pub fn command_line(php: &str, console: &str, command: &str, args: &str) -> Vec<String> {
    let mut out = vec![php.to_string(), console.to_string()];
    out.extend(shell_split(command));
    out.extend(shell_split(args));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture;

    #[test]
    fn finds_attribute_and_legacy_commands() {
        let dir = fixture::project();
        let php = crate::index_php_files(dir.path(), "src");
        let cmds = commands(&php);
        let purge = cmds.iter().find(|c| c.name == "app:orders:purge").unwrap();
        assert_eq!(purge.description, "Delete abandoned orders");
        assert_eq!(
            purge.class.as_deref(),
            Some("App\\Command\\PurgeOrdersCommand")
        );
        let import = cmds.iter().find(|c| c.name == "app:import").unwrap();
        assert_eq!(import.description, "Import products from a CSV file");
        assert_eq!(import.aliases, vec!["app:imp"]);
    }

    #[test]
    fn all_commands_put_application_first() {
        let app = vec![ConsoleCommand {
            name: "app:x".into(),
            description: String::new(),
            aliases: vec![],
            class: None,
            location: None,
            hidden: false,
            builtin: false,
        }];
        let all = all_commands(&app);
        assert_eq!(all[0].name, "app:x");
        assert!(all.iter().any(|c| c.name == "cache:clear" && c.builtin));
    }

    #[test]
    fn splits_arguments() {
        assert_eq!(
            shell_split(r#"--env=prod "a b" 'c d' e\ f"#),
            vec!["--env=prod", "a b", "c d", "e f"]
        );
        assert_eq!(
            command_line("php", "bin/console", "cache:clear", "--no-warmup"),
            vec!["php", "bin/console", "cache:clear", "--no-warmup"]
        );
        assert_eq!(shell_split("''"), vec![""]);
    }
}
