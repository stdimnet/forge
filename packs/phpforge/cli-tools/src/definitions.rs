//! Command definitions of a tool: parsed from Symfony Console's `list --format=json` (Artisan,
//! Symfony, Composer, Drush and every tool based on Symfony Console) or WP-CLI's
//! `wp cli cmd-dump`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A positional argument.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ArgumentDef {
    pub name: String,
    pub required: bool,
    /// Takes all remaining values (`<files>...`).
    pub array: bool,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    /// Known values (WP-CLI `options:`), offered by completion.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<String>,
}

/// An option (`--name`, `-n`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OptionDef {
    /// Without the leading `--`.
    pub name: String,
    /// Without the leading `-` (`v`, `vv`, `vvv`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub shortcuts: Vec<String>,
    pub accept_value: bool,
    pub value_required: bool,
    pub multiple: bool,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<String>,
    /// Defined by the application for every command (`--help`, `--ansi`, `--path`).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub global: bool,
}

/// A command of a tool. WP-CLI commands have several words (`cache add`); their parents are
/// listed too, as groups.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CommandDef {
    pub name: String,
    pub description: String,
    /// `make:model [options] [--] <name>`
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub usage: Vec<String>,
    /// The long help text (Symfony `help`, WP-CLI `longdesc`).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub help: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub hidden: bool,
    /// A namespace of subcommands (WP-CLI `cache`), not runnable on its own.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub group: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub arguments: Vec<ArgumentDef>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<OptionDef>,
}

impl CommandDef {
    pub fn option(&self, name: &str) -> Option<&OptionDef> {
        self.options.iter().find(|o| o.name == name)
    }

    pub fn option_by_shortcut(&self, shortcut: &str) -> Option<&OptionDef> {
        self.options
            .iter()
            .find(|o| o.shortcuts.iter().any(|s| s == shortcut))
    }

    /// The first usage line, else the name.
    pub fn synopsis(&self) -> &str {
        self.usage.first().map(String::as_str).unwrap_or(&self.name)
    }
}

/// Options Symfony Console adds to every command of an application.
const SYMFONY_GLOBALS: &[&str] = &[
    "help",
    "quiet",
    "verbose",
    "version",
    "ansi",
    "no-ansi",
    "no-interaction",
    "silent",
    "env",
    "profile",
    "no-plugins",
    "no-scripts",
    "working-dir",
    "no-cache",
    "no-debug",
];

/// The JSON object in `text`: tools may print warnings or deprecation notices before it.
fn json_object(text: &str) -> Option<Value> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    serde_json::from_str(text.get(start..=end)?).ok()
}

/// A JSON default as text: `null`, `false` and empty arrays are no default.
fn default_text(v: &Value) -> Option<String> {
    match v {
        Value::Null | Value::Bool(false) => None,
        Value::Bool(true) => Some("true".into()),
        Value::String(s) => Some(s.clone()),
        Value::Array(a) if a.is_empty() => None,
        Value::Array(a) => Some(
            a.iter()
                .map(|x| {
                    x.as_str()
                        .map(String::from)
                        .unwrap_or_else(|| x.to_string())
                })
                .collect::<Vec<_>>()
                .join(", "),
        ),
        other => Some(other.to_string()),
    }
}

/// A JSON object whose entries keep their order (PHP encodes an empty one as `[]`).
#[derive(Debug, Default)]
struct Ordered(Vec<(String, Value)>);

impl<'de> Deserialize<'de> for Ordered {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = Ordered;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("an object or an empty array")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut m: A,
            ) -> Result<Ordered, A::Error> {
                let mut out = Vec::new();
                while let Some((k, v)) = m.next_entry::<String, Value>()? {
                    out.push((k, v));
                }
                Ok(Ordered(out))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut s: A,
            ) -> Result<Ordered, A::Error> {
                while s.next_element::<Value>()?.is_some() {}
                Ok(Ordered(Vec::new()))
            }
        }
        d.deserialize_any(V)
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct SfDefinition {
    arguments: Ordered,
    options: Ordered,
}

