//! The run dashboard: Spring Boot run configurations and their running applications as
//! nodes and details of the Services tool window (the same shapes as
//! `forge-containers`' tree), with port, profiles, startup time and the actuator's health,
//! request mappings and beans.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::actuator::Snapshot;
use crate::run::Observed;

/// A Spring Boot configuration and its run.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardApp {
    /// The run configuration's name.
    pub name: String,
    pub main_class: String,
    /// The profiles the configuration activates.
    #[serde(default)]
    pub profiles: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    pub running: bool,
    #[serde(default)]
    pub debug: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub observed: Observed,
    /// `server.port` of the configuration files / overrides (before the console says).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub configured_port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub configured_context_path: Option<String>,
    /// The latest health status (`UP`, `DOWN`), when the actuator answered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health: Option<String>,
}

impl DashboardApp {
    pub fn port(&self) -> Option<u16> {
        self.observed.port.or(self.configured_port)
    }

    pub fn context_path(&self) -> String {
        self.observed
            .context_path
            .clone()
            .or_else(|| self.configured_context_path.clone())
            .unwrap_or_default()
    }

    /// `http://localhost:8080/shop`.
    pub fn url(&self) -> Option<String> {
        self.port()
            .map(|p| format!("http://localhost:{p}{}", self.context_path()))
    }

    /// `running`, `busy` (starting), `error` (failed, health down, exit code) or `stopped`.
    pub fn status(&self) -> &'static str {
        if self.running {
            if self.observed.failed || self.health.as_deref() == Some("DOWN") {
                "error"
            } else if self.observed.started {
                "running"
            } else {
                "busy"
            }
        } else if self.observed.failed
            || self
                .exit_code
                .is_some_and(|c| c != 0 && c != 130 && c != 143)
        {
            "error"
        } else {
            "stopped"
        }
    }

    fn profiles_shown(&self) -> Vec<String> {
        if !self.observed.profiles.is_empty() {
            self.observed.profiles.clone()
        } else {
            self.profiles.clone()
        }
    }
}

fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b' ') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Node id of an application.
pub fn node_id(name: &str) -> String {
    format!("spring/app/{}", encode(name))
}

/// The configuration name of an application node id.
pub fn app_name(node_id: &str) -> Option<String> {
    let enc = node_id.strip_prefix("spring/app/")?;
    let bytes = enc.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() + 1 {
            if let Ok(b) = u8::from_str_radix(enc.get(i + 1..i + 3)?, 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).ok()
}

fn app_node(a: &DashboardApp) -> Value {
    let mut detail = Vec::new();
    if a.running {
        if let Some(p) = a.port() {
            detail.push(format!(":{p}"));
        }
    }
    let profiles = a.profiles_shown();
    if !profiles.is_empty() && profiles != ["default"] {
        detail.push(profiles.join(", "));
    }
    if a.debug && a.running {
        detail.push("debug".into());
    }
    json!({
        "id": node_id(&a.name),
        "label": a.name,
        "detail": detail.join(" · "),
        "icon": "springApp",
        "status": a.status(),
    })
}

/// The Services nodes: Spring Boot → Running / Not Started → applications. Empty without
/// Spring Boot configurations.
pub fn tree(apps: &[DashboardApp]) -> Vec<Value> {
    if apps.is_empty() {
        return Vec::new();
    }
    let running: Vec<Value> = apps.iter().filter(|a| a.running).map(app_node).collect();
    let stopped: Vec<Value> = apps.iter().filter(|a| !a.running).map(app_node).collect();
    let mut groups = Vec::new();
    if !running.is_empty() {
        groups.push(json!({ "id": "spring/running", "label": "Running", "icon": "folder", "children": running, "expanded": true }));
    }
    if !stopped.is_empty() {
        groups.push(json!({ "id": "spring/stopped", "label": "Not Started", "icon": "folder", "children": stopped, "expanded": true }));
    }
    vec![json!({
        "id": "spring",
        "label": "Spring Boot",
        "icon": "springBoot",
        "children": groups,
        "expanded": true,
    })]
}

fn health_status(s: &str) -> &'static str {
    match s {
        "UP" => "running",
        "DOWN" => "error",
        _ => "stopped",
    }
}

