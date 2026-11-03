//! The tools Forge offers to MCP clients: name, description, JSON Schema of the arguments,
//! the group a user can turn off (Settings | Tools | MCP Server) and what a call may do.
//!
//! Positions are 1-based (lines and columns), paths are relative to the project root.

use serde::Serialize;
use serde_json::{json, Value};

/// Groups of tools that can be allowed or blocked as a whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ToolGroup {
    /// Open editors, files, search, navigation, problems, run configurations (list).
    Read,
    /// Git status and diffs.
    Vcs,
    /// Changing files in the editor.
    Edit,
    /// Running run configurations and tests.
    Run,
    /// Shell commands in the project.
    Terminal,
    /// Data sources and read-only queries.
    Database,
}

impl ToolGroup {
    pub const ALL: [ToolGroup; 6] = [
        ToolGroup::Read,
        ToolGroup::Vcs,
        ToolGroup::Edit,
        ToolGroup::Run,
        ToolGroup::Terminal,
        ToolGroup::Database,
    ];

    pub fn id(self) -> &'static str {
        match self {
            ToolGroup::Read => "read",
            ToolGroup::Vcs => "vcs",
            ToolGroup::Edit => "edit",
            ToolGroup::Run => "run",
            ToolGroup::Terminal => "terminal",
            ToolGroup::Database => "database",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            ToolGroup::Read => "Read the project",
            ToolGroup::Vcs => "Version control",
            ToolGroup::Edit => "Edit files",
            ToolGroup::Run => "Run configurations and tests",
            ToolGroup::Terminal => "Terminal commands",
            ToolGroup::Database => "Databases",
        }
    }

    /// The setting that allows the group (`mcp.tools.<id>`).
    pub fn setting_key(self) -> String {
        format!("mcp.tools.{}", self.id())
    }

    /// Whether the group is allowed when the setting is missing.
    pub fn default_enabled(self) -> bool {
        !matches!(self, ToolGroup::Terminal)
    }

    pub fn from_id(id: &str) -> Option<ToolGroup> {
        ToolGroup::ALL.into_iter().find(|g| g.id() == id)
    }
}

/// What a call may change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ToolKind {
    /// Reads the project or the IDE state.
    Read,
    /// Changes files.
    Write,
    /// Starts programs or queries a database.
    Exec,
}

/// Where the IDE answers a call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Runner {
    /// Rust services of the IDE (files, search, code intelligence, Git, databases).
    Backend,
    /// The editor UI (open editors, selections, undoable edits, problems, run configurations).
    Editor,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolSpec {
    pub name: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub group: ToolGroup,
    pub kind: ToolKind,
    pub runner: Runner,
    /// Starts programs of the project (refused in Safe Mode).
    pub runs_project_code: bool,
    /// Arguments that hold project-relative paths (`.aiignore` and the project boundary
    /// are checked for them). Array arguments are checked item by item.
    pub path_args: &'static [&'static str],
    pub input_schema: Value,
}

impl ToolSpec {
    /// Whether a call needs the user's approval when "require approval" is on.
    pub fn needs_approval(&self) -> bool {
        self.kind != ToolKind::Read
    }

    /// Terminal commands always ask, whatever the settings say.
    pub fn always_asks(&self) -> bool {
        self.group == ToolGroup::Terminal
    }

    /// The `tools/list` entry.
    pub fn to_mcp(&self) -> Value {
        json!({
            "name": self.name,
            "title": self.title,
            "description": self.description,
            "inputSchema": self.input_schema,
            "annotations": {
                "title": self.title,
                "readOnlyHint": self.kind == ToolKind::Read,
                "destructiveHint": self.kind == ToolKind::Write,
                "idempotentHint": self.kind == ToolKind::Read,
                "openWorldHint": self.group == ToolGroup::Database || self.group == ToolGroup::Terminal,
            },
        })
    }

