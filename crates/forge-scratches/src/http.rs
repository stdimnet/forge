//! `.http` request files (PHPStorm's HTTP Client, lite): parsing, `{{variable}}`
//! substitution and the console text of a response.
//!
//! ```http
//! @host = localhost:8000
//!
//! ### List users
//! GET http://{{host}}/api/users?page=1
//! Accept: application/json
//!
//! ###
//! # @name createUser
//! POST http://{{host}}/api/users
//! Content-Type: application/json
//!
//! { "name": "Ada" }
//! ```
//!
//! Requests are separated by `###` (text after it names the request); `# @name x` also names
//! one. A request is a request line (`METHOD URL [HTTP/version]`, the method defaults to
//! GET), indented `?`/`&` lines continuing the query, headers, a blank line and the body.
//! `@name = value` lines define variables. Response handlers (`> {% … %}`, `> file.js`) and
//! response references (`<> file`) are skipped.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

const METHODS: &[&str] = &[
    "GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS", "TRACE", "CONNECT",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Header {
    pub name: String,
    pub value: String,
}

/// One request of an `.http` file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpRequest {
    /// Position in the file (0-based).
    pub index: usize,
    /// 0-based line of the request line (where the gutter ▶ goes).
    pub line: usize,
    /// 0-based last line of the request's block (body included).
    pub end_line: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub method: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default)]
    pub headers: Vec<Header>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}

impl HttpRequest {
    /// Name shown in the gutter tooltip and the Run tab: the request's name, else
    /// `METHOD url`.
    pub fn title(&self) -> String {
        self.name
            .clone()
            .unwrap_or_else(|| format!("{} {}", self.method, self.url))
    }

    /// Value of a header (case-insensitive).
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|h| h.name.eq_ignore_ascii_case(name))
            .map(|h| h.value.as_str())
    }
}

/// A parsed `.http` file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpFile {
    pub requests: Vec<HttpRequest>,
    /// In-file variables (`@host = localhost`).
    pub variables: BTreeMap<String, String>,
}

impl HttpFile {
    /// The request whose block contains `line` (0-based); before the first request, the
    /// first one.
    pub fn request_at(&self, line: usize) -> Option<&HttpRequest> {
        self.requests
            .iter()
            .find(|r| line <= r.end_line)
            .or(self.requests.last())
    }
}

fn is_comment(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with('#') || t.starts_with("//")
}

/// `# @name foo` / `// @name foo` → `foo`.
fn name_tag(line: &str) -> Option<String> {
    let t = line.trim_start();
    let rest = t
        .strip_prefix("//")
        .or_else(|| t.strip_prefix('#'))?
        .trim_start();
    let name = rest.strip_prefix("@name")?;
    let name = name.trim_start_matches(['=', ' ', '\t']).trim();
    (!name.is_empty()).then(|| name.to_string())
}

/// `@host = localhost` → `("host", "localhost")`.
fn variable(line: &str) -> Option<(String, String)> {
    let rest = line.trim().strip_prefix('@')?;
    let (name, value) = rest.split_once('=')?;
    let name = name.trim();
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.')
    {
        return None;
    }
    Some((name.to_string(), value.trim().to_string()))
}

/// Parse an `.http` file.
pub fn parse(text: &str) -> HttpFile {
    let lines: Vec<&str> = text.lines().collect();
    let mut file = HttpFile::default();
    // Blocks: (first line, end line exclusive, name from the `###` separator).
    let mut blocks: Vec<(usize, usize, Option<String>)> = Vec::new();
    let mut start = 0;
    let mut name: Option<String> = None;
    for (i, line) in lines.iter().enumerate() {
        if let Some(rest) = line.trim_start().strip_prefix("###") {
            blocks.push((start, i, name.take()));
            start = i + 1;
            let rest = rest.trim();
            name = (!rest.is_empty()).then(|| rest.to_string());
        }
    }
    blocks.push((start, lines.len(), name));

    for (from, to, separator_name) in blocks {
        if let Some(req) = parse_block(&lines, from, to, separator_name, &mut file.variables) {
            let mut req = req;
            req.index = file.requests.len();
            file.requests.push(req);
        }
    }
    file
}