fn row(label: &str, value: impl Into<String>) -> Value {
    json!({ "label": label, "value": value.into() })
}

/// Details of a node: an application (with the actuator's answer, when it was asked) or a
/// group.
pub fn details(node_id: &str, apps: &[DashboardApp], snapshot: Option<&Snapshot>) -> Option<Value> {
    if let Some(name) = app_name(node_id) {
        let a = apps.iter().find(|a| a.name == name)?;
        return Some(app_details(a, snapshot));
    }
    let list: Vec<&DashboardApp> = match node_id {
        "spring" => apps.iter().collect(),
        "spring/running" => apps.iter().filter(|a| a.running).collect(),
        "spring/stopped" => apps.iter().filter(|a| !a.running).collect(),
        _ => return None,
    };
    let rows: Vec<Value> = list
        .iter()
        .map(|a| {
            json!([
                a.name,
                match a.status() {
                    "running" => "Running",
                    "busy" => "Starting…",
                    "error" => "Failed",
                    _ => "Not started",
                },
                if a.running {
                    a.port().map(|p| p.to_string()).unwrap_or_default()
                } else {
                    String::new()
                },
                a.profiles_shown().join(", "),
            ])
        })
        .collect();
    let status: Vec<&str> = list.iter().map(|a| a.status()).collect();
    Some(json!({
        "nodeId": node_id,
        "target": { "kind": "springGroup" },
        "title": match node_id { "spring/running" => "Running", "spring/stopped" => "Not Started", _ => "Spring Boot" },
        "icon": if node_id == "spring" { "springBoot" } else { "folder" },
        "tabs": ["dashboard"],
        "sections": [{ "title": "Applications", "table": { "columns": ["Configuration", "Status", "Port", "Profiles"], "rows": rows, "status": status } }],
        "actions": [],
    }))
}

