//! Installing the Forge agent on a remote host: probe the host with one shell command,
//! decide whether the agent must be uploaded, upload it through `cat` (no SFTP needed) and
//! start it. Agents live in `~/.cache/forge-agent/<version>/forge-remote-agent`; a binary
//! whose SHA-256 differs from the bundled one (development builds) is replaced.

use serde::{Deserialize, Serialize};

/// The agent prints this line on stdout before speaking the protocol, so output of login
/// scripts (motd, `echo` in `.profile`) can be skipped.
pub const READY_MARKER: &str = "FORGE-AGENT-READY";

/// What the probe found out about the host.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Probe {
    /// `uname -s`, e.g. `Linux`.
    pub os: String,
    /// `uname -m`, e.g. `x86_64`.
    pub arch: String,
    pub home: String,
    /// `--version` line of the installed agent of this version, if any.
    pub installed: Option<String>,
    /// SHA-256 of the installed agent binary.
    pub sha256: Option<String>,
}

/// The decision after probing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "camelCase")]
pub enum DeployPlan {
    /// The right agent is installed.
    Start,
    /// Upload the agent for `arch` (`x86_64` or `aarch64`).
    Upload { arch: String, reason: String },
    /// Forge has no agent for this platform.
    Unsupported { message: String },
}

/// Directory of an agent version relative to the home directory.
pub fn agent_dir(version: &str) -> String {
    format!(".cache/forge-agent/{version}")
}

/// Path of the agent binary relative to the home directory.
pub fn agent_path(version: &str) -> String {
    format!("{}/forge-remote-agent", agent_dir(version))
}

/// File name of the bundled agent for a remote architecture.
pub fn bundled_name(arch: &str) -> String {
    format!("forge-remote-agent-{arch}-linux")
}

/// Normalize `uname -m` to the agent's architecture names.
pub fn normalize_arch(machine: &str) -> Option<&'static str> {
    match machine.trim() {
        "x86_64" | "amd64" => Some("x86_64"),
        "aarch64" | "arm64" | "armv8l" => Some("aarch64"),
        _ => None,
    }
}

/// Shell command that prints `KEY=value` lines about the host and the installed agent.
/// Runs under any POSIX shell; lines the parser does not know are ignored.
pub fn probe_command(version: &str) -> String {
    let path = agent_path(version);
    format!(
        "echo \"FORGE_OS=$(uname -s)\"; echo \"FORGE_ARCH=$(uname -m)\"; echo \"FORGE_HOME=$HOME\"; \
         f=\"$HOME/{path}\"; if [ -x \"$f\" ]; then \
         echo \"FORGE_AGENT=$(\"$f\" --version 2>/dev/null)\"; \
         echo \"FORGE_SHA=$( (sha256sum \"$f\" 2>/dev/null || shasum -a 256 \"$f\" 2>/dev/null) | cut -d' ' -f1)\"; \
         fi"
    )
}

/// Parse the output of [`probe_command`].
pub fn parse_probe(output: &str) -> Probe {
    let mut p = Probe::default();
    for line in output.lines() {
        let Some((key, value)) = line.trim_end_matches('\r').split_once('=') else {
            continue;
        };
        let value = value.trim().to_string();
        match key.trim() {
            "FORGE_OS" => p.os = value,
            "FORGE_ARCH" => p.arch = value,
            "FORGE_HOME" => p.home = value,
            "FORGE_AGENT" if !value.is_empty() => p.installed = Some(value),
            "FORGE_SHA" if !value.is_empty() => p.sha256 = Some(value.to_lowercase()),
            _ => {}
        }
    }
    p
}

/// Decide what to do with the probed host. `version_line` is what the bundled agent prints
/// for `--version`; `local_sha256` is the bundled binary's hash when known.
pub fn plan(probe: &Probe, version_line: &str, local_sha256: Option<&str>) -> DeployPlan {
    if !probe.os.eq_ignore_ascii_case("linux") {
        return DeployPlan::Unsupported {
            message: format!(
                "Remote development needs a Linux host (this one runs {}).",
                if probe.os.is_empty() {
                    "an unknown system"
                } else {
                    &probe.os
                }
            ),
        };
    }
    let Some(arch) = normalize_arch(&probe.arch) else {
        return DeployPlan::Unsupported {
            message: format!(
                "There is no Forge agent for the {} architecture (x86_64 and aarch64 are supported).",
                probe.arch
            ),
        };
    };
    let reason = match (&probe.installed, &probe.sha256, local_sha256) {
        (None, _, _) => "the agent is not installed".to_string(),
        (Some(v), _, _) if v.trim() != version_line => format!("installed agent is {v}"),
        (Some(_), Some(remote), Some(local)) if !remote.eq_ignore_ascii_case(local) => {
            "the installed agent differs from this build".to_string()
        }
        _ => return DeployPlan::Start,
    };
    DeployPlan::Upload {
        arch: arch.to_string(),
        reason,
    }
}