    /// Required arguments that are missing or of the wrong JSON type.
    pub fn check_arguments(&self, args: &Value) -> Result<(), String> {
        let Some(obj) = args.as_object() else {
            return Err("arguments must be an object".into());
        };
        let props = self.input_schema["properties"].as_object();
        if let Some(required) = self.input_schema["required"].as_array() {
            for name in required.iter().filter_map(Value::as_str) {
                if obj.get(name).is_none_or(Value::is_null) {
                    return Err(format!("missing argument '{name}'"));
                }
            }
        }
        for (name, value) in obj {
            let Some(expected) = props
                .and_then(|p| p.get(name))
                .and_then(|p| p["type"].as_str())
            else {
                continue;
            };
            let ok = match expected {
                "string" => value.is_string(),
                "integer" => value.is_i64() || value.is_u64(),
                "number" => value.is_number(),
                "boolean" => value.is_boolean(),
                "array" => value.is_array(),
                "object" => value.is_object(),
                _ => true,
            } || value.is_null();
            if !ok {
                return Err(format!("argument '{name}' must be of type {expected}"));
            }
        }
        Ok(())
    }
}

fn schema(props: Value, required: &[&str]) -> Value {
    json!({ "type": "object", "properties": props, "required": required, "additionalProperties": false })
}

fn path_prop(what: &str) -> Value {
    json!({ "type": "string", "description": format!("{what}, relative to the project root") })
}

fn position_props() -> Value {
    json!({
        "path": path_prop("File"),
        "line": { "type": "integer", "minimum": 1, "description": "1-based line" },
        "column": { "type": "integer", "minimum": 1, "description": "1-based column (character in the line)" },
    })
}

