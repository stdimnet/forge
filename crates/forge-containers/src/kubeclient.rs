//! Kubernetes API client for one kubeconfig context: TLS with the cluster's CA (rustls),
//! bearer tokens, client certificates, basic auth and exec credential plugins
//! (`gke-gcloud-auth-plugin`, `aws eks get-token`, …). Lists namespaces, pods, deployments and
//! services, streams pod logs, deletes pods and restarts deployments.

use serde_json::Value;

use crate::http::{send, split_url, tls_config, Request, Response, TlsOptions, Transport};
use crate::kube::{
    deployments_path, encode_query, parse_namespaces, pod_log_path, pods_path, services_path,
    RawWorkloads, ResolvedContext,
};
use crate::{err, Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
enum Auth {
    None,
    Bearer(String),
    Basic(String),
}

#[derive(Debug, Clone)]
pub struct KubeClient {
    transport: Transport,
    prefix: String,
    auth: Auth,
}

/// What an exec credential plugin printed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExecCredential {
    pub token: Option<String>,
    pub client_cert_pem: Option<String>,
    pub client_key_pem: Option<String>,
}

pub fn parse_exec_credential(stdout: &str) -> Result<ExecCredential> {
    let v: Value = serde_json::from_str(stdout.trim()).map_err(|e| {
        Error(format!(
            "The credential plugin printed no ExecCredential: {e}"
        ))
    })?;
    let s = |k: &str| {
        v.pointer(&format!("/status/{k}"))
            .and_then(Value::as_str)
            .map(str::to_string)
    };
    let cred = ExecCredential {
        token: s("token"),
        client_cert_pem: s("clientCertificateData"),
        client_key_pem: s("clientKeyData"),
    };
    if cred.token.is_none() && cred.client_cert_pem.is_none() {
        return err("The credential plugin returned neither a token nor a client certificate");
    }
    Ok(cred)
}

