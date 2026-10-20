//! Services tool window (`forge-containers`) for the browser-mode mock: the mock keeps a fake
//! Docker daemon as Engine API JSON, compose files and a kubeconfig as text, and asks Rust
//! for the models, the tree, the details pane, the gutter markers, command lines and pull
//! progress, like the desktop app's hub.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};

use forge_containers::compose::{self, ComposeCommand};
use forge_containers::docker::RawSnapshot;
use forge_containers::endpoint;
use forge_containers::exec;
use forge_containers::gutter;
use forge_containers::kube::{self, RawWorkloads};
use forge_containers::logs;
use forge_containers::portforward;
use forge_containers::pull::PullProgress;
use forge_containers::tree::{self, ServicesInput};
use serde::Deserialize;
use wasm_bindgen::prelude::*;

use crate::parsers::parse;
use crate::to_json;

thread_local! {
    static PULLS: RefCell<(u32, HashMap<u32, PullProgress>)> = RefCell::new((0, HashMap::new()));
}

fn error(e: impl std::fmt::Display) -> String {
    to_json(&serde_json::json!({ "error": e.to_string() }))
}

/// Engine API responses (`RawSnapshot`) → `DockerSnapshot`.
#[wasm_bindgen]
pub fn containers_docker_snapshot(raw_json: &str) -> String {
    match parse::<RawSnapshot>(raw_json) {
        Ok(raw) => to_json(&raw.parse()),
        Err(e) => e,
    }
}

#[derive(Deserialize)]
struct ComposeRequest {
    path: String,
    content: String,
    #[serde(default)]
    env: BTreeMap<String, String>,
    /// Project name from the directory (the mock knows the project root's name).
    #[serde(default)]
    project: Option<String>,
}

/// `{ path, content, env?, project? }` → `ComposeFile`.
#[wasm_bindgen]
pub fn containers_compose_parse(request_json: &str) -> String {
    match parse::<ComposeRequest>(request_json) {
        Ok(r) => {
            let mut f = compose::parse(&r.path, &r.content, &r.env);
            if let Some(p) = r
                .project
                .filter(|_| !r.content.lines().any(|l| l.starts_with("name:")))
            {
                f.project = compose::normalize_project_name(&p);
            }
            to_json(&f)
        }
        Err(e) => e,
    }
}

