//! The tools of Forge's built-in agent and the text protocol to call them. The protocol works
//! with every chat model (no native function calling needed): the model answers with one
//! JSON object `{"tool": "...", "args": {...}}` inside a `<tool>` tag (a fenced JSON block or
//! a bare object are accepted too).

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::commands::{tail, OUTPUT_LIMIT};
use crate::workspace::Workspace;

/// One tool call of the agent. Serialized for the remote protocol (the file tools of a
/// remote project's sandbox run on its host).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "tool",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ToolCall {
    ListFiles {
        path: String,
    },
    ReadFile {
        path: String,
        start_line: Option<usize>,
        end_line: Option<usize>,
    },
    Search {
        query: String,
        path: String,
    },
    WriteFile {
        path: String,
        content: String,
    },
    EditFile {
        path: String,
        old: String,
        new: String,
    },
    DeleteFile {
        path: String,
    },
    RunTests,
    RunCommand {
        command: String,
    },
    Finish {
        summary: String,
    },
}

impl ToolCall {
    pub fn name(&self) -> &'static str {
        match self {
            Self::ListFiles { .. } => "list_files",
            Self::ReadFile { .. } => "read_file",
            Self::Search { .. } => "search",
            Self::WriteFile { .. } => "write_file",
            Self::EditFile { .. } => "edit_file",
            Self::DeleteFile { .. } => "delete_file",
            Self::RunTests => "run_tests",
            Self::RunCommand { .. } => "run_command",
            Self::Finish { .. } => "finish",
        }
    }

    /// One line for the log.
    pub fn summary(&self) -> String {
        match self {
            Self::ListFiles { path } if path.is_empty() => "/".into(),
            Self::ListFiles { path } | Self::DeleteFile { path } => path.clone(),
            Self::ReadFile {
                path,
                start_line,
                end_line,
            } => match (start_line, end_line) {
                (None, None) => path.clone(),
                (s, e) => format!(
                    "{path}:{}-{}",
                    s.unwrap_or(1),
                    e.map_or(String::from("end"), |e| e.to_string())
                ),
            },
            Self::Search { query, path } if path.is_empty() => format!("{query:?}"),
            Self::Search { query, path } => format!("{query:?} in {path}"),
            Self::WriteFile { path, .. } | Self::EditFile { path, .. } => path.clone(),
            Self::RunTests => "tests".into(),
            Self::RunCommand { command } => command.clone(),
            Self::Finish { summary } => summary.lines().next().unwrap_or_default().into(),
        }
    }

    /// The arguments as JSON for the log (file contents shortened).
    pub fn input(&self) -> Value {
        let short = |s: &str| {
            if s.chars().count() > 2000 {
                format!("{}…", s.chars().take(2000).collect::<String>())
            } else {
                s.to_string()
            }
        };
        match self {
            Self::ListFiles { path } | Self::DeleteFile { path } => json!({ "path": path }),
            Self::ReadFile {
                path,
                start_line,
                end_line,
            } => json!({ "path": path, "startLine": start_line, "endLine": end_line }),
            Self::Search { query, path } => json!({ "query": query, "path": path }),
            Self::WriteFile { path, content } => json!({ "path": path, "content": short(content) }),
            Self::EditFile { path, old, new } => {
                json!({ "path": path, "old": short(old), "new": short(new) })
            }
            Self::RunTests => json!({}),
            Self::RunCommand { command } => json!({ "command": command }),
            Self::Finish { summary } => json!({ "summary": summary }),
        }
    }

    /// Whether the call may change files.
    pub fn mutates(&self) -> bool {
        matches!(
            self,
            Self::WriteFile { .. }
                | Self::EditFile { .. }
                | Self::DeleteFile { .. }
                | Self::RunCommand { .. }
        )
    }
}

/// Run a file tool (`list_files`, `read_file`, `search`, `write_file`, `edit_file`,
/// `delete_file`) in `ws`; `None` for the other tools.
pub fn execute_file_tool(ws: &Workspace, call: &ToolCall) -> Option<Result<String, String>> {
    Some(match call {
        ToolCall::ListFiles { path } => ws.list(path),
        ToolCall::ReadFile {
            path,
            start_line,
            end_line,
        } => ws
            .read(path, *start_line, *end_line)
            .map(|t| tail(&t, OUTPUT_LIMIT * 4)),
        ToolCall::Search { query, path } => ws.search(query, path),
        ToolCall::WriteFile { path, content } => ws.write(path, content),
        ToolCall::EditFile { path, old, new } => ws.edit(path, old, new),
        ToolCall::DeleteFile { path } => ws.delete(path),
        ToolCall::RunTests | ToolCall::RunCommand { .. } | ToolCall::Finish { .. } => return None,
    })
}

#[derive(Deserialize)]
struct RawCall {
    #[serde(alias = "name")]
    tool: String,
    #[serde(default, alias = "arguments", alias = "input")]
    args: Value,
}

fn str_arg(args: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|k| args.get(*k))
        .and_then(Value::as_str)
        .map(String::from)
}

