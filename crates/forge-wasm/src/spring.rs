//! Spring Boot and Jakarta EE (`javaforge-fw-spring`) for the browser-mode mocks, mirroring
//! the `spring_*` IPC commands: the project model and its tool window rows (with a running
//! application's actuator answers), gutter markers, configuration file completion,
//! inspections, hover and navigation with canned metadata, HTTP Client requests, the run
//! configuration's command line and the run dashboard.

use std::collections::BTreeMap;

use javaforge_fw_spring::actuator::{self, Snapshot};
use javaforge_fw_spring::assist::{self, Context};
use javaforge_fw_spring::dashboard::{self, DashboardApp};
use javaforge_fw_spring::metadata::Metadata;
use javaforge_fw_spring::model::{self, Model};
use javaforge_fw_spring::run::{self, Observed};
use javaforge_fw_spring::views::{self, EndpointRow};
use serde::Deserialize;
use serde_json::json;
use wasm_bindgen::prelude::*;

use crate::to_json;

fn error(e: impl ToString) -> String {
    to_json(&json!({ "error": e.to_string() }))
}

fn parse<T: for<'de> Deserialize<'de>>(json: &str) -> Result<T, String> {
    serde_json::from_str(json).map_err(|e| e.to_string())
}

/// What the actuator of the demo's running application answered (JSON texts).
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Runtime {
    #[serde(default)]
    base_url: String,
    #[serde(default)]
    health: Option<String>,
    #[serde(default)]
    beans: Option<String>,
    #[serde(default)]
    mappings: Option<String>,
}