/// Every tool, in the order `tools/list` shows them.
pub fn catalog() -> Vec<ToolSpec> {
    use Runner::{Backend, Editor};
    use ToolGroup::*;
    use ToolKind::{Exec, Read as R, Write};
    let t = |name,
             title,
             description,
             group,
             kind,
             runner,
             runs_project_code,
             path_args,
             input_schema| ToolSpec {
        name,
        title,
        description,
        group,
        kind,
        runner,
        runs_project_code,
        path_args,
        input_schema,
    };
    vec![
        t(
            "get_open_editors",
            "Get Open Editors",
            "Files open in Forge's editor tabs: path, whether it has unsaved changes, and which one is active.",
            Read,
            R,
            Editor,
            false,
            &[],
            schema(json!({}), &[]),
        ),
        t(
            "get_current_file",
            "Get Current File and Selection",
            "The file in the active editor with the caret position and the selected text (1-based lines and columns). With includeText the whole editor text is returned, including unsaved changes.",
            Read,
            R,
            Editor,
            false,
            &[],
            schema(
                json!({ "includeText": { "type": "boolean", "description": "Also return the editor's text" } }),
                &[],
            ),
        ),
        t(
            "open_file_in_editor",
            "Open File in Editor",
            "Open a project file in Forge's editor, optionally at a line and column, so the user sees it.",
            Read,
            R,
            Editor,
            false,
            &["path"],
            schema(
                json!({
                    "path": path_prop("File"),
                    "line": { "type": "integer", "minimum": 1 },
                    "column": { "type": "integer", "minimum": 1 },
                }),
                &["path"],
            ),
        ),
        t(
            "read_file",
            "Read File",
            "Read a project file from disk. Optionally only the lines startLine..endLine (1-based, inclusive). Lines are prefixed with their numbers when a range is given.",
            Read,
            R,
            Backend,
            false,
            &["path"],
            schema(
                json!({
                    "path": path_prop("File"),
                    "startLine": { "type": "integer", "minimum": 1 },
                    "endLine": { "type": "integer", "minimum": 1 },
                }),
                &["path"],
            ),
        ),
        t(
            "list_directory",
            "List Directory",
            "List files and folders of a project directory (the root when path is empty), up to depth levels deep. Ignored (.gitignore) and excluded folders are marked and not descended into.",
            Read,
            R,
            Backend,
            false,
            &["path"],
            schema(
                json!({
                    "path": path_prop("Directory"),
                    "depth": { "type": "integer", "minimum": 1, "maximum": 5, "description": "Levels to list (default 1)" },
                }),
                &[],
            ),
        ),
        t(
            "find_files",
            "Find Files by Name",
            "Find project files whose name or path matches a query (fuzzy, like Go to File: 'UserCtrl' finds UserController.php).",
            Read,
            R,
            Backend,
            false,
            &[],
            schema(
                json!({
                    "query": { "type": "string" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 500 },
                }),
                &["query"],
            ),
        ),
        t(
            "search_in_files",
            "Search in Files",
            "Find in Files over the project (uses Forge's search index): matching lines with 1-based line and column. Supports regular expressions, case sensitivity, whole words, file masks ('*.php, !*Test.php') and a directory.",
            Read,
            R,
            Backend,
            false,
            &["directory"],
            schema(
                json!({
                    "query": { "type": "string" },
                    "regex": { "type": "boolean" },
                    "caseSensitive": { "type": "boolean" },
                    "wholeWords": { "type": "boolean" },
                    "fileMask": { "type": "string", "description": "Comma-separated globs, '!' excludes" },
                    "directory": path_prop("Directory to search in"),
                    "limit": { "type": "integer", "minimum": 1, "maximum": 2000, "description": "Maximum matches (default 200)" },
                }),
                &["query"],
            ),
        ),
        t(
            "find_symbol",
            "Find Symbol",
            "Find classes, functions, methods and other declarations by name in the project's symbol index (like Go to Symbol / Go to Class).",
            Read,
            R,
            Backend,
            false,
            &[],
            schema(
                json!({
                    "query": { "type": "string" },
                    "classesOnly": { "type": "boolean", "description": "Only classes, interfaces, traits and other types" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 200 },
                }),
                &["query"],
            ),
        ),
        t(
            "go_to_definition",
            "Go to Definition",
            "The declaration of the symbol at a position, from the language servers and Forge's indexes.",
            Read,
            R,
            Backend,
            false,
            &["path"],
            schema(position_props(), &["path", "line", "column"]),
        ),
        t(
            "find_usages",
            "Find Usages",
            "Usages of the symbol at a position across the project, grouped by file.",
            Read,
            R,
            Backend,
            false,
            &["path"],
            schema(position_props(), &["path", "line", "column"]),
        ),
        t(
            "get_problems",
            "Get Problems",
            "Errors and warnings Forge shows for a file (language servers and inspections), or for every file with problems when path is empty.",
            Read,
            R,
            Editor,
            false,
            &["path"],
            schema(json!({ "path": path_prop("File") }), &[]),
        ),
        t(
            "run_inspections",
            "Run Inspections",
            "Run Forge's code inspections (the project's inspection profile) on a file or directory and return the problems found.",
            Read,
            R,
            Backend,
            false,
            &["path"],
            schema(json!({ "path": path_prop("File or directory") }), &["path"]),
        ),
        t(
            "list_run_configurations",
            "List Run Configurations",
            "The project's run configurations (including test configurations) with their type.",
            Read,
            R,
            Editor,
            false,
            &[],
            schema(json!({}), &[]),
        ),
        t(
            "git_status",
            "Git Status",
            "The current branch and the changed, staged and untracked files.",
            Vcs,
            R,
            Backend,
            false,
            &[],
            schema(json!({}), &[]),
        ),
        t(
            "git_diff",
            "Git Diff",
            "Unified diff of the working tree against the index (or the index against HEAD with staged), optionally only for some paths.",
            Vcs,
            R,
            Backend,
            false,
            &["paths"],
            schema(
                json!({
                    "paths": { "type": "array", "items": { "type": "string" } },
                    "staged": { "type": "boolean" },
                }),
                &[],
            ),
        ),
        t(
            "replace_text_in_file",
            "Replace Text in File",
            "Replace oldText with newText in a file. The change is made in Forge's editor (one undo step, the user sees it) and saved. oldText must occur exactly once unless replaceAll is set.",
            Edit,
            Write,
            Editor,
            false,
            &["path"],
            schema(
                json!({
                    "path": path_prop("File"),
                    "oldText": { "type": "string" },
                    "newText": { "type": "string" },
                    "replaceAll": { "type": "boolean" },
                }),
                &["path", "oldText", "newText"],
            ),
        ),
        t(
            "write_file",
            "Write File",
            "Set the whole text of a file (created when missing). Applied in Forge's editor as one undo step and saved.",
            Edit,
            Write,
            Editor,
            false,
            &["path"],
            schema(
                json!({ "path": path_prop("File"), "text": { "type": "string" } }),
                &["path", "text"],
            ),
        ),
        t(
            "reformat_file",
            "Reformat File",
            "Reformat a file with the project's code style (Reformat Code, Ctrl+Alt+L) and save it.",
            Edit,
            Write,
            Editor,
            false,
            &["path"],
            schema(json!({ "path": path_prop("File") }), &["path"]),
        ),
        t(
            "run_configuration",
            "Run Configuration",
            "Run a run configuration (tests, scripts, servers) in Forge's Run tool window and wait for it to finish: exit code, test results and the end of the console output.",
            Run,
            Exec,
            Editor,
            true,
            &[],
            schema(
                json!({
                    "name": { "type": "string", "description": "Name from list_run_configurations" },
                    "timeoutSeconds": { "type": "integer", "minimum": 1, "maximum": 1800, "description": "Stop waiting after this (default 300); the run keeps going" },
                }),
                &["name"],
            ),
        ),
        t(
            "execute_terminal_command",
            "Execute Terminal Command",
            "Run a shell command in the project root and return its exit code and output. The user approves every call.",
            Terminal,
            Exec,
            Backend,
            true,
            &[],
            schema(
                json!({
                    "command": { "type": "string" },
                    "timeoutSeconds": { "type": "integer", "minimum": 1, "maximum": 600, "description": "Kill the command after this (default 60)" },
                }),
                &["command"],
            ),
        ),
        t(
            "list_data_sources",
            "List Data Sources",
            "The database data sources configured in Forge: id, name, engine, and whether they are read-only or production.",
            Database,
            R,
            Backend,
            false,
            &[],
            schema(json!({}), &[]),
        ),
        t(
            "run_sql_query",
            "Run Read-only Query",
            "Run a read-only query (SELECT, SHOW, EXPLAIN …) on a data source. Statements that change data or schema are refused.",
            Database,
            Exec,
            Backend,
            false,
            &[],
            schema(
                json!({
                    "dataSource": { "type": "string", "description": "Id or name from list_data_sources" },
                    "sql": { "type": "string" },
                    "maxRows": { "type": "integer", "minimum": 1, "maximum": 1000, "description": "Rows per statement (default 100)" },
                }),
                &["dataSource", "sql"],
            ),
        ),
    ]
}

