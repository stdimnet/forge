//! Completion in the Command Line Tools Console: tool aliases, commands (prefix, substring
//! and Symfony-style abbreviations like `m:mig`), WP-CLI subcommands, options, option
//! values and arguments, plus the help of the command at the cursor (usage, description,
//! arguments and options, with the one being typed marked).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::cmdline::{self, Token};
use crate::definitions::{namespaces, ArgumentDef, CommandDef, OptionDef};
use crate::model::Tool;

/// Items shown at most.
const MAX_ITEMS: usize = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ItemKind {
    Tool,
    Command,
    /// A namespace of subcommands (WP-CLI `plugin`).
    Group,
    Option,
    Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionItem {
    pub label: String,
    /// Replaces the input between `from` and `to`.
    pub insert: String,
    pub kind: ItemKind,
    /// Short text after the label: the tool's name, the arguments, an option's shortcut.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub detail: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
}

/// The part of the command the cursor is in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "name")]
pub enum Active {
    Argument(String),
    Option(String),
}

/// Help of the command being typed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandHelp {
    /// The tool's name ("Laravel Artisan").
    pub tool: String,
    pub alias: String,
    pub command: CommandDef,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active: Option<Active>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Completion {
    /// UTF-16 range of the input the items replace.
    pub from: usize,
    pub to: usize,
    pub items: Vec<CompletionItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help: Option<CommandHelp>,
    /// What is expected at the cursor when nothing can be offered (`<name>`), or why
    /// nothing can be completed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

/// How well `name` matches what was typed: lower is better, `None` = no match.
pub fn score(name: &str, typed: &str) -> Option<u8> {
    let n = name.to_lowercase();
    let q = typed.to_lowercase();
    if q.is_empty() {
        return Some(1);
    }
    if n == q {
        return Some(0);
    }
    if n.starts_with(&q) {
        return Some(1);
    }
    // Abbreviations: `m:mi` for `make:migration`, `d:s:u` for `doctrine:schema:update`.
    if q.contains(':') {
        let ns: Vec<&str> = n.split(':').collect();
        let qs: Vec<&str> = q.split(':').collect();
        if qs.len() <= ns.len() && qs.iter().zip(&ns).all(|(q, n)| n.starts_with(q)) {
            return Some(2);
        }
    }
    if n.split([':', '-', ' ', '_'])
        .any(|part| part.starts_with(&q))
    {
        return Some(3);
    }
    n.contains(&q).then_some(4)
}

/// `<name> [<group>] <files>...`
pub fn arguments_hint(args: &[ArgumentDef]) -> String {
    args.iter()
        .map(|a| {
            let dots = if a.array { "..." } else { "" };
            if a.required {
                format!("<{}>{dots}", a.name)
            } else {
                format!("[<{}>{dots}]", a.name)
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn option_description(o: &OptionDef) -> String {
    match &o.default {
        Some(d) if !d.is_empty() => format!("{} [default: {d}]", o.description),
        _ => o.description.clone(),
    }
}

fn sorted(mut scored: Vec<(u8, usize, CompletionItem)>) -> Vec<CompletionItem> {
    scored.sort_by_key(|a| (a.0, a.1));
    scored.into_iter().take(MAX_ITEMS).map(|x| x.2).collect()
}

/// Commands or groups one word below `words` (`[]` = top level) matching `typed`.
fn command_items(defs: &[CommandDef], words: &[String], typed: &str) -> Vec<CompletionItem> {
    let level = words.len();
    let parent = words.join(" ");
    let mut scored = Vec::new();
    for d in defs.iter().filter(|d| !d.hidden) {
        let parts: Vec<&str> = d.name.split(' ').collect();
        if parts.len() != level + 1 || (level > 0 && !d.name.starts_with(&format!("{parent} "))) {
            continue;
        }
        let last = parts[level];
        let Some(s) = score(last, typed) else {
            continue;
        };
        scored.push((
            s,
            0,
            CompletionItem {
                label: last.to_string(),
                insert: format!("{} ", cmdline::quote(last)),
                kind: if d.group {
                    ItemKind::Group
                } else {
                    ItemKind::Command
                },
                detail: arguments_hint(&d.arguments),
                description: d.description.clone(),
            },
        ));
    }
    scored.sort_by(|a, b| (a.0, &a.2.label).cmp(&(b.0, &b.2.label)));
    scored.into_iter().take(MAX_ITEMS).map(|x| x.2).collect()
}

/// The command `name` stands for: exact, an alias, or an unambiguous abbreviation.
fn resolve<'a>(defs: &'a [CommandDef], name: &str) -> Option<&'a CommandDef> {
    if let Some(d) = defs
        .iter()
        .find(|d| d.name == name || d.aliases.iter().any(|a| a == name))
    {
        return Some(d);
    }
    if name.contains(' ') {
        return None;
    }
    let mut hits = defs
        .iter()
        .filter(|d| !d.hidden && !d.name.contains(' '))
        .filter(|d| matches!(score(&d.name, name), Some(1 | 2)));
    let first = hits.next()?;
    hits.next().is_none().then_some(first)
}

fn value_items(values: &[String], typed: &str) -> Vec<CompletionItem> {
    sorted(
        values
            .iter()
            .enumerate()
            .filter_map(|(i, v)| {
                Some((
                    score(v, typed)?,
                    i,
                    CompletionItem {
                        label: v.clone(),
                        insert: format!("{} ", cmdline::quote(v)),
                        kind: ItemKind::Value,
                        detail: String::new(),
                        description: String::new(),
                    },
                ))
            })
            .collect(),
    )
}

fn option_items(def: &CommandDef, used: &[String], typed: &str) -> Vec<CompletionItem> {
    let mut scored = Vec::new();
    for (i, o) in def.options.iter().enumerate() {
        if !o.multiple && used.contains(&o.name) {
            continue;
        }
        let long = format!("--{}", o.name);
        let short = o.shortcuts.iter().any(|s| format!("-{s}") == typed);
        let s = if typed.is_empty() || long.starts_with(typed) {
            Some(if long == typed || short { 0 } else { 1 })
        } else if short {
            Some(0)
        } else if typed.len() > 2 && typed.starts_with("--") && o.name.contains(&typed[2..]) {
            Some(3)
        } else {
            None
        };
        let Some(s) = s else {
            continue;
        };
        scored.push((
            s,
            i + if o.global { 10_000 } else { 0 },
            CompletionItem {
                insert: if o.value_required {
                    format!("{long}=")
                } else {
                    format!("{long} ")
                },
                label: long,
                kind: ItemKind::Option,
                detail: o
                    .shortcuts
                    .iter()
                    .map(|s| format!("-{s}"))
                    .collect::<Vec<_>>()
                    .join(", "),
                description: option_description(o),
            },
        ));
    }
    sorted(scored)
}

/// What the tokens after the command say about the cursor position.
struct ArgState<'a> {
    positional: usize,
    used: Vec<String>,
    after_double_dash: bool,
    /// The previous word was an option that needs a value.
    expect_value: Option<&'a OptionDef>,
}

fn scan<'a>(def: &'a CommandDef, tokens: &[Token]) -> ArgState<'a> {
    let mut st = ArgState {
        positional: 0,
        used: Vec::new(),
        after_double_dash: false,
        expect_value: None,
    };
    for t in tokens {
        if st.expect_value.take().is_some() {
            continue;
        }
        let v = t.value.as_str();
        if st.after_double_dash || t.quoted {
            st.positional += 1;
        } else if v == "--" {
            st.after_double_dash = true;
        } else if let Some(rest) = v.strip_prefix("--") {
            let (name, has_value) = match rest.split_once('=') {
                Some((n, _)) => (n, true),
                None => (rest, false),
            };
            if let Some(o) = def.option(name) {
                st.used.push(o.name.clone());
                if o.value_required && !has_value {
                    st.expect_value = Some(o);
                }
            }
        } else if v.len() > 1 && v.starts_with('-') {
            let s = &v[1..];
            if let Some(o) = def.option_by_shortcut(s) {
                st.used.push(o.name.clone());
                if o.value_required {
                    st.expect_value = Some(o);
                }
            } else {
                // Combined flags (`-mfs`) or a value attached to a shortcut (`-eprod`).
                for (i, c) in s.char_indices() {
                    let Some(o) = def.option_by_shortcut(&c.to_string()) else {
                        break;
                    };
                    st.used.push(o.name.clone());
                    if o.accept_value {
                        if o.value_required && i + c.len_utf8() == s.len() {
                            st.expect_value = Some(o);
                        }
                        break;
                    }
                }
            }
        } else {
            st.positional += 1;
        }
    }
    st
}

/// Complete `input` at the UTF-16 offset `offset`. `commands` holds the known commands of
/// each tool by alias.
pub fn complete(
    input: &str,
    offset: usize,
    tools: &[Tool],
    commands: &HashMap<String, Vec<CommandDef>>,
) -> Completion {
    let offset = offset.min(cmdline::utf16_len(input));
    let tokens = cmdline::tokenize(input);
    let (cur, from, to, typed) = match tokens
        .iter()
        .position(|t| t.start <= offset && offset <= t.end)
    {
        Some(i) => {
            let t = &tokens[i];
            let typed = cmdline::tokenize(cmdline::slice_utf16(input, t.start, offset))
                .into_iter()
                .next()
                .map(|t| t.value)
                .unwrap_or_default();
            (i, t.start, t.end, typed)
        }
        None => (
            tokens.iter().filter(|t| t.end < offset).count(),
            offset,
            offset,
            String::new(),
        ),
    };
    let mut out = Completion {
        from,
        to,
        ..Default::default()
    };
    let enabled: Vec<&Tool> = tools.iter().filter(|t| t.enabled).collect();

    if cur == 0 {
        if enabled.is_empty() {
            out.hint = Some("No command line tools are configured".into());
        }
        out.items = sorted(
            enabled
                .iter()
                .enumerate()
                .filter_map(|(i, t)| {
                    let s = score(&t.alias, &typed)
                        .filter(|s| *s <= 1)
                        .or_else(|| score(t.display_name(), &typed).map(|_| 5))?;
                    Some((
                        s,
                        i,
                        CompletionItem {
                            label: t.alias.clone(),
                            insert: format!("{} ", t.alias),
                            kind: ItemKind::Tool,
                            detail: t.display_name().to_string(),
                            description: t.path.clone(),
                        },
                    ))
                })
                .collect(),
        );
        return out;
    }

    let alias = &tokens[0].value;
    let Some(tool) = enabled.iter().find(|t| &t.alias == alias) else {
        out.hint = Some(format!("Unknown tool alias '{alias}'"));
        return out;
    };
    let defs: &[CommandDef] = commands.get(alias).map(Vec::as_slice).unwrap_or(&[]);
    let help = |command: &CommandDef, active: Option<Active>| CommandHelp {
        tool: tool.display_name().to_string(),
        alias: tool.alias.clone(),
        command: command.clone(),
        active,
    };

    // Options before the command (`c -d sub install`) are left alone.
    let mut j = 1;
    while j < cur && tokens[j].value.starts_with('-') {
        j += 1;
    }
    if cur < j || (cur == j && typed.starts_with('-')) {
        return out;
    }
    let mut words: Vec<String> = Vec::new();
    let mut command: Option<&CommandDef> = None;
    loop {
        if j == cur {
            out.items = command_items(defs, &words, &typed);
            if defs.is_empty() {
                out.hint = Some(format!("No commands of {} are known", tool.display_name()));
            }
            out.help = command.map(|c| help(c, None));
            return out;
        }
        let mut candidate = words.clone();
        candidate.push(tokens[j].value.clone());
        match resolve(defs, &candidate.join(" ")) {
            Some(d) => {
                words = candidate;
                command = Some(d);
                j += 1;
                if !d.group {
                    break;
                }
            }
            None => {
                if !defs.is_empty() {
                    out.hint = Some(format!("Unknown command '{}'", candidate.join(" ")));
                }
                return out;
            }
        }
    }
    let def = command.expect("a resolved command");
    let st = scan(def, &tokens[j..cur]);

    if let Some(o) = st.expect_value {
        out.items = value_items(&o.values, &typed);
        if out.items.is_empty() {
            out.hint = Some(format!("--{}: {}", o.name, o.description));
        }
        out.help = Some(help(def, Some(Active::Option(o.name.clone()))));
        return out;
    }
    if !st.after_double_dash && typed.starts_with("--") && typed.contains('=') {
        let (name, value) = typed[2..].split_once('=').expect("has '='");
        if let Some(o) = def.option(name) {
            out.from = from + 2 + cmdline::utf16_len(name) + 1;
            out.items = value_items(&o.values, value);
            if out.items.is_empty() {
                out.hint = Some(format!("--{}: {}", o.name, o.description));
            }
            out.help = Some(help(def, Some(Active::Option(o.name.clone()))));
        } else {
            out.help = Some(help(def, None));
        }
        return out;
    }
    if !st.after_double_dash && typed.starts_with('-') {
        out.items = option_items(def, &st.used, &typed);
        let exact = typed
            .strip_prefix("--")
            .and_then(|n| def.option(n))
            .or_else(|| {
                typed
                    .strip_prefix('-')
                    .and_then(|s| def.option_by_shortcut(s))
            });
        out.help = Some(help(def, exact.map(|o| Active::Option(o.name.clone()))));
        return out;
    }

    // A positional argument.
    let arg = def
        .arguments
        .get(st.positional)
        .or_else(|| def.arguments.last().filter(|a| a.array));
    let mut items = Vec::new();
    if let Some(a) = arg {
        let names: Vec<String> = match a.name.as_str() {
            "command_name" => defs
                .iter()
                .filter(|d| !d.hidden)
                .map(|d| d.name.clone())
                .collect(),
            "namespace" => namespaces(defs),
            _ => a.values.clone(),
        };
        items = value_items(&names, &typed);
        out.hint = Some(if a.description.is_empty() {
            format!("<{}>", a.name)
        } else {
            format!("<{}>: {}", a.name, a.description)
        });
    }
    if typed.is_empty() && !st.after_double_dash {
        items.extend(option_items(def, &st.used, ""));
    }
    items.truncate(MAX_ITEMS);
    out.items = items;
    out.help = Some(help(def, arg.map(|a| Active::Argument(a.name.clone()))));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scores_prefixes_abbreviations_and_substrings() {
        assert_eq!(score("make:model", "make:model"), Some(0));
        assert_eq!(score("make:model", "make"), Some(1));
        assert_eq!(score("make:migration", "m:mi"), Some(2));
        assert_eq!(score("doctrine:schema:update", "d:s:u"), Some(2));
        assert_eq!(score("make:model", "model"), Some(3));
        assert_eq!(score("make:model", "ode"), Some(4));
        assert_eq!(score("make:model", "xyz"), None);
    }

    #[test]
    fn hints_arguments() {
        let args = vec![
            ArgumentDef {
                name: "name".into(),
                required: true,
                ..Default::default()
            },
            ArgumentDef {
                name: "files".into(),
                array: true,
                ..Default::default()
            },
        ];
        assert_eq!(arguments_hint(&args), "<name> [<files>...]");
    }
}