/// `ComposeCommand` → arguments after `docker`.
#[wasm_bindgen]
pub fn containers_compose_args(request_json: &str) -> String {
    match parse::<ComposeCommand>(request_json) {
        Ok(c) => to_json(&compose::compose_args(&c)),
        Err(e) => e,
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct KubeconfigRequest {
    content: String,
    #[serde(default)]
    base_dir: String,
}

/// `{ content, baseDir }` → `KubeConfig`.
#[wasm_bindgen]
pub fn containers_kubeconfig_parse(request_json: &str) -> String {
    match parse::<KubeconfigRequest>(request_json) {
        Ok(r) => match kube::parse_kubeconfig(&r.content, &r.base_dir) {
            Ok(c) => to_json(&c),
            Err(e) => error(e),
        },
        Err(e) => e,
    }
}

/// `RawWorkloads` (API lists) → `Workloads`.
#[wasm_bindgen]
pub fn containers_workloads_parse(raw_json: &str) -> String {
    match parse::<RawWorkloads>(raw_json) {
        Ok(raw) => to_json(&raw.parse()),
        Err(e) => e,
    }
}

/// `ServicesInput` → `Node[]`, mirroring `services_tree`.
#[wasm_bindgen]
pub fn services_tree(input_json: &str) -> String {
    match parse::<ServicesInput>(input_json) {
        Ok(input) => to_json(&tree::build_tree(&input)),
        Err(e) => e,
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DetailsRequest {
    input: ServicesInput,
    node_id: String,
}

/// `{ input, nodeId }` → `Details | null`, mirroring `services_details`.
#[wasm_bindgen]
pub fn services_details(request_json: &str) -> String {
    match parse::<DetailsRequest>(request_json) {
        Ok(r) => {
            let mut d = tree::details(&r.input, &r.node_id);
            if let Some(d) = d.as_mut().filter(|d| d.target == tree::Target::Docker) {
                if !d.tabs.contains(&tree::Tab::Log) {
                    d.tabs.push(tree::Tab::Log);
                }
            }
            to_json(&d)
        }
        Err(e) => e,
    }
}

#[derive(Deserialize)]
struct GutterRequest {
    path: String,
    content: String,
}

/// `{ path, content }` → `GutterMarker[]`, mirroring `services_gutter`.
#[wasm_bindgen]
pub fn services_gutter(request_json: &str) -> String {
    match parse::<GutterRequest>(request_json) {
        Ok(r) => to_json(&gutter::markers(&r.path, &r.content)),
        Err(e) => e,
    }
}

#[derive(Deserialize)]
struct DockerfileRequest {
    path: String,
    project: String,
    content: String,
}

/// `{ path, project, content }` → `DockerfileRun`.
#[wasm_bindgen]
pub fn containers_dockerfile_plan(request_json: &str) -> String {
    match parse::<DockerfileRequest>(request_json) {
        Ok(r) => to_json(&gutter::dockerfile_run(&r.path, &r.project, &r.content)),
        Err(e) => e,
    }
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum TerminalRequest {
    Exec {
        container: String,
        name: String,
    },
    Attach {
        container: String,
        name: String,
    },
    #[serde(rename_all = "camelCase")]
    Kubectl {
        context: String,
        namespace: String,
        pod: String,
        kubeconfig: Option<String>,
        container: Option<String>,
        #[serde(default)]
        attach: bool,
    },
}

/// Exec / attach / kubectl exec command lines (`TerminalCommand`), mirroring
/// `services_terminal`.
#[wasm_bindgen]
pub fn containers_terminal_command(request_json: &str) -> String {
    match parse::<TerminalRequest>(request_json) {
        Ok(TerminalRequest::Exec { container, name }) => {
            to_json(&exec::docker_exec("docker", &container, &name, Vec::new()))
        }
        Ok(TerminalRequest::Attach { container, name }) => to_json(&exec::docker_attach(
            "docker",
            &container,
            &name,
            Vec::new(),
        )),
        Ok(TerminalRequest::Kubectl {
            context,
            namespace,
            pod,
            kubeconfig,
            container,
            attach,
        }) => {
            let make = if attach {
                exec::kubectl_attach
            } else {
                exec::kubectl_exec
            };
            to_json(&make(
                "kubectl",
                kubeconfig.as_deref(),
                &context,
                &namespace,
                &pod,
                container.as_deref(),
            ))
        }
        Err(e) => e,
    }
}

#[derive(Deserialize)]
struct SpecRequest {
    spec: String,
}

/// `{ spec }` → `[local, remote][]` (Forward Ports), or an error.
#[wasm_bindgen]
pub fn containers_port_forward_specs(request_json: &str) -> String {
    match parse::<SpecRequest>(request_json) {
        Ok(r) => match portforward::parse_specs(&r.spec) {
            Ok(specs) => to_json(&specs),
            Err(e) => error(&e.0),
        },
        Err(e) => e,
    }
}

#[derive(Deserialize)]
struct LogPrefixRequest {
    pod: String,
    container: String,
    text: String,
}

/// `{ pod, container, text }` → the text with every line prefixed like
/// `kubectl logs --prefix` (deployment logs).
#[wasm_bindgen]
pub fn containers_prefix_log(request_json: &str) -> String {
    match parse::<LogPrefixRequest>(request_json) {
        Ok(r) => {
            let mut p = logs::LinePrefixer::new(logs::pod_log_prefix(&r.pod, &r.container));
            let mut out = p.push(&r.text);
            out.push_str(&p.finish());
            to_json(&out)
        }
        Err(e) => e,
    }
}

#[derive(Deserialize)]
struct ContextsRequest {
    metas: Vec<String>,
    current: Option<String>,
}

/// `{ metas: [meta.json text], current }` → `DockerContext[]` (the mock's
/// `~/.docker/contexts`).
#[wasm_bindgen]
pub fn containers_docker_contexts(request_json: &str) -> String {
    match parse::<ContextsRequest>(request_json) {
        Ok(r) => {
            let mut out: Vec<endpoint::DockerContext> = r
                .metas
                .iter()
                .filter_map(|m| endpoint::parse_context_meta(m))
                .map(|mut c| {
                    c.current = r.current.as_deref() == Some(c.name.as_str());
                    c
                })
                .collect();
            out.sort_by(|a, b| a.name.cmp(&b.name));
            to_json(&out)
        }
        Err(e) => e,
    }
}

#[derive(Deserialize)]
struct PullNew {
    image: String,
}

/// `{ image }` → handle for [`containers_pull_push`].
#[wasm_bindgen]
pub fn containers_pull_new(request_json: &str) -> String {
    match parse::<PullNew>(request_json) {
        Ok(r) => PULLS.with(|p| {
            let mut p = p.borrow_mut();
            p.0 += 1;
            let id = p.0;
            p.1.insert(id, PullProgress::new(&r.image));
            to_json(&id)
        }),
        Err(e) => e,
    }
}

#[derive(Deserialize)]
struct PullPush {
    handle: u32,
    /// Body text; `null` ends the pull.
    text: Option<String>,
}

/// `{ handle, text }` → `{ lines, state }`; `text: null` finishes and drops the handle.
#[wasm_bindgen]
pub fn containers_pull_push(request_json: &str) -> String {
    match parse::<PullPush>(request_json) {
        Ok(r) => PULLS.with(|p| {
            let mut p = p.borrow_mut();
            let Some(progress) = p.1.get_mut(&r.handle) else {
                return error("unknown pull");
            };
            let lines = match &r.text {
                Some(t) => progress.push(t),
                None => progress.finish(),
            };
            let out = to_json(&serde_json::json!({ "lines": lines, "state": progress.state() }));
            if r.text.is_none() {
                p.1.remove(&r.handle);
            }
            out
        }),
        Err(e) => e,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_from_raw_responses() {
        let snap = containers_docker_snapshot(
            r#"{"containers":[{"Id":"a","Names":["/web"],"State":"running"}],"images":[],"volumes":{"Volumes":[]},"networks":[]}"#,
        );
        let input = format!(
            r#"{{"docker":{{"state":"connected","endpoint":"unix:///var/run/docker.sock","snapshot":{snap}}},"now":0}}"#
        );
        let tree = services_tree(&input);
        assert!(tree.contains("\"label\":\"web\""), "{tree}");
        let d = services_details(&format!(r#"{{"input":{input},"nodeId":"docker"}}"#));
        assert!(d.contains("\"log\""), "{d}");
        let compose = containers_compose_parse(
            r#"{"path":"docker-compose.yml","content":"services:\n  app:\n    image: x\n","project":"Acme Shop"}"#,
        );
        assert!(compose.contains("\"project\":\"acmeshop\""), "{compose}");
        let g = services_gutter(r#"{"path":"Dockerfile","content":"FROM php\n"}"#);
        assert!(g.contains("dockerfile"));
        let t = containers_terminal_command(r#"{"kind":"exec","container":"a","name":"web"}"#);
        assert!(t.contains("\"exec\""));
        let t = containers_terminal_command(
            r#"{"kind":"kubectl","context":"k","namespace":"n","pod":"p","container":"c","attach":true}"#,
        );
        assert!(t.contains("p [c] (attached)"), "{t}");
        assert_eq!(
            containers_port_forward_specs(r#"{"spec":"8080:80, 9000"}"#),
            "[[8080,80],[9000,9000]]"
        );
        assert!(containers_port_forward_specs(r#"{"spec":"x"}"#).contains("error"));
        let p = containers_prefix_log(r#"{"pod":"web-1","container":"php","text":"a\nb"}"#);
        assert!(p.contains("[pod/web-1/php] b"), "{p}");
        let meta = r#"{\"Name\":\"b\",\"Endpoints\":{\"docker\":{\"Host\":\"ssh://x\"}}}"#;
        let c = containers_docker_contexts(&format!(r#"{{"metas":["{meta}"],"current":"b"}}"#));
        assert!(c.contains("\"current\":true"), "{c}");
        let h = containers_pull_new(r#"{"image":"redis"}"#);
        let out = containers_pull_push(&format!(
            r#"{{"handle":{h},"text":"{{\"status\":\"Pulling fs layer\",\"id\":\"l1\"}}\n"}}"#
        ));
        assert!(out.contains("l1: Pulling fs layer"), "{out}");
        let done = containers_pull_push(&format!(r#"{{"handle":{h},"text":null}}"#));
        assert!(done.contains("\"done\":true"));
    }
}