impl Runtime {
    fn snapshot(&self) -> Snapshot {
        Snapshot {
            base_url: self.base_url.clone(),
            health: self.health.as_deref().and_then(actuator::parse_health),
            beans: self.beans.as_deref().map(actuator::parse_beans),
            mappings: self.mappings.as_deref().map(actuator::parse_mappings),
            problems: Vec::new(),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectRequest {
    /// (path, text) pairs.
    files: Vec<(String, String)>,
    /// `spring-configuration-metadata.json` texts (the "jars").
    #[serde(default)]
    metadata: Vec<(String, String)>,
    /// Enum constants of library types the canned metadata uses.
    #[serde(default)]
    enums: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    runtime: Option<Runtime>,
    #[serde(default)]
    path: String,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    offset: usize,
}

impl ProjectRequest {
    fn model(&self) -> Model {
        let mut files = self.files.clone();
        if let Some(c) = &self.content {
            match files.iter_mut().find(|(p, _)| *p == self.path) {
                Some(f) => f.1 = c.clone(),
                None => files.push((self.path.clone(), c.clone())),
            }
        }
        model::analyze(&files)
    }

    fn metadata(&self, model: &Model) -> Metadata {
        let mut m = Metadata::default();
        for (name, json) in &self.metadata {
            let _ = m.add_json(json, name);
        }
        m.enums.extend(self.enums.clone());
        for p in model.metadata.properties.values() {
            m.add_property(p.clone());
        }
        for (k, v) in &model.enums {
            m.enums.entry(k.clone()).or_insert_with(|| v.clone());
        }
        if !model.metadata.is_empty() {
            m.sources.push("project".into());
        }
        m
    }
}

fn profiles(model: &Model) -> Vec<String> {
    let mut p = model.profiles.clone();
    if !p.iter().any(|x| x == "default") {
        p.push("default".into());
    }
    p
}

/// `{ files, metadata, enums, runtime? }` → the `spring_model` answer.
#[wasm_bindgen]
pub fn spring_overview(request_json: &str) -> String {
    let r = match parse::<ProjectRequest>(request_json) {
        Ok(r) => r,
        Err(e) => return error(e),
    };
    let model = r.model();
    let meta = r.metadata(&model);
    let snap = r.runtime.as_ref().map(Runtime::snapshot);
    to_json(&json!({
        "spring": model.spring,
        "jakarta": model.jakarta,
        "beans": views::bean_rows(&model.beans, snap.as_ref().and_then(|s| s.beans.as_deref())),
        "endpoints": views::endpoint_rows(&model.endpoints, snap.as_ref().and_then(|s| s.mappings.as_deref())),
        "entities": model.entities,
        "configClasses": model.config_classes,
        "applications": model.applications,
        "apis": model.apis,
        "profiles": profiles(&model),
        "configFiles": model.config_files,
        "metadata": { "properties": meta.properties.len(), "sources": meta.sources, "problems": [] },
    }))
}

/// `{ files, path, content }` → `Marker[]` (`spring_gutter`).
#[wasm_bindgen]
pub fn spring_gutter(request_json: &str) -> String {
    match parse::<ProjectRequest>(request_json) {
        Ok(r) => to_json(&model::gutter(&r.model(), &r.path)),
        Err(e) => error(e),
    }
}

/// `{ files, metadata, enums, path, content, offset }` plus `kind`: `complete` | `inspect`
/// | `hover` | `goto` (the `spring_config_*` commands).
#[wasm_bindgen]
pub fn spring_config(kind: &str, request_json: &str) -> String {
    let r = match parse::<ProjectRequest>(request_json) {
        Ok(r) => r,
        Err(e) => return error(e),
    };
    let model = r.model();
    let meta = r.metadata(&model);
    let profiles = profiles(&model);
    let ctx = Context {
        metadata: &meta,
        profiles: &profiles,
    };
    let content = r.content.clone().unwrap_or_default();
    match kind {
        "complete" => to_json(&assist::complete(&r.path, &content, r.offset, &ctx)),
        "inspect" => to_json(&assist::inspect(&r.path, &content, &ctx)),
        "hover" => to_json(&assist::hover(&r.path, &content, r.offset, &ctx)),
        "goto" => to_json(&assist::goto(&r.path, &content, r.offset, &ctx, &|c| {
            model.classes.get(c).cloned()
        })),
        other => error(format!("unknown kind {other}")),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HttpRequest {
    endpoint: EndpointRow,
    base_url: String,
}

/// `{ endpoint, baseUrl }` → `.http` text (`spring_http_request`).
#[wasm_bindgen]
pub fn spring_http_request(request_json: &str) -> String {
    match parse::<HttpRequest>(request_json) {
        Ok(r) => to_json(&views::http_request(&r.endpoint, &r.base_url)),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LaunchRequest {
    #[serde(default)]
    options: BTreeMap<String, String>,
    #[serde(default)]
    vm_options: String,
    #[serde(default)]
    arguments: String,
}

/// `{ options, vmOptions, arguments }` → `{ vmOptions, arguments }` of the Application run.
#[wasm_bindgen]
pub fn spring_launch(request_json: &str) -> String {
    match parse::<LaunchRequest>(request_json) {
        Ok(r) => to_json(&run::launch(&r.options, &r.vm_options, &r.arguments)),
        Err(e) => error(e),
    }
}

/// Console lines → what the dashboard reads from them (`Observed`).
#[wasm_bindgen]
pub fn spring_observe(lines_json: &str) -> String {
    match parse::<Vec<String>>(lines_json) {
        Ok(lines) => {
            let mut o = Observed::default();
            for l in &lines {
                o.observe(l);
            }
            to_json(&o)
        }
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DashboardRequest {
    apps: Vec<DashboardApp>,
    #[serde(default)]
    node_id: Option<String>,
    #[serde(default)]
    runtime: Option<Runtime>,
}

/// `{ apps }` → nodes; `{ apps, nodeId, runtime? }` → details (`spring_dashboard*`).
#[wasm_bindgen]
pub fn spring_dashboard(request_json: &str) -> String {
    let r = match parse::<DashboardRequest>(request_json) {
        Ok(r) => r,
        Err(e) => return error(e),
    };
    match &r.node_id {
        None => to_json(&dashboard::tree(&r.apps)),
        Some(id) => {
            let snap = r.runtime.as_ref().map(Runtime::snapshot);
            to_json(&dashboard::details(id, &r.apps, snap.as_ref()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridges_the_spring_model() {
        let files = json!({
            "files": [
                ["src/main/java/com/acme/Api.java", "package com.acme;\n@RestController\n@RequestMapping(\"/api\")\nclass Api { @GetMapping(\"/x\") String x() { return \"\"; } }"],
                ["src/main/resources/application.properties", "server.port=8081\nserver.prot=1\n"]
            ],
            "metadata": [["boot.jar", "{\"properties\":[{\"name\":\"server.port\",\"type\":\"java.lang.Integer\"}]}"]],
            "path": "src/main/resources/application.properties",
            "content": "server.port=8081\nserver.prot=1\n",
            "offset": 3
        })
        .to_string();
        let o: serde_json::Value = serde_json::from_str(&spring_overview(&files)).unwrap();
        assert_eq!(o["endpoints"][0]["path"], "/api/x");
        assert_eq!(o["metadata"]["properties"], 1);
        let problems: serde_json::Value =
            serde_json::from_str(&spring_config("inspect", &files)).unwrap();
        assert_eq!(problems.as_array().unwrap().len(), 1);
        let hover: serde_json::Value =
            serde_json::from_str(&spring_config("hover", &files)).unwrap();
        assert!(hover["markdown"].as_str().unwrap().contains("server.port"));
        let gutter: serde_json::Value = serde_json::from_str(&spring_gutter(
            &json!({ "files": [["A.java", "@Service class A {}"]], "path": "A.java" }).to_string(),
        ))
        .unwrap();
        assert_eq!(gutter[0]["kind"], "bean");
        let launch: serde_json::Value =
            serde_json::from_str(&spring_launch(r#"{"options":{"activeProfiles":"dev"}}"#))
                .unwrap();
        assert!(launch["vmOptions"]
            .as_str()
            .unwrap()
            .contains("-Dspring.profiles.active=dev"));
        let observed: serde_json::Value = serde_json::from_str(&spring_observe(
            r#"["Tomcat started on port 8080 (http) with context path '/'"]"#,
        ))
        .unwrap();
        assert_eq!(observed["port"], 8080);
        let tree: serde_json::Value = serde_json::from_str(&spring_dashboard(
            r#"{"apps":[{"name":"App","mainClass":"a.App","running":false}]}"#,
        ))
        .unwrap();
        assert_eq!(tree[0]["id"], "spring");
        assert!(spring_config("nope", &files).contains("error"));
    }
}
