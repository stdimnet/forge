//! What the Endpoints and Beans tool windows show: the static model merged with what the
//! actuator of a running application reports (`live` rows exist at runtime; `runtime` rows
//! come only from the application, such as actuator and library endpoints), and the
//! "Open in HTTP Client" requests (`.http` text).

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::actuator::{RuntimeBean, RuntimeMapping};
use crate::model::{Bean, Endpoint, EndpointParam};
use crate::Location;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EndpointRow {
    pub method: String,
    pub path: String,
    pub handler: String,
    /// `spring`, `jaxrs` or `runtime`.
    pub framework: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<Location>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub produces: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub consumes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<EndpointParam>,
    /// The running application serves it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub live: bool,
}

impl From<&Endpoint> for EndpointRow {
    fn from(e: &Endpoint) -> Self {
        Self {
            method: e.method.clone(),
            path: e.path.clone(),
            handler: e.handler.clone(),
            framework: e.framework.clone(),
            location: Some(e.location.clone()),
            produces: e.produces.clone(),
            consumes: e.consumes.clone(),
            params: e.params.clone(),
            live: false,
        }
    }
}

/// Static endpoints, marked `live` when the application maps them, followed by the
/// mappings only the application knows.
pub fn endpoint_rows(
    endpoints: &[Endpoint],
    runtime: Option<&[RuntimeMapping]>,
) -> Vec<EndpointRow> {
    let mut rows: Vec<EndpointRow> = endpoints.iter().map(EndpointRow::from).collect();
    let Some(runtime) = runtime else { return rows };
    for m in runtime {
        let methods: Vec<String> = if m.methods.is_empty() {
            vec!["ANY".into()]
        } else {
            m.methods.clone()
        };
        for p in &m.patterns {
            for verb in &methods {
                let found = rows.iter_mut().find(|r| {
                    r.path == *p
                        && (r.method == *verb || r.method == "ANY" || verb == "ANY")
                        && r.framework != "runtime"
                });
                match found {
                    Some(r) => r.live = true,
                    None => rows.push(EndpointRow {
                        method: verb.clone(),
                        path: p.clone(),
                        handler: match (&m.class, &m.method) {
                            (Some(c), Some(n)) => {
                                format!("{}.{n}", c.rsplit('.').next().unwrap_or(c))
                            }
                            _ => m.handler.clone(),
                        },
                        framework: "runtime".into(),
                        location: None,
                        produces: Vec::new(),
                        consumes: Vec::new(),
                        params: Vec::new(),
                        live: true,
                    }),
                }
            }
        }
    }
    rows
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BeanRow {
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
    /// The model's kind, or `runtime` for beans only the application reports.
    pub kind: String,
    pub framework: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub profiles: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<Location>,
    /// Injection points it is a candidate for.
    pub usages: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub live: bool,
}

/// Static beans (`live` when the application has a bean of that name), then the
/// application's own beans that the sources do not declare (auto-configuration).
pub fn bean_rows(beans: &[Bean], runtime: Option<&[RuntimeBean]>) -> Vec<BeanRow> {
    let live: BTreeSet<&str> = runtime
        .into_iter()
        .flatten()
        .map(|b| b.name.as_str())
        .collect();
    let mut rows: Vec<BeanRow> = beans
        .iter()
        .map(|b| BeanRow {
            name: b.name.clone(),
            type_name: b.type_name.clone(),
            kind: b.kind.clone(),
            framework: b.framework.clone(),
            scope: b.scope.clone(),
            profiles: b.profiles.clone(),
            location: Some(b.location.clone()),
            usages: b.injected_into.len(),
            dependencies: runtime
                .into_iter()
                .flatten()
                .find(|r| r.name == b.name)
                .map(|r| r.dependencies.clone())
                .unwrap_or_default(),
            live: live.contains(b.name.as_str()),
        })
        .collect();
    let known: BTreeSet<String> = beans.iter().map(|b| b.name.clone()).collect();
    for r in runtime
        .into_iter()
        .flatten()
        .filter(|r| !known.contains(&r.name))
    {
        rows.push(BeanRow {
            name: r.name.clone(),
            type_name: r.type_name.clone(),
            kind: "runtime".into(),
            framework: "spring".into(),
            scope: r.scope.clone(),
            profiles: Vec::new(),
            location: None,
            usages: 0,
            dependencies: r.dependencies.clone(),
            live: true,
        });
    }
    rows
}

/// An `.http` request for an endpoint (IntelliJ's "Open in HTTP Client"): path variables
/// become `{{variables}}` declared above the request, query parameters are listed,
/// bodies get their content type and an empty JSON object.
pub fn http_request(row: &EndpointRow, base_url: &str) -> String {
    let mut out = String::new();
    let mut path = row.path.clone();
    let mut vars = Vec::new();
    // `{id}`, `{id:\d+}` → `{{id}}`.
    while let Some(start) = path.find('{').filter(|s| !path[*s..].starts_with("{{")) {
        let Some(len) = path[start..].find('}') else {
            break;
        };
        let inner = &path[start + 1..start + len];
        let name = inner
            .split(':')
            .next()
            .unwrap_or(inner)
            .trim()
            .trim_start_matches('*')
            .to_string();
        path.replace_range(start..start + len + 1, &format!("{{{{{name}}}}}"));
        if !vars.contains(&name) {
            vars.push(name);
        }
    }
    for v in &vars {
        let example = row
            .params
            .iter()
            .find(|p| p.kind == "path" && p.name == *v)
            .map(|p| {
                if p.type_name.to_ascii_lowercase().contains("string") {
                    "value"
                } else {
                    "1"
                }
            })
            .unwrap_or("1");
        out.push_str(&format!("@{v} = {example}\n"));
    }
    if !vars.is_empty() {
        out.push('\n');
    }
    out.push_str(&format!("### {}\n", row.handler));
    let query: Vec<String> = row
        .params
        .iter()
        .filter(|p| p.kind == "query")
        .map(|p| format!("{}=", p.name))
        .collect();
    let method = if row.method == "ANY" {
        "GET"
    } else {
        row.method.as_str()
    };
    let url = format!("{}{}", base_url.trim_end_matches('/'), path);
    if query.is_empty() {
        out.push_str(&format!("{method} {url}\n"));
    } else {
        out.push_str(&format!("{method} {url}?{}\n", query.join("&")));
    }
    let accept = row
        .produces
        .first()
        .cloned()
        .unwrap_or_else(|| "application/json".into());
    out.push_str(&format!("Accept: {accept}\n"));
    for h in row.params.iter().filter(|p| p.kind == "header") {
        out.push_str(&format!("{}: \n", h.name));
    }
    let body = row.params.iter().any(|p| p.kind == "body")
        || (matches!(method, "POST" | "PUT" | "PATCH") && !row.consumes.is_empty());
    if body {
        let ct = row
            .consumes
            .first()
            .cloned()
            .unwrap_or_else(|| "application/json".into());
        out.push_str(&format!("Content-Type: {ct}\n\n"));
        out.push_str(if ct.contains("json") { "{\n}\n" } else { "\n" });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actuator::{parse_beans, parse_mappings};

    fn ep(method: &str, path: &str, params: Vec<EndpointParam>) -> Endpoint {
        Endpoint {
            method: method.into(),
            path: path.into(),
            handler: "OrderController.find".into(),
            class: "com.acme.shop.web.OrderController".into(),
            framework: "spring".into(),
            location: Location {
                path: "OrderController.java".into(),
                line: 3,
                column: 4,
            },
            produces: Vec::new(),
            consumes: Vec::new(),
            params,
        }
    }

    fn param(name: &str, kind: &str, t: &str) -> EndpointParam {
        EndpointParam {
            name: name.into(),
            kind: kind.into(),
            type_name: t.into(),
        }
    }

    #[test]
    fn merges_runtime_mappings() {
        let statics = vec![
            ep("GET", "/api/orders/{id}", vec![]),
            ep("DELETE", "/api/orders/{id}", vec![]),
        ];
        let rows = endpoint_rows(
            &statics,
            Some(&parse_mappings(crate::actuator::tests::MAPPINGS)),
        );
        assert!(rows[0].live && !rows[1].live);
        let extra: Vec<(&str, &str, &str)> = rows[2..]
            .iter()
            .map(|r| (r.method.as_str(), r.path.as_str(), r.handler.as_str()))
            .collect();
        assert_eq!(
            extra,
            [
                ("GET", "/actuator/health", "Actuator web endpoint 'health'"),
                ("ANY", "/error", "BasicErrorController.error")
            ]
        );
        assert_eq!(endpoint_rows(&statics, None).len(), 2);
    }

    #[test]
    fn merges_runtime_beans() {
        let bean = Bean {
            name: "orderService".into(),
            type_name: "com.acme.shop.orders.OrderService".into(),
            kind: "service".into(),
            framework: "spring".into(),
            scope: None,
            profiles: Vec::new(),
            primary: false,
            qualifier: None,
            location: Location {
                path: "OrderService.kt".into(),
                line: 1,
                column: 0,
            },
            declared_in: None,
            injected_into: vec![0, 1],
        };
        let rows = bean_rows(&[bean], Some(&parse_beans(crate::actuator::tests::BEANS)));
        assert_eq!(rows.len(), 2);
        assert!(rows[0].live);
        assert_eq!(rows[0].usages, 2);
        assert_eq!(rows[0].dependencies, ["orderRepository", "notifiers"]);
        assert_eq!(
            (rows[1].name.as_str(), rows[1].kind.as_str()),
            ("jacksonObjectMapper", "runtime")
        );
    }

    #[test]
    fn http_requests() {
        let get = EndpointRow::from(&ep(
            "GET",
            "/api/orders/{id}",
            vec![
                param("id", "path", "Long"),
                param("expand", "query", "String"),
            ],
        ));
        assert_eq!(
            http_request(&get, "http://localhost:8080/shop/"),
            "@id = 1\n\n### OrderController.find\nGET http://localhost:8080/shop/api/orders/{{id}}?expand=\nAccept: application/json\n"
        );
        let mut post = EndpointRow::from(&ep(
            "POST",
            "/rest/customers/{name: [a-z]+}",
            vec![
                param("name", "path", "String"),
                param("c", "body", "Customer"),
                param("X-Tenant", "header", "String"),
            ],
        ));
        post.produces = vec!["application/xml".into()];
        assert_eq!(
            http_request(&post, "http://localhost:9080"),
            "@name = value\n\n### OrderController.find\nPOST http://localhost:9080/rest/customers/{{name}}\nAccept: application/xml\nX-Tenant: \nContent-Type: application/json\n\n{\n}\n"
        );
        let any = EndpointRow::from(&ep("ANY", "/", vec![]));
        assert!(
            http_request(&any, "http://localhost:8080").contains("GET http://localhost:8080/\n")
        );
    }
}
