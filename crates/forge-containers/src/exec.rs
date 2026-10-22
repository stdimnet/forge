//! Interactive command lines that run in a terminal PTY: a shell inside a container
//! (`docker exec -it`), attaching to a container's console (`docker attach`) and a shell in
//! a pod (`kubectl exec -it`) and attaching to a pod's container (`kubectl attach -it`).

use serde::Serialize;

/// Starts bash when the image has it, else sh.
pub const SHELL_PROBE: &str =
    "if command -v bash >/dev/null 2>&1; then exec bash; else exec sh; fi";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalCommand {
    /// Terminal tab title.
    pub title: String,
    pub program: String,
    pub args: Vec<String>,
    #[serde(default)]
    pub env: Vec<(String, String)>,
    /// Run on a remote project's host (the Docker daemon of that host); otherwise the
    /// command runs on this machine, also in a remote project.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub on_host: bool,
}

impl TerminalCommand {
    pub fn display(&self) -> String {
        crate::compose::display_command(&self.program, &self.args)
    }
}

/// `docker exec -it <container> sh -c '<bash or sh>'`.
pub fn docker_exec(
    docker: &str,
    container: &str,
    name: &str,
    env: Vec<(String, String)>,
) -> TerminalCommand {
    TerminalCommand {
        title: name.to_string(),
        program: docker.to_string(),
        args: vec![
            "exec".into(),
            "-it".into(),
            container.into(),
            "sh".into(),
            "-c".into(),
            SHELL_PROBE.into(),
        ],
        env,
        on_host: false,
    }
}

/// `docker attach --sig-proxy=false <container>` (the Attached Console tab).
pub fn docker_attach(
    docker: &str,
    container: &str,
    name: &str,
    env: Vec<(String, String)>,
) -> TerminalCommand {
    TerminalCommand {
        title: format!("{name} (attached)"),
        program: docker.to_string(),
        args: vec![
            "attach".into(),
            "--sig-proxy=false".into(),
            container.into(),
        ],
        env,
        on_host: false,
    }
}

/// `kubectl --context <ctx> -n <ns> exec -it <pod> [-c <container>] -- sh -c '<bash or sh>'`.
pub fn kubectl_exec(
    kubectl: &str,
    kubeconfig: Option<&str>,
    context: &str,
    namespace: &str,
    pod: &str,
    container: Option<&str>,
) -> TerminalCommand {
    let mut args = vec![
        "--context".to_string(),
        context.into(),
        "-n".into(),
        namespace.into(),
        "exec".into(),
        "-it".into(),
        pod.into(),
    ];
    if let Some(c) = container {
        args.push("-c".into());
        args.push(c.into());
    }
    args.extend(["--".into(), "sh".into(), "-c".into(), SHELL_PROBE.into()]);
    TerminalCommand {
        title: pod_title(pod, container),
        program: kubectl.to_string(),
        args,
        env: kubeconfig_env(kubeconfig),
        on_host: false,
    }
}

/// `kubectl --context <ctx> -n <ns> attach -it <pod> [-c <container>]` (Attached Console).
pub fn kubectl_attach(
    kubectl: &str,
    kubeconfig: Option<&str>,
    context: &str,
    namespace: &str,
    pod: &str,
    container: Option<&str>,
) -> TerminalCommand {
    let mut args = vec![
        "--context".to_string(),
        context.into(),
        "-n".into(),
        namespace.into(),
        "attach".into(),
        "-it".into(),
        pod.into(),
    ];
    if let Some(c) = container {
        args.push("-c".into());
        args.push(c.into());
    }
    TerminalCommand {
        title: format!("{} (attached)", pod_title(pod, container)),
        program: kubectl.to_string(),
        args,
        env: kubeconfig_env(kubeconfig),
        on_host: false,
    }
}

/// `web-1` or `web-1 [php]`.
fn pod_title(pod: &str, container: Option<&str>) -> String {
    match container {
        Some(c) => format!("{pod} [{c}]"),
        None => pod.to_string(),
    }
}

fn kubeconfig_env(kubeconfig: Option<&str>) -> Vec<(String, String)> {
    kubeconfig
        .filter(|k| !k.is_empty())
        .map(|k| vec![("KUBECONFIG".to_string(), k.to_string())])
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_lines() {
        let e = docker_exec(
            "docker",
            "abc",
            "acme-shop-app-1",
            vec![("DOCKER_HOST".into(), "unix:///x.sock".into())],
        );
        assert_eq!(e.args[..3], ["exec", "-it", "abc"]);
        assert!(e
            .display()
            .starts_with("docker exec -it abc sh -c \"if command -v bash"));
        assert_eq!(
            docker_attach("docker", "abc", "web", vec![]).args,
            ["attach", "--sig-proxy=false", "abc"]
        );
        let k = kubectl_exec(
            "kubectl",
            Some("/k/config"),
            "kind-acme",
            "shop",
            "web-1",
            Some("php"),
        );
        assert_eq!(
            k.args[..9],
            [
                "--context",
                "kind-acme",
                "-n",
                "shop",
                "exec",
                "-it",
                "web-1",
                "-c",
                "php"
            ]
        );
        assert_eq!(k.env, [("KUBECONFIG".to_string(), "/k/config".to_string())]);
        assert_eq!(k.title, "web-1 [php]");
        let a = kubectl_attach("kubectl", None, "kind-acme", "shop", "web-1", Some("php"));
        assert_eq!(
            a.args,
            [
                "--context",
                "kind-acme",
                "-n",
                "shop",
                "attach",
                "-it",
                "web-1",
                "-c",
                "php"
            ]
        );
        assert_eq!(a.title, "web-1 [php] (attached)");
        assert!(kubectl_exec("kubectl", None, "c", "n", "p", None)
            .env
            .is_empty());
    }
}