fn num_arg(args: &Value, keys: &[&str]) -> Option<usize> {
    keys.iter().find_map(|k| args.get(*k)).and_then(|v| {
        v.as_u64()
            .map(|n| n as usize)
            .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
    })
}

fn required(args: &Value, tool: &str, keys: &[&str]) -> Result<String, String> {
    str_arg(args, keys).ok_or_else(|| format!("{tool} needs the argument {:?}", keys[0]))
}

impl TryFrom<RawCall> for ToolCall {
    type Error = String;

    fn try_from(raw: RawCall) -> Result<Self, String> {
        let a = &raw.args;
        let t = raw.tool.as_str();
        Ok(match t {
            "list_files" | "list" | "ls" => Self::ListFiles {
                path: str_arg(a, &["path", "dir"]).unwrap_or_default(),
            },
            "read_file" | "read" => Self::ReadFile {
                path: required(a, t, &["path", "file"])?,
                start_line: num_arg(a, &["start_line", "startLine"]),
                end_line: num_arg(a, &["end_line", "endLine"]),
            },
            "search" | "grep" => Self::Search {
                query: required(a, t, &["query", "pattern"])?,
                path: str_arg(a, &["path"]).unwrap_or_default(),
            },
            "write_file" | "write" => Self::WriteFile {
                path: required(a, t, &["path", "file"])?,
                content: required(a, t, &["content", "text"])?,
            },
            "edit_file" | "edit" => Self::EditFile {
                path: required(a, t, &["path", "file"])?,
                old: required(a, t, &["old", "old_text", "oldText"])?,
                new: str_arg(a, &["new", "new_text", "newText"]).unwrap_or_default(),
            },
            "delete_file" | "delete" => Self::DeleteFile {
                path: required(a, t, &["path", "file"])?,
            },
            "run_tests" | "test" => Self::RunTests,
            "run_command" | "run" | "bash" | "shell" => Self::RunCommand {
                command: required(a, t, &["command", "cmd"])?,
            },
            "finish" | "done" => Self::Finish {
                summary: str_arg(a, &["summary", "message"]).unwrap_or_default(),
            },
            other => return Err(format!("unknown tool {other:?}")),
        })
    }
}

/// A parsed model reply: the prose before the call and the call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub thought: String,
    pub call: Result<ToolCall, String>,
}

/// Parse a model reply. `call` is `Err` with a message for the model when no valid call
/// was found.
pub fn parse_reply(text: &str) -> Reply {
    if let Some(start) = text.find("<tool>") {
        let after = &text[start + 6..];
        let body = after.find("</tool>").map_or(after, |end| &after[..end]);
        return Reply {
            thought: text[..start].trim().to_string(),
            call: parse_json(strip_fence(body)),
        };
    }
    // A fenced block with a call.
    let mut rest = text;
    let mut offset = 0;
    while let Some(open) = rest.find("```") {
        let after = &rest[open + 3..];
        let body_start = after.find('\n').map_or(0, |n| n + 1);
        let Some(close) = after[body_start..].find("```") else {
            break;
        };
        let body = &after[body_start..body_start + close];
        if let Ok(call) = parse_json(body) {
            return Reply {
                thought: text[..offset + open].trim().to_string(),
                call: Ok(call),
            };
        }
        let consumed = open + 3 + body_start + close + 3;
        offset += consumed;
        rest = &rest[consumed..];
    }
    // A bare object at the end.
    if let Some(open) = text.find('{') {
        if let Ok(call) = parse_json(&text[open..]) {
            return Reply {
                thought: text[..open].trim().to_string(),
                call: Ok(call),
            };
        }
    }
    Reply {
        thought: text.trim().to_string(),
        call: Err("No tool call found. Answer with exactly one tool call in a <tool> tag.".into()),
    }
}

fn strip_fence(body: &str) -> &str {
    let b = body.trim();
    match b.strip_prefix("```") {
        Some(rest) => {
            let rest = rest.split_once('\n').map_or("", |(_, r)| r);
            rest.trim_end().strip_suffix("```").unwrap_or(rest)
        }
        None => b,
    }
}

fn parse_json(body: &str) -> Result<ToolCall, String> {
    let body = body.trim();
    let mut de = serde_json::Deserializer::from_str(body);
    let raw = RawCall::deserialize(&mut de)
        .map_err(|e| format!("The tool call is not valid JSON: {e}"))?;
    ToolCall::try_from(raw)
}

