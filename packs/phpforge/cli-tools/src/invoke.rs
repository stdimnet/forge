//! How a tool is started: through PHP (`php artisan …`, `php composer.phar …`) or as its
//! own program (`composer …`, `wp …`), and the parsed console input.

use std::collections::BTreeMap;

use phpforge_interpreters::Program;
use serde::{Deserialize, Serialize};

use crate::cmdline;
use crate::model::{ListFormat, Tool, ToolKind};

/// What to start for a tool, as if PHP ran locally in the project root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInvocation {
    /// [`Program::Php`]: the interpreter runs `args` (the script first); otherwise the
    /// tool's own program.
    pub program: Program,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
}

impl ToolInvocation {
    /// The command line for display (`php artisan make:model Order --ansi`).
    pub fn display(&self) -> String {
        let program = match &self.program {
            Program::Php => "php".to_string(),
            Program::Other(p) => p.clone(),
        };
        let mut words = vec![program];
        words.extend(self.args.iter().cloned());
        cmdline::join(&words)
    }
}

/// Whether the tool runs as a PHP script (always through the PHP interpreter).
pub fn runs_with_php(tool: &Tool) -> bool {
    let path = tool.path.trim().to_ascii_lowercase();
    match tool.kind {
        ToolKind::Composer | ToolKind::WpCli => {
            path.ends_with(".phar") || path.ends_with(".php") || path.starts_with("vendor/bin/")
        }
        _ => true,
    }
}

fn base(tool: &Tool) -> ToolInvocation {
    let path = match tool.path.trim() {
        "" => tool.kind.default_path().to_string(),
        p => p.to_string(),
    };
    if runs_with_php(tool) {
        ToolInvocation {
            program: Program::Php,
            args: vec![path],
            env: BTreeMap::new(),
        }
    } else {
        ToolInvocation {
            program: Program::Other(path),
            args: Vec::new(),
            env: BTreeMap::new(),
        }
    }
}

/// The invocation that runs `args` with `tool` for the console: colored output (the
/// console renders ANSI) unless the arguments say otherwise.
pub fn run_invocation(tool: &Tool, args: &[String]) -> ToolInvocation {
    let mut inv = base(tool);
    inv.args.extend(args.iter().cloned());
    let has = |flag: &str| args.iter().any(|a| a == flag);
    match tool.kind.format() {
        ListFormat::Symfony => {
            if !has("--ansi") && !has("--no-ansi") && !args.is_empty() {
                inv.args.push("--ansi".into());
            }
            inv.env.insert("COLUMNS".into(), "120".into());
        }
        ListFormat::WpCli => {
            if !has("--color") && !has("--no-color") && !args.is_empty() {
                inv.args.push("--color".into());
            }
        }
        ListFormat::None => {}
    }
    inv
}

/// The invocation that lists the tool's commands, `None` for tools without a list.
pub fn list_invocation(tool: &Tool) -> Option<ToolInvocation> {
    let mut inv = base(tool);
    match tool.kind.format() {
        ListFormat::Symfony => {
            inv.args.extend(["list", "--format=json"].map(String::from));
            inv.env.insert("NO_COLOR".into(), "1".into());
        }
        ListFormat::WpCli => inv.args.extend(["cli", "cmd-dump"].map(String::from)),
        ListFormat::None => return None,
    }
    Some(inv)
}

/// The console input split into the tool and its arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedInput<'a> {
    pub tool: &'a Tool,
    pub args: Vec<String>,
}

/// Find the tool of `input`'s first word among the enabled `tools`.
pub fn parse_input<'a>(input: &str, tools: &'a [Tool]) -> Result<ParsedInput<'a>, String> {
    let mut words = cmdline::split(input);
    if words.is_empty() {
        return Err("Type a command: <alias> <command> [arguments]".into());
    }
    let alias = words.remove(0);
    let tool = tools
        .iter()
        .find(|t| t.enabled && t.alias == alias)
        .ok_or_else(|| {
            let known: Vec<&str> = tools
                .iter()
                .filter(|t| t.enabled)
                .map(|t| t.alias.as_str())
                .collect();
            if known.is_empty() {
                "No command line tools are configured. Add one in Settings | Tools | Command Line Tool Support.".to_string()
            } else {
                format!(
                    "Unknown tool alias '{alias}'. Known aliases: {}",
                    known.join(", ")
                )
            }
        })?;
    Ok(ParsedInput { tool, args: words })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool(kind: ToolKind, path: &str) -> Tool {
        let mut t = Tool::new(kind);
        t.path = path.into();
        t
    }

    #[test]
    fn php_scripts_and_programs() {
        let artisan = run_invocation(
            &tool(ToolKind::Artisan, "artisan"),
            &["make:model".into(), "Order".into()],
        );
        assert_eq!(artisan.program, Program::Php);
        assert_eq!(artisan.args, ["artisan", "make:model", "Order", "--ansi"]);
        assert_eq!(artisan.display(), "php artisan make:model Order --ansi");

        let composer = run_invocation(&tool(ToolKind::Composer, "composer"), &["install".into()]);
        assert_eq!(composer.program, Program::Other("composer".into()));
        assert_eq!(composer.args, ["install", "--ansi"]);

        let phar = run_invocation(
            &tool(ToolKind::Composer, "tools/composer.phar"),
            &["update".into(), "--no-ansi".into()],
        );
        assert_eq!(phar.program, Program::Php);
        assert_eq!(phar.args, ["tools/composer.phar", "update", "--no-ansi"]);

        let wp = run_invocation(
            &tool(ToolKind::WpCli, "wp"),
            &["plugin".into(), "list".into()],
        );
        assert_eq!(wp.args, ["plugin", "list", "--color"]);
        let script = run_invocation(&tool(ToolKind::PhpScript, "bin/tool.php"), &["x".into()]);
        assert_eq!(script.args, ["bin/tool.php", "x"]);
    }

    #[test]
    fn list_commands() {
        let l = list_invocation(&tool(ToolKind::Symfony, "")).unwrap();
        assert_eq!(l.args, ["bin/console", "list", "--format=json"]);
        let wp = list_invocation(&tool(ToolKind::WpCli, "vendor/bin/wp")).unwrap();
        assert_eq!(
            (wp.program, wp.args),
            (
                Program::Php,
                vec!["vendor/bin/wp".to_string(), "cli".into(), "cmd-dump".into()]
            )
        );
        assert!(list_invocation(&tool(ToolKind::PhpScript, "x.php")).is_none());
    }

    #[test]
    fn parses_input() {
        let mut off = tool(ToolKind::Symfony, "bin/console");
        off.enabled = false;
        let tools = vec![tool(ToolKind::Artisan, "artisan"), off];
        let p = parse_input("a make:model 'Order Item' -m", &tools).unwrap();
        assert_eq!(p.tool.kind, ToolKind::Artisan);
        assert_eq!(p.args, ["make:model", "Order Item", "-m"]);
        assert!(parse_input("s cache:clear", &tools)
            .unwrap_err()
            .contains("Known aliases: a"));
        assert!(parse_input("  ", &tools).is_err());
        assert!(parse_input("x", &[])
            .unwrap_err()
            .contains("Settings | Tools"));
    }
}
