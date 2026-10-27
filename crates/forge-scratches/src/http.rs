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
//! `@name = value` lines define variables. Body lines `< ./file.json` insert a file (`<@`
//! substitutes variables in it). `{{$uuid}}`, `{{$random.uuid}}`, `{{$timestamp}}`,
//! `{{$isoTimestamp}}`, `{{$randomInt}}` and `{{$random.integer(from, to)}}` are dynamic
//! variables. Response handlers (`> {% … %}` or `> ./handler.js`) are kept for
//! [`crate::handler`]; response references (`<> file`) are skipped.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

const METHODS: &[&str] = &[
    "GET",
    "POST",
    "PUT",
    "PATCH",
    "DELETE",
    "HEAD",
    "OPTIONS",
    "TRACE",
    "CONNECT",
    // PHPStorm's request kinds beyond HTTP: a GraphQL query sent as a POST, a WebSocket
    // connection with messages, a unary gRPC call.
    "GRAPHQL",
    "WEBSOCKET",
    "GRPC",
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
    /// Response handler script (`> {% … %}`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handler: Option<String>,
    /// Response handler file (`> ./check.js`), relative to the `.http` file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handler_file: Option<String>,
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

    /// Add the Xdebug session cookie (`XDEBUG_SESSION=<ide key>`), so the request is
    /// debugged ("Debug" in `.http` files, PHP HTTP Request debug runs). An existing
    /// `XDEBUG_SESSION` cookie is kept.
    pub fn add_debug_cookie(&mut self, ide_key: &str) {
        let cookie = format!("XDEBUG_SESSION={ide_key}");
        match self
            .headers
            .iter_mut()
            .find(|h| h.name.eq_ignore_ascii_case("Cookie"))
        {
            Some(h) if h.value.contains("XDEBUG_SESSION=") => {}
            Some(h) if h.value.trim().is_empty() => h.value = cookie,
            Some(h) => {
                let v = h.value.trim_end().trim_end_matches(';').to_string();
                h.value = format!("{v}; {cookie}");
            }
            None => self.headers.push(Header {
                name: "Cookie".into(),
                value: cookie,
            }),
        }
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
    let mut handler: Option<String> = None;
    let mut handler_file: Option<String> = None;
    let mut script: Option<Vec<&str>> = None;
    while i < to {
        let line = lines[i];
        i += 1;
        let t = line.trim_start();
        if let Some(lines) = script.as_mut() {
            match line.find("%}") {
                Some(end) => {
                    lines.push(&line[..end]);
                    handler = Some(lines.join("\n").trim().to_string());
                    script = None;
                }
                None => lines.push(line),
            }
            continue;
        }
        if let Some(rest) = t.strip_prefix("> {%") {
            match rest.find("%}") {
                Some(end) => handler = Some(rest[..end].trim().to_string()),
                None => script = Some(vec![rest]),
            }
            continue;
        }
        if let Some(file) = t.strip_prefix("> ") {
            handler_file = Some(file.trim().to_string());
            continue;
        }
        if t.starts_with("<> ") || t.starts_with(">> ") || t.starts_with(">>! ") {
            continue;
        }
        body.push(line);
    }
    if let Some(lines) = script {
        // Unterminated `{%`: take the rest of the block.
        handler = Some(lines.join("\n").trim().to_string());
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
        handler,
        handler_file,
    })
}

/// Replace `{{name}}` with variables (values may use other variables). Unknown variables
/// stay as they are.
pub fn substitute(text: &str, vars: &BTreeMap<String, String>) -> String {
    substitute_with(text, &mut |key| vars.get(key).cloned())
}