#[derive(Deserialize)]
struct SfCommand {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    usage: Value,
    #[serde(default)]
    help: Option<String>,
    #[serde(default)]
    aliases: Value,
    #[serde(default)]
    hidden: bool,
    #[serde(default)]
    definition: Option<SfDefinition>,
}

#[derive(Deserialize)]
struct SfList {
    commands: Vec<SfCommand>,
}

/// Parse Symfony Console's `list --format=json`. Hidden commands are kept (flagged) so
/// they still run and show help, but completion skips them. Commands come back sorted.
pub fn parse_symfony_list(text: &str) -> Option<Vec<CommandDef>> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    let list: SfList = serde_json::from_str(text.get(start..=end)?).ok()?;
    let mut out: Vec<CommandDef> = list.commands.into_iter().map(symfony_command).collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Some(out)
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

fn symfony_command(c: SfCommand) -> CommandDef {
    let flag = |v: &Value, k: &str| v.get(k).and_then(Value::as_bool).unwrap_or(false);
    let text = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let definition = c.definition.unwrap_or_default();
    let arguments = definition
        .arguments
        .0
        .into_iter()
        // The application's `command` argument (the command name itself).
        .filter(|(k, _)| k != "command")
        .map(|(k, a)| ArgumentDef {
            name: k,
            required: flag(&a, "is_required"),
            array: flag(&a, "is_array"),
            description: text(&a, "description"),
            default: a.get("default").and_then(default_text),
            values: Vec::new(),
        })
        .collect();
    let options = definition
        .options
        .0
        .into_iter()
        .map(|(k, o)| OptionDef {
            shortcuts: o
                .get("shortcut")
                .and_then(Value::as_str)
                .unwrap_or("")
                .split('|')
                .map(|s| s.trim_start_matches('-'))
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect(),
            accept_value: flag(&o, "accept_value"),
            value_required: flag(&o, "is_value_required"),
            multiple: flag(&o, "is_multiple"),
            description: text(&o, "description"),
            default: if flag(&o, "accept_value") {
                o.get("default").and_then(default_text)
            } else {
                None
            },
            values: Vec::new(),
            global: SYMFONY_GLOBALS.contains(&k.as_str()),
            name: k,
        })
        .collect();
    CommandDef {
        description: c.description.unwrap_or_default(),
        usage: strings(&c.usage),
        help: c.help.unwrap_or_default(),
        aliases: strings(&c.aliases),
        hidden: c.hidden,
        group: false,
        arguments,
        options,
        name: c.name,
    }
}