fn parse_block(
    lines: &[&str],
    from: usize,
    to: usize,
    mut name: Option<String>,
    variables: &mut BTreeMap<String, String>,
) -> Option<HttpRequest> {
    let mut i = from;
    // Comments, variables and blank lines before the request line.
    while i < to {
        let line = lines[i];
        if line.trim().is_empty() {
            i += 1;
        } else if is_comment(line) {
            if let Some(n) = name_tag(line) {
                name = Some(n);
            }
            i += 1;
        } else if let Some((k, v)) = variable(line) {
            variables.insert(k, v);
            i += 1;
        } else {
            break;
        }
    }
    if i >= to {
        return None;
    }
    let line_no = i;
    let mut parts: Vec<&str> = lines[i].split_whitespace().collect();
    let method = if parts
        .first()
        .is_some_and(|m| METHODS.contains(&m.to_ascii_uppercase().as_str()))
    {
        parts.remove(0).to_ascii_uppercase()
    } else {
        "GET".to_string()
    };
    let version = parts
        .last()
        .filter(|v| v.starts_with("HTTP/") && parts.len() > 1)
        .map(|v| v.to_string());
    if version.is_some() {
        parts.pop();
    }
    let mut url = parts.join(" ");
    i += 1;
    // Indented query continuation lines.
    while i < to {
        let line = lines[i];
        let t = line.trim_start();
        if line.len() != t.len() && (t.starts_with('?') || t.starts_with('&')) {
            url.push_str(t.trim_end());
            i += 1;
        } else {
            break;
        }
    }
    let mut headers = Vec::new();
    while i < to && !lines[i].trim().is_empty() {
        let line = lines[i];
        i += 1;
        if is_comment(line) {
            continue;
        }
        if let Some((n, v)) = line.split_once(':') {
            headers.push(Header {
                name: n.trim().to_string(),
                value: v.trim().to_string(),
            });
        }
    }
    // Body: up to the end of the block, without response handlers and references.
    let mut body: Vec<&str> = Vec::new();
    let mut in_handler = false;
    while i < to {
        let line = lines[i];
        i += 1;
        let t = line.trim_start();
        if in_handler {
            in_handler = !t.contains("%}");
            continue;
        }
        if t.starts_with("> {%") {
            in_handler = !t.contains("%}");
            continue;
        }
        if t.starts_with("<> ")
            || t.starts_with(">> ")
            || t.starts_with(">>! ")
            || t.starts_with("> ")
        {
            continue;
        }
        body.push(line);
    }
    while body.first().is_some_and(|l| l.trim().is_empty()) {
        body.remove(0);
    }
    while body.last().is_some_and(|l| l.trim().is_empty()) {
        body.pop();
    }
    Some(HttpRequest {
        index: 0,
        line: line_no,
        end_line: to.saturating_sub(1).max(line_no),
        name,
        method,
        url,
        version,
        headers,
        body: (!body.is_empty()).then(|| body.join("\n")),
    })
}

/// Replace `{{name}}` with variables (values may use other variables). Unknown variables
/// stay as they are.
pub fn substitute(text: &str, vars: &BTreeMap<String, String>) -> String {
    let mut out = text.to_string();
    for _ in 0..5 {
        let mut changed = false;
        let mut result = String::with_capacity(out.len());
        let mut rest = out.as_str();
        while let Some(start) = rest.find("{{") {
            result.push_str(&rest[..start]);
            let after = &rest[start + 2..];
            match after.find("}}") {
                Some(end) => {
                    let key = after[..end].trim();
                    match vars.get(key) {
                        Some(v) => {
                            result.push_str(v);
                            changed = true;
                        }
                        None => result.push_str(&rest[start..start + 2 + end + 2]),
                    }
                    rest = &after[end + 2..];
                }
                None => {
                    result.push_str(&rest[start..]);
                    rest = "";
                }
            }
        }
        result.push_str(rest);
        out = result;
        if !changed {
            break;
        }
    }
    out
}

/// The request with variables substituted in its URL, headers and body. `env` (e.g. from
/// an environment file) is overridden by the file's own variables.
pub fn resolve(
    req: &HttpRequest,
    file_vars: &BTreeMap<String, String>,
    env: &BTreeMap<String, String>,
) -> HttpRequest {
    let mut vars = env.clone();
    vars.extend(file_vars.iter().map(|(k, v)| (k.clone(), v.clone())));
    let mut out = req.clone();
    out.url = substitute(&req.url, &vars);
    if !out.url.contains("://") {
        out.url = format!("http://{}", out.url);
    }
    for h in &mut out.headers {
        h.value = substitute(&h.value, &vars);
    }
    out.body = req.body.as_ref().map(|b| substitute(b, &vars));
    out
}