/// [`substitute`] with a lookup function (variables plus dynamic `$` variables).
pub fn substitute_with(text: &str, lookup: &mut dyn FnMut(&str) -> Option<String>) -> String {
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
                    match lookup(key) {
                        Some(v) => {
                            result.push_str(&v);
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
/// an environment file) is overridden by the file's own variables. Body files are not read
/// and dynamic variables come from a fixed seed; runs use a [`Resolver`].
pub fn resolve(
    req: &HttpRequest,
    file_vars: &BTreeMap<String, String>,
    env: &BTreeMap<String, String>,
) -> HttpRequest {
    Resolver::new(env, &BTreeMap::new(), file_vars, Dynamic::new(0, 0))
        .resolve(req)
        .request
}

/// Generates the values of dynamic variables (`{{$uuid}}`, …). Pure and seeded, so the
/// WebAssembly build and tests get reproducible values; [`Dynamic::system`] seeds from the
/// clock.
#[derive(Debug, Clone)]
pub struct Dynamic {
    timestamp: u64,
    state: u64,
}

impl Dynamic {
    /// `timestamp`: Unix seconds for `$timestamp` / `$isoTimestamp`.
    pub fn new(timestamp: u64, seed: u64) -> Self {
        Dynamic {
            timestamp,
            state: seed,
        }
    }

    /// Seeded from the system clock and the process's hash keys.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn system() -> Self {
        use std::hash::{BuildHasher, Hasher};
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u128(now.as_nanos());
        Dynamic::new(now.as_secs(), h.finish())
    }

    /// splitmix64.
    fn next(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn uuid(&mut self) -> String {
        let (a, b) = (self.next(), self.next());
        let mut bytes = [0u8; 16];
        bytes[..8].copy_from_slice(&a.to_be_bytes());
        bytes[8..].copy_from_slice(&b.to_be_bytes());
        bytes[6] = (bytes[6] & 0x0f) | 0x40;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        format!(
            "{}-{}-{}-{}-{}",
            &hex[..8],
            &hex[8..12],
            &hex[12..16],
            &hex[16..20],
            &hex[20..]
        )
    }

    /// The value of a dynamic variable (`$uuid`, `$random.integer(1, 6)`), `None` for
    /// anything else.
    pub fn value(&mut self, name: &str) -> Option<String> {
        let name = name.trim();
        match name {
            "$uuid" | "$random.uuid" => Some(self.uuid()),
            "$timestamp" => Some(self.timestamp.to_string()),
            "$isoTimestamp" => Some(iso_timestamp(self.timestamp)),
            "$randomInt" => Some((self.next() % 1001).to_string()),
            _ => {
                let args = name.strip_prefix("$random.integer(")?.strip_suffix(')')?;
                let (from, to) = args.split_once(',').unwrap_or(("0", "1000"));
                let from: i64 = from.trim().parse().ok()?;
                let to: i64 = to.trim().parse().ok()?;
                if to < from {
                    return None;
                }
                let span = (to - from) as u64 + 1;
                Some((from + (self.next() % span) as i64).to_string())
            }
        }
    }
}

/// `2026-10-21T09:15:02Z` from Unix seconds.
pub fn iso_timestamp(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// Reads a file referenced by a request (`< ./body.json`, `> ./handler.js`), relative to
/// the `.http` file. Errors are worded for the console.
pub type FileLoader<'a> = &'a dyn Fn(&str) -> Result<String, String>;

/// A request ready to send, or why it cannot be sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub request: HttpRequest,
    /// Body files that could not be read; the request is not sent then.
    pub problems: Vec<String>,
}

/// Resolves requests of one file: variables (environment < `client.global` < in-file),
/// dynamic variables and body files.
pub struct Resolver<'a> {
    vars: BTreeMap<String, String>,
    dynamic: Dynamic,
    load: Option<FileLoader<'a>>,
}

impl<'a> Resolver<'a> {
    pub fn new(
        env: &BTreeMap<String, String>,
        globals: &BTreeMap<String, String>,
        file_vars: &BTreeMap<String, String>,
        dynamic: Dynamic,
    ) -> Self {
        let mut vars = env.clone();
        vars.extend(globals.iter().map(|(k, v)| (k.clone(), v.clone())));
        vars.extend(file_vars.iter().map(|(k, v)| (k.clone(), v.clone())));
        Resolver {
            vars,
            dynamic,
            load: None,
        }
    }

    /// Read `< file` bodies and `> file` handlers with `load`.
    pub fn with_loader(mut self, load: FileLoader<'a>) -> Self {
        self.load = Some(load);
        self
    }

    fn subst(&mut self, text: &str) -> String {
        let (vars, dynamic) = (&self.vars, &mut self.dynamic);
        substitute_with(text, &mut |key| {
            if key.starts_with('$') {
                dynamic.value(key)
            } else {
                vars.get(key).cloned()
            }
        })
    }

    fn load(&self, path: &str) -> Result<String, String> {
        match self.load {
            Some(load) => load(path),
            None => Err(format!("Cannot read {path}: files are not available here")),
        }
    }

    pub fn resolve(&mut self, req: &HttpRequest) -> Resolved {
        let mut out = req.clone();
        let mut problems = Vec::new();
        out.url = self.subst(&req.url);
        if !out.url.contains("://") {
            let scheme = if req.method == "WEBSOCKET" { "ws" } else { "http" };
            out.url = format!("{scheme}://{}", out.url);
        }
        for h in &mut out.headers {
            h.value = self.subst(&h.value);
        }
        out.body = req.body.as_ref().map(|body| {
            let mut lines = Vec::new();
            for line in body.split('\n') {
                let t = line.trim_start();
                let (raw, path) = if let Some(p) = t.strip_prefix("<@") {
                    (false, p)
                } else if let Some(p) = t.strip_prefix("< ") {
                    (true, p)
                } else {
                    lines.push(self.subst(line));
                    continue;
                };
                let path = self.subst(path.trim());
                match self.load(&path) {
                    Ok(text) if raw => lines.push(text.trim_end_matches('\n').to_string()),
                    Ok(text) => lines.push(self.subst(text.trim_end_matches('\n'))),
                    Err(e) => problems.push(e),
                }
            }
            lines.join("\n")
        });
        Resolved {
            request: out,
            problems,
        }
    }

    /// The response handler script of a request: inline, or read from its file.
    pub fn handler(&self, req: &HttpRequest) -> Result<Option<String>, String> {
        if let Some(script) = &req.handler {
            return Ok(Some(script.clone()));
        }
        match &req.handler_file {
            Some(path) => self.load(path).map(Some),
            None => Ok(None),
        }
    }
}

/// A `GRAPHQL` request's body: the operation, then optionally a JSON object with its
/// variables (`(query, variables)`).
pub fn graphql_parts(body: &str) -> (String, Option<String>) {
    let lines: Vec<&str> = body.lines().collect();
    for i in (1..lines.len()).rev() {
        if !lines[i].trim_start().starts_with('{') {
            continue;
        }
        let candidate = lines[i..].join("\n");
        let is_object = serde_json::from_str::<serde_json::Value>(&candidate)
            .is_ok_and(|v| v.is_object());
        let query = lines[..i].join("\n");
        if is_object && !query.trim().is_empty() {
            return (query.trim().to_string(), Some(candidate.trim().to_string()));
        }
    }
    (body.trim().to_string(), None)
}

/// A resolved `GRAPHQL` request as the HTTP request that carries it: a `POST` with the
/// JSON body `{ "query", "variables" }` (GraphQL over HTTP); a `ws://` / `wss://` URL stays
/// for a subscription.
pub fn graphql_as_http(req: &HttpRequest) -> Result<HttpRequest, String> {
    let (query, variables) = graphql_parts(req.body.as_deref().unwrap_or(""));
    if query.is_empty() {
        return Err("The GraphQL request has no query".into());
    }
    let mut body = serde_json::Map::new();
    body.insert("query".into(), serde_json::Value::String(query));
    if let Some(v) = variables {
        let parsed: serde_json::Value =
            serde_json::from_str(&v).map_err(|e| format!("Variables are not valid JSON: {e}"))?;
        body.insert("variables".into(), parsed);
    }
    let mut out = req.clone();
    out.method = "POST".into();
    out.body = Some(serde_json::Value::Object(body).to_string());
    if out.header("Content-Type").is_none() {
        out.headers.push(Header {
            name: "Content-Type".into(),
            value: "application/json".into(),
        });
    }
    if out.header("Accept").is_none() {
        out.headers.push(Header {
            name: "Accept".into(),
            value: "application/graphql-response+json, application/json".into(),
        });
    }
    Ok(out)
}

/// A `WEBSOCKET` request's messages: the body split at `===` lines; a
/// `=== wait-for-server` line makes the next message wait for a server message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WsMessage {
    pub text: String,
    /// Wait for one message from the server before sending this one.
    pub wait_for_server: bool,
}

pub fn websocket_messages(body: Option<&str>) -> Vec<WsMessage> {
    let Some(body) = body else { return Vec::new() };
    let mut out = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    let mut wait = false;
    let flush = |current: &mut Vec<&str>, wait: bool, out: &mut Vec<WsMessage>| {
        let text = current.join("\n").trim().to_string();
        if !text.is_empty() {
            out.push(WsMessage {
                text,
                wait_for_server: wait,
            });
        }
        current.clear();
    };
    for line in body.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("===") {
            let had = !current.join("").trim().is_empty();
            flush(&mut current, wait, &mut out);
            if had {
                wait = false;
            }
            if rest.trim() == "wait-for-server" {
                wait = true;
            }
            continue;
        }
        current.push(line);
    }
    flush(&mut current, wait, &mut out);
    out
}

