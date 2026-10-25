//! Kubernetes through `kubectl` on a remote project's host. A remote project's clusters are
//! often only reachable from its host (a k3s on `127.0.0.1:6443`, a private network, exec
//! credential plugins installed there), so Forge runs the host's `kubectl` with the host's
//! kubeconfig (`$KUBECONFIG` or `~/.kube/config`) instead of calling the API server from
//! this machine. The command lines and the parsing of kubectl's output are pure; the
//! [`KubectlClient`] (feature `runtime`) runs them through the project's backend.
//!
//! kubectl prints the same JSON as the API server (`kubectl get pods -o json` is a `List`
//! with `items`), so [`crate::kube`] parses both.

use serde::{Deserialize, Serialize};

use crate::kube::{self, KubeConfig};
use crate::Result;

/// The kubectl of a host: `kubernetes.hostKubectlPaths` has one `host<TAB>path` line per
/// host (`deploy@shop.example.com`, the project's `user@host[:port]` label); other hosts use
/// `kubectl` from their `PATH`.
pub fn kubectl_for_host(lines: &[String], host: &str) -> String {
    lines
        .iter()
        .filter_map(|l| l.split_once('\t'))
        .find(|(h, _)| h.trim() == host)
        .map(|(_, p)| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| "kubectl".into())
}

/// `--context <ctx> [-n <ns>]`, the prefix of every command that targets a cluster.
fn target(context: &str, namespace: Option<&str>) -> Vec<String> {
    let mut args = vec!["--context".to_string(), context.to_string()];
    if let Some(ns) = namespace {
        args.push("-n".into());
        args.push(ns.into());
    }
    args
}

/// `kubectl config view -o json`: the merged kubeconfig of `$KUBECONFIG` /
/// `~/.kube/config` without secrets (kubectl redacts them; it authenticates itself).
pub fn config_view_args() -> Vec<String> {
    ["config", "view", "-o", "json"].map(String::from).to_vec()
}

/// `kubectl --context <ctx> get namespaces -o json`.
pub fn namespaces_args(context: &str) -> Vec<String> {
    let mut a = target(context, None);
    a.extend(["get", "namespaces", "-o", "json"].map(String::from));
    a
}

/// `kubectl --context <ctx> -n <ns> get <resource> [<name>] [-l <selector>] -o json`.
pub fn get_args(
    context: &str,
    namespace: &str,
    resource: &str,
    name: Option<&str>,
    selector: Option<&str>,
) -> Vec<String> {
    let mut a = target(context, Some(namespace));
    a.push("get".into());
    a.push(resource.into());
    if let Some(n) = name {
        a.push(n.into());
    }
    if let Some(s) = selector {
        a.push("-l".into());
        a.push(s.into());
    }
    a.extend(["-o", "json"].map(String::from));
    a
}

/// `kubectl logs -f --tail=<n> <pod> [-c <container>]`.
pub fn logs_args(
    context: &str,
    namespace: &str,
    pod: &str,
    container: Option<&str>,
    tail: u32,
) -> Vec<String> {
    let mut a = target(context, Some(namespace));
    a.extend([
        "logs".into(),
        "-f".into(),
        format!("--tail={tail}"),
        pod.into(),
    ]);
    if let Some(c) = container {
        a.push("-c".into());
        a.push(c.into());
    }
    a
}

/// `kubectl delete pod <name> --wait=false`.
pub fn delete_pod_args(context: &str, namespace: &str, pod: &str) -> Vec<String> {
    let mut a = target(context, Some(namespace));
    a.extend([
        "delete".into(),
        "pod".into(),
        pod.into(),
        "--wait=false".into(),
    ]);
    a
}

/// `kubectl rollout restart deployment/<name>`.
pub fn rollout_restart_args(context: &str, namespace: &str, deployment: &str) -> Vec<String> {
    let mut a = target(context, Some(namespace));
    a.extend([
        "rollout".into(),
        "restart".into(),
        format!("deployment/{deployment}"),
    ]);
    a
}

/// `kubectl port-forward --address 127.0.0.1 pod/<name> :<port>`: kubectl picks a free
/// port on the host and prints it ([`parse_forwarding`]).
pub fn port_forward_args(context: &str, namespace: &str, pod: &str, port: u16) -> Vec<String> {
    let mut a = target(context, Some(namespace));
    a.extend([
        "port-forward".into(),
        "--address".into(),
        "127.0.0.1".into(),
        format!("pod/{pod}"),
        format!(":{port}"),
    ]);
    a
}