fn app_details(a: &DashboardApp, snapshot: Option<&Snapshot>) -> Value {
    let status = a.status();
    let status_text = match status {
        "running" => "Running".to_string(),
        "busy" => "Starting…".to_string(),
        "error" if a.running => "Running, health DOWN".to_string(),
        "error" if a.observed.failed => "Failed to start".to_string(),
        "error" => format!("Exited with code {}", a.exit_code.unwrap_or(1)),
        _ => match a.exit_code {
            Some(c) => format!("Stopped (exit code {c})"),
            None => "Not started".to_string(),
        },
    };
    let mut app_rows = vec![json!({ "label": "Status", "value": status_text, "status": status })];
    app_rows.push(row("Main class", a.main_class.clone()));
    let profiles = a.profiles_shown();
    app_rows.push(row(
        "Active profiles",
        if profiles.is_empty() {
            "default".to_string()
        } else {
            profiles.join(", ")
        },
    ));
    if let (true, Some(url)) = (a.running, a.url()) {
        app_rows.push(json!({ "label": "URL", "value": "", "url": url }));
    } else if let Some(p) = a.port() {
        app_rows.push(row("Port", format!("{p} (from the configuration)")));
    }
    if let Some(s) = a.observed.startup_seconds {
        app_rows.push(row("Started in", format!("{s:.1} s")));
    }
    if let Some(pid) = a.observed.pid {
        app_rows.push(row("PID", pid.to_string()));
    }
    if let Some(server) = &a.observed.server {
        let mut s = server.clone();
        if let Some(f) = s.get_mut(0..1) {
            f.make_ascii_uppercase();
        }
        app_rows.push(row("Web server", s));
    }
    if let Some(v) = &a.observed.boot_version {
        app_rows.push(row("Spring Boot", v.clone()));
    }
    if a.debug && a.running {
        app_rows.push(row("Debugger", "Attached"));
    }
    let mut sections = vec![json!({ "title": "Application", "rows": app_rows })];
    let mut message = None;
    if let Some(snap) = snapshot.filter(|_| a.running) {
        if let Some(h) = &snap.health {
            let rows: Vec<Value> = h
                .components
                .iter()
                .map(|c| json!([c.name, c.status, c.details.clone().unwrap_or_default()]))
                .collect();
            let st: Vec<&str> = h
                .components
                .iter()
                .map(|c| health_status(&c.status))
                .collect();
            let mut s = json!({
                "title": "Health",
                "rows": [{ "label": "Status", "value": h.status, "status": health_status(&h.status), "url": format!("{}/health", snap.base_url) }],
            });
            if !rows.is_empty() {
                s["table"] = json!({ "columns": ["Component", "Status", "Details"], "rows": rows, "status": st });
            }
            sections.push(s);
        }
        if let Some(m) = &snap.mappings {
            let rows: Vec<Value> = m
                .iter()
                .take(200)
                .map(|x| {
                    let handler = match (&x.class, &x.method) {
                        (Some(c), Some(n)) => {
                            format!("{}.{n}()", c.rsplit('.').next().unwrap_or(c))
                        }
                        _ => x.handler.clone(),
                    };
                    json!([
                        if x.methods.is_empty() {
                            "ANY".to_string()
                        } else {
                            x.methods.join(", ")
                        },
                        x.patterns.join(", "),
                        handler
                    ])
                })
                .collect();
            sections.push(json!({ "title": format!("Request Mappings ({})", m.len()), "table": { "columns": ["Method", "Path", "Handler"], "rows": rows } }));
        }
        if let Some(b) = &snap.beans {
            let mut list: Vec<&crate::actuator::RuntimeBean> = b.iter().collect();
            // Application beans before the framework's.
            list.sort_by_key(|x| {
                (
                    x.type_name.starts_with("org.springframework") || x.type_name.is_empty(),
                    x.name.clone(),
                )
            });
            let rows: Vec<Value> = list
                .iter()
                .take(200)
                .map(|x| {
                    json!([
                        x.name,
                        x.type_name,
                        x.scope.clone().unwrap_or_default(),
                        x.dependencies.join(", ")
                    ])
                })
                .collect();
            sections.push(json!({ "title": format!("Beans ({})", b.len()), "table": { "columns": ["Bean", "Type", "Scope", "Dependencies"], "rows": rows } }));
        }
        if !snap.problems.is_empty() {
            let none = snap.health.is_none() && snap.beans.is_none() && snap.mappings.is_none();
            message = Some(if none && a.observed.started {
                format!(
                    "The actuator did not answer at {} ({}).\nAdd spring-boot-starter-actuator to the build to see health, beans and request mappings here.",
                    snap.base_url,
                    snap.problems.join("; ")
                )
            } else if none {
                "Waiting for the application to start…".to_string()
            } else {
                format!("Actuator: {}", snap.problems.join("; "))
            });
        }
    }
    let actions = if a.running {
        json!(["stop", "restart"])
    } else {
        json!(["start"])
    };
    let mut out = json!({
        "nodeId": node_id(&a.name),
        "target": { "kind": "springApp", "name": a.name, "runId": a.run_id.clone().unwrap_or_default() },
        "title": a.name,
        "icon": "springApp",
        "status": status,
        "subtitle": a.main_class,
        "tabs": ["dashboard"],
        "sections": sections,
        "actions": actions,
    });
    if let Some(m) = message {
        out["message"] = json!(m);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actuator::{parse_beans, parse_health, parse_mappings};

    fn app(name: &str, running: bool) -> DashboardApp {
        DashboardApp {
            name: name.into(),
            main_class: "com.acme.ShopApplication".into(),
            profiles: vec!["dev".into()],
            running,
            configured_port: Some(8080),
            ..DashboardApp::default()
        }
    }

    #[test]
    fn tree_groups_running_and_stopped_apps() {
        assert!(tree(&[]).is_empty());
        let mut a = app("ShopApplication", true);
        a.observed.started = true;
        a.observed.port = Some(8081);
        let b = app("Shop (prod)", false);
        let t = tree(&[a, b]);
        assert_eq!(t[0]["id"], "spring");
        let groups = t[0]["children"].as_array().unwrap();
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0]["children"][0]["detail"], ":8081 · dev");
        assert_eq!(groups[0]["children"][0]["status"], "running");
        assert_eq!(groups[1]["children"][0]["id"], "spring/app/Shop %28prod%29");
        assert_eq!(
            app_name("spring/app/Shop %28prod%29").as_deref(),
            Some("Shop (prod)")
        );
    }

    #[test]
    fn statuses() {
        let mut a = app("A", true);
        assert_eq!(a.status(), "busy");
        a.observed.started = true;
        assert_eq!(a.status(), "running");
        a.health = Some("DOWN".into());
        assert_eq!(a.status(), "error");
        let mut s = app("S", false);
        assert_eq!(s.status(), "stopped");
        s.exit_code = Some(143);
        assert_eq!(s.status(), "stopped");
        s.exit_code = Some(1);
        assert_eq!(s.status(), "error");
    }

    #[test]
    fn details_with_actuator_data() {
        let mut a = app("ShopApplication", true);
        a.observed = Observed {
            port: Some(8080),
            context_path: Some("/shop".into()),
            started: true,
            startup_seconds: Some(2.345),
            pid: Some(4711),
            server: Some("tomcat".into()),
            profiles: vec!["dev".into(), "local".into()],
            ..Observed::default()
        };
        let snap = Snapshot {
            base_url: "http://localhost:8080/shop/actuator".into(),
            health: parse_health(crate::actuator::tests::HEALTH),
            beans: Some(parse_beans(crate::actuator::tests::BEANS)),
            mappings: Some(parse_mappings(crate::actuator::tests::MAPPINGS)),
            problems: Vec::new(),
        };
        let d = details(&node_id("ShopApplication"), &[a.clone()], Some(&snap)).unwrap();
        assert_eq!(d["actions"], json!(["stop", "restart"]));
        let sections = d["sections"].as_array().unwrap();
        let titles: Vec<&str> = sections
            .iter()
            .map(|s| s["title"].as_str().unwrap())
            .collect();
        assert_eq!(
            titles,
            ["Application", "Health", "Request Mappings (3)", "Beans (2)"]
        );
        let rows = sections[0]["rows"].as_array().unwrap();
        assert!(rows
            .iter()
            .any(|r| r["url"] == "http://localhost:8080/shop"));
        assert!(rows
            .iter()
            .any(|r| r["label"] == "Active profiles" && r["value"] == "dev, local"));
        assert!(rows.iter().any(|r| r["value"] == "2.3 s"));
        assert_eq!(sections[1]["table"]["status"][3], "error");
        assert_eq!(
            sections[2]["table"]["rows"][0],
            json!(["GET", "/api/orders/{id}", "OrderController.find()"])
        );
        assert_eq!(sections[3]["table"]["rows"][1][0], "orderService");

        // No actuator in the build.
        let none = Snapshot {
            base_url: "http://localhost:8080/actuator".into(),
            problems: vec!["health: not exposed (404)".into()],
            ..Snapshot::default()
        };
        let d = details(&node_id("ShopApplication"), &[a], Some(&none)).unwrap();
        assert!(d["message"]
            .as_str()
            .unwrap()
            .contains("spring-boot-starter-actuator"));
        let stopped = details(&node_id("Other"), &[app("Other", false)], None).unwrap();
        assert_eq!(stopped["actions"], json!(["start"]));
        let group = details("spring", &[app("Other", false)], None).unwrap();
        assert_eq!(group["sections"][0]["table"]["rows"][0][1], "Not started");
        assert!(details("spring/app/Missing", &[], None).is_none());
    }
}