/// The relative path of a referenced file joined to the `.http` file's directory, without
/// `.` and `..` segments; `None` when it leaves the base (the project root).
pub fn join_relative(dir: &str, path: &str) -> Option<String> {
    let mut parts: Vec<&str> = dir.split('/').filter(|p| !p.is_empty()).collect();
    for seg in path.split(['/', '\\']) {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            s => parts.push(s),
        }
    }
    Some(parts.join("/"))
}

/// Variables of environment `name` from `http-client.env.json` and
/// `http-client.private.env.json` (private values win), as PHPStorm reads them. Without a
/// name, `dev` or else the first environment of the public file is used. Non-string values
/// are taken as their JSON text.
pub fn environment(
    public: Option<&str>,
    private: Option<&str>,
    name: Option<&str>,
) -> BTreeMap<String, String> {
    let parse = |text: Option<&str>| {
        text.and_then(|t| {
            serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(t).ok()
        })
        .unwrap_or_default()
    };
    let (public, private) = (parse(public), parse(private));
    let name = name
        .map(str::to_string)
        .or_else(|| default_name(&public, &private));
    let mut vars = BTreeMap::new();
    let Some(name) = name else { return vars };
    for file in [&public, &private] {
        if let Some(serde_json::Value::Object(env)) = file.get(&name) {
            for (k, v) in env {
                let value = match v {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                vars.insert(k.clone(), value);
            }
        }
    }
    vars
}

type EnvFile = serde_json::Map<String, serde_json::Value>;

/// The environment used when none is selected: `dev`, else the first one.
fn default_name(public: &EnvFile, private: &EnvFile) -> Option<String> {
    public
        .contains_key("dev")
        .then(|| "dev".to_string())
        .or_else(|| public.keys().next().cloned())
        .or_else(|| private.keys().next().cloned())
}

/// The environments a `.http` editor offers.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Environments {
    pub names: Vec<String>,
    /// The one requests use while none is selected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
}