/// `Forwarding from 127.0.0.1:41235 -> 80`: the port kubectl listens on.
pub fn parse_forwarding(output: &str) -> Option<u16> {
    output.lines().find_map(|l| {
        let rest = l.trim().strip_prefix("Forwarding from ")?;
        let (addr, _) = rest.split_once(" -> ")?;
        addr.rsplit_once(':')?.1.trim().parse().ok()
    })
}

/// Apply or delete a manifest file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ManifestAction {
    Apply,
    Delete,
}

/// `kubectl --context <ctx> [-n <ns>] apply|delete -f <file>`. `namespace` is passed only
/// for manifests whose objects name none ([`manifest_needs_namespace`]); kubectl refuses
/// `-n` that differs from an object's own namespace.
pub fn manifest_args(
    context: &str,
    namespace: Option<&str>,
    action: ManifestAction,
    file: &str,
) -> Vec<String> {
    let mut a = target(context, namespace);
    a.push(
        match action {
            ManifestAction::Apply => "apply",
            ManifestAction::Delete => "delete",
        }
        .into(),
    );
    a.push("-f".into());
    a.push(file.into());
    a
}

/// The Kubernetes objects of a YAML file: `(line of apiVersion/kind, kind, name, namespace)`
/// per document that has both `apiVersion` and `kind` at the top level. Compose files and
/// other YAML files have none.
pub fn manifest_objects(content: &str) -> Vec<ManifestObject> {
    let mut out = Vec::new();
    let mut cur = ManifestObject::default();
    let mut first: Option<u32> = None;
    let mut has_api = false;
    let mut in_metadata = false;
    let mut flush = |cur: &mut ManifestObject, first: &mut Option<u32>, has_api: &mut bool| {
        let obj = std::mem::take(cur);
        if *has_api && !obj.kind.is_empty() {
            out.push(ManifestObject {
                line: first.unwrap_or(0),
                ..obj
            });
        }
        *first = None;
        *has_api = false;
    };
    let value = |l: &str, key: &str| {
        l.strip_prefix(key)
            .map(|v| v.trim().trim_matches(['"', '\'']).to_string())
    };
    for (i, line) in content.lines().enumerate() {
        if line.starts_with("---") {
            flush(&mut cur, &mut first, &mut has_api);
            in_metadata = false;
            continue;
        }
        if !line.starts_with([' ', '\t', '#']) {
            in_metadata = line.trim_end() == "metadata:";
            if value(line, "apiVersion:").is_some() {
                has_api = true;
                first.get_or_insert(i as u32);
            } else if let Some(v) = value(line, "kind:") {
                cur.kind = v;
                first.get_or_insert(i as u32);
            }
        } else if in_metadata {
            // Direct children of `metadata:` only (labels have their own `name:` keys).
            let t = line.trim_start();
            if line.len() - t.len() <= 2 {
                if let Some(v) = value(t, "name:") {
                    cur.name = v;
                } else if let Some(v) = value(t, "namespace:") {
                    cur.namespace = Some(v).filter(|n| !n.is_empty());
                }
            }
        }
    }
    flush(&mut cur, &mut first, &mut has_api);
    out
}

/// A Kubernetes object in a manifest file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestObject {
    /// 0-based line of the object's `apiVersion:` / `kind:` (the earlier one).
    pub line: u32,
    pub kind: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
}

/// `-n` is passed when no object of the manifest names its namespace.
pub fn manifest_needs_namespace(objects: &[ManifestObject]) -> bool {
    objects.iter().all(|o| o.namespace.is_none())
}

/// The host's kubeconfig from [`config_view_args`] (JSON, which the YAML parser reads too).
/// File references stay as the host sees them.
pub fn parse_config_view(stdout: &str) -> Result<KubeConfig> {
    if stdout.trim().is_empty() {
        return Ok(KubeConfig::default());
    }
    kube::parse_kubeconfig(stdout, "")
}

/// A failed kubectl run as a balloon text: kubectl's own `error: …` line when it printed
/// one, else its output.
pub fn failure(program: &str, args: &[String], code: Option<i32>, stderr: &str) -> String {
    let text = stderr.trim();
    if text.is_empty() {
        // The subcommand: the first word that is no option and no option's value.
        let mut what = "";
        let mut skip = false;
        for a in args {
            if skip {
                skip = false;
            } else if a == "--context" || a == "-n" {
                skip = true;
            } else if !a.starts_with('-') {
                what = a;
                break;
            }
        }
        return format!(
            "`{program} {what}` failed (exit code {})",
            code.map(|c| c.to_string()).unwrap_or_else(|| "?".into())
        );
    }
    text.lines()
        .rev()
        .find(|l| l.starts_with("error:") || l.starts_with("Error from server"))
        .unwrap_or(text)
        .trim()
        .to_string()
}