async fn run_exec_plugin(cfg: &crate::kube::ExecConfig) -> Result<ExecCredential> {
    let mut cmd = tokio::process::Command::new(&cfg.command);
    cmd.args(&cfg.args)
        .envs(cfg.env.iter().cloned())
        .env(
            "KUBERNETES_EXEC_INFO",
            serde_json::json!({
                "apiVersion": cfg.api_version.clone().unwrap_or_else(|| "client.authentication.k8s.io/v1".into()),
                "kind": "ExecCredential",
                "spec": { "interactive": false }
            })
            .to_string(),
        )
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    let out = tokio::time::timeout(std::time::Duration::from_secs(30), cmd.output())
        .await
        .map_err(|_| Error(format!("The credential plugin `{}` timed out", cfg.command)))?
        .map_err(|e| {
            Error(format!(
                "Cannot run the credential plugin `{}`: {e}",
                cfg.command
            ))
        })?;
    if !out.status.success() {
        return err(format!(
            "The credential plugin `{}` failed: {}",
            cfg.command,
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    parse_exec_credential(&String::from_utf8_lossy(&out.stdout))
}

fn read(path: &Option<String>, what: &str) -> Result<Option<String>> {
    match path {
        None => Ok(None),
        Some(p) => {
            let p = match p.strip_prefix("~/") {
                Some(rest) => dirs::home_dir()
                    .map(|h| h.join(rest))
                    .unwrap_or_else(|| p.into()),
                None => p.into(),
            };
            std::fs::read_to_string(&p)
                .map(Some)
                .map_err(|e| Error(format!("Cannot read the {what} {}: {e}", p.display())))
        }
    }
}

impl KubeClient {
    pub async fn new(rc: &ResolvedContext) -> Result<Self> {
        let (https, host, port, prefix) = split_url(&rc.cluster.server)?;
        let u = &rc.user;
        let mut tls = TlsOptions {
            ca_pem: rc
                .cluster
                .ca_data
                .clone()
                .or(read(&rc.cluster.ca_file, "CA certificate")?),
            client_cert_pem: u
                .client_cert_data
                .clone()
                .or(read(&u.client_cert_file, "client certificate")?),
            client_key_pem: u
                .client_key_data
                .clone()
                .or(read(&u.client_key_file, "client key")?),
            insecure: rc.cluster.insecure_skip_tls_verify,
        };
        let mut auth = if let Some(t) = &u.token {
            Auth::Bearer(t.clone())
        } else if let Some(t) = read(&u.token_file, "token file")? {
            Auth::Bearer(t.trim().to_string())
        } else if let (Some(user), Some(pass)) = (&u.username, &u.password) {
            use base64::Engine;
            Auth::Basic(base64::engine::general_purpose::STANDARD.encode(format!("{user}:{pass}")))
        } else {
            Auth::None
        };
        if let Some(exec) = &u.exec {
            let cred = run_exec_plugin(exec).await?;
            if let Some(t) = cred.token {
                auth = Auth::Bearer(t);
            }
            if cred.client_cert_pem.is_some() {
                tls.client_cert_pem = cred.client_cert_pem;
                tls.client_key_pem = cred.client_key_pem;
            }
        }
        let transport = Transport::Tcp {
            host,
            port,
            tls: if https { Some(tls_config(&tls)?) } else { None },
            server_name: rc.cluster.tls_server_name.clone(),
        };
        Ok(KubeClient {
            transport,
            prefix,
            auth,
        })
    }

    /// A client on a given transport with a bearer token (tests).
    pub fn with_transport(transport: Transport, token: Option<String>) -> Self {
        KubeClient {
            transport,
            prefix: String::new(),
            auth: token.map(Auth::Bearer).unwrap_or(Auth::None),
        }
    }

    fn request(&self, method: &str, path: &str) -> Request {
        let req = Request::new(method, format!("{}{path}", self.prefix))
            .header("Accept", "application/json");
        match &self.auth {
            Auth::None => req,
            Auth::Bearer(t) => req.header("Authorization", format!("Bearer {t}")),
            Auth::Basic(b) => req.header("Authorization", format!("Basic {b}")),
        }
    }

    async fn call(&self, req: Request) -> Result<Response> {
        send(&self.transport, &req).await?.error_for_status().await
    }

    pub async fn get(&self, path: &str) -> Result<Value> {
        self.call(self.request("GET", path)).await?.json().await
    }

    pub async fn namespaces(&self) -> Result<Vec<String>> {
        Ok(parse_namespaces(&self.get("/api/v1/namespaces").await?))
    }

    pub async fn workloads(&self, ns: &str) -> Result<RawWorkloads> {
        let (p, d, s) = (pods_path(ns), deployments_path(ns), services_path(ns));
        let (pods, deployments, services) = tokio::join!(self.get(&p), self.get(&d), self.get(&s));
        Ok(RawWorkloads {
            pods: pods?,
            deployments: deployments.unwrap_or(Value::Null),
            services: services.unwrap_or(Value::Null),
        })
    }

    pub async fn pod(&self, ns: &str, name: &str) -> Result<Value> {
        self.get(&format!("{}/{}", pods_path(ns), encode_query(name)))
            .await
    }

    pub async fn deployment(&self, ns: &str, name: &str) -> Result<Value> {
        self.get(&format!("{}/{}", deployments_path(ns), encode_query(name)))
            .await
    }

    pub async fn service(&self, ns: &str, name: &str) -> Result<Value> {
        self.get(&format!("{}/{}", services_path(ns), encode_query(name)))
            .await
    }

    pub async fn delete_pod(&self, ns: &str, name: &str) -> Result<()> {
        self.call(self.request(
            "DELETE",
            &format!("{}/{}", pods_path(ns), encode_query(name)),
        ))
        .await?
        .bytes()
        .await?;
        Ok(())
    }

    /// `kubectl rollout restart`: bump the pod template's `restartedAt` annotation.
    pub async fn restart_deployment(&self, ns: &str, name: &str, now_rfc3339: &str) -> Result<()> {
        let patch = serde_json::json!({ "spec": { "template": { "metadata": { "annotations": {
            "kubectl.kubernetes.io/restartedAt": now_rfc3339
        } } } } });
        let mut req = self
            .request(
                "PATCH",
                &format!("{}/{}", deployments_path(ns), encode_query(name)),
            )
            .header("Content-Type", "application/strategic-merge-patch+json");
        req.body = Some(patch.to_string().into_bytes());
        self.call(req).await?.bytes().await?;
        Ok(())
    }

    /// Open a connection to a pod port (`portforward` subresource over a WebSocket).
    pub async fn port_forward(
        &self,
        ns: &str,
        pod: &str,
        port: u16,
    ) -> Result<crate::forward::WsConnection> {
        let req = self.request("GET", &crate::portforward::portforward_path(ns, pod, port));
        crate::forward::open_port_forward(&self.transport, req).await
    }

    /// Pods of a deployment (by its label selector).
    pub async fn pods_matching(&self, ns: &str, selector: &str) -> Result<Vec<crate::kube::Pod>> {
        Ok(crate::kube::parse_pods(
            &self
                .get(&crate::kube::pods_by_selector_path(ns, selector))
                .await?,
        ))
    }

    /// Stream a pod's log text into `sink`.
    pub async fn pod_logs(
        &self,
        ns: &str,
        pod: &str,
        container: Option<&str>,
        tail: u32,
        follow: bool,
        mut sink: impl FnMut(String) + Send,
    ) -> Result<()> {
        let mut resp = self
            .call(self.request("GET", &pod_log_path(ns, pod, container, tail, follow)))
            .await?;
        let mut decoder = crate::logs::LogDecoder::new(Some(true));
        while let Some(bytes) = resp.chunk().await? {
            for c in decoder.push(&bytes) {
                sink(c.text);
            }
        }
        for c in decoder.finish() {
            sink(c.text);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_credentials() {
        let c =
            parse_exec_credential(r#"{"kind":"ExecCredential","status":{"token":"t0k"}}"#).unwrap();
        assert_eq!(c.token.as_deref(), Some("t0k"));
        assert!(parse_exec_credential(r#"{"status":{}}"#).is_err());
        assert!(parse_exec_credential("not json").is_err());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn lists_workloads_and_streams_logs() {
        use crate::http::tests::{chunked, fake_server, ok_json};
        let (_d, path) = fake_server(|head, body| {
            assert!(head.contains("Authorization: Bearer secret"), "{head}");
            let target = head.split_whitespace().nth(1).unwrap();
            match target {
                "/api/v1/namespaces" => ok_json(r#"{"items":[{"metadata":{"name":"shop"}},{"metadata":{"name":"default"}}]}"#),
                "/api/v1/namespaces/shop/pods" => ok_json(r#"{"items":[{"metadata":{"name":"web-1","namespace":"shop"},"status":{"phase":"Running"}}]}"#),
                "/apis/apps/v1/namespaces/shop/deployments" => ok_json(r#"{"items":[]}"#),
                "/api/v1/namespaces/shop/services" => b"HTTP/1.1 403 Forbidden\r\nContent-Length: 35\r\n\r\n{\"message\":\"services is forbidden\"}".to_vec(),
                "/api/v1/namespaces/shop/pods/web-1/log?tailLines=10&follow=true" => chunked("200 OK", &[b"line 1\nli", b"ne 2\n"]),
                "/apis/apps/v1/namespaces/shop/deployments/web" => {
                    assert!(head.starts_with("PATCH"));
                    assert!(std::str::from_utf8(body).unwrap().contains("restartedAt"));
                    ok_json("{}")
                }
                _ => b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_vec(),
            }
        })
        .await;
        let c = KubeClient::with_transport(Transport::Unix(path), Some("secret".into()));
        assert_eq!(c.namespaces().await.unwrap(), ["default", "shop"]);
        let w = c.workloads("shop").await.unwrap().parse();
        assert_eq!(w.pods[0].name, "web-1");
        assert!(w.services.is_empty(), "a forbidden list is shown empty");
        let mut text = String::new();
        c.pod_logs("shop", "web-1", None, 10, true, |t| text.push_str(&t))
            .await
            .unwrap();
        assert_eq!(text, "line 1\nline 2\n");
        c.restart_deployment("shop", "web", "2026-10-20T09:00:00Z")
            .await
            .unwrap();
        assert!(c.pod("shop", "nope").await.is_err());
    }
}