/// Parse WP-CLI's `wp cli cmd-dump`: every command with its parents as groups, named by
/// their words without the leading `wp` (`cache`, `cache add`). Sorted by name.
pub fn parse_wp_cli_dump(text: &str) -> Option<Vec<CommandDef>> {
    let root = json_object(text)?;
    let mut out = Vec::new();
    for sub in root.get("subcommands")?.as_array()? {
        walk_wp(sub, "", &mut out);
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Some(out)
}

fn walk_wp(node: &Value, parent: &str, out: &mut Vec<CommandDef>) {
    let Some(word) = node.get("name").and_then(Value::as_str) else {
        return;
    };
    let name = if parent.is_empty() {
        word.to_string()
    } else {
        format!("{parent} {word}")
    };
    let text = |k: &str| {
        node.get(k)
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string()
    };
    let subs = node
        .get("subcommands")
        .and_then(Value::as_array)
        .filter(|s| !s.is_empty());
    let longdesc = text("longdesc");
    let synopsis = text("synopsis");
    let mut def = CommandDef {
        name: name.clone(),
        description: text("description"),
        help: longdesc.clone(),
        group: subs.is_some(),
        usage: vec![format!("wp {name} {synopsis}").trim_end().to_string()],
        ..Default::default()
    };
    if subs.is_none() {
        let (arguments, mut options) = parse_wp_synopsis(&synopsis);
        let docs = parse_wp_longdesc(&longdesc);
        let mut arguments = arguments;
        for a in &mut arguments {
            if let Some(d) = docs.iter().find(|d| !d.option && d.name == a.name) {
                a.description = d.description.clone();
                a.values = d.values.clone();
                a.default = d.default.clone();
            }
        }
        for o in &mut options {
            if let Some(d) = docs.iter().find(|d| d.option && d.name == o.name) {
                o.description = d.description.clone();
                o.values = d.values.clone();
                o.default = d.default.clone();
            }
        }
        options.extend(wp_globals());
        def.arguments = arguments;
        def.options = options;
    }
    out.push(def);
    for sub in subs.into_iter().flatten() {
        walk_wp(sub, &name, out);
    }
}

/// Global parameters of every WP-CLI command.
fn wp_globals() -> Vec<OptionDef> {
    const LIST: &[(&str, bool, &str)] = &[
        ("path", true, "Path to the WordPress files."),
        ("url", true, "Pretend request came from given URL."),
        ("user", true, "Set the WordPress user."),
        ("skip-plugins", false, "Skip loading all plugins."),
        ("skip-themes", false, "Skip loading all themes."),
        (
            "skip-packages",
            false,
            "Skip loading all installed packages.",
        ),
        ("context", true, "Load WordPress in a given context."),
        ("color", false, "Whether to colorize the output."),
        ("no-color", false, "Do not colorize the output."),
        (
            "debug",
            false,
            "Show all PHP errors and add verbosity to WP-CLI output.",
        ),
        (
            "prompt",
            false,
            "Prompt the user to enter values for all command arguments.",
        ),
        ("quiet", false, "Suppress informational messages."),
    ];
    LIST.iter()
        .map(|(name, value, description)| OptionDef {
            name: (*name).into(),
            accept_value: *value,
            value_required: *value,
            description: (*description).into(),
            global: true,
            ..Default::default()
        })
        .collect()
}

/// Arguments and options of a WP-CLI synopsis such as
/// `<key> [<group>] [--format=<format>] [--force] [<file>...]`.
pub fn parse_wp_synopsis(synopsis: &str) -> (Vec<ArgumentDef>, Vec<OptionDef>) {
    let mut args = Vec::new();
    let mut opts = Vec::new();
    for raw in synopsis.split_whitespace() {
        let optional = raw.starts_with('[') && raw.ends_with(']');
        let tok = if optional {
            &raw[1..raw.len() - 1]
        } else {
            raw
        };
        let (tok, repeat) = match tok.strip_suffix("...") {
            Some(t) => (t, true),
            None => (tok, false),
        };
        if let Some(name) = tok.strip_prefix('<').and_then(|t| t.strip_suffix('>')) {
            args.push(ArgumentDef {
                name: name.into(),
                required: !optional,
                array: repeat,
                ..Default::default()
            });
        } else if let Some(rest) = tok.strip_prefix("--") {
            // `--<field>=<value>`: any field; nothing to complete.
            if rest.starts_with('<') {
                continue;
            }
            let (name, accept, required) = match rest.find(['=', '[']) {
                Some(i) => (&rest[..i], true, rest[i..].starts_with('=')),
                None => (rest, false, false),
            };
            opts.push(OptionDef {
                name: name.into(),
                accept_value: accept,
                value_required: required,
                multiple: repeat,
                ..Default::default()
            });
        }
    }
    (args, opts)
}

struct WpDoc {
    option: bool,
    name: String,
    description: String,
    default: Option<String>,
    values: Vec<String>,
}

/// Descriptions, defaults and allowed values from a WP-CLI `longdesc`:
///
/// ```text
/// [--format=<format>]
/// : Render output in a particular format.
/// ---
/// default: table
/// options:
///   - table
///   - json
/// ---
/// ```
fn parse_wp_longdesc(longdesc: &str) -> Vec<WpDoc> {
    let lines: Vec<&str> = longdesc.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let head = lines[i].trim();
        let Some(desc) = lines.get(i + 1).and_then(|l| l.trim().strip_prefix(": ")) else {
            i += 1;
            continue;
        };
        let (args, opts) = parse_wp_synopsis(head);
        let (option, name) = match (args.first(), opts.first()) {
            (Some(a), None) => (false, a.name.clone()),
            (None, Some(o)) => (true, o.name.clone()),
            _ => {
                i += 1;
                continue;
            }
        };
        let mut doc = WpDoc {
            option,
            name,
            description: desc.trim().to_string(),
            default: None,
            values: Vec::new(),
        };
        i += 2;
        if lines.get(i).map(|l| l.trim()) == Some("---") {
            i += 1;
            let mut in_options = false;
            while i < lines.len() && lines[i].trim() != "---" {
                let l = lines[i].trim();
                if let Some(d) = l.strip_prefix("default:") {
                    doc.default =
                        Some(d.trim().trim_matches('"').to_string()).filter(|d| !d.is_empty());
                    in_options = false;
                } else if l == "options:" {
                    in_options = true;
                } else if let Some(v) = l.strip_prefix("- ").filter(|_| in_options) {
                    doc.values.push(v.trim().trim_matches('"').to_string());
                } else {
                    in_options = false;
                }
                i += 1;
            }
            i += 1;
        }
        out.push(doc);
    }
    out
}

