//! The Forge agent over a process's stdio instead of SSH: `docker exec -i <container> …` for
//! dev containers, `wsl.exe -d <distro> --exec …` for WSL. The steps are those of an SSH
//! host (`forge_remote::deploy`): probe with one shell command, copy the agent in through
//! `cat` when it is missing or differs, start it and read past its ready marker; the
//! protocol then runs over the process's stdin/stdout with the usual [`RpcClient`].

use std::collections::BTreeMap;
use std::time::Duration;

use forge_backend::protocol::{Call, Hello, PROTOCOL_VERSION};
use forge_backend::rpc::RpcClient;
use forge_remote::agent::{
    read_agent, read_until_ready, sha256_hex, version_line, AgentBinaries, AGENT_VERSION,
};
use forge_remote::deploy::{self, DeployPlan};
use tokio::io::AsyncReadExt;

use crate::runner::{Runner, StdioProcess};
use crate::{docker, wsl};

/// Where an agent runs: turns a POSIX shell script into a command line on this machine.
pub trait AgentTarget: Send + Sync {
    /// Program to start (`docker`, `wsl.exe`).
    fn program(&self) -> String;
    /// Arguments that run `script` with `/bin/sh -c` on the target; `stdin` keeps the
    /// process's input open (uploads and the agent itself).
    fn script_args(&self, script: &str, stdin: bool) -> Vec<String>;
    /// For messages ("dev container shop", "WSL Ubuntu").
    fn label(&self) -> String;
}

fn sh(script: &str) -> Vec<String> {
    vec!["/bin/sh".into(), "-c".into(), script.to_string()]
}

/// A running dev container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DockerTarget {
    pub docker: String,
    pub container: String,
    pub user: Option<String>,
    pub workdir: Option<String>,
    /// `remoteEnv` for the agent (and so for terminals and runs).
    pub env: BTreeMap<String, String>,
    pub name: String,
}

impl AgentTarget for DockerTarget {
    fn program(&self) -> String {
        self.docker.clone()
    }

    fn script_args(&self, script: &str, stdin: bool) -> Vec<String> {
        docker::exec_args(
            &self.container,
            self.user.as_deref(),
            self.workdir.as_deref(),
            &self.env,
            stdin,
            &sh(script),
        )
    }

    fn label(&self) -> String {
        format!("dev container {}", self.name)
    }
}

/// A WSL distribution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WslTarget {
    pub wsl: String,
    pub distro: String,
    pub user: Option<String>,
}

impl AgentTarget for WslTarget {
    fn program(&self) -> String {
        self.wsl.clone()
    }

    fn script_args(&self, script: &str, _stdin: bool) -> Vec<String> {
        wsl::exec_args(&self.distro, self.user.as_deref(), &sh(script))
    }

    fn label(&self) -> String {
        format!("WSL {}", self.distro)
    }
}

/// A started agent and the process it runs in.
pub struct StartedAgent {
    pub client: RpcClient,
    pub hello: Hello,
    /// The agent was copied in during this start.
    pub uploaded: bool,
    /// Ends the `docker exec` / `wsl.exe` process when dropped.
    pub process: Box<dyn Send + Sync>,
}

