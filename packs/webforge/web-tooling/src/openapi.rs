//! OpenAPI (3.x) and Swagger (2.0) documents in YAML or JSON: detection, the endpoints of
//! the shared Endpoints tool window, the model of the native preview panel (operations
//! with parameters, request bodies and responses, schemas) and "Generate HTTP Request"
//! into the HTTP client. Validation is `forge-jsonschema`'s, with the bundled official
//! OpenAPI schemas.

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value as Json};

use forge_jsonschema::tree::{Node, Value};
use forge_jsonschema::DocKind;

/// HTTP methods of a path item, in the order the preview lists them.
pub const METHODS: &[&str] = &[
    "get", "put", "post", "delete", "options", "head", "patch", "trace",
];

/// Whether a JSON / YAML text is an OpenAPI or Swagger document (a top-level `openapi`
/// or `swagger` key in its first lines).
pub fn is_openapi(text: &str) -> bool {
    version(text).is_some()
}

/// `("openapi", "3.0.3")` or `("swagger", "2.0")`.
pub fn version(text: &str) -> Option<(String, String)> {
    let head: String = text.chars().take(16 * 1024).collect();
    for key in ["openapi", "swagger"] {
        let yaml = head
            .lines()
            .find_map(|l| l.strip_prefix(&format!("{key}:")))
            .map(|v| v.trim().trim_matches(['"', '\'']).to_string());
        let json = || {
            let q = format!("\"{key}\"");
            let i = head.find(&q)?;
            let rest = head[i + q.len()..]
                .trim_start()
                .strip_prefix(':')?
                .trim_start();
            let rest = rest.strip_prefix('"')?;
            Some(rest[..rest.find('"')?].to_string())
        };
        if let Some(v) = yaml.or_else(json) {
            if v.starts_with('2') || v.starts_with('3') {
                return Some((key.to_string(), v));
            }
        }
    }
    None
}

fn kind_of(path: &str, text: &str) -> DocKind {
    DocKind::from_path(path).unwrap_or(if text.trim_start().starts_with('{') {
        DocKind::Json
    } else {
        DocKind::Yaml
    })
}

fn root(path: &str, text: &str) -> Option<Node> {
    forge_jsonschema::parse(kind_of(path, text), text)
        .into_iter()
        .next()
}

fn line_of(text: &str, byte: usize) -> u32 {
    text[..byte.min(text.len())].matches('\n').count() as u32
}

fn str_of<'a>(n: &'a Node, key: &str) -> Option<&'a str> {
    n.get(key).and_then(Node::as_str)
}

/// One operation of the document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Endpoint {
    /// Upper case: `GET`.
    pub method: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub deprecated: bool,
    /// 0-based line of the method key.
    pub line: u32,
}