/// The namespaces of Symfony command names (`make` of `make:model`), sorted.
pub fn namespaces(commands: &[CommandDef]) -> Vec<String> {
    let mut out: Vec<String> = commands
        .iter()
        .filter_map(|c| c.name.rsplit_once(':').map(|(ns, _)| ns.to_string()))
        .collect();
    out.sort();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symfony_defaults_and_shortcuts() {
        let json = r#"PHP Deprecated: something
{"commands":[{"name":"x","usage":["x"],"description":"X","help":"","hidden":false,"aliases":[],
 "definition":{"arguments":{},"options":{
  "verbose":{"name":"--verbose","shortcut":"-v|-vv|-vvv","accept_value":false,"is_value_required":false,"is_multiple":false,"description":"Verbosity","default":false},
  "only":{"name":"--only","shortcut":"","accept_value":true,"is_value_required":true,"is_multiple":true,"description":"Only","default":[]},
  "env":{"name":"--env","shortcut":"-e","accept_value":true,"is_value_required":true,"is_multiple":false,"description":"Env","default":"dev"}}}}]}"#;
        let cmds = parse_symfony_list(json).unwrap();
        let o = &cmds[0].options;
        assert_eq!(o[0].shortcuts, ["v", "vv", "vvv"]);
        assert!(o[0].global && !o[1].global);
        assert_eq!(o[1].default, None);
        assert!(o[1].multiple && o[1].value_required);
        assert_eq!(o[2].default.as_deref(), Some("dev"));
        assert!(parse_symfony_list("not json").is_none());
    }

    #[test]
    fn wp_synopsis_tokens() {
        let (a, o) = parse_wp_synopsis(
            "<key> [<group>] [<file>...] --role=<role> [--format=<format>] [--porcelain] [--<field>=<value>] [--skip-email[=<x>]]",
        );
        assert_eq!(
            a.iter()
                .map(|a| (a.name.as_str(), a.required, a.array))
                .collect::<Vec<_>>(),
            [
                ("key", true, false),
                ("group", false, false),
                ("file", false, true)
            ]
        );
        assert_eq!(
            o.iter()
                .map(|o| (o.name.as_str(), o.accept_value, o.value_required))
                .collect::<Vec<_>>(),
            [
                ("role", true, true),
                ("format", true, true),
                ("porcelain", false, false),
                ("skip-email", true, false)
            ]
        );
    }
}