/// Probe the target, copy the agent in when needed, start it and say hello.
pub async fn start_agent(
    runner: &dyn Runner,
    target: &dyn AgentTarget,
    binaries: &AgentBinaries,
    progress: &(dyn Fn(&str) + Send + Sync),
) -> Result<StartedAgent, String> {
    let program = target.program();
    progress(&format!("Checking the {}…", target.label()));
    let probe_args = target.script_args(&deploy::probe_command(AGENT_VERSION), false);
    let out = runner
        .output(&program, &probe_args, None)
        .await
        .map_err(|e| e.to_string())?;
    if !out.success() {
        return Err(format!(
            "Cannot reach the {}: {}",
            target.label(),
            out.message()
        ));
    }
    let probe = deploy::parse_probe(&out.stdout_text());
    let local = deploy::normalize_arch(&probe.arch).and_then(|a| binaries.find(a));
    let local_bytes = local.as_deref().and_then(|p| read_agent(p).ok());
    let local_sha = local_bytes.as_deref().map(sha256_hex);
    let mut uploaded = false;
    match deploy::plan(&probe, &version_line(), local_sha.as_deref()) {
        DeployPlan::Unsupported { message } => return Err(message),
        DeployPlan::Upload { arch, reason } => {
            let bytes = local_bytes.ok_or_else(|| {
                format!(
                    "The Forge agent for Linux {arch} is not part of this build ({reason}). \
                     Build it with `pnpm --dir app/ui agents` or point FORGE_REMOTE_AGENT_DIR \
                     at a folder with forge-remote-agent-{arch}-linux."
                )
            })?;
            progress(&format!(
                "Copying the Forge agent into the {}…",
                target.label()
            ));
            tracing::info!(
                "copying the Forge agent into the {} ({reason})",
                target.label()
            );
            let args = target.script_args(&deploy::upload_command(AGENT_VERSION), true);
            let out = runner
                .output(&program, &args, Some(bytes))
                .await
                .map_err(|e| e.to_string())?;
            if !out.success() {
                return Err(format!("Cannot copy the Forge agent: {}", out.message()));
            }
            uploaded = true;
        }
        DeployPlan::Start => {}
    }
    progress("Starting the Forge agent…");
    let args = target.script_args(&deploy::start_command(AGENT_VERSION), true);
    let StdioProcess {
        mut stdout,
        stdin,
        guard,
    } = runner
        .spawn_stdio(&program, &args)
        .map_err(|e| e.to_string())?;
    let rest = tokio::time::timeout(Duration::from_secs(30), read_until_ready(&mut stdout))
        .await
        .map_err(|_| "The Forge agent did not start in time".to_string())?
        .map_err(|e| e.to_string())?;
    let reader = std::io::Cursor::new(rest).chain(stdout);
    let client = RpcClient::spawn(reader, stdin);
    let hello: Hello = client
        .call(
            Call::Hello {
                version: AGENT_VERSION.into(),
                protocol: PROTOCOL_VERSION,
            },
            Duration::from_secs(30),
        )
        .await
        .map_err(|e| e.to_string())?;
    Ok(StartedAgent {
        client,
        hello,
        uploaded,
        process: guard,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::runner::{BoxFuture, LogSink, Output};
    use std::sync::{Arc, Mutex};
    use tokio::io::AsyncWriteExt;

    /// A fake `docker` / `wsl.exe`: answers the probe, records uploads and serves the real
    /// agent in-process for the start command.
    #[derive(Default)]
    pub struct FakeRunner {
        pub calls: Mutex<Vec<(String, Vec<String>)>>,
        pub uploads: Mutex<Vec<Vec<u8>>>,
        /// What the probe prints.
        pub probe: Mutex<String>,
        /// Answers for other commands, by the first matching argument.
        pub answers: Mutex<Vec<(String, Output)>>,
        pub streamed: Mutex<Vec<Vec<String>>>,
    }

    impl FakeRunner {
        pub fn new(probe: &str) -> Arc<Self> {
            let r = Self::default();
            *r.probe.lock().unwrap() = probe.to_string();
            Arc::new(r)
        }

        pub fn answer(&self, contains: &str, out: Output) {
            self.answers.lock().unwrap().push((contains.into(), out));
        }

        pub fn commands(&self) -> Vec<String> {
            self.calls
                .lock()
                .unwrap()
                .iter()
                .map(|(p, a)| format!("{p} {}", a.join(" ")))
                .collect()
        }
    }

    impl Runner for FakeRunner {
        fn output<'a>(
            &'a self,
            program: &'a str,
            args: &'a [String],
            stdin: Option<Vec<u8>>,
        ) -> BoxFuture<'a, std::io::Result<Output>> {
            self.calls
                .lock()
                .unwrap()
                .push((program.to_string(), args.to_vec()));
            let script = args.last().cloned().unwrap_or_default();
            let answer = self
                .answers
                .lock()
                .unwrap()
                .iter()
                .find(|(k, _)| args.iter().any(|a| a.contains(k.as_str())))
                .map(|(_, o)| o.clone());
            let out = if let Some(answer) = answer {
                answer
            } else if script.contains("FORGE_OS=") {
                Output::ok(self.probe.lock().unwrap().clone())
            } else if script.contains(".part") {
                self.uploads.lock().unwrap().push(stdin.unwrap_or_default());
                Output::ok("")
            } else {
                Output::ok("")
            };
            Box::pin(async move { Ok(out) })
        }

        fn stream<'a>(
            &'a self,
            program: &'a str,
            args: &'a [String],
            sink: LogSink<'a>,
        ) -> BoxFuture<'a, std::io::Result<Option<i32>>> {
            self.calls
                .lock()
                .unwrap()
                .push((program.to_string(), args.to_vec()));
            self.streamed.lock().unwrap().push(args.to_vec());
            let fail = self
                .answers
                .lock()
                .unwrap()
                .iter()
                .find(|(k, _)| args.iter().any(|a| a.contains(k.as_str())))
                .map(|(_, o)| o.clone());
            Box::pin(async move {
                match fail {
                    Some(o) => {
                        sink(crate::runner::Stream::Stderr, &o.stderr_text());
                        Ok(o.code)
                    }
                    None => {
                        sink(crate::runner::Stream::Stdout, &format!("ran {}\n", args[0]));
                        Ok(Some(0))
                    }
                }
            })
        }

        fn spawn_stdio(&self, program: &str, args: &[String]) -> std::io::Result<StdioProcess> {
            self.calls
                .lock()
                .unwrap()
                .push((program.to_string(), args.to_vec()));
            let (client_end, agent_end) = tokio::io::duplex(1 << 16);
            let (agent_read, mut agent_write) = tokio::io::split(agent_end);
            let task = tokio::spawn(async move {
                // Login noise, then the marker, then the protocol.
                agent_write
                    .write_all(b"Welcome to the container\nFORGE-AGENT-READY\n")
                    .await
                    .unwrap();
                let _ = forge_remote_agent::serve(agent_read, agent_write).await;
            });
            let (read, write) = tokio::io::split(client_end);
            struct Abort(tokio::task::JoinHandle<()>);
            impl Drop for Abort {
                fn drop(&mut self) {
                    self.0.abort();
                }
            }
            Ok(StdioProcess {
                stdout: Box::new(read),
                stdin: Box::new(write),
                guard: Box::new(Abort(task)),
            })
        }
    }

    pub fn binaries() -> (tempfile::TempDir, AgentBinaries) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("forge-remote-agent-x86_64-linux"),
            b"\x7fELF agent",
        )
        .unwrap();
        let bins = AgentBinaries::new(vec![dir.path().to_path_buf()]);
        (dir, bins)
    }

    pub const PROBE_EMPTY: &str = "FORGE_OS=Linux\nFORGE_ARCH=x86_64\nFORGE_HOME=/home/vscode\n";

    pub fn docker_target() -> DockerTarget {
        DockerTarget {
            docker: "docker".into(),
            container: "c0ffee".into(),
            user: Some("vscode".into()),
            workdir: Some("/workspaces/shop".into()),
            env: [("APP_ENV".to_string(), "local".to_string())].into(),
            name: "Shop".into(),
        }
    }

    #[tokio::test]
    async fn copies_and_starts_the_agent_over_docker_exec() {
        let runner = FakeRunner::new(PROBE_EMPTY);
        let (_dir, bins) = binaries();
        let steps = Mutex::new(Vec::new());
        let started = start_agent(runner.as_ref(), &docker_target(), &bins, &|m: &str| {
            steps.lock().unwrap().push(m.to_string())
        })
        .await
        .unwrap();
        assert!(started.uploaded);
        assert_eq!(started.hello.protocol, PROTOCOL_VERSION);
        assert_eq!(runner.uploads.lock().unwrap()[0], b"\x7fELF agent");
        let cmds = runner.commands();
        assert_eq!(cmds.len(), 3);
        assert!(
            cmds[0].starts_with(
                "docker exec -u vscode -w /workspaces/shop -e APP_ENV=local c0ffee /bin/sh -c"
            ),
            "{}",
            cmds[0]
        );
        assert!(
            cmds[1].starts_with("docker exec -i -u vscode"),
            "{}",
            cmds[1]
        );
        assert!(cmds[2].contains(" serve"), "{}", cmds[2]);
        assert!(steps.lock().unwrap()[0].contains("dev container Shop"));
        // The protocol works over the pipe.
        let browse: forge_backend::protocol::BrowseResult = started
            .client
            .call(Call::Browse { path: "/".into() }, Duration::from_secs(10))
            .await
            .unwrap();
        assert_eq!(browse.path, "/");
    }

    #[tokio::test]
    async fn starts_an_installed_agent_in_wsl_without_copying() {
        let (_dir, bins) = binaries();
        let sha = sha256_hex(b"\x7fELF agent");
        let probe = format!(
            "{PROBE_EMPTY}FORGE_AGENT={}\nFORGE_SHA={sha}\n",
            version_line()
        );
        let runner = FakeRunner::new(&probe);
        let target = WslTarget {
            wsl: "wsl.exe".into(),
            distro: "Ubuntu".into(),
            user: None,
        };
        let started = start_agent(runner.as_ref(), &target, &bins, &|_| {})
            .await
            .unwrap();
        assert!(!started.uploaded);
        let cmds = runner.commands();
        assert_eq!(cmds.len(), 2);
        assert!(
            cmds[0].starts_with("wsl.exe -d Ubuntu --exec /bin/sh -c"),
            "{}",
            cmds[0]
        );
    }

    #[tokio::test]
    async fn reports_unsupported_targets_and_missing_binaries() {
        let (_dir, bins) = binaries();
        let arm = FakeRunner::new("FORGE_OS=Linux\nFORGE_ARCH=aarch64\nFORGE_HOME=/root\n");
        let err = start_agent(arm.as_ref(), &docker_target(), &bins, &|_| {})
            .await
            .err()
            .unwrap();
        assert!(err.contains("aarch64"), "{err}");
        let mac = FakeRunner::new("FORGE_OS=Darwin\nFORGE_ARCH=arm64\n");
        let err = start_agent(mac.as_ref(), &docker_target(), &bins, &|_| {})
            .await
            .err()
            .unwrap();
        assert!(err.contains("Linux"), "{err}");
        let down = FakeRunner::new("");
        down.answer(
            "FORGE_OS",
            Output::failed(1, "Error: No such container: c0ffee"),
        );
        let err = start_agent(down.as_ref(), &docker_target(), &bins, &|_| {})
            .await
            .err()
            .unwrap();
        assert_eq!(
            err,
            "Cannot reach the dev container Shop: Error: No such container: c0ffee"
        );
    }
}
