//! A tiny, synchronous language server used by the integration tests.
//!
//! - publishes one warning per line containing `TODO` on open/change,
//! - answers hover/completion/definition/references/symbols/signature help/formatting
//!   with canned data,
//! - with the argument `secondary`, behaves like a linter attached next to a primary
//!   server: no hover, its own completion item and diagnostics source,
//! - answers `workspace/symbol` with `<query>Symbol` in every open document,
//! - the primary server also renames (with `prepareRename`), finds implementations,
//!   offers code actions (one resolved lazily, one running a command that sends
//!   `workspace/applyEdit`); the secondary one announces none of these, like free
//!   Intelephense,
//! - after `initialized` asks the client for `workspace/configuration` and creates a
//!   work-done progress, then echoes what it got back as `window/logMessage`.

use std::collections::HashMap;
use std::io::{self, BufRead, Write};

use serde_json::{json, Value};

fn read(stdin: &mut impl BufRead) -> Option<Value> {
    let mut len = None;
    loop {
        let mut line = String::new();
        if stdin.read_line(&mut line).ok()? == 0 {
            return None;
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some(v) = line.strip_prefix("Content-Length:") {
            len = v.trim().parse().ok();
        }
    }
    let mut buf = vec![0; len?];
    stdin.read_exact(&mut buf).ok()?;
    serde_json::from_slice(&buf).ok()
}

fn write(msg: Value) {
    let body = msg.to_string();
    let mut out = io::stdout().lock();
    write!(out, "Content-Length: {}\r\n\r\n{}", body.len(), body).unwrap();
    out.flush().unwrap();
}

fn notify(method: &str, params: Value) {
    write(json!({"jsonrpc": "2.0", "method": method, "params": params}));
}

fn range(line: u32, start: u32, end: u32) -> Value {
    json!({"start": {"line": line, "character": start}, "end": {"line": line, "character": end}})
}

fn publish(uri: &str, text: &str, secondary: bool) {
    let (marker, source) = if secondary {
        ("FIXME", "lint")
    } else {
        ("TODO", "fake")
    };
    let diagnostics: Vec<Value> = text
        .lines()
        .enumerate()
        .filter_map(|(i, l)| {
            l.find(marker).map(|c| {
                json!({"range": range(i as u32, c as u32, c as u32 + marker.len() as u32), "severity": 2,
                       "source": source, "message": "Unfinished work"})
            })
        })
        .collect();
    notify(
        "textDocument/publishDiagnostics",
        json!({"uri": uri, "diagnostics": diagnostics}),
    );
}

fn main() {
    let mut stdin = io::stdin().lock();
    let mut docs: HashMap<String, String> = HashMap::new();
    let secondary = std::env::args().nth(1).as_deref() == Some("secondary");
    while let Some(msg) = read(&mut stdin) {
        let method = msg["method"].as_str().unwrap_or_default().to_string();
        let id = msg.get("id").cloned();
        let params = &msg["params"];
        let uri = params["textDocument"]["uri"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let pos = &params["position"];

        // Responses to our own requests.
        if method.is_empty() {
            if id == Some(json!("cfg-1")) {
                notify(
                    "window/logMessage",
                    json!({"type": 3, "message": format!("config: {}", msg["result"])}),
                );
            }
            continue;
        }

        if method == "initialize" {
            notify(
                "window/logMessage",
                json!({"type": 3, "message": format!("processId: {}", params["processId"])}),
            );
        }
        let result = match method.as_str() {
            "initialize" => json!({
                "capabilities": {
                    "textDocumentSync": {"openClose": true, "change": 1, "save": {"includeText": true}},
                    "hoverProvider": true,
                    "completionProvider": {"resolveProvider": true, "triggerCharacters": [">", "$"]},
                    "definitionProvider": true,
                    "referencesProvider": true,
                    "documentSymbolProvider": true,
                    "signatureHelpProvider": {"triggerCharacters": ["(", ","]},
                    "documentFormattingProvider": !secondary,
                    "workspaceSymbolProvider": true,
                    "renameProvider": if secondary { Value::Null } else { json!({"prepareProvider": true}) },
                    "implementationProvider": !secondary,
                    "codeActionProvider": if secondary { Value::Null } else { json!({"resolveProvider": true}) },
                    "executeCommandProvider": {"commands": ["fake.apply"]}
                },
                "serverInfo": {"name": "fake", "version": "1"}
            }),
            "initialized" => {
                write(
                    json!({"jsonrpc": "2.0", "id": "cfg-1", "method": "workspace/configuration",
                             "params": {"items": [{"section": "fake.answer"}, {"section": "missing"}]}}),
                );
                write(
                    json!({"jsonrpc": "2.0", "id": 77, "method": "window/workDoneProgress/create",
                             "params": {"token": "idx"}}),
                );
                notify(
                    "$/progress",
                    json!({"token": "idx", "value": {"kind": "begin", "title": "Indexing", "percentage": 0}}),
                );
                notify(
                    "$/progress",
                    json!({"token": "idx", "value": {"kind": "end"}}),
                );
                continue;
            }
            "textDocument/didOpen" => {
                let text = params["textDocument"]["text"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                publish(&uri, &text, secondary);
                docs.insert(uri, text);
                continue;
            }
            "textDocument/didChange" => {
                let text = params["contentChanges"][0]["text"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                publish(&uri, &text, secondary);
                docs.insert(uri, text);
                continue;
            }
            "textDocument/didSave" => {
                notify(
                    "window/logMessage",
                    json!({"type": 4, "message": format!("saved with text: {}", params["text"].is_string())}),
                );
                continue;
            }
            "textDocument/didClose" => {
                docs.remove(&uri);
                continue;
            }
            "exit" => std::process::exit(0),
            "workspace/didChangeConfiguration" | "$/cancelRequest" => continue,
            "shutdown" => Value::Null,
            "textDocument/hover" if secondary => Value::Null,
            "textDocument/completion" if secondary => json!([{"label": "lint-ignore", "kind": 15}]),
            "workspace/symbol" => {
                let query = params["query"].as_str().unwrap_or_default();
                let mut uris: Vec<&String> = docs.keys().collect();
                uris.sort();
                json!(uris
                    .into_iter()
                    .map(|u| json!({"name": format!("{query}Symbol"), "kind": 5,
                                    "location": {"uri": u, "range": range(0, 0, 3)}}))
                    .collect::<Vec<_>>())
            }
            "textDocument/hover" => json!({
                "contents": {"kind": "markdown",
                             "value": format!("**hover** {}:{}", pos["line"], pos["character"])}
            }),
            "textDocument/completion" => json!({"isIncomplete": false, "items": [
                {"label": "hasMany", "kind": 2, "detail": "HasMany", "data": 1},
                {"label": "$orders", "kind": 10}
            ]}),
            "completionItem/resolve" => {
                let mut item = params.clone();
                item["documentation"] = json!({"kind": "markdown", "value": "Resolved docs"});
                item
            }
            "textDocument/definition" => json!([{"uri": uri, "range": range(0, 0, 5)}]),
            "textDocument/implementation" => json!([{"uri": uri, "range": range(3, 0, 4)}]),
            "textDocument/prepareRename" => {
                json!({"range": range(0, 6, 10), "placeholder": "User"})
            }
            "textDocument/rename" => json!({"documentChanges": [{
                "textDocument": {"uri": uri, "version": null},
                "edits": [{"range": range(0, 6, 10), "newText": params["newName"]}]
            }]}),
            "textDocument/codeAction" => {
                let n = params["context"]["diagnostics"]
                    .as_array()
                    .map_or(0, Vec::len);
                json!([
                    {"title": format!("Fix {n} problem(s)"), "kind": "quickfix", "data": {"uri": uri}},
                    {"title": "Apply via command", "command": "fake.apply", "arguments": [uri]}
                ])
            }
            "codeAction/resolve" => {
                let mut action = params.clone();
                let target = params["data"]["uri"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                action["edit"] = json!({"changes": {target: [{"range": range(0, 0, 0), "newText": "// fixed\n"}]}});
                action
            }
            "workspace/executeCommand" => {
                let target = params["arguments"][0]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                write(
                    json!({"jsonrpc": "2.0", "id": "apply-1", "method": "workspace/applyEdit",
                             "params": {"label": "fake", "edit": {"changes": {target: [
                                 {"range": range(0, 0, 0), "newText": "applied"}]}}}}),
                );
                Value::Null
            }
            "textDocument/references" => {
                json!([{"uri": uri, "range": range(1, 0, 2)}, {"uri": uri, "range": range(2, 0, 2)}])
            }
            "textDocument/documentSymbol" => json!([{
                "name": "User", "kind": 5, "range": range(0, 0, 10), "selectionRange": range(0, 6, 10),
                "children": [{"name": "orders", "kind": 6, "range": range(1, 0, 10), "selectionRange": range(1, 0, 6)}]
            }]),
            "textDocument/signatureHelp" => json!({
                "signatures": [{"label": "f(int $a, int $b)", "parameters": [{"label": [2, 8]}, {"label": [10, 16]}]}],
                "activeSignature": 0, "activeParameter": 1
            }),
            "textDocument/formatting" => {
                let text = docs.get(&uri).cloned().unwrap_or_default();
                let first = text.lines().next().unwrap_or_default();
                json!([{"range": range(0, 0, first.len() as u32), "newText": first.trim()}])
            }
            _ => {
                if let Some(id) = id {
                    write(json!({"jsonrpc": "2.0", "id": id,
                                 "error": {"code": -32601, "message": format!("unknown method {method}")}}));
                }
                continue;
            }
        };
        if let Some(id) = id {
            write(json!({"jsonrpc": "2.0", "id": id, "result": result}));
        }
    }
}