/// The tool reference of the system prompt.
pub fn tool_reference(has_tests: bool) -> String {
    let mut s = String::from(
        "\
- list_files {\"path\": \"src\"}: files and folders below a folder (\"\" for the project root).
- read_file {\"path\": \"src/a.php\", \"start_line\": 1, \"end_line\": 80}: a file's text (lines optional).
- search {\"query\": \"function total\", \"path\": \"src\"}: lines containing the text (case-insensitive).
- write_file {\"path\": \"src/a.php\", \"content\": \"...\"}: create or overwrite a whole file.
- edit_file {\"path\": \"src/a.php\", \"old\": \"exact old text\", \"new\": \"new text\"}: replace text that occurs exactly once.
- delete_file {\"path\": \"src/old.php\"}: delete a file.
",
    );
    if has_tests {
        s.push_str("- run_tests {}: run the project's tests and get the failures.\n");
    }
    s.push_str(
        "\
- run_command {\"command\": \"php -l src/a.php\"}: run one program in the project root (no shell, no pipes; the user may have to approve it; no network tools).
- finish {\"summary\": \"what you changed and why\"}: you are done.
",
    );
    s
}

/// The system prompt of the built-in agent.
pub fn system_prompt(has_tests: bool, scope: &[String]) -> String {
    let scope = if scope.is_empty() {
        String::new()
    } else {
        format!(
            "\nYou may only change files in: {}. Read anything else you need.\n",
            scope.join(", ")
        )
    };
    let verify = if has_tests {
        "Run the tests after your changes and fix failures until they pass. "
    } else {
        ""
    };
    format!(
        "You are a coding agent working on a copy of the user's project inside Forge, an IDE. \
Nothing you do touches the user's files until they review and apply your changes.\n\
Work step by step: explore the code, make focused changes, verify them. {verify}\
Keep changes minimal and in the style of the surrounding code.\n{scope}\n\
Answer every turn with a short thought followed by exactly one tool call:\n\
<tool>{{\"tool\": \"read_file\", \"args\": {{\"path\": \"src/a.php\"}}}}</tool>\n\n\
Tools (paths are relative to the project root):\n{}",
        tool_reference(has_tests)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tagged_fenced_and_bare_calls() {
        let r = parse_reply(
            "I will read it.\n<tool>{\"tool\": \"read_file\", \"args\": {\"path\": \"a.php\", \"start_line\": \"3\"}}</tool>",
        );
        assert_eq!(r.thought, "I will read it.");
        assert_eq!(
            r.call,
            Ok(ToolCall::ReadFile {
                path: "a.php".into(),
                start_line: Some(3),
                end_line: None
            })
        );
        let r = parse_reply(
            "Done.\n```json\n{\"name\":\"finish\",\"arguments\":{\"summary\":\"Fixed\"}}\n```",
        );
        assert_eq!(
            r.call,
            Ok(ToolCall::Finish {
                summary: "Fixed".into()
            })
        );
        let r = parse_reply("Example:\n```php\n<?php echo 1;\n```\nNow {\"tool\":\"run_tests\"}");
        assert_eq!(r.call, Ok(ToolCall::RunTests));
        assert_eq!(r.thought, "Example:\n```php\n<?php echo 1;\n```\nNow");
        let r = parse_reply("<tool>```json\n{\"tool\":\"list_files\",\"args\":{}}\n```</tool>");
        assert_eq!(r.call, Ok(ToolCall::ListFiles { path: "".into() }));
    }

    #[test]
    fn reports_bad_calls_to_the_model() {
        assert!(parse_reply("I think the bug is in Order.php")
            .call
            .unwrap_err()
            .contains("No tool call"));
        assert!(
            parse_reply("<tool>{\"tool\":\"fetch_url\",\"args\":{}}</tool>")
                .call
                .unwrap_err()
                .contains("unknown tool")
        );
        assert!(
            parse_reply("<tool>{\"tool\":\"write_file\",\"args\":{\"path\":\"a\"}}</tool>")
                .call
                .unwrap_err()
                .contains("content")
        );
        assert!(parse_reply("<tool>{oops</tool>")
            .call
            .unwrap_err()
            .contains("not valid JSON"));
    }

    #[test]
    fn file_tools_run_in_the_workspace_and_round_trip_as_json() {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::new(dir.path(), Default::default(), Vec::new());
        let write = ToolCall::WriteFile {
            path: "a.txt".into(),
            content: "hello\n".into(),
        };
        assert!(execute_file_tool(&ws, &write).unwrap().is_ok());
        let read = ToolCall::ReadFile {
            path: "a.txt".into(),
            start_line: None,
            end_line: None,
        };
        assert!(execute_file_tool(&ws, &read)
            .unwrap()
            .unwrap()
            .contains("hello"));
        assert!(execute_file_tool(&ws, &ToolCall::RunTests).is_none());
        let json = serde_json::to_value(&read).unwrap();
        assert_eq!(json["tool"], "readFile");
        assert_eq!(serde_json::from_value::<ToolCall>(json).unwrap(), read);
    }

    #[test]
    fn prompt_lists_tools_and_scope() {
        let p = system_prompt(true, &["src".into()]);
        assert!(p.contains("run_tests"));
        assert!(p.contains("only change files in: src"));
        assert!(!system_prompt(false, &[]).contains("run_tests"));
        let call = ToolCall::EditFile {
            path: "a".into(),
            old: "x".repeat(3000),
            new: "y".into(),
        };
        assert!(call.mutates());
        assert_eq!(call.input()["old"].as_str().unwrap().chars().count(), 2001);
        assert_eq!(
            ToolCall::ReadFile {
                path: "a.php".into(),
                start_line: Some(5),
                end_line: None
            }
            .summary(),
            "a.php:5-end"
        );
    }
}