/// Shell command that stores stdin as the agent binary (atomically, executable).
pub fn upload_command(version: &str) -> String {
    let dir = agent_dir(version);
    let path = agent_path(version);
    format!(
        "set -e; mkdir -p \"$HOME/{dir}\"; cat > \"$HOME/{path}.part\"; \
         chmod 755 \"$HOME/{path}.part\"; mv -f \"$HOME/{path}.part\" \"$HOME/{path}\""
    )
}

/// Shell command that starts the agent in a login shell (so `PATH` includes the user's
/// tools, e.g. Intelephense installed with npm) and prints [`READY_MARKER`] first.
pub fn start_command(version: &str) -> String {
    let path = agent_path(version);
    format!("exec \"${{SHELL:-/bin/sh}}\" -lc 'echo {READY_MARKER}; exec \"$HOME/{path}\" serve'")
}

/// Index just after [`READY_MARKER`] and its line break in `buf`, if it arrived.
pub fn find_ready(buf: &[u8]) -> Option<usize> {
    let marker = READY_MARKER.as_bytes();
    let pos = buf.windows(marker.len()).position(|w| w == marker)?;
    let mut end = pos + marker.len();
    if buf.get(end) == Some(&b'\r') {
        end += 1;
    }
    if buf.get(end) == Some(&b'\n') {
        Some(end + 1)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINE: &str = "forge-remote-agent 0.1.0 protocol 1";

    #[test]
    fn parses_probe_output_with_noise() {
        let out = "Welcome to Ubuntu!\nFORGE_OS=Linux\nFORGE_ARCH=x86_64\r\nFORGE_HOME=/home/deploy\nFORGE_AGENT=forge-remote-agent 0.1.0 protocol 1\nFORGE_SHA=ABC123\n";
        let p = parse_probe(out);
        assert_eq!(p.os, "Linux");
        assert_eq!(p.arch, "x86_64");
        assert_eq!(p.home, "/home/deploy");
        assert_eq!(p.installed.as_deref(), Some(LINE));
        assert_eq!(p.sha256.as_deref(), Some("abc123"));

        let fresh = parse_probe("FORGE_OS=Linux\nFORGE_ARCH=aarch64\nFORGE_HOME=/root\n");
        assert_eq!(fresh.installed, None);
    }

    fn probe(installed: Option<&str>, sha: Option<&str>) -> Probe {
        Probe {
            os: "Linux".into(),
            arch: "amd64".into(),
            home: "/home/deploy".into(),
            installed: installed.map(String::from),
            sha256: sha.map(String::from),
        }
    }

    #[test]
    fn plans_uploads_and_starts() {
        assert_eq!(
            plan(&probe(Some(LINE), Some("aa")), LINE, Some("AA")),
            DeployPlan::Start
        );
        assert_eq!(
            plan(&probe(Some(LINE), None), LINE, Some("aa")),
            DeployPlan::Start
        );
        assert!(matches!(
            plan(&probe(None, None), LINE, None),
            DeployPlan::Upload { ref arch, .. } if arch == "x86_64"
        ));
        assert!(matches!(
            plan(
                &probe(Some("forge-remote-agent 0.0.9 protocol 1"), None),
                LINE,
                None
            ),
            DeployPlan::Upload { .. }
        ));
        assert!(matches!(
            plan(&probe(Some(LINE), Some("bb")), LINE, Some("aa")),
            DeployPlan::Upload { ref reason, .. } if reason.contains("differs")
        ));
    }

    #[test]
    fn refuses_other_platforms() {
        let mac = Probe {
            os: "Darwin".into(),
            arch: "arm64".into(),
            ..Probe::default()
        };
        assert!(matches!(
            plan(&mac, LINE, None),
            DeployPlan::Unsupported { .. }
        ));
        let mips = Probe {
            os: "Linux".into(),
            arch: "mips".into(),
            ..Probe::default()
        };
        assert!(
            matches!(plan(&mips, LINE, None), DeployPlan::Unsupported { ref message } if message.contains("mips"))
        );
    }

    #[test]
    fn commands_use_the_versioned_cache_dir() {
        assert!(
            probe_command("0.1.0").contains("$HOME/.cache/forge-agent/0.1.0/forge-remote-agent")
        );
        assert!(upload_command("0.1.0").contains("mkdir -p \"$HOME/.cache/forge-agent/0.1.0\""));
        let start = start_command("0.1.0");
        assert!(start.contains("-lc"));
        assert!(start.contains("serve"));
        assert_eq!(bundled_name("aarch64"), "forge-remote-agent-aarch64-linux");
    }

    #[test]
    fn finds_the_ready_marker_after_login_noise() {
        let buf = b"Last login: today\nFORGE-AGENT-READY\n\x00\x00\x00\x05";
        let end = find_ready(buf).unwrap();
        assert_eq!(&buf[end..], b"\x00\x00\x00\x05");
        assert_eq!(find_ready(b"FORGE-AGENT-READY"), None);
        assert_eq!(find_ready(b"FORGE-AGENT-READY\r\nx"), Some(19));
    }
}
