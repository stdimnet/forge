//! Spring Boot actuator: `/actuator/health`, `/actuator/beans` and `/actuator/mappings`
//! of a running application (Spring Boot 2 and 3, Spring MVC and WebFlux), read over plain
//! HTTP/1.1 from localhost (feature `net`). The parsers are pure.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthComponent {
    /// `db`, `diskSpace`, `redis/primary` for nested components.
    pub name: String,
    pub status: String,
    /// A short summary of the details (`database: H2`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Health {
    /// `UP`, `DOWN`, `OUT_OF_SERVICE`, `UNKNOWN`.
    pub status: String,
    pub components: Vec<HealthComponent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeBean {
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<String>,
    /// The application context (`application`, `management`).
    pub context: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeMapping {
    pub methods: Vec<String>,
    pub patterns: Vec<String>,
    /// `com.acme.OrderController#find(Long)` or `Actuator web endpoint 'health'`.
    pub handler: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
}

fn summary(details: &Value) -> Option<String> {
    let obj = details.as_object()?;
    let parts: Vec<String> = obj
        .iter()
        .filter_map(|(k, v)| match v {
            Value::String(s) => Some(format!("{k}: {s}")),
            Value::Number(n) => Some(format!("{k}: {n}")),
            Value::Bool(b) => Some(format!("{k}: {b}")),
            _ => None,
        })
        .take(3)
        .collect();
    (!parts.is_empty()).then(|| parts.join(", "))
}

fn components(prefix: &str, v: &Value, out: &mut Vec<HealthComponent>) {
    let Some(map) = v.as_object() else { return };
    for (name, c) in map {
        let full = if prefix.is_empty() { name.clone() } else { format!("{prefix}/{name}") };
        let status = c.get("status").and_then(Value::as_str).unwrap_or("UNKNOWN").to_string();
        let nested = c.get("components");
        out.push(HealthComponent {
            name: full.clone(),
            status,
            details: c.get("details").and_then(summary),
        });
        if let Some(n) = nested {
            components(&full, n, out);
        }
    }
}

/// `/actuator/health` (Spring Boot 2.2+: `components`; older: `details`).
pub fn parse_health(json: &str) -> Option<Health> {
    let v: Value = serde_json::from_str(json).ok()?;
    let status = v.get("status")?.as_str()?.to_string();
    let mut list = Vec::new();
    if let Some(c) = v.get("components") {
        components("", c, &mut list);
    } else if let Some(d) = v.get("details").filter(|d| d.as_object().is_some_and(|o| o.values().all(|x| x.get("status").is_some()))) {
        components("", d, &mut list);
    }
    Some(Health { status, components: list })
}

/// `/actuator/beans`.
pub fn parse_beans(json: &str) -> Vec<RuntimeBean> {
    let Ok(v) = serde_json::from_str::<Value>(json) else { return Vec::new() };
    let mut out = Vec::new();
    for (context, ctx) in v.get("contexts").and_then(Value::as_object).into_iter().flatten() {
        for (name, b) in ctx.get("beans").and_then(Value::as_object).into_iter().flatten() {
            out.push(RuntimeBean {
                name: name.clone(),
                type_name: b.get("type").and_then(Value::as_str).unwrap_or_default().to_string(),
                scope: b.get("scope").and_then(Value::as_str).map(str::to_string),
                resource: b.get("resource").and_then(Value::as_str).map(str::to_string),
                dependencies: b
                    .get("dependencies")
                    .and_then(Value::as_array)
                    .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
                    .unwrap_or_default(),
                context: context.clone(),
            });
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

fn mapping(entry: &Value) -> Option<RuntimeMapping> {
    let handler = entry.get("handler").and_then(Value::as_str).unwrap_or_default().to_string();
    let details = entry.get("details");
    let cond = details.and_then(|d| d.get("requestMappingConditions"));
    let strings = |v: Option<&Value>| -> Vec<String> {
        v.and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
            .unwrap_or_default()
    };
    let (methods, patterns) = match cond {
        Some(c) => (strings(c.get("methods")), strings(c.get("patterns"))),
        None => {
            // Functional routes and resource handlers: the predicate (`/webjars/**`).
            let p = entry.get("predicate").and_then(Value::as_str)?;
            if p.starts_with('{') || handler.contains("ResourceHttpRequestHandler") || handler.contains("ResourceWebHandler") {
                return None;
            }
            (Vec::new(), vec![p.to_string()])
        }
    };
    let hm = details.and_then(|d| d.get("handlerMethod"));
    Some(RuntimeMapping {
        methods,
        patterns,
        handler,
        class: hm.and_then(|h| h.get("className")).and_then(Value::as_str).map(str::to_string),
        method: hm.and_then(|h| h.get("name")).and_then(Value::as_str).map(str::to_string),
    })
}

/// `/actuator/mappings`: request mappings of the dispatcher servlets (MVC) and handlers
/// (WebFlux) of every context.
pub fn parse_mappings(json: &str) -> Vec<RuntimeMapping> {
    let Ok(v) = serde_json::from_str::<Value>(json) else { return Vec::new() };
    let mut out = Vec::new();
    for ctx in v.get("contexts").and_then(Value::as_object).into_iter().flatten().map(|(_, c)| c) {
        let m = ctx.get("mappings");
        for group in ["dispatcherServlets", "dispatcherHandlers"] {
            for list in m.and_then(|m| m.get(group)).and_then(Value::as_object).into_iter().flatten().map(|(_, l)| l) {
                out.extend(list.as_array().into_iter().flatten().filter_map(mapping));
            }
        }
    }
    out
}

/// What the actuator said about a running application.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    /// `http://localhost:8080/actuator`.
    pub base_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health: Option<Health>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub beans: Option<Vec<RuntimeBean>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mappings: Option<Vec<RuntimeMapping>>,
    /// Endpoints that could not be read (`beans: 404 (not exposed)`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub problems: Vec<String>,
}

#[cfg(feature = "net")]
mod net {
    use std::io::{Read, Write};
    use std::net::{SocketAddr, TcpStream};
    use std::time::Duration;

    use super::*;

    fn dechunk(body: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < body.len() {
            let Some(eol) = body[i..].windows(2).position(|w| w == b"\r\n") else { break };
            let size_line = String::from_utf8_lossy(&body[i..i + eol]);
            let size = usize::from_str_radix(size_line.split(';').next().unwrap_or("0").trim(), 16).unwrap_or(0);
            i += eol + 2;
            if size == 0 {
                break;
            }
            out.extend_from_slice(&body[i..(i + size).min(body.len())]);
            i += size + 2;
        }
        out
    }

    /// `GET path` on `127.0.0.1:port`: (status, body).
    pub fn get(port: u16, path: &str, timeout: Duration) -> Result<(u16, String), String> {
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        let mut s = TcpStream::connect_timeout(&addr, timeout).map_err(|e| format!("localhost:{port}: {e}"))?;
        s.set_read_timeout(Some(timeout)).map_err(|e| e.to_string())?;
        s.set_write_timeout(Some(timeout)).map_err(|e| e.to_string())?;
        let req = format!("GET {path} HTTP/1.1\r\nHost: localhost:{port}\r\nAccept: application/json\r\nUser-Agent: Forge\r\nConnection: close\r\n\r\n");
        s.write_all(req.as_bytes()).map_err(|e| e.to_string())?;
        let mut buf = Vec::new();
        s.read_to_end(&mut buf).map_err(|e| format!("localhost:{port}{path}: {e}"))?;
        let head_end = buf.windows(4).position(|w| w == b"\r\n\r\n").ok_or("malformed HTTP response")?;
        let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
        let status = head
            .lines()
            .next()
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|c| c.parse::<u16>().ok())
            .ok_or("malformed HTTP status line")?;
        let chunked = head.lines().any(|l| {
            let l = l.to_ascii_lowercase();
            l.starts_with("transfer-encoding:") && l.contains("chunked")
        });
        let body = &buf[head_end + 4..];
        let body = if chunked { dechunk(body) } else { body.to_vec() };
        Ok((status, String::from_utf8_lossy(&body).to_string()))
    }

    /// Read health, beans and mappings from `http://localhost:<port><base>`.
    pub fn probe(port: u16, base_path: &str, timeout: Duration) -> Snapshot {
        let base = format!("/{}", base_path.trim_matches('/'));
        let base = if base == "/" { String::new() } else { base };
        let mut snap = Snapshot {
            base_url: format!("http://localhost:{port}{base}"),
            ..Snapshot::default()
        };
        for what in ["health", "beans", "mappings"] {
            match get(port, &format!("{base}/{what}"), timeout) {
                // Health answers 503 when the application is down, with the same body.
                Ok((status, body)) if status == 200 || (what == "health" && status == 503) => match what {
                    "health" => snap.health = parse_health(&body),
                    "beans" => snap.beans = Some(parse_beans(&body)),
                    _ => snap.mappings = Some(parse_mappings(&body)),
                },
                Ok((404, _)) => snap.problems.push(format!("{what}: not exposed (404)")),
                Ok((status, _)) => snap.problems.push(format!("{what}: HTTP {status}")),
                Err(e) => {
                    snap.problems.push(e);
                    // Nothing listens: the other endpoints would fail the same way.
                    break;
                }
            }
        }
        snap
    }
}

#[cfg(feature = "net")]
pub use net::{get, probe};

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) const HEALTH: &str = r#"{"status":"UP","components":{"db":{"status":"UP","details":{"database":"H2","validationQuery":"isValid()"}},"diskSpace":{"status":"UP","details":{"total":499963174912,"free":91300814848,"threshold":10485760,"path":"/srv/.","exists":true}},"ping":{"status":"UP"},"redis":{"status":"DOWN","components":{"primary":{"status":"DOWN","details":{"error":"Connection refused"}}}}}}"#;

    pub(crate) const BEANS: &str = r#"{"contexts":{"application":{"beans":{"orderService":{"aliases":[],"scope":"singleton","type":"com.acme.shop.orders.OrderService","resource":"file [/srv/OrderService.class]","dependencies":["orderRepository","notifiers"]},"jacksonObjectMapper":{"aliases":[],"scope":"singleton","type":"com.fasterxml.jackson.databind.ObjectMapper","dependencies":[]}},"parentId":null}}}"#;

    pub(crate) const MAPPINGS: &str = r#"{"contexts":{"application":{"mappings":{"dispatcherServlets":{"dispatcherServlet":[
      {"handler":"com.acme.shop.web.OrderController#find(Long)","predicate":"{GET [/api/orders/{id}], produces [application/json]}","details":{"handlerMethod":{"className":"com.acme.shop.web.OrderController","name":"find","descriptor":"(Ljava/lang/Long;)Lcom/acme/shop/orders/Order;"},"requestMappingConditions":{"consumes":[],"headers":[],"methods":["GET"],"params":[],"patterns":["/api/orders/{id}"],"produces":[{"mediaType":"application/json","negated":false}]}}},
      {"handler":"Actuator web endpoint 'health'","predicate":"{GET [/actuator/health], produces [application/vnd.spring-boot.actuator.v3+json]}","details":{"handlerMethod":null,"requestMappingConditions":{"consumes":[],"headers":[],"methods":["GET"],"params":[],"patterns":["/actuator/health"],"produces":[]}}},
      {"handler":"org.springframework.boot.autoconfigure.web.servlet.error.BasicErrorController#error(HttpServletRequest)","predicate":"{ [/error]}","details":{"handlerMethod":{"className":"org.springframework.boot.autoconfigure.web.servlet.error.BasicErrorController","name":"error","descriptor":"()V"},"requestMappingConditions":{"consumes":[],"headers":[],"methods":[],"params":[],"patterns":["/error"],"produces":[]}}},
      {"handler":"ResourceHttpRequestHandler [classpath [META-INF/resources/webjars/]]","predicate":"/webjars/**"}
    ]},"servletFilters":[],"servlets":[]},"parentId":null}}}"#;

    #[test]
    fn parses_health_beans_and_mappings() {
        let h = parse_health(HEALTH).unwrap();
        assert_eq!(h.status, "UP");
        let names: Vec<(&str, &str)> = h.components.iter().map(|c| (c.name.as_str(), c.status.as_str())).collect();
        assert_eq!(names, [("db", "UP"), ("diskSpace", "UP"), ("ping", "UP"), ("redis", "DOWN"), ("redis/primary", "DOWN")]);
        assert_eq!(h.components[0].details.as_deref(), Some("database: H2, validationQuery: isValid()"));
        // Spring Boot 2.0 / 2.1.
        let old = parse_health(r#"{"status":"DOWN","details":{"db":{"status":"DOWN","details":{"error":"x"}}}}"#).unwrap();
        assert_eq!((old.status.as_str(), old.components.len()), ("DOWN", 1));
        assert_eq!(parse_health("{}"), None);

        let beans = parse_beans(BEANS);
        assert_eq!(beans.len(), 2);
        assert_eq!(beans[1].name, "orderService");
        assert_eq!(beans[1].dependencies, ["orderRepository", "notifiers"]);
        assert_eq!(beans[1].context, "application");

        let maps = parse_mappings(MAPPINGS);
        assert_eq!(maps.len(), 3);
        assert_eq!(maps[0].methods, ["GET"]);
        assert_eq!(maps[0].patterns, ["/api/orders/{id}"]);
        assert_eq!(maps[0].class.as_deref(), Some("com.acme.shop.web.OrderController"));
        assert_eq!(maps[1].class, None);
        assert!(maps[2].methods.is_empty());
        let flux = parse_mappings(r#"{"contexts":{"app":{"mappings":{"dispatcherHandlers":{"webHandler":[{"handler":"x#y()","predicate":"{POST /a}","details":{"handlerMethod":{"className":"x","name":"y"},"requestMappingConditions":{"methods":["POST"],"patterns":["/a"]}}}]}}}}}"#);
        assert_eq!(flux[0].methods, ["POST"]);
    }

    #[cfg(feature = "net")]
    #[test]
    fn probes_a_local_server() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::time::Duration;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            for _ in 0..3 {
                let (mut s, _) = listener.accept().unwrap();
                let mut buf = [0u8; 1024];
                let n = s.read(&mut buf).unwrap();
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                let path = req.split_whitespace().nth(1).unwrap_or_default().to_string();
                let resp = match path.as_str() {
                    "/manage/health" => {
                        // Chunked, and 503 like a DOWN application.
                        let body = HEALTH;
                        let (a, b) = body.split_at(40);
                        format!("HTTP/1.1 503 Service Unavailable\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{a}\r\n{:x}\r\n{b}\r\n0\r\n\r\n", a.len(), b.len())
                    }
                    "/manage/beans" => format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{BEANS}", BEANS.len()),
                    _ => "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_string(),
                };
                s.write_all(resp.as_bytes()).unwrap();
            }
        });
        let snap = probe(port, "/manage/", Duration::from_secs(5));
        server.join().unwrap();
        assert_eq!(snap.base_url, format!("http://localhost:{port}/manage"));
        assert_eq!(snap.health.unwrap().components.len(), 5);
        assert_eq!(snap.beans.unwrap().len(), 2);
        assert_eq!(snap.mappings, None);
        assert_eq!(snap.problems, ["mappings: not exposed (404)"]);

        // Nothing listens on a closed port.
        let closed = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let none = probe(closed, "/actuator", Duration::from_millis(500));
        assert!(none.health.is_none() && none.problems.len() == 1);
    }
}
