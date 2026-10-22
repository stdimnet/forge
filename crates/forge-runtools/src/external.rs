//! External Tools (Settings | Tools | External Tools): programs with macro arguments that
//! run from Tools | External Tools, the editor and Project view context menus, the keymap
//! and as a "Before launch" task.
//!
//! Tools are stored in the user setting `tools.externalTools` as an `objects` list (string
//! values): see [`ExternalTool::from_object`] / [`ExternalTool::to_object`].

use std::collections::BTreeMap;

use forge_output::OutputFilter;
use serde::{Deserialize, Serialize};

use crate::args::{quote_arg, split_args};
use crate::macros::{self, Expander, MacroContext};

/// Group of tools without one, like PHPStorm's default group.
pub const DEFAULT_GROUP: &str = "External Tools";

fn yes() -> bool {
    true
}

/// One External Tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalTool {
    pub name: String,
    /// Group (submenu) name; empty = [`DEFAULT_GROUP`].
    #[serde(default)]
    pub group: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub program: String,
    #[serde(default)]
    pub arguments: String,
    /// Empty = the project directory.
    #[serde(default)]
    pub working_directory: String,
    /// "Synchronize files after execution".
    #[serde(default = "yes")]
    pub synchronize: bool,
    /// "Open console for tool output".
    #[serde(default = "yes")]
    pub open_console: bool,
    /// "Make console active on message in stdout" / "… in stderr".
    #[serde(default)]
    pub activate_on_stdout: bool,
    #[serde(default)]
    pub activate_on_stderr: bool,
    /// Output filters (`$FILE_PATH$:$LINE$`), one regular expression each.
    #[serde(default)]
    pub output_filters: Vec<String>,
    /// Unchecked tools are hidden from menus and the keymap.
    #[serde(default = "yes")]
    pub enabled: bool,
}

impl Default for ExternalTool {
    fn default() -> Self {
        Self {
            name: String::new(),
            group: String::new(),
            description: String::new(),
            program: String::new(),
            arguments: String::new(),
            working_directory: String::new(),
            synchronize: true,
            open_console: true,
            activate_on_stdout: false,
            activate_on_stderr: false,
            output_filters: Vec::new(),
            enabled: true,
        }
    }
}

fn flag(map: &BTreeMap<String, String>, key: &str, default: bool) -> bool {
    match map.get(key).map(|v| v.trim()) {
        Some("true") => true,
        Some("false") => false,
        _ => default,
    }
}

impl ExternalTool {
    /// The group it is shown in.
    pub fn group_name(&self) -> &str {
        match self.group.trim() {
            "" => DEFAULT_GROUP,
            g => g,
        }
    }

    /// The keymap action id, like PHPStorm's `Tool_<group>_<name>`.
    pub fn action_id(&self) -> String {
        format!("Tool_{}_{}", self.group_name(), self.name.trim())
    }

    /// Read a tool from its settings object (string values; filters one per line).
    pub fn from_object(map: &BTreeMap<String, String>) -> Self {
        let get = |k: &str| map.get(k).cloned().unwrap_or_default();
        Self {
            name: get("name"),
            group: get("group"),
            description: get("description"),
            program: get("program"),
            arguments: get("arguments"),
            working_directory: get("workingDirectory"),
            synchronize: flag(map, "synchronize", true),
            open_console: flag(map, "openConsole", true),
            activate_on_stdout: flag(map, "activateOnStdout", false),
            activate_on_stderr: flag(map, "activateOnStderr", false),
            output_filters: get("outputFilters")
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect(),
            enabled: flag(map, "enabled", true),
        }
    }

    /// The settings object of the tool.
    pub fn to_object(&self) -> BTreeMap<String, String> {
        let mut m = BTreeMap::new();
        let mut put = |k: &str, v: String| {
            m.insert(k.to_string(), v);
        };
        put("name", self.name.clone());
        put("group", self.group.clone());
        put("description", self.description.clone());
        put("program", self.program.clone());
        put("arguments", self.arguments.clone());
        put("workingDirectory", self.working_directory.clone());
        put("synchronize", self.synchronize.to_string());
        put("openConsole", self.open_console.to_string());
        put("activateOnStdout", self.activate_on_stdout.to_string());
        put("activateOnStderr", self.activate_on_stderr.to_string());
        put("outputFilters", self.output_filters.join("\n"));
        put("enabled", self.enabled.to_string());
        m
    }