#[cfg(feature = "runtime")]
pub use client::KubectlClient;

#[cfg(feature = "runtime")]
mod client {
    use std::sync::{Arc, Mutex};

    use serde_json::Value;

    use super::*;
    use crate::kube::{Pod, RawWorkloads};
    use crate::logs::LogStream;
    use crate::project::ProjectHost;
    use crate::{err, Error};

    /// kubectl on a remote project's host for one kubeconfig context.
    #[derive(Clone)]
    pub struct KubectlClient {
        pub host: Arc<dyn ProjectHost>,
        pub program: String,
        pub context: String,
    }

    impl std::fmt::Debug for KubectlClient {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("KubectlClient")
                .field("host", &self.host.remote())
                .field("program", &self.program)
                .field("context", &self.context)
                .finish()
        }
    }

    /// Run kubectl on `host` and return its standard output; an error with kubectl's
    /// message when it fails.
    pub async fn output(host: &dyn ProjectHost, program: &str, args: &[String]) -> Result<String> {
        let out = Mutex::new((String::new(), String::new()));
        let code = host
            .run(
                program,
                args,
                &[],
                host.root(),
                Box::new(|stream, text| {
                    let mut o = out.lock().unwrap_or_else(|e| e.into_inner());
                    match stream {
                        LogStream::Stdout => o.0.push_str(&text),
                        _ => o.1.push_str(&text),
                    }
                }),
            )
            .await
            .map_err(|e| not_found(program, e))?;
        let (stdout, stderr) = out.into_inner().unwrap_or_else(|e| e.into_inner());
        if code == Some(0) {
            Ok(stdout)
        } else {
            err(failure(program, args, code, &stderr))
        }
    }

    fn not_found(program: &str, e: Error) -> Error {
        let lower = e.0.to_ascii_lowercase();
        if lower.contains("not found") || lower.contains("no such file") {
            Error(format!(
                "`{program}` was not found on the project host. Install kubectl there or set its path in Settings | Build, Execution, Deployment | Kubernetes ({e})"
            ))
        } else {
            e
        }
    }

    /// The host's kubeconfig (`kubectl config view`).
    pub async fn config_view(host: &dyn ProjectHost, program: &str) -> Result<KubeConfig> {
        parse_config_view(&output(host, program, &config_view_args()).await?)
    }

    impl KubectlClient {
        async fn run(&self, args: Vec<String>) -> Result<String> {
            output(self.host.as_ref(), &self.program, &args).await
        }

        async fn json(&self, args: Vec<String>) -> Result<Value> {
            Ok(serde_json::from_str(self.run(args).await?.trim())?)
        }

        pub async fn namespaces(&self) -> Result<Vec<String>> {
            Ok(kube::parse_namespaces(
                &self.json(namespaces_args(&self.context)).await?,
            ))
        }

        pub async fn workloads(&self, ns: &str) -> Result<RawWorkloads> {
            let get = |r: &'static str| self.json(get_args(&self.context, ns, r, None, None));
            let (pods, deployments, services) =
                tokio::join!(get("pods"), get("deployments"), get("services"));
            Ok(RawWorkloads {
                pods: pods?,
                deployments: deployments.unwrap_or(Value::Null),
                services: services.unwrap_or(Value::Null),
            })
        }

        pub async fn get(&self, ns: &str, resource: &str, name: &str) -> Result<Value> {
            self.json(get_args(&self.context, ns, resource, Some(name), None))
                .await
        }

        pub async fn delete_pod(&self, ns: &str, name: &str) -> Result<()> {
            self.run(delete_pod_args(&self.context, ns, name))
                .await
                .map(|_| ())
        }

        pub async fn restart_deployment(&self, ns: &str, name: &str) -> Result<()> {
            self.run(rollout_restart_args(&self.context, ns, name))
                .await
                .map(|_| ())
        }

        pub async fn pods_matching(&self, ns: &str, selector: &str) -> Result<Vec<Pod>> {
            Ok(kube::parse_pods(
                &self
                    .json(get_args(&self.context, ns, "pods", None, Some(selector)))
                    .await?,
            ))
        }

        /// `kubectl logs -f` into `sink` until the pod's log ends (or the future is dropped).
        pub async fn pod_logs(
            &self,
            ns: &str,
            pod: &str,
            container: Option<&str>,
            tail: u32,
            mut sink: impl FnMut(String) + Send,
        ) -> Result<()> {
            let args = logs_args(&self.context, ns, pod, container, tail);
            let errors = Mutex::new(String::new());
            let code = self
                .host
                .run(
                    &self.program,
                    &args,
                    &[],
                    self.host.root(),
                    Box::new(|stream, text| match stream {
                        LogStream::Stdout => sink(text),
                        _ => errors
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .push_str(&text),
                    }),
                )
                .await
                .map_err(|e| not_found(&self.program, e))?;
            let errors = errors.into_inner().unwrap_or_else(|e| e.into_inner());
            if code == Some(0) {
                Ok(())
            } else {
                err(failure(&self.program, &args, code, &errors))
            }
        }

        /// Start `kubectl port-forward` on the host; resolves with the port kubectl listens
        /// on (on the host's `127.0.0.1`) and the task that keeps it running. Dropping or
        /// aborting the task stops kubectl.
        pub async fn port_forward(
            &self,
            ns: &str,
            pod: &str,
            port: u16,
        ) -> Result<(u16, tokio::task::JoinHandle<Result<()>>)> {
            let args = port_forward_args(&self.context, ns, pod, port);
            let (tx, rx) = tokio::sync::oneshot::channel::<Result<u16>>();
            let me = self.clone();
            let task = tokio::spawn(async move {
                let tx = Mutex::new(Some(tx));
                let seen = Mutex::new(String::new());
                let code = me
                    .host
                    .run(
                        &me.program,
                        &args,
                        &[],
                        me.host.root(),
                        Box::new(|_stream, text| {
                            let mut s = seen.lock().unwrap_or_else(|e| e.into_inner());
                            s.push_str(&text);
                            if let Some(p) = parse_forwarding(&s) {
                                if let Some(tx) =
                                    tx.lock().unwrap_or_else(|e| e.into_inner()).take()
                                {
                                    let _ = tx.send(Ok(p));
                                }
                            }
                        }),
                    )
                    .await
                    .map_err(|e| not_found(&me.program, e));
                let seen = seen.into_inner().unwrap_or_else(|e| e.into_inner());
                let failed = match code {
                    Ok(code) => failure(&me.program, &args, code, &seen),
                    Err(e) => e.0,
                };
                if let Some(tx) = tx.lock().unwrap_or_else(|e| e.into_inner()).take() {
                    let _ = tx.send(Err(Error(failed.clone())));
                }
                err(format!("kubectl port-forward ended: {failed}"))
            });
            match tokio::time::timeout(std::time::Duration::from_secs(30), rx).await {
                Ok(Ok(Ok(p))) => Ok((p, task)),
                Ok(Ok(Err(e))) => {
                    task.abort();
                    Err(e)
                }
                Ok(Err(_)) => {
                    task.abort();
                    err("kubectl port-forward ended")
                }
                Err(_) => {
                    task.abort();
                    err("kubectl port-forward did not start within 30 seconds")
                }
            }
        }
    }
}

