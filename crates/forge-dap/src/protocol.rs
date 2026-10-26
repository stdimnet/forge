//! Debug Adapter Protocol messages and the typed bodies Forge uses.
//!
//! Field names follow the specification (<https://microsoft.github.io/debug-adapter-protocol/specification>),
//! so the types serialize to and from the wire format directly. Unknown fields are ignored.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A protocol message (`type` = `request`, `response` or `event`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Message {
    Request(Request),
    Response(Response),
    Event(Event),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub seq: i64,
    pub command: String,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub arguments: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Response {
    pub seq: i64,
    pub request_seq: i64,
    pub success: bool,
    pub command: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub body: Value,
}

impl Response {
    /// The adapter's error text: `body.error.format` (with `{var}` placeholders filled in),
    /// else `message`.
    pub fn error_text(&self) -> String {
        if let Some(err) = self.body.get("error") {
            if let Some(format) = err.get("format").and_then(Value::as_str) {
                let mut text = format.to_string();
                if let Some(vars) = err.get("variables").and_then(Value::as_object) {
                    for (k, v) in vars {
                        if let Some(v) = v.as_str() {
                            text = text.replace(&format!("{{{k}}}"), v);
                        }
                    }
                }
                return text;
            }
        }
        self.message
            .clone()
            .unwrap_or_else(|| format!("'{}' failed", self.command))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub seq: i64,
    pub event: String,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub body: Value,
}

macro_rules! dap_struct {
    ($(#[$meta:meta])* pub struct $name:ident { $($body:tt)* }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
        #[serde(rename_all = "camelCase", default)]
        pub struct $name { $($body)* }
    };
}

dap_struct! {
    /// What an adapter supports (answer of `initialize`, updated by `capabilities` events).
    pub struct Capabilities {
        pub supports_configuration_done_request: bool,
        pub supports_function_breakpoints: bool,
        pub supports_conditional_breakpoints: bool,
        pub supports_hit_conditional_breakpoints: bool,
        pub supports_evaluate_for_hovers: bool,
        pub exception_breakpoint_filters: Vec<ExceptionBreakpointsFilter>,
        pub supports_step_back: bool,
        pub supports_set_variable: bool,
        pub supports_restart_frame: bool,
        pub supports_goto_targets_request: bool,
        pub supports_step_in_targets_request: bool,
        pub supports_completions_request: bool,
        pub supports_modules_request: bool,
        pub supports_restart_request: bool,
        pub supports_exception_options: bool,
        pub supports_value_formatting_options: bool,
        pub supports_exception_info_request: bool,
        pub support_terminate_debuggee: bool,
        pub supports_delayed_stack_trace_loading: bool,
        pub supports_loaded_sources_request: bool,
        pub supports_log_points: bool,
        pub supports_terminate_threads_request: bool,
        pub supports_set_expression: bool,
        pub supports_terminate_request: bool,
        pub supports_data_breakpoints: bool,
        pub supports_read_memory_request: bool,
        pub supports_write_memory_request: bool,
        pub supports_disassemble_request: bool,
        pub supports_cancel_request: bool,
        pub supports_breakpoint_locations_request: bool,
        pub supports_clipboard_context: bool,
        pub supports_stepping_granularity: bool,
        pub supports_instruction_breakpoints: bool,
        pub supports_exception_filter_options: bool,
        pub supports_single_thread_execution_requests: bool,
        pub supports_data_breakpoint_bytes: bool,
    }
}

dap_struct! {
    pub struct ExceptionBreakpointsFilter {
        pub filter: String,
        pub label: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub description: Option<String>,
        pub default: bool,
        pub supports_condition: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub condition_description: Option<String>,
    }
}

dap_struct! {
    pub struct Source {
        #[serde(skip_serializing_if = "Option::is_none")]
        pub name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub path: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub source_reference: Option<i64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub presentation_hint: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub origin: Option<String>,
    }
}

dap_struct! {
    pub struct SourceBreakpoint {
        pub line: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub column: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub condition: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub hit_condition: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub log_message: Option<String>,
    }
}

dap_struct! {
    pub struct FunctionBreakpoint {
        pub name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub condition: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub hit_condition: Option<String>,
    }
}

dap_struct! {
    pub struct DataBreakpoint {
        pub data_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub access_type: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub condition: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub hit_condition: Option<String>,
    }
}

dap_struct! {
    /// Answer of `dataBreakpointInfo`: `data_id` is `None` when no watchpoint is possible.
    pub struct DataBreakpointInfo {
        pub data_id: Option<String>,
        pub description: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub access_types: Option<Vec<String>>,
        pub can_persist: bool,
    }
}

dap_struct! {
    pub struct ExceptionFilterOptions {
        pub filter_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub condition: Option<String>,
    }
}

dap_struct! {
    /// A breakpoint as the adapter reports it.
    pub struct Breakpoint {
        #[serde(skip_serializing_if = "Option::is_none")]
        pub id: Option<i64>,
        pub verified: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub message: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub source: Option<Source>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub line: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub instruction_reference: Option<String>,
    }
}

dap_struct! {
    pub struct Thread {
        pub id: i64,
        pub name: String,
    }
}

dap_struct! {
    pub struct StackFrame {
        pub id: i64,
        pub name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub source: Option<Source>,
        pub line: u32,
        pub column: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub instruction_pointer_reference: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub module_id: Option<Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub presentation_hint: Option<String>,
    }
}

dap_struct! {
    pub struct Scope {
        pub name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub presentation_hint: Option<String>,
        pub variables_reference: i64,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub named_variables: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub indexed_variables: Option<u32>,
        pub expensive: bool,
    }
}

impl Scope {
    /// Registers scopes are shown in the Registers view, not in the Variables tree.
    pub fn is_registers(&self) -> bool {
        self.presentation_hint.as_deref() == Some("registers")
            || self.name.to_ascii_lowercase().starts_with("register")
    }
}

dap_struct! {
    pub struct VariablePresentationHint {
        #[serde(skip_serializing_if = "Option::is_none")]
        pub kind: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub attributes: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub visibility: Option<String>,
    }
}

dap_struct! {
    pub struct Variable {
        pub name: String,
        pub value: String,
        #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
        pub r#type: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub presentation_hint: Option<VariablePresentationHint>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub evaluate_name: Option<String>,
        pub variables_reference: i64,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub named_variables: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub indexed_variables: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub memory_reference: Option<String>,
    }
}

dap_struct! {
    pub struct EvaluateResult {
        pub result: String,
        #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
        pub r#type: Option<String>,
        pub variables_reference: i64,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub named_variables: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub indexed_variables: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub memory_reference: Option<String>,
    }
}

dap_struct! {
    pub struct CompletionItem {
        pub label: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub text: Option<String>,
        #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
        pub r#type: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub detail: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub start: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub length: Option<u32>,
    }
}

dap_struct! {
    pub struct ReadMemoryResult {
        pub address: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub unreadable_bytes: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub data: Option<String>,
    }
}

dap_struct! {
    pub struct DisassembledInstruction {
        pub address: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub instruction_bytes: Option<String>,
        pub instruction: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub symbol: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub location: Option<Source>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub line: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub presentation_hint: Option<String>,
    }
}

dap_struct! {
    pub struct Module {
        /// Number or string.
        pub id: Value,
        pub name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub path: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub version: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub symbol_status: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub address_range: Option<String>,
    }
}

dap_struct! {
    pub struct GotoTarget {
        pub id: i64,
        pub label: String,
        pub line: u32,
    }
}

dap_struct! {
    pub struct StoppedEvent {
        pub reason: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub description: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub thread_id: Option<i64>,
        pub all_threads_stopped: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub hit_breakpoint_ids: Option<Vec<i64>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub text: Option<String>,
    }
}

dap_struct! {
    pub struct OutputEvent {
        #[serde(skip_serializing_if = "Option::is_none")]
        pub category: Option<String>,
        pub output: String,
    }
}

dap_struct! {
    pub struct ProcessEvent {
        pub name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub system_process_id: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub start_method: Option<String>,
    }
}

dap_struct! {
    /// Arguments of the `runInTerminal` reverse request.
    pub struct RunInTerminalArgs {
        #[serde(skip_serializing_if = "Option::is_none")]
        pub kind: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub title: Option<String>,
        pub cwd: String,
        pub args: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub env: Option<serde_json::Map<String, Value>>,
    }
}

dap_struct! {
    /// Arguments of the `startDebugging` reverse request (a child session).
    pub struct StartDebuggingArgs {
        pub configuration: Value,
        /// `launch` or `attach`.
        pub request: String,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn messages_use_the_wire_names() {
        let m: Message = serde_json::from_value(json!({
            "seq": 3, "type": "response", "request_seq": 1, "success": false,
            "command": "evaluate", "message": "x",
            "body": {"error": {"format": "cannot evaluate {expr}", "variables": {"expr": "y"}}}
        }))
        .unwrap();
        let Message::Response(r) = m else {
            panic!("not a response")
        };
        assert_eq!(r.error_text(), "cannot evaluate y");
        let req = serde_json::to_value(Message::Request(Request {
            seq: 1,
            command: "threads".into(),
            arguments: Value::Null,
        }))
        .unwrap();
        assert_eq!(
            req,
            json!({"type": "request", "seq": 1, "command": "threads"})
        );
    }

    #[test]
    fn capabilities_default_missing_fields() {
        let caps: Capabilities = serde_json::from_value(json!({
            "supportsReadMemoryRequest": true,
            "exceptionBreakpointFilters": [{"filter": "cpp_throw", "label": "C++: on throw"}],
            "somethingNew": 1
        }))
        .unwrap();
        assert!(caps.supports_read_memory_request && !caps.supports_disassemble_request);
        assert_eq!(caps.exception_breakpoint_filters[0].filter, "cpp_throw");
        let v: Variable =
            serde_json::from_value(json!({"name": "x", "value": "1", "type": "int"})).unwrap();
        assert_eq!(v.r#type.as_deref(), Some("int"));
        assert_eq!(v.variables_reference, 0);
    }
}