    /// How many `$Prompt$` dialogs a run needs.
    pub fn prompt_count(&self) -> usize {
        [&self.program, &self.arguments, &self.working_directory]
            .iter()
            .map(|t| macros::prompt_count(t))
            .sum()
    }

    /// Whether any field uses `$Name$` (`ClipboardContent`, `SelectedText`, …).
    pub fn uses(&self, name: &str) -> bool {
        [&self.program, &self.arguments, &self.working_directory]
            .iter()
            .any(|t| macros::used(t).iter().any(|m| m == name))
    }
}

/// Problems that keep the Edit Tool dialog from closing.
pub fn validate(tool: &ExternalTool, others: &[ExternalTool]) -> Vec<String> {
    let mut out = Vec::new();
    let name = tool.name.trim();
    if name.is_empty() {
        out.push("Specify the tool name".to_string());
    } else if others
        .iter()
        .any(|o| o.name.trim() == name && o.group_name() == tool.group_name())
    {
        out.push(format!(
            "Tool \"{name}\" already exists in group \"{}\"",
            tool.group_name()
        ));
    }
    if tool.program.trim().is_empty() {
        out.push("Specify the program".to_string());
    }
    for f in &tool.output_filters {
        if let Err(e) = OutputFilter::compile(f) {
            out.push(e);
        }
    }
    out
}

/// What a tool run starts.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCommand {
    pub program: String,
    pub args: Vec<String>,
    /// Absolute, or empty for the project directory.
    pub cwd: String,
    /// The tool's output filters.
    pub output_filters: Vec<String>,
    /// The command line for the console header.
    pub command_line: String,
    /// Macros without a value (no file open for `$FilePath$`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub missing: Vec<String>,
}

/// Expand the tool's program, arguments and working directory. Arguments are split first
/// and expanded one by one, so a `$FilePath$` with spaces stays one argument.
pub fn command(tool: &ExternalTool, ctx: &MacroContext) -> Result<ToolCommand, String> {
    let mut e = Expander::new(ctx);
    let program = e.expand(tool.program.trim());
    if program.trim().is_empty() {
        return Err(format!(
            "Program of tool \"{}\" is not specified",
            tool.name
        ));
    }
    let args: Vec<String> = split_args(&tool.arguments)
        .iter()
        .map(|a| e.expand(a))
        .collect();
    let cwd = match tool.working_directory.trim() {
        "" => ctx.project_dir.clone().unwrap_or_default(),
        wd => e.expand(wd),
    };
    let command_line = std::iter::once(program.as_str())
        .chain(args.iter().map(String::as_str))
        .map(quote_arg)
        .collect::<Vec<_>>()
        .join(" ");
    Ok(ToolCommand {
        program,
        args,
        cwd,
        output_filters: tool.output_filters.clone(),
        command_line,
        missing: e.into_missing(),
    })
}

/// Tools of one group, in settings order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolGroup {
    pub name: String,
    pub tools: Vec<ExternalTool>,
}

/// Enabled tools grouped for menus: groups in order of first appearance.
pub fn groups(tools: &[ExternalTool]) -> Vec<ToolGroup> {
    let mut out: Vec<ToolGroup> = Vec::new();
    for t in tools
        .iter()
        .filter(|t| t.enabled && !t.name.trim().is_empty())
    {
        match out.iter_mut().find(|g| g.name == t.group_name()) {
            Some(g) => g.tools.push(t.clone()),
            None => out.push(ToolGroup {
                name: t.group_name().to_string(),
                tools: vec![t.clone()],
            }),
        }
    }
    out
}