/// The operations of a document, in document order.
pub fn endpoints(path: &str, text: &str) -> Vec<Endpoint> {
    let Some(doc) = root(path, text) else {
        return Vec::new();
    };
    let Some(paths) = doc.get("paths").and_then(Node::as_object) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for p in paths {
        let Some(item) = p.value.as_ref().and_then(Node::as_object) else {
            continue;
        };
        for op in item {
            let m = op.key.to_ascii_lowercase();
            if !METHODS.contains(&m.as_str()) {
                continue;
            }
            let node = op.value.as_ref();
            out.push(Endpoint {
                method: m.to_ascii_uppercase(),
                path: p.key.clone(),
                operation_id: node
                    .and_then(|n| str_of(n, "operationId"))
                    .map(str::to_string),
                summary: node.and_then(|n| str_of(n, "summary")).map(str::to_string),
                tags: node
                    .and_then(|n| n.get("tags"))
                    .and_then(Node::as_array)
                    .map(|a| {
                        a.iter()
                            .filter_map(|t| t.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default(),
                deprecated: node
                    .and_then(|n| n.get("deprecated"))
                    .and_then(Node::as_bool)
                    .unwrap_or(false),
                line: line_of(text, op.key_start),
            });
        }
    }
    out
}

// ---- Preview model ------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Server {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Parameter {
    pub name: String,
    /// `path`, `query`, `header`, `cookie` (Swagger: also `body`, `formData`).
    pub location: String,
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Short type text: `integer`, `string (date-time)`, `User[]`.
    pub type_text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Body {
    pub content_type: String,
    pub type_text: String,
    /// A generated example (pretty JSON) from `example`, `default`, `enum` and types.
    pub example: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Response {
    pub status: String,
    pub description: String,
    #[serde(default)]
    pub bodies: Vec<Body>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Operation {
    #[serde(flatten)]
    pub endpoint: Endpoint,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub parameters: Vec<Parameter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_body: Option<Body>,
    pub responses: Vec<Response>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaProperty {
    pub name: String,
    pub type_text: String,
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaInfo {
    pub name: String,
    pub type_text: String,
    pub line: u32,
    pub properties: Vec<SchemaProperty>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TagGroup {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub operations: Vec<Operation>,
}

/// What the preview panel shows: Swagger UI's layout, rendered natively.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    /// `OpenAPI 3.0.3` / `Swagger 2.0`.
    pub spec: String,
    pub title: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub servers: Vec<Server>,
    /// Operations grouped by their first tag (untagged ones under `default`).
    pub groups: Vec<TagGroup>,
    pub schemas: Vec<SchemaInfo>,
}

/// Resolves local `$ref`s (`#/components/schemas/User`, `#/definitions/User`).
struct Doc<'a> {
    root: &'a Node,
}

impl<'a> Doc<'a> {
    fn pointer(&self, r: &str) -> Option<&'a Node> {
        let path = r.strip_prefix("#/")?;
        let mut n = self.root;
        for seg in path.split('/') {
            let seg = seg.replace("~1", "/").replace("~0", "~");
            n = n.get(&seg)?;
        }
        Some(n)
    }

    /// Follow `$ref` (a few hops).
    fn deref(&self, mut n: &'a Node) -> &'a Node {
        for _ in 0..8 {
            match str_of(n, "$ref").and_then(|r| self.pointer(r)) {
                Some(t) => n = t,
                None => break,
            }
        }
        n
    }

    fn type_text(&self, n: &Node) -> String {
        if let Some(r) = str_of(n, "$ref") {
            return r.rsplit('/').next().unwrap_or(r).to_string();
        }
        if let Some(items) = n.get("items") {
            return format!("{}[]", self.type_text(items));
        }
        for comb in ["oneOf", "anyOf", "allOf"] {
            if let Some(list) = n.get(comb).and_then(Node::as_array) {
                let sep = if comb == "allOf" { " & " } else { " | " };
                return list
                    .iter()
                    .map(|s| self.type_text(s))
                    .collect::<Vec<_>>()
                    .join(sep);
            }
        }
        let t = match n.get("type") {
            Some(t) => match t.as_array() {
                Some(list) => list
                    .iter()
                    .filter_map(Node::as_str)
                    .collect::<Vec<_>>()
                    .join(" | "),
                None => t.as_str().unwrap_or("any").to_string(),
            },
            None if n.get("properties").is_some() => "object".to_string(),
            None => "any".to_string(),
        };
        match str_of(n, "format") {
            Some(f) => format!("{t} ({f})"),
            None => t,
        }
    }

    /// An example value for a schema.
    fn example(&self, n: &Node, depth: u32) -> Json {
        if depth > 6 {
            return Json::Null;
        }
        let n = self.deref(n);
        if let Some(e) = n.get("example").or_else(|| n.get("default")) {
            return to_json(e);
        }
        if let Some(e) = n
            .get("examples")
            .and_then(Node::as_array)
            .and_then(|a| a.first())
        {
            return to_json(e);
        }
        if let Some(first) = n
            .get("enum")
            .and_then(Node::as_array)
            .and_then(|a| a.first())
        {
            return to_json(first);
        }
        for comb in ["oneOf", "anyOf"] {
            if let Some(first) = n.get(comb).and_then(Node::as_array).and_then(|a| a.first()) {
                return self.example(first, depth + 1);
            }
        }
        if let Some(all) = n.get("allOf").and_then(Node::as_array) {
            let mut merged = Map::new();
            for part in all {
                if let Json::Object(o) = self.example(part, depth + 1) {
                    merged.extend(o);
                }
            }
            return Json::Object(merged);
        }
        let ty = n
            .get("type")
            .and_then(|t| t.as_str().or_else(|| t.as_array()?.first()?.as_str()))
            .unwrap_or(if n.get("properties").is_some() {
                "object"
            } else {
                ""
            });
        match ty {
            "object" | "" if n.get("properties").is_some() => {
                let mut m = Map::new();
                for p in n.get("properties").and_then(Node::as_object).unwrap_or(&[]) {
                    if let Some(v) = &p.value {
                        if self.deref(v).get("readOnly").and_then(Node::as_bool) == Some(true) {
                            continue;
                        }
                        m.insert(p.key.clone(), self.example(v, depth + 1));
                    }
                }
                Json::Object(m)
            }
            "object" => json!({}),
            "array" => match n.get("items") {
                Some(items) => Json::Array(vec![self.example(items, depth + 1)]),
                None => json!([]),
            },
            "integer" => json!(0),
            "number" => json!(0.0),
            "boolean" => json!(true),
            "string" => json!(match str_of(n, "format") {
                Some("date-time") => "2024-01-01T00:00:00Z",
                Some("date") => "2024-01-01",
                Some("email") => "user@example.com",
                Some("uuid") => "3fa85f64-5717-4562-b3fc-2c963f66afa6",
                Some("uri") | Some("url") => "https://example.com",
                _ => "string",
            }),
            _ => Json::Null,
        }
    }

    fn parameter(&self, n: &Node) -> Option<Parameter> {
        let n = self.deref(n);
        let schema = n.get("schema");
        Some(Parameter {
            name: str_of(n, "name")?.to_string(),
            location: str_of(n, "in").unwrap_or("query").to_string(),
            required: n.get("required").and_then(Node::as_bool).unwrap_or(false),
            description: str_of(n, "description").map(str::to_string),
            type_text: match schema {
                Some(s) => self.type_text(s),
                None => self.type_text(n),
            },
        })
    }

    fn bodies(&self, content: Option<&Node>) -> Vec<Body> {
        let Some(content) = content.and_then(Node::as_object) else {
            return Vec::new();
        };
        content
            .iter()
            .filter_map(|c| {
                let media = c.value.as_ref()?;
                let example = media
                    .get("example")
                    .map(to_json)
                    .or_else(|| {
                        let first = media
                            .get("examples")?
                            .as_object()?
                            .first()?
                            .value
                            .as_ref()?;
                        let first = self.deref(first);
                        Some(to_json(first.get("value")?))
                    })
                    .or_else(|| media.get("schema").map(|s| self.example(s, 0)));
                Some(Body {
                    content_type: c.key.clone(),
                    type_text: media
                        .get("schema")
                        .map(|s| self.type_text(s))
                        .unwrap_or_default(),
                    example: example
                        .map(|e| serde_json::to_string_pretty(&e).unwrap_or_default())
                        .unwrap_or_default(),
                })
            })
            .collect()
    }

    fn operation(&self, text: &str, ep: Endpoint, path_item: &Node, op: &Node) -> Operation {
        let mut parameters: Vec<Parameter> = Vec::new();
        for list in [path_item.get("parameters"), op.get("parameters")] {
            for p in list.and_then(Node::as_array).unwrap_or(&[]) {
                if let Some(p) = self.parameter(p) {
                    parameters.retain(|q| !(q.name == p.name && q.location == p.location));
                    parameters.push(p);
                }
            }
        }
        // OpenAPI 3: requestBody; Swagger 2: a `body` parameter with a schema.
        let request_body = match op.get("requestBody").map(|b| self.deref(b)) {
            Some(b) => self.bodies(b.get("content")).into_iter().next(),
            None => op
                .get("parameters")
                .and_then(Node::as_array)
                .unwrap_or(&[])
                .iter()
                .map(|p| self.deref(p))
                .find(|p| str_of(p, "in") == Some("body"))
                .and_then(|p| p.get("schema"))
                .map(|s| Body {
                    content_type: "application/json".into(),
                    type_text: self.type_text(s),
                    example: serde_json::to_string_pretty(&self.example(s, 0)).unwrap_or_default(),
                }),
        };
        parameters.retain(|p| p.location != "body");
        let responses = op
            .get("responses")
            .and_then(Node::as_object)
            .unwrap_or(&[])
            .iter()
            .filter_map(|r| {
                let v = self.deref(r.value.as_ref()?);
                let mut bodies = self.bodies(v.get("content"));
                if bodies.is_empty() {
                    if let Some(s) = v.get("schema") {
                        bodies.push(Body {
                            content_type: "application/json".into(),
                            type_text: self.type_text(s),
                            example: serde_json::to_string_pretty(&self.example(s, 0))
                                .unwrap_or_default(),
                        });
                    }
                }
                Some(Response {
                    status: r.key.clone(),
                    description: str_of(v, "description").unwrap_or("").to_string(),
                    bodies,
                })
            })
            .collect();
        let _ = text;
        Operation {
            endpoint: ep,
            description: str_of(op, "description").map(str::to_string),
            parameters,
            request_body,
            responses,
        }
    }
}

fn to_json(n: &Node) -> Json {
    match &n.value {
        Value::Null | Value::Error => Json::Null,
        Value::Bool(b) => json!(b),
        Value::Number(f) => {
            if f.fract() == 0.0 && f.abs() < 1e15 {
                json!(*f as i64)
            } else {
                json!(f)
            }
        }
        Value::String(s) => json!(s),
        Value::Array(a) => Json::Array(a.iter().map(to_json).collect()),
        Value::Object(o) => Json::Object(
            o.iter()
                .filter_map(|p| Some((p.key.clone(), to_json(p.value.as_ref()?))))
                .collect(),
        ),
    }
}

/// The preview model of a document; `None` when it is not OpenAPI / Swagger.
pub fn preview(path: &str, text: &str) -> Option<Preview> {
    let (key, ver) = version(text)?;
    let root = root(path, text)?;
    let doc = Doc { root: &root };
    let info = root.get("info");
    let mut servers: Vec<Server> = root
        .get("servers")
        .and_then(Node::as_array)
        .unwrap_or(&[])
        .iter()
        .filter_map(|s| {
            Some(Server {
                url: str_of(s, "url")?.to_string(),
                description: str_of(s, "description").map(str::to_string),
            })
        })
        .collect();
    if key == "swagger" {
        if let Some(host) = str_of(&root, "host") {
            let scheme = root
                .get("schemes")
                .and_then(Node::as_array)
                .and_then(|a| a.first())
                .and_then(Node::as_str)
                .unwrap_or("https");
            servers.push(Server {
                url: format!(
                    "{scheme}://{host}{}",
                    str_of(&root, "basePath").unwrap_or("")
                ),
                description: None,
            });
        }
    }
    let mut groups: Vec<TagGroup> = root
        .get("tags")
        .and_then(Node::as_array)
        .unwrap_or(&[])
        .iter()
        .filter_map(|t| {
            Some(TagGroup {
                name: str_of(t, "name")?.to_string(),
                description: str_of(t, "description").map(str::to_string),
                operations: Vec::new(),
            })
        })
        .collect();
    let paths = root.get("paths").and_then(Node::as_object).unwrap_or(&[]);
    let eps = endpoints(path, text);
    let mut i = 0;
    for p in paths {
        let Some(item) = p.value.as_ref() else {
            continue;
        };
        for op in item.as_object().unwrap_or(&[]) {
            if !METHODS.contains(&op.key.to_ascii_lowercase().as_str()) {
                continue;
            }
            let (Some(ep), Some(node)) = (eps.get(i).cloned(), op.value.as_ref()) else {
                i += 1;
                continue;
            };
            i += 1;
            let tag = ep.tags.first().cloned().unwrap_or_else(|| "default".into());
            let operation = doc.operation(text, ep, item, node);
            match groups.iter_mut().find(|g| g.name == tag) {
                Some(g) => g.operations.push(operation),
                None => groups.push(TagGroup {
                    name: tag,
                    description: None,
                    operations: vec![operation],
                }),
            }
        }
    }
    groups.retain(|g| !g.operations.is_empty());
    let schema_root = root
        .get("components")
        .and_then(|c| c.get("schemas"))
        .or_else(|| root.get("definitions"));
    let schemas = schema_root
        .and_then(Node::as_object)
        .unwrap_or(&[])
        .iter()
        .filter_map(|s| {
            let n = s.value.as_ref()?;
            let required: Vec<&str> = n
                .get("required")
                .and_then(Node::as_array)
                .unwrap_or(&[])
                .iter()
                .filter_map(Node::as_str)
                .collect();
            Some(SchemaInfo {
                name: s.key.clone(),
                type_text: doc.type_text(n),
                line: line_of(text, s.key_start),
                properties: n
                    .get("properties")
                    .and_then(Node::as_object)
                    .unwrap_or(&[])
                    .iter()
                    .filter_map(|p| {
                        let v = p.value.as_ref()?;
                        Some(SchemaProperty {
                            name: p.key.clone(),
                            type_text: doc.type_text(v),
                            required: required.contains(&p.key.as_str()),
                            description: str_of(v, "description").map(str::to_string),
                        })
                    })
                    .collect(),
            })
        })
        .collect();
    Some(Preview {
        spec: format!(
            "{} {ver}",
            if key == "swagger" {
                "Swagger"
            } else {
                "OpenAPI"
            }
        ),
        title: info
            .and_then(|i| str_of(i, "title"))
            .unwrap_or("Untitled API")
            .to_string(),
        version: info
            .and_then(|i| str_of(i, "version"))
            .unwrap_or("")
            .to_string(),
        description: info
            .and_then(|i| str_of(i, "description"))
            .map(str::to_string),
        servers,
        groups,
        schemas,
    })
}

// ---- HTTP client ------------------------------------------------------------------------

/// "Generate HTTP Request": the `.http` text of an operation, with path parameters as
/// `{{name}}` variables, required query parameters, an `Accept` header, the example
/// request body and the server URL as `{{baseUrl}}`'s default (`@baseUrl = …`).
pub fn http_request(path: &str, text: &str, method: &str, op_path: &str) -> Option<String> {
    let p = preview(path, text)?;
    let op =
        p.groups.iter().flat_map(|g| &g.operations).find(|o| {
            o.endpoint.method.eq_ignore_ascii_case(method) && o.endpoint.path == op_path
        })?;
    let base = p
        .servers
        .first()
        .map(|s| s.url.trim_end_matches('/').to_string())
        .unwrap_or_else(|| "http://localhost".into());
    let mut out = format!("@baseUrl = {base}\n\n");
    let title = op
        .endpoint
        .summary
        .clone()
        .or_else(|| op.endpoint.operation_id.clone())
        .unwrap_or_else(|| format!("{} {}", op.endpoint.method, op.endpoint.path));
    out.push_str(&format!("### {title}\n"));
    if let Some(id) = &op.endpoint.operation_id {
        out.push_str(&format!("# @name {id}\n"));
    }
    let mut url = format!(
        "{{{{baseUrl}}}}{}",
        op.endpoint.path.replace('{', "{{").replace('}', "}}")
    );
    let query: Vec<String> = op
        .parameters
        .iter()
        .filter(|q| q.location == "query" && q.required)
        .map(|q| format!("{0}={{{{{0}}}}}", q.name))
        .collect();
    if !query.is_empty() {
        url.push('?');
        url.push_str(&query.join("&"));
    }
    out.push_str(&format!("{} {url}\n", op.endpoint.method));
    let accept = op
        .responses
        .iter()
        .flat_map(|r| &r.bodies)
        .map(|b| b.content_type.clone())
        .next()
        .unwrap_or_else(|| "application/json".into());
    out.push_str(&format!("Accept: {accept}\n"));
    for h in op
        .parameters
        .iter()
        .filter(|h| h.location == "header" && h.required)
    {
        out.push_str(&format!("{0}: {{{{{0}}}}}\n", h.name));
    }
    if let Some(body) = &op.request_body {
        out.push_str(&format!(
            "Content-Type: {}\n\n{}\n",
            body.content_type, body.example
        ));
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEC: &str = r##"openapi: 3.0.3
info:
  title: Pet Store
  version: 1.2.0
  description: Pets for everyone
servers:
  - url: https://api.example.com/v1/
tags:
  - name: pets
    description: Everything about pets
paths:
  /pets:
    get:
      tags: [pets]
      summary: List pets
      operationId: listPets
      parameters:
        - name: limit
          in: query
          required: true
          schema: { type: integer, format: int32 }
      responses:
        '200':
          description: A list of pets
          content:
            application/json:
              schema:
                type: array
                items: { $ref: '#/components/schemas/Pet' }
    post:
      tags: [pets]
      summary: Create a pet
      operationId: createPet
      requestBody:
        content:
          application/json:
            schema: { $ref: '#/components/schemas/Pet' }
      responses:
        '201': { description: Created }
  /pets/{petId}:
    parameters:
      - name: petId
        in: path
        required: true
        schema: { type: string }
    delete:
      deprecated: true
      responses:
        '204': { description: Deleted }
components:
  schemas:
    Pet:
      type: object
      required: [name]
      properties:
        id: { type: integer, readOnly: true }
        name: { type: string, example: Rex }
        tag: { type: string, enum: [dog, cat] }
        born: { type: string, format: date }
"##;

    #[test]
    fn detects_documents() {
        assert_eq!(version(SPEC), Some(("openapi".into(), "3.0.3".into())));
        assert_eq!(
            version(r#"{"swagger": "2.0", "info": {}}"#),
            Some(("swagger".into(), "2.0".into()))
        );
        assert!(!is_openapi("name: shop\nversion: 1\n"));
    }

    #[test]
    fn extracts_endpoints() {
        let eps = endpoints("api.yaml", SPEC);
        let got: Vec<_> = eps
            .iter()
            .map(|e| (e.method.as_str(), e.path.as_str(), e.line))
            .collect();
        assert_eq!(
            got,
            vec![
                ("GET", "/pets", 12),
                ("POST", "/pets", 29),
                ("DELETE", "/pets/{petId}", 45)
            ]
        );
        assert_eq!(eps[0].operation_id.as_deref(), Some("listPets"));
        assert_eq!(eps[0].tags, vec!["pets"]);
        assert!(eps[2].deprecated);
    }

    #[test]
    fn builds_the_preview() {
        let p = preview("api.yaml", SPEC).unwrap();
        assert_eq!(
            (p.spec.as_str(), p.title.as_str(), p.version.as_str()),
            ("OpenAPI 3.0.3", "Pet Store", "1.2.0")
        );
        assert_eq!(p.servers[0].url, "https://api.example.com/v1/");
        assert_eq!(
            p.groups.iter().map(|g| g.name.as_str()).collect::<Vec<_>>(),
            vec!["pets", "default"]
        );
        let list = &p.groups[0].operations[0];
        assert_eq!(list.parameters[0].type_text, "integer (int32)");
        assert_eq!(list.responses[0].bodies[0].type_text, "Pet[]");
        let create = &p.groups[0].operations[1];
        let body: Json =
            serde_json::from_str(&create.request_body.as_ref().unwrap().example).unwrap();
        assert_eq!(
            body,
            json!({ "name": "Rex", "tag": "dog", "born": "2024-01-01" })
        );
        let delete = &p.groups[1].operations[0];
        assert_eq!(delete.parameters[0].name, "petId");
        assert_eq!(p.schemas[0].name, "Pet");
        assert!(p.schemas[0]
            .properties
            .iter()
            .any(|s| s.name == "name" && s.required));
    }

    #[test]
    fn generates_http_requests() {
        let req = http_request("api.yaml", SPEC, "post", "/pets").unwrap();
        assert!(req.starts_with("@baseUrl = https://api.example.com/v1\n\n### Create a pet\n# @name createPet\nPOST {{baseUrl}}/pets\n"), "{req}");
        assert!(
            req.contains("Content-Type: application/json\n\n{\n  \"born\""),
            "{req}"
        );
        let get = http_request("api.yaml", SPEC, "GET", "/pets").unwrap();
        assert!(
            get.contains("GET {{baseUrl}}/pets?limit={{limit}}\nAccept: application/json\n"),
            "{get}"
        );
        let del = http_request("api.yaml", SPEC, "DELETE", "/pets/{petId}").unwrap();
        assert!(del.contains("DELETE {{baseUrl}}/pets/{{petId}}\n"), "{del}");
        assert!(http_request("api.yaml", SPEC, "PUT", "/pets").is_none());
    }

    #[test]
    fn swagger_two() {
        let s = r##"{"swagger": "2.0", "info": {"title": "Old", "version": "1"}, "host": "old.example.com", "basePath": "/api", "schemes": ["http"],
  "paths": {"/users": {"post": {"parameters": [{"in": "body", "name": "body", "schema": {"$ref": "#/definitions/User"}}], "responses": {"200": {"description": "ok", "schema": {"$ref": "#/definitions/User"}}}}}},
  "definitions": {"User": {"type": "object", "properties": {"email": {"type": "string", "format": "email"}}}}}"##;
        let p = preview("old.json", s).unwrap();
        assert_eq!(p.spec, "Swagger 2.0");
        assert_eq!(p.servers[0].url, "http://old.example.com/api");
        let op = &p.groups[0].operations[0];
        assert!(op.parameters.is_empty());
        assert!(op
            .request_body
            .as_ref()
            .unwrap()
            .example
            .contains("user@example.com"));
        assert_eq!(op.responses[0].bodies[0].type_text, "User");
    }
}