/// The spec of one tool.
pub fn find(name: &str) -> Option<ToolSpec> {
    catalog().into_iter().find(|t| t.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_consistent() {
        let all = catalog();
        let mut names: Vec<_> = all.iter().map(|t| t.name).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), all.len(), "tool names are unique");
        for t in &all {
            assert_eq!(t.input_schema["type"], "object", "{}", t.name);
            for p in t.path_args {
                assert!(
                    t.input_schema["properties"].get(*p).is_some(),
                    "{}: path argument {p} is in the schema",
                    t.name
                );
            }
            if t.runs_project_code {
                assert_eq!(t.kind, ToolKind::Exec, "{}", t.name);
            }
            let mcp = t.to_mcp();
            assert_eq!(mcp["name"], t.name);
            assert_eq!(mcp["annotations"]["readOnlyHint"], t.kind == ToolKind::Read);
        }
        for g in ToolGroup::ALL {
            assert!(all.iter().any(|t| t.group == g), "{g:?} has tools");
            assert_eq!(ToolGroup::from_id(g.id()), Some(g));
        }
    }

    #[test]
    fn checks_arguments() {
        let t = find("read_file").unwrap();
        assert!(t.check_arguments(&json!({ "path": "a.php" })).is_ok());
        assert_eq!(
            t.check_arguments(&json!({})).unwrap_err(),
            "missing argument 'path'"
        );
        assert_eq!(
            t.check_arguments(&json!({ "path": "a", "startLine": "x" }))
                .unwrap_err(),
            "argument 'startLine' must be of type integer"
        );
        assert!(t.check_arguments(&json!([1])).is_err());
    }

    #[test]
    fn write_and_exec_tools_need_approval() {
        assert!(!find("read_file").unwrap().needs_approval());
        assert!(find("replace_text_in_file").unwrap().needs_approval());
        assert!(find("run_configuration").unwrap().needs_approval());
        assert!(find("execute_terminal_command").unwrap().always_asks());
        assert!(!find("run_configuration").unwrap().always_asks());
    }
}
