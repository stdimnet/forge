//! Installing and starting the Forge agent over an SSH session.

use std::path::{Path, PathBuf};
use std::time::Duration;

use forge_backend::protocol::{Call, Hello, PROTOCOL_VERSION};
use forge_backend::rpc::RpcClient;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncRead, AsyncReadExt};

use crate::deploy::{self, DeployPlan};
use crate::session::{ConnectError, SshSession};

/// Version of the agent this build ships (same as the agent crate's).
pub const AGENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// What the bundled agent prints for `--version`.
pub fn version_line() -> String {
    format!("forge-remote-agent {AGENT_VERSION} protocol {PROTOCOL_VERSION}")
}

/// Most bytes of login-script output skipped before the agent's ready marker.
const MAX_PREAMBLE: usize = 256 * 1024;

/// Where the agent binaries for remote hosts are found: the app's bundled resources,
/// `FORGE_REMOTE_AGENT_DIR`, and next to the running executable (a development build of
/// `forge-remote-agent` serves hosts of the same architecture).
#[derive(Debug, Clone, Default)]
pub struct AgentBinaries {
    dirs: Vec<PathBuf>,
}

impl AgentBinaries {
    pub fn new(dirs: Vec<PathBuf>) -> Self {
        Self { dirs }
    }

    /// The usual places for this process (plus `extra`, e.g. Tauri's resource dir).
    pub fn discover(extra: impl IntoIterator<Item = PathBuf>) -> Self {
        let mut dirs: Vec<PathBuf> = extra.into_iter().collect();
        if let Some(dir) = std::env::var_os("FORGE_REMOTE_AGENT_DIR") {
            dirs.insert(0, PathBuf::from(dir));
        }
        if let Some(dir) = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf))
        {
            dirs.push(dir.join("agents"));
            // `pnpm agents` writes to `target/agents`, next to `target/debug`.
            if let Some(parent) = dir.parent() {
                dirs.push(parent.join("agents"));
            }
            dirs.push(dir);
        }
        Self { dirs }
    }

    /// The agent binary for a Linux host of `arch` (`x86_64` / `aarch64`), plain or
    /// gzip-compressed (`.gz`, as bundled: AppImage tools patch uncompressed ELF files).
    pub fn find(&self, arch: &str) -> Option<PathBuf> {
        let bundled = deploy::bundled_name(arch);
        let same_platform = cfg!(target_os = "linux") && std::env::consts::ARCH == arch;
        self.dirs.iter().find_map(|d| {
            let p = d.join(&bundled);
            if p.is_file() {
                return Some(p);
            }
            let gz = d.join(format!("{bundled}.gz"));
            if gz.is_file() {
                return Some(gz);
            }
            let dev = d.join("forge-remote-agent");
            (same_platform && dev.is_file()).then_some(dev)
        })
    }
}

/// The agent binary at `path`, decompressed when it is a `.gz` file.
pub fn read_agent(path: &Path) -> std::io::Result<Vec<u8>> {
    let bytes = std::fs::read(path)?;
    if path.extension().is_some_and(|e| e == "gz") {
        let mut out = Vec::new();
        std::io::Read::read_to_end(
            &mut flate2::read::GzDecoder::new(bytes.as_slice()),
            &mut out,
        )?;
        Ok(out)
    } else {
        Ok(bytes)
    }
}

/// SHA-256 of `bytes` as lowercase hex (what the probe reports for the installed agent).
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn other(message: impl Into<String>) -> ConnectError {
    ConnectError::Other {
        message: message.into(),
    }
}

/// A started agent.
pub struct StartedAgent {
    pub client: RpcClient,
    pub hello: Hello,
    /// The agent was uploaded during this start.
    pub uploaded: bool,
}

/// Skip everything up to the ready marker; returns the bytes read after it. Also used by
/// agents started over a process's stdio (`forge-devenv`: `docker exec`, `wsl.exe`).
pub async fn read_until_ready<R: AsyncRead + Unpin>(
    reader: &mut R,
) -> Result<Vec<u8>, ConnectError> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        if let Some(end) = deploy::find_ready(&buf) {
            return Ok(buf.split_off(end));
        }
        if buf.len() > MAX_PREAMBLE {
            return Err(other("The Forge agent did not start (unexpected output)"));
        }
        let n = reader
            .read(&mut chunk)
            .await
            .map_err(|e| other(format!("Cannot start the Forge agent: {e}")))?;
        if n == 0 {
            let text = String::from_utf8_lossy(&buf);
            return Err(other(format!(
                "The Forge agent exited during startup{}",
                if text.trim().is_empty() {
                    String::new()
                } else {
                    format!(": {}", text.trim())
                }
            )));
        }
        buf.extend_from_slice(&chunk[..n]);
    }
}