/// Environments of `http-client.env.json` and `http-client.private.env.json`.
pub fn environments(public: Option<&str>, private: Option<&str>) -> Environments {
    let parse = |text: Option<&str>| {
        text.and_then(|t| serde_json::from_str::<EnvFile>(t).ok())
            .unwrap_or_default()
    };
    Environments {
        names: environment_names(public, private),
        default: default_name(&parse(public), &parse(private)),
    }
}

/// Names of the environments in `http-client.env.json` and `http-client.private.env.json`
/// (the selector of `.http` editors), sorted, without duplicates.
pub fn environment_names(public: Option<&str>, private: Option<&str>) -> Vec<String> {
    let mut names: Vec<String> = [public, private]
        .into_iter()
        .flatten()
        .filter_map(|t| serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(t).ok())
        .flat_map(|m| {
            m.into_iter()
                .filter(|(k, v)| v.is_object() && !k.starts_with('$'))
                .map(|(k, _)| k)
                .collect::<Vec<_>>()
        })
        .collect();
    names.sort();
    names.dedup();
    names
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
    let mut out = format!("\x1b[1m{} {}{RESET}\n", req.method, req.url);
    // A debugged request says so: the Xdebug cookie starts the debug session.
    if let Some(cookie) = req
        .header("Cookie")
        .filter(|c| c.contains("XDEBUG_SESSION="))
    {
        out.push_str(&format!(
            "\x1b[90mCookie: {cookie} (debugging with Xdebug){RESET}\n"
        ));
    }
    out.push('\n');
    out
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
mod debug_cookie_tests {
    use super::*;

    #[test]
    fn debug_cookie_is_added_or_merged() {
        let file =
            parse("GET http://shop.test/\n\n###\nPOST http://shop.test/a\nCookie: lang=de;\n");
        let mut a = file.requests[0].clone();
        a.add_debug_cookie("FORGE");
        assert_eq!(a.header("cookie"), Some("XDEBUG_SESSION=FORGE"));
        let mut b = file.requests[1].clone();
        b.add_debug_cookie("FORGE");
        assert_eq!(b.header("Cookie"), Some("lang=de; XDEBUG_SESSION=FORGE"));
        b.add_debug_cookie("OTHER");
        assert_eq!(b.header("Cookie"), Some("lang=de; XDEBUG_SESSION=FORGE"));
        assert!(render_request(&b).contains("XDEBUG_SESSION=FORGE (debugging with Xdebug)"));
        assert!(!render_request(&file.requests[0]).contains("Cookie"));
    }
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

    #[test]
    fn reads_environment_files() {
        let public =
            r#"{"prod":{"host":"shop.example"},"dev":{"host":"localhost:8000","port":8000}}"#;
        let private = r#"{"dev":{"token":"s3cret"}}"#;
        let dev = environment(Some(public), Some(private), None);
        assert_eq!(dev["host"], "localhost:8000");
        assert_eq!(dev["port"], "8000");
        assert_eq!(dev["token"], "s3cret");
        assert_eq!(
            environment(Some(public), None, Some("prod"))["host"],
            "shop.example"
        );
        assert!(environment(None, None, None).is_empty());
        assert!(environment(Some("not json"), None, None).is_empty());
    }

    #[test]
    fn parses_response_handlers() {
        let f = parse(FILE);
        assert_eq!(
            f.requests[1].handler.as_deref(),
            Some("client.global.set(\"id\", response.body.id);")
        );
        assert_eq!(f.requests[0].handler, None);
        let f = parse(
            "GET http://h/a\n\n> {% client.log(1) %}\n\n###\nGET http://h/b\n\n> ./check.js\n",
        );
        assert_eq!(f.requests[0].handler.as_deref(), Some("client.log(1)"));
        assert_eq!(f.requests[0].body, None);
        assert_eq!(f.requests[1].handler_file.as_deref(), Some("./check.js"));
    }

    #[test]
    fn dynamic_variables() {
        let mut d = Dynamic::new(1_792_574_102, 7);
        let uuid = d.value("$uuid").unwrap();
        assert_eq!(uuid.len(), 36);
        assert_eq!(&uuid[14..15], "4");
        assert!(matches!(&uuid[19..20], "8" | "9" | "a" | "b"));
        assert_ne!(uuid, d.value("$random.uuid").unwrap());
        assert_eq!(d.value("$timestamp").unwrap(), "1792574102");
        assert_eq!(d.value("$isoTimestamp").unwrap(), "2026-10-21T09:15:02Z");
        assert_eq!(iso_timestamp(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso_timestamp(951_782_400), "2000-02-29T00:00:00Z");
        let n: u32 = d.value("$randomInt").unwrap().parse().unwrap();
        assert!(n <= 1000);
        let dice: i64 = d.value("$random.integer(1, 6)").unwrap().parse().unwrap();
        assert!((1..=6).contains(&dice));
        assert_eq!(d.value("$nope"), None);
        assert_eq!(d.value("$random.integer(5, 1)"), None);
        // Each occurrence gets its own value; unknown `$` variables stay.
        let req = &parse("POST http://h/{{$uuid}}\nX-Id: {{$uuid}}\n\n{{$unknown}}").requests[0];
        let r = Resolver::new(&BTreeMap::new(), &BTreeMap::new(), &BTreeMap::new(), d)
            .resolve(req)
            .request;
        assert_ne!(&r.url["http://h/".len()..], r.header("X-Id").unwrap());
        assert_eq!(r.body.as_deref(), Some("{{$unknown}}"));
    }

    #[test]
    fn body_files_globals_and_precedence() {
        let file = parse(
            "@who = file\nPOST http://h/{{who}}/{{token}}\n\n< ./body.json\n<@ ./tpl.json\n< ./missing.json\n",
        );
        let load = |p: &str| match p {
            "./body.json" => Ok("{\"raw\": \"{{who}}\"}\n".to_string()),
            "./tpl.json" => Ok("{\"who\": \"{{who}}\"}".to_string()),
            other => Err(format!("Cannot read {other}")),
        };
        let env = BTreeMap::from([("who".into(), "env".into()), ("token".into(), "e".into())]);
        let globals = BTreeMap::from([("token".into(), "g".into())]);
        let r = Resolver::new(&env, &globals, &file.variables, Dynamic::new(0, 1))
            .with_loader(&load)
            .resolve(&file.requests[0]);
        assert_eq!(r.request.url, "http://h/file/g");
        assert_eq!(
            r.request.body.as_deref(),
            Some("{\"raw\": \"{{who}}\"}\n{\"who\": \"file\"}")
        );
        assert_eq!(r.problems, vec!["Cannot read ./missing.json"]);
        // Without a loader, body files are a problem.
        let r = Resolver::new(&env, &globals, &file.variables, Dynamic::new(0, 1))
            .resolve(&file.requests[0]);
        assert_eq!(r.problems.len(), 3);
        assert_eq!(
            join_relative("api", "./data/../body.json").as_deref(),
            Some("api/body.json")
        );
        assert_eq!(join_relative("", "../x"), None);
    }

    #[test]
    fn lists_environment_names() {
        let public = r#"{"prod":{"host":"a"},"dev":{"host":"b"},"$shared":{"x":1}}"#;
        let private = r#"{"dev":{"token":"t"},"staging":{}}"#;
        assert_eq!(
            environment_names(Some(public), Some(private)),
            vec!["dev", "prod", "staging"]
        );
        assert!(environment_names(None, Some("broken")).is_empty());
        let envs = environments(Some(public), Some(private));
        assert_eq!(envs.default.as_deref(), Some("dev"));
        assert_eq!(
            environments(Some(r#"{"prod":{}}"#), None)
                .default
                .as_deref(),
            Some("prod")
        );
        assert_eq!(environments(None, None), Environments::default());
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

#[cfg(test)]
mod extras_tests {
    use super::*;

    #[test]
    fn parses_graphql_websocket_and_grpc_requests() {
        let file = parse(
            "### Query\nGRAPHQL http://localhost:4000/graphql\nAuthorization: Bearer x\n\nquery User($id: ID!) {\n  user(id: $id) { name }\n}\n\n{\n  \"id\": 7\n}\n\n### Socket\nWEBSOCKET ws://localhost:8080/ws\n\n{\"op\": \"hello\"}\n=== wait-for-server\n{\"op\": \"next\"}\n===\nbye\n\n### gRPC\nGRPC localhost:50051/shop.Users/Get\n\n{\"id\": 1}\n",
        );
        let methods: Vec<_> = file.requests.iter().map(|r| r.method.as_str()).collect();
        assert_eq!(methods, vec!["GRAPHQL", "WEBSOCKET", "GRPC"]);

        let http = graphql_as_http(&file.requests[0]).unwrap();
        assert_eq!(http.method, "POST");
        let body: serde_json::Value = serde_json::from_str(http.body.as_deref().unwrap()).unwrap();
        assert_eq!(body["variables"]["id"], 7);
        assert!(body["query"].as_str().unwrap().starts_with("query User($id: ID!)"));
        assert_eq!(http.header("Content-Type"), Some("application/json"));
        assert_eq!(http.header("Authorization"), Some("Bearer x"));
        let (q, v) = graphql_parts("{ me { id } }");
        assert_eq!((q.as_str(), v), ("{ me { id } }", None));

        let msgs = websocket_messages(file.requests[1].body.as_deref());
        assert_eq!(
            msgs,
            vec![
                WsMessage { text: "{\"op\": \"hello\"}".into(), wait_for_server: false },
                WsMessage { text: "{\"op\": \"next\"}".into(), wait_for_server: true },
                WsMessage { text: "bye".into(), wait_for_server: false },
            ]
        );
        // Schemes: ws:// for sockets without one.
        let r = resolve(
            &parse("WEBSOCKET localhost:8080/ws").requests[0],
            &BTreeMap::new(),
            &BTreeMap::new(),
        );
        assert_eq!(r.url, "ws://localhost:8080/ws");
    }
}