/// A tool by the value of a "Run External tool" task: `Group/Name`, or a bare name.
pub fn find<'a>(tools: &'a [ExternalTool], reference: &str) -> Option<&'a ExternalTool> {
    let reference = reference.trim();
    tools
        .iter()
        .find(|t| format!("{}/{}", t.group_name(), t.name.trim()) == reference)
        .or_else(|| tools.iter().find(|t| t.name.trim() == reference))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool() -> ExternalTool {
        ExternalTool {
            name: "PHP CS Fixer".into(),
            group: "Linters".into(),
            program: "$ProjectFileDir$/vendor/bin/php-cs-fixer".into(),
            arguments: "fix $FilePath$ --rules=$Prompt$".into(),
            output_filters: vec!["$FILE_PATH$:$LINE$".into()],
            ..ExternalTool::default()
        }
    }

    fn ctx() -> MacroContext {
        MacroContext {
            project_dir: Some("/p/my shop".into()),
            file_path: Some("/p/my shop/app/User Model.php".into()),
            prompts: vec!["@PSR12".into()],
            ..MacroContext::default()
        }
    }

    #[test]
    fn builds_the_command_with_expanded_macros() {
        let cmd = command(&tool(), &ctx()).unwrap();
        assert_eq!(cmd.program, "/p/my shop/vendor/bin/php-cs-fixer");
        assert_eq!(
            cmd.args,
            ["fix", "/p/my shop/app/User Model.php", "--rules=@PSR12"]
        );
        assert_eq!(cmd.cwd, "/p/my shop");
        assert_eq!(cmd.output_filters, ["$FILE_PATH$:$LINE$"]);
        assert_eq!(
            cmd.command_line,
            "'/p/my shop/vendor/bin/php-cs-fixer' fix '/p/my shop/app/User Model.php' --rules=@PSR12"
        );
        assert!(cmd.missing.is_empty());

        let mut t = tool();
        t.working_directory = "$FileDir$".into();
        assert_eq!(command(&t, &ctx()).unwrap().cwd, "/p/my shop/app");
        t.program = "  ".into();
        assert!(command(&t, &ctx()).is_err());
        let no_file = MacroContext {
            project_dir: Some("/p".into()),
            ..MacroContext::default()
        };
        assert_eq!(
            command(&tool(), &no_file).unwrap().missing,
            ["$FilePath$", "$Prompt$"]
        );
    }

    #[test]
    fn round_trips_through_the_settings_object() {
        let mut t = tool();
        t.output_filters
            .push("in $FILE_PATH$ on line $LINE$".into());
        t.open_console = false;
        let back = ExternalTool::from_object(&t.to_object());
        assert_eq!(back, t);
        // Missing keys get PHPStorm's defaults.
        let mut m = BTreeMap::new();
        m.insert("name".to_string(), "ls".to_string());
        let d = ExternalTool::from_object(&m);
        assert!(d.synchronize && d.open_console && d.enabled);
        assert_eq!(d.group_name(), DEFAULT_GROUP);
        assert_eq!(d.action_id(), "Tool_External Tools_ls");
        assert_eq!(tool().prompt_count(), 1);
        assert!(tool().uses("FilePath"));
        assert!(!tool().uses("ClipboardContent"));
    }

    #[test]
    fn validates_names_programs_and_filters() {
        let mut t = ExternalTool::default();
        t.output_filters = vec!["$LINE$".into()];
        let problems = validate(&t, &[]);
        assert_eq!(problems.len(), 3, "{problems:?}");
        assert!(validate(&tool(), &[]).is_empty());
        let dup = validate(&tool(), &[tool()]);
        assert_eq!(
            dup,
            ["Tool \"PHP CS Fixer\" already exists in group \"Linters\""]
        );
    }

    #[test]
    fn groups_enabled_tools_and_finds_task_references() {
        let a = tool();
        let b = ExternalTool {
            name: "ls".into(),
            program: "ls".into(),
            ..ExternalTool::default()
        };
        let c = ExternalTool {
            name: "PHPStan".into(),
            group: "Linters".into(),
            program: "phpstan".into(),
            enabled: false,
            ..ExternalTool::default()
        };
        let all = [a.clone(), b.clone(), c];
        let g = groups(&all);
        assert_eq!(g.len(), 2);
        assert_eq!(g[0].name, "Linters");
        assert_eq!(g[0].tools, [a.clone()]);
        assert_eq!(g[1].name, DEFAULT_GROUP);
        assert_eq!(find(&all, "Linters/PHP CS Fixer"), Some(&a));
        assert_eq!(find(&all, "ls"), Some(&b));
        assert_eq!(find(&all, "missing"), None);
    }
}