/// A received response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpResponse {
    /// `HTTP/1.1`, `HTTP/2`.
    pub version: String,
    pub status: u16,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub headers: Vec<Header>,
    /// Body text; binary bodies are summarised.
    #[serde(default)]
    pub body: String,
    /// Body size in bytes.
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub elapsed_ms: u64,
}

const RESET: &str = "\x1b[0m";

/// Console text (with ANSI styles) shown before a request is sent.
pub fn render_request(req: &HttpRequest) -> String {
    format!("\x1b[1m{} {}{RESET}\n\n", req.method, req.url)
}

/// Console text of a response, laid out like PHPStorm's HTTP Client: status line, headers,
/// the (pretty-printed) body and the summary line.
pub fn render_response(resp: &HttpResponse) -> String {
    let color = match resp.status {
        200..=299 => "\x1b[32m",
        300..=399 => "\x1b[36m",
        400..=499 => "\x1b[33m",
        _ => "\x1b[31m",
    };
    let mut out = format!(
        "{color}{} {} {}{RESET}\n",
        resp.version, resp.status, resp.reason
    );
    for h in &resp.headers {
        out.push_str(&format!("\x1b[36m{}{RESET}: {}\n", h.name, h.value));
    }
    out.push('\n');
    let content_type = resp
        .headers
        .iter()
        .find(|h| h.name.eq_ignore_ascii_case("content-type"))
        .map(|h| h.value.to_ascii_lowercase())
        .unwrap_or_default();
    let body = if content_type.contains("json") || looks_like_json(&resp.body) {
        pretty_json(&resp.body).unwrap_or_else(|| resp.body.clone())
    } else {
        resp.body.clone()
    };
    if !body.is_empty() {
        out.push_str(&body);
        if !body.ends_with('\n') {
            out.push('\n');
        }
        out.push('\n');
    }
    out.push_str(&format!(
        "\x1b[90mResponse code: {} ({}); Time: {}ms ({} ms); Content length: {} bytes ({}){RESET}\n",
        resp.status,
        if resp.reason.is_empty() { "-" } else { &resp.reason },
        resp.elapsed_ms,
        resp.elapsed_ms,
        resp.size,
        human_size(resp.size)
    ));
    out
}

/// Console text of a request that failed (connection refused, timeout, bad URL).
pub fn render_error(message: &str) -> String {
    format!("\x1b[31m{message}{RESET}\n")
}

/// `254 B`, `1.2 KB`, `3.4 MB`.
pub fn human_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

fn looks_like_json(text: &str) -> bool {
    let t = text.trim_start();
    (t.starts_with('{') || t.starts_with('['))
        && serde_json::from_str::<serde::de::IgnoredAny>(text).is_ok()
}