/// Probe the host, upload the agent when needed, start it and say hello.
pub async fn start_agent(
    session: &SshSession,
    binaries: &AgentBinaries,
    progress: &(dyn Fn(&str) + Send + Sync),
) -> Result<StartedAgent, ConnectError> {
    progress("Checking the remote host…");
    let probe_out = session
        .exec(&deploy::probe_command(AGENT_VERSION), None)
        .await?;
    let probe = deploy::parse_probe(&probe_out.stdout_text());
    let local = deploy::normalize_arch(&probe.arch).and_then(|a| binaries.find(a));
    let local_bytes = local.as_deref().and_then(|p| read_agent(p).ok());
    let local_sha = local_bytes.as_deref().map(sha256_hex);
    let mut uploaded = false;
    match deploy::plan(&probe, &version_line(), local_sha.as_deref()) {
        DeployPlan::Unsupported { message } => return Err(other(message)),
        DeployPlan::Upload { arch, reason } => {
            let bytes = local_bytes.ok_or_else(|| {
                other(format!(
                    "The Forge agent for Linux {arch} is not part of this build ({reason}). \
                     Build it with `pnpm --dir app/ui agents` or point FORGE_REMOTE_AGENT_DIR \
                     at a folder with forge-remote-agent-{arch}-linux."
                ))
            })?;
            progress("Uploading the Forge agent…");
            tracing::info!("uploading the Forge agent ({reason})");
            let out = session
                .exec(&deploy::upload_command(AGENT_VERSION), Some(&bytes))
                .await?;
            if !out.success() {
                return Err(other(format!(
                    "Cannot upload the Forge agent: {}",
                    out.stderr_text().trim()
                )));
            }
            uploaded = true;
        }
        DeployPlan::Start => {}
    }
    progress("Starting the Forge agent…");
    let stream = session
        .exec_stream(&deploy::start_command(AGENT_VERSION))
        .await?;
    let (mut read, write) = tokio::io::split(stream);
    let rest = tokio::time::timeout(Duration::from_secs(30), read_until_ready(&mut read))
        .await
        .map_err(|_| other("The Forge agent did not start in time"))??;
    let reader = std::io::Cursor::new(rest).chain(read);
    let client = RpcClient::spawn(reader, write);
    let hello: Hello = client
        .call(
            Call::Hello {
                version: AGENT_VERSION.into(),
                protocol: PROTOCOL_VERSION,
            },
            Duration::from_secs(30),
        )
        .await
        .map_err(|e| other(e.to_string()))?;
    Ok(StartedAgent {
        client,
        hello,
        uploaded,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_line_matches_the_agent() {
        assert_eq!(version_line(), forge_remote_agent::version_line());
    }

    #[test]
    fn finds_bundled_and_development_binaries() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("forge-remote-agent-aarch64-linux"), b"arm").unwrap();
        let bins = AgentBinaries::new(vec![dir.path().to_path_buf()]);
        assert!(bins.find("aarch64").is_some());
        assert_eq!(bins.find("riscv64"), None);
        // A development build serves hosts of its own platform.
        std::fs::write(dir.path().join("forge-remote-agent"), b"dev").unwrap();
        if cfg!(target_os = "linux") && std::env::consts::ARCH == "x86_64" {
            assert_eq!(
                bins.find("x86_64"),
                Some(dir.path().join("forge-remote-agent"))
            );
        }
    }

    #[test]
    fn reads_compressed_bundled_binaries() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("forge-remote-agent-aarch64-linux.gz");
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(b"\x7fELF agent").unwrap();
        std::fs::write(&path, gz.finish().unwrap()).unwrap();
        let bins = AgentBinaries::new(vec![dir.path().to_path_buf()]);
        let found = bins.find("aarch64").unwrap();
        assert_eq!(found, path);
        assert_eq!(read_agent(&found).unwrap(), b"\x7fELF agent");
    }

    #[tokio::test]
    async fn skips_login_noise_before_the_marker() {
        let data = b"motd line\nFORGE-AGENT-READY\nrest".to_vec();
        let mut reader = std::io::Cursor::new(data);
        let rest = read_until_ready(&mut reader).await.unwrap();
        assert_eq!(rest, b"rest");
        let mut early_exit = std::io::Cursor::new(b"command not found\n".to_vec());
        let err = read_until_ready(&mut early_exit).await.unwrap_err();
        assert!(err.to_string().contains("command not found"));
    }
}