#[cfg(feature = "runtime")]
pub use client::{config_view, output};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_lines() {
        assert_eq!(
            get_args("k3s", "shop", "pods", None, Some("app=web")).join(" "),
            "--context k3s -n shop get pods -l app=web -o json"
        );
        assert_eq!(
            namespaces_args("k3s").join(" "),
            "--context k3s get namespaces -o json"
        );
        assert_eq!(
            logs_args("k3s", "shop", "web-1", Some("php"), 500).join(" "),
            "--context k3s -n shop logs -f --tail=500 web-1 -c php"
        );
        assert_eq!(
            delete_pod_args("k3s", "shop", "web-1").join(" "),
            "--context k3s -n shop delete pod web-1 --wait=false"
        );
        assert_eq!(
            rollout_restart_args("k3s", "shop", "web").join(" "),
            "--context k3s -n shop rollout restart deployment/web"
        );
        assert_eq!(
            port_forward_args("k3s", "shop", "web-1", 80).join(" "),
            "--context k3s -n shop port-forward --address 127.0.0.1 pod/web-1 :80"
        );
        assert_eq!(
            manifest_args(
                "k3s",
                Some("shop"),
                ManifestAction::Apply,
                "/srv/k8s/web.yaml"
            )
            .join(" "),
            "--context k3s -n shop apply -f /srv/k8s/web.yaml"
        );
        assert_eq!(
            manifest_args("k3s", None, ManifestAction::Delete, "web.yaml").join(" "),
            "--context k3s delete -f web.yaml"
        );
    }

    #[test]
    fn kubectl_paths_per_host() {
        let lines = vec![
            "deploy@shop.example.com\t/usr/local/bin/kubectl".to_string(),
            "root@k3s:2222\t k3s kubectl ".to_string(),
            "broken".to_string(),
        ];
        assert_eq!(
            kubectl_for_host(&lines, "deploy@shop.example.com"),
            "/usr/local/bin/kubectl"
        );
        assert_eq!(kubectl_for_host(&lines, "root@k3s:2222"), "k3s kubectl");
        assert_eq!(kubectl_for_host(&lines, "other@host"), "kubectl");
    }

    #[test]
    fn forwarding_port() {
        assert_eq!(
            parse_forwarding("Forwarding from 127.0.0.1:41235 -> 80\n"),
            Some(41235)
        );
        assert_eq!(
            parse_forwarding("Forwarding from [::1]:5000 -> 5000\nHandling connection for 5000"),
            Some(5000)
        );
        assert_eq!(parse_forwarding("error: unable to forward"), None);
    }

    #[test]
    fn host_kubeconfig_from_config_view() {
        let json = r#"{
            "kind": "Config",
            "apiVersion": "v1",
            "current-context": "k3s-shop",
            "clusters": [
                {"name": "k3s", "cluster": {"server": "https://127.0.0.1:6443", "certificate-authority-data": "DATA+OMITTED"}},
                {"name": "prod-eu", "cluster": {"server": "https://k8s.acme.example:6443"}}
            ],
            "contexts": [
                {"name": "k3s-shop", "context": {"cluster": "k3s", "user": "k3s", "namespace": "shop"}},
                {"name": "prod-eu", "context": {"cluster": "prod-eu", "user": "deploy"}}
            ],
            "users": [{"name": "k3s", "user": {"client-certificate-data": "DATA+OMITTED", "token": "REDACTED"}}]
        }"#;
        let cfg = parse_config_view(json).unwrap();
        assert_eq!(cfg.current_context.as_deref(), Some("k3s-shop"));
        assert_eq!(cfg.contexts.len(), 2);
        assert_eq!(cfg.contexts[0].namespace.as_deref(), Some("shop"));
        assert_eq!(cfg.clusters[0].server, "https://127.0.0.1:6443");
        // Redacted data does not decode into certificates.
        assert_eq!(cfg.clusters[0].ca_data, None);
        assert!(parse_config_view("").unwrap().contexts.is_empty());
        assert!(parse_config_view("{ not json").is_err());
    }

    #[test]
    fn manifests() {
        let text = "# web\napiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: web\n  labels:\n    name: x\nspec:\n  replicas: 2\n---\nkind: Service\napiVersion: v1\nmetadata:\n  name: web\n  namespace: shop\n";
        let objs = manifest_objects(text);
        assert_eq!(objs.len(), 2);
        assert_eq!(
            (objs[0].line, objs[0].kind.as_str(), objs[0].name.as_str()),
            (1, "Deployment", "web")
        );
        assert_eq!(objs[0].namespace, None);
        assert_eq!(objs[1].line, 10);
        assert_eq!(objs[1].namespace.as_deref(), Some("shop"));
        assert!(!manifest_needs_namespace(&objs));
        assert!(manifest_needs_namespace(&objs[..1]));
        // Compose files and other YAML are no manifests.
        assert!(manifest_objects("services:\n  app:\n    image: php\n").is_empty());
        assert!(manifest_objects("kind: ConfigMap\n").is_empty());
    }

    #[test]
    fn failures() {
        let args = get_args("k", "n", "pods", None, None);
        assert_eq!(
            failure(
                "kubectl",
                &args,
                Some(1),
                "W1026 warning\nerror: context \"k\" does not exist\n"
            ),
            "error: context \"k\" does not exist"
        );
        assert_eq!(
            failure("kubectl", &args, Some(1), "Unable to connect to the server"),
            "Unable to connect to the server"
        );
        assert!(failure("kubectl", &args, Some(2), "").contains("exit code 2"));
    }
}