/// Indent JSON by two spaces, keeping the key order; `None` when it is not valid JSON.
pub fn pretty_json(text: &str) -> Option<String> {
    serde_json::from_str::<serde::de::IgnoredAny>(text).ok()?;
    let mut out = String::with_capacity(text.len() * 2);
    let mut indent = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    let chars: Vec<char> = text.trim().chars().collect();
    let newline = |out: &mut String, indent: usize| {
        out.push('\n');
        out.push_str(&"  ".repeat(indent));
    };
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        i += 1;
        if in_string {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => {
                in_string = true;
                out.push(c);
            }
            '{' | '[' => {
                let close = if c == '{' { '}' } else { ']' };
                let next = chars[i..].iter().position(|c| !c.is_whitespace());
                if next.is_some_and(|n| chars[i + n] == close) {
                    out.push(c);
                    out.push(close);
                    i += next.unwrap() + 1;
                } else {
                    out.push(c);
                    indent += 1;
                    newline(&mut out, indent);
                }
            }
            '}' | ']' => {
                indent = indent.saturating_sub(1);
                newline(&mut out, indent);
                out.push(c);
            }
            ',' => {
                out.push(c);
                newline(&mut out, indent);
            }
            ':' => out.push_str(": "),
            c if c.is_whitespace() => {}
            c => out.push(c),
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "\
@host = localhost:8000
@api = http://{{host}}/api

### List users
GET {{api}}/users
    ?page=1
    &sort=name
Accept: application/json
# a comment between headers

###
# @name createUser
POST {{api}}/users HTTP/1.1
Content-Type: application/json
Authorization: Bearer {{token}}

{
  \"name\": \"Ada\"
}

> {%
  client.global.set(\"id\", response.body.id);
%}
<> 2026-10-20T091500.200.json

###
https://example.com/health

### Not a request, just notes
# nothing here
";

    #[test]
    fn parses_requests() {
        let f = parse(FILE);
        assert_eq!(f.variables["host"], "localhost:8000");
        assert_eq!(f.requests.len(), 3);

        let list = &f.requests[0];
        assert_eq!(list.name.as_deref(), Some("List users"));
        assert_eq!((list.method.as_str(), list.line), ("GET", 4));
        assert_eq!(list.url, "{{api}}/users?page=1&sort=name");
        assert_eq!(list.header("accept"), Some("application/json"));
        assert_eq!(list.headers.len(), 1);
        assert_eq!(list.body, None);

        let create = &f.requests[1];
        assert_eq!(create.name.as_deref(), Some("createUser"));
        assert_eq!((create.method.as_str(), create.line), ("POST", 12));
        assert_eq!(create.version.as_deref(), Some("HTTP/1.1"));
        assert_eq!(create.url, "{{api}}/users");
        assert_eq!(create.body.as_deref(), Some("{\n  \"name\": \"Ada\"\n}"));
        assert_eq!(create.index, 1);

        let health = &f.requests[2];
        assert_eq!(
            (health.method.as_str(), health.url.as_str()),
            ("GET", "https://example.com/health")
        );
        assert_eq!(health.title(), "GET https://example.com/health");
    }

    #[test]
    fn finds_the_request_at_a_line() {
        let f = parse(FILE);
        assert_eq!(f.request_at(0).unwrap().index, 0);
        assert_eq!(f.request_at(5).unwrap().index, 0);
        assert_eq!(f.request_at(17).unwrap().index, 1);
        assert_eq!(f.request_at(26).unwrap().index, 2);
        assert_eq!(f.request_at(999).unwrap().index, 2);
        assert!(parse("# only comments\n").request_at(0).is_none());
    }

    #[test]
    fn substitutes_variables() {
        let f = parse(FILE);
        let env = BTreeMap::from([
            ("token".to_string(), "s3cret".to_string()),
            ("host".to_string(), "ignored".to_string()),
        ]);
        let r = resolve(&f.requests[1], &f.variables, &env);
        assert_eq!(r.url, "http://localhost:8000/api/users");
        assert_eq!(r.header("Authorization"), Some("Bearer s3cret"));
        // Unknown variables stay; URLs without a scheme get http://.
        let r = resolve(
            &parse("GET {{nope}}/x").requests[0],
            &BTreeMap::new(),
            &BTreeMap::new(),
        );
        assert_eq!(r.url, "http://{{nope}}/x");
        assert_eq!(
            substitute("{{a}}{{b", &BTreeMap::from([("a".into(), "1".into())])),
            "1{{b"
        );
    }

    #[test]
    fn parses_edge_cases() {
        let f = parse("get localhost\r\nX-A: 1\r\n\r\nbody\r\n");
        assert_eq!(f.requests[0].method, "GET");
        assert_eq!(f.requests[0].url, "localhost");
        assert_eq!(f.requests[0].body.as_deref(), Some("body"));
        assert!(parse("").requests.is_empty());
        assert!(parse("###\n###\n").requests.is_empty());
        let f = parse("// @name ping\nHEAD http://h/ping\n");
        assert_eq!(f.requests[0].title(), "ping");
    }

    #[test]
    fn renders_responses_like_phpstorm() {
        let resp = HttpResponse {
            version: "HTTP/1.1".into(),
            status: 201,
            reason: "Created".into(),
            headers: vec![Header {
                name: "Content-Type".into(),
                value: "application/json".into(),
            }],
            body: r#"{"id":7,"tags":[],"name":"Ada \"A\", L","nested":{"ok":true}}"#.into(),
            size: 59,
            elapsed_ms: 42,
        };
        let text = render_response(&resp);
        let plain: String = strip_ansi(&text);
        assert_eq!(
            plain,
            "HTTP/1.1 201 Created\nContent-Type: application/json\n\n{\n  \"id\": 7,\n  \"tags\": [],\n  \"name\": \"Ada \\\"A\\\", L\",\n  \"nested\": {\n    \"ok\": true\n  }\n}\n\nResponse code: 201 (Created); Time: 42ms (42 ms); Content length: 59 bytes (59 B)\n"
        );
        assert!(text.starts_with("\x1b[32m"));
        assert_eq!(pretty_json("not json"), None);
        assert_eq!(human_size(2048), "2.0 KB");
        assert!(render_error("Connection refused").contains("Connection refused"));
    }

    fn strip_ansi(s: &str) -> String {
        let mut out = String::new();
        let mut chars = s.chars();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                for c in chars.by_ref() {
                    if c == 'm' {
                        break;
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
    }
}
