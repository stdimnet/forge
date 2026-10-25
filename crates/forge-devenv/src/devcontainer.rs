//! `devcontainer.json` (https://containers.dev/implementors/json_reference/): where the
//! files are, the parsed configuration and its resolved form (paths made absolute,
//! `${localWorkspaceFolder}` and the other variables replaced, defaults applied).
//!
//! Supported: `image`, `build` (`dockerfile`, `context`, `args`, `target`; also the old
//! top-level `dockerFile` / `context`), `dockerComposeFile` + `service` + `runServices`,
//! `workspaceFolder`, `workspaceMount`, `forwardPorts`, the lifecycle commands
//! (`onCreateCommand`, `postCreateCommand`, `postStartCommand`), `remoteUser`,
//! `containerUser`, `containerEnv`, `remoteEnv`, `mounts`, `runArgs`, `overrideCommand` and
//! `features` (listed, not installed).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::jsonc;

/// Where `devcontainer.json` files are looked for, relative to the project folder.
pub const PRIMARY_CONFIG: &str = ".devcontainer/devcontainer.json";
pub const ROOT_CONFIG: &str = ".devcontainer.json";

/// A project-relative path (`/` or `\` separated) is one of the places a configuration
/// lives: `.devcontainer/devcontainer.json`, `.devcontainer.json` or
/// `.devcontainer/<name>/devcontainer.json`.
pub fn is_config_rel(rel: &str) -> bool {
    let rel = rel.replace('\\', "/");
    let rel = rel.trim_start_matches("./");
    if rel == PRIMARY_CONFIG || rel == ROOT_CONFIG {
        return true;
    }
    let parts: Vec<&str> = rel.split('/').collect();
    parts.len() == 3
        && parts[0] == ".devcontainer"
        && !parts[1].is_empty()
        && parts[2] == "devcontainer.json"
}

/// Any path whose file is a dev container configuration (for the gutter icon).
pub fn is_config_file(path: &str) -> bool {
    let path = path.replace('\\', "/");
    let name = path.rsplit('/').next().unwrap_or(&path);
    if name == ROOT_CONFIG {
        return true;
    }
    if name != "devcontainer.json" {
        return false;
    }
    let mut parts = path.rsplit('/').skip(1);
    matches!(
        (parts.next(), parts.next()),
        (Some(".devcontainer"), _) | (Some(_), Some(".devcontainer"))
    )
}

/// The 0-based line for the "Create Dev Container" gutter icon: the line with the opening
/// `{` of the top-level object (comments before it are skipped).
pub fn gutter_line(content: &str) -> Option<u32> {
    let stripped = jsonc::strip(content);
    let pos = stripped.find('{')?;
    Some(stripped[..pos].matches('\n').count() as u32)
}

/// A lifecycle command: a shell command line, an argument vector (no shell) or several
/// named commands that run in parallel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LifecycleCommand {
    Shell(String),
    Exec(Vec<String>),
    Parallel(BTreeMap<String, LifecycleCommand>),
}

impl LifecycleCommand {
    /// The argument vectors to run (`/bin/sh -c …` for shell lines), with their names for
    /// the build log. Empty commands are skipped.
    pub fn argvs(&self) -> Vec<(Option<String>, Vec<String>)> {
        match self {
            LifecycleCommand::Shell(s) if s.trim().is_empty() => Vec::new(),
            LifecycleCommand::Shell(s) => {
                vec![(None, vec!["/bin/sh".into(), "-c".into(), s.clone()])]
            }
            LifecycleCommand::Exec(argv) if argv.is_empty() => Vec::new(),
            LifecycleCommand::Exec(argv) => vec![(None, argv.clone())],
            LifecycleCommand::Parallel(map) => map
                .iter()
                .flat_map(|(name, c)| {
                    c.argvs()
                        .into_iter()
                        .map(|(_, argv)| (Some(name.clone()), argv))
                        .collect::<Vec<_>>()
                })
                .collect(),
        }
    }

    /// One line for the UI.
    pub fn display(&self) -> String {
        match self {
            LifecycleCommand::Shell(s) => s.clone(),
            LifecycleCommand::Exec(argv) => argv.join(" "),
            LifecycleCommand::Parallel(map) => map
                .iter()
                .map(|(k, v)| format!("{k}: {}", v.display()))
                .collect::<Vec<_>>()
                .join("; "),
        }
    }
}

/// `build` of `devcontainer.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildSection {
    #[serde(default)]
    pub dockerfile: Option<String>,
    #[serde(default)]
    pub context: Option<String>,
    #[serde(default)]
    pub args: BTreeMap<String, String>,
    #[serde(default)]
    pub target: Option<String>,
}

/// One or several compose files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OneOrMany {
    One(String),
    Many(Vec<String>),
}

impl OneOrMany {
    pub fn to_vec(&self) -> Vec<String> {
        match self {
            OneOrMany::One(s) => vec![s.clone()],
            OneOrMany::Many(v) => v.clone(),
        }
    }
}

/// An entry of `mounts`: the `--mount` string or an object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MountSpec {
    Text(String),
    Object {
        #[serde(default)]
        source: Option<String>,
        target: String,
        #[serde(default, rename = "type")]
        kind: Option<String>,
    },
}

impl MountSpec {
    /// The `--mount` argument.
    pub fn to_mount_arg(&self) -> String {
        match self {
            MountSpec::Text(s) => s.clone(),
            MountSpec::Object {
                source,
                target,
                kind,
            } => {
                let mut parts = vec![format!("type={}", kind.as_deref().unwrap_or("bind"))];
                if let Some(s) = source.as_deref().filter(|s| !s.is_empty()) {
                    parts.push(format!("source={s}"));
                }
                parts.push(format!("target={target}"));
                parts.join(",")
            }
        }
    }
}

/// `devcontainer.json` as written.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevContainerConfig {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub build: Option<BuildSection>,
    /// Old spelling of `build.dockerfile`.
    #[serde(default, rename = "dockerFile")]
    pub docker_file: Option<String>,
    /// Old spelling of `build.context`.
    #[serde(default)]
    pub context: Option<String>,
    #[serde(default)]
    pub docker_compose_file: Option<OneOrMany>,
    #[serde(default)]
    pub service: Option<String>,
    #[serde(default)]
    pub run_services: Vec<String>,
    #[serde(default)]
    pub workspace_folder: Option<String>,
    #[serde(default)]
    pub workspace_mount: Option<String>,
    #[serde(default)]
    pub forward_ports: Vec<Value>,
    #[serde(default)]
    pub on_create_command: Option<LifecycleCommand>,
    #[serde(default)]
    pub post_create_command: Option<LifecycleCommand>,
    #[serde(default)]
    pub post_start_command: Option<LifecycleCommand>,
    #[serde(default)]
    pub remote_user: Option<String>,
    #[serde(default)]
    pub container_user: Option<String>,
    #[serde(default)]
    pub container_env: BTreeMap<String, String>,
    #[serde(default)]
    pub remote_env: BTreeMap<String, Option<String>>,
    #[serde(default)]
    pub mounts: Vec<MountSpec>,
    #[serde(default)]
    pub run_args: Vec<String>,
    #[serde(default)]
    pub override_command: Option<bool>,
    #[serde(default)]
    pub features: BTreeMap<String, Value>,
}

/// Parse `devcontainer.json` (JSONC). Errors are worded for the build log.
pub fn parse(text: &str) -> Result<DevContainerConfig, String> {
    let value = jsonc::parse(text).map_err(|e| format!("devcontainer.json is not valid: {e}"))?;
    if !value.is_object() {
        return Err("devcontainer.json must contain an object".into());
    }
    serde_json::from_value(value).map_err(|e| format!("devcontainer.json: {e}"))
}

/// How the container comes to be.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ContainerSource {
    Image {
        image: String,
    },
    #[serde(rename_all = "camelCase")]
    Dockerfile {
        /// Absolute path of the Dockerfile.
        dockerfile: String,
        /// Absolute build context.
        context: String,
        args: BTreeMap<String, String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Compose {
        /// Absolute compose file paths.
        files: Vec<String>,
        service: String,
        run_services: Vec<String>,
    },
}

/// A forwarded port: a container port published on `127.0.0.1`, or a `service:port` of
/// another compose service (listed only).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForwardPort {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    pub port: u16,
}

/// A feature of `features` (listed; Forge does not install features yet).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureRef {
    pub id: String,
    /// Options as written (`{}` or a version string).
    pub options: Value,
}

/// The configuration ready to run: absolute paths, variables replaced, defaults applied.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedConfig {
    /// Stable id of this folder + configuration (container labels and names).
    pub id: String,
    /// `name`, or the folder's name.
    pub name: String,
    /// Absolute path of `devcontainer.json`.
    pub config_path: String,
    /// The local project folder mounted into the container.
    pub local_folder: String,
    pub source: ContainerSource,
    /// The project folder inside the container.
    pub workspace_folder: String,
    /// `--mount` for the sources (`None` for compose: the compose file mounts them).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_mount: Option<String>,
    pub mounts: Vec<String>,
    pub forward_ports: Vec<ForwardPort>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_create_command: Option<LifecycleCommand>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post_create_command: Option<LifecycleCommand>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post_start_command: Option<LifecycleCommand>,
    /// User for the agent, terminals and lifecycle commands (`remoteUser`, else
    /// `containerUser`; `None` = the image's user).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_user: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container_user: Option<String>,
    pub container_env: BTreeMap<String, String>,
    /// `remoteEnv` without the entries set to `null`.
    pub remote_env: BTreeMap<String, String>,
    pub run_args: Vec<String>,
    /// Keep the container running with a sleep loop instead of its own command.
    pub override_command: bool,
    pub features: Vec<FeatureRef>,
    /// Things Forge ignores or could not read, for the build log.
    pub warnings: Vec<String>,
}

/// What variables refer to while resolving.
pub struct LocalContext<'a> {
    /// The local project folder (absolute).
    pub folder: &'a str,
    /// Absolute path of `devcontainer.json`.
    pub config_path: &'a str,
    /// Environment of this machine (`${localEnv:NAME}`).
    pub env: &'a dyn Fn(&str) -> Option<String>,
}

/// FNV-1a 64 of `text` as 16 hex digits.
fn fnv_hex(text: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// Stable id of a folder and its configuration file.
pub fn config_id(folder: &str, config_path: &str) -> String {
    fnv_hex(&format!("{folder}\n{config_path}"))[..12].to_string()
}

/// The last path segment (`/` or `\` separated).
pub fn basename(path: &str) -> &str {
    path.trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(path)
}

/// The parent folder of a path (`/` or `\` separated).
pub fn parent(path: &str) -> &str {
    let trimmed = path.trim_end_matches(['/', '\\']);
    match trimmed.rfind(['/', '\\']) {
        Some(0) => &trimmed[..1],
        Some(i) => &trimmed[..i],
        None => "",
    }
}

fn is_absolute(path: &str) -> bool {
    let b = path.as_bytes();
    path.starts_with('/')
        || path.starts_with('\\')
        || (b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':')
}

/// `rel` against the folder `base`, with `.` and `..` folded. Uses `\` when `base` does.
pub fn join(base: &str, rel: &str) -> String {
    let joined = if is_absolute(rel) || base.is_empty() {
        rel.to_string()
    } else {
        format!("{}/{}", base.trim_end_matches(['/', '\\']), rel)
    };
    let windows = joined.contains('\\') || (joined.len() >= 2 && joined.as_bytes()[1] == b':');
    let unc = joined.starts_with("\\\\") || joined.starts_with("//");
    let leading = joined.starts_with('/') || joined.starts_with('\\');
    let mut parts: Vec<&str> = Vec::new();
    for part in joined.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => {
                // Never climb above a drive letter or the root.
                if parts
                    .last()
                    .is_some_and(|p| *p != ".." && !p.ends_with(':'))
                {
                    parts.pop();
                } else if !leading && parts.is_empty() {
                    parts.push("..");
                }
            }
            p => parts.push(p),
        }
    }
    let sep = if windows { "\\" } else { "/" };
    let body = parts.join(sep);
    if unc {
        format!("{sep}{sep}{body}")
    } else if leading {
        format!("{sep}{body}")
    } else {
        body
    }
}

/// Replace `${localWorkspaceFolder}`, `${localWorkspaceFolderBasename}`,
/// `${containerWorkspaceFolder}`, `${containerWorkspaceFolderBasename}`, `${devcontainerId}`
/// and `${localEnv:NAME[:default]}`. Other variables (e.g. `${containerEnv:PATH}`) stay.
pub fn substitute(text: &str, ctx: &LocalContext, container_folder: &str, id: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find('}') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let var = &after[..end];
        let value = match var {
            "localWorkspaceFolder" => Some(ctx.folder.to_string()),
            "localWorkspaceFolderBasename" => Some(basename(ctx.folder).to_string()),
            "containerWorkspaceFolder" => Some(container_folder.to_string()),
            "containerWorkspaceFolderBasename" => Some(basename(container_folder).to_string()),
            "devcontainerId" => Some(id.to_string()),
            v => match v
                .strip_prefix("localEnv:")
                .or_else(|| v.strip_prefix("env:"))
            {
                Some(spec) => {
                    let (name, default) = match spec.split_once(':') {
                        Some((n, d)) => (n, Some(d)),
                        None => (spec, None),
                    };
                    Some(
                        (ctx.env)(name)
                            .or_else(|| default.map(str::to_string))
                            .unwrap_or_default(),
                    )
                }
                None => None,
            },
        };
        match value {
            Some(v) => out.push_str(&v),
            None => out.push_str(&rest[start..start + 2 + end + 1]),
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

fn parse_port(value: &Value) -> Option<ForwardPort> {
    match value {
        Value::Number(n) => n
            .as_u64()
            .and_then(|p| u16::try_from(p).ok())
            .map(|port| ForwardPort { host: None, port }),
        Value::String(s) => match s.rsplit_once(':') {
            Some((host, port)) => port.trim().parse().ok().map(|port| ForwardPort {
                host: Some(host.trim().to_string()),
                port,
            }),
            None => s
                .trim()
                .parse()
                .ok()
                .map(|port| ForwardPort { host: None, port }),
        },
        _ => None,
    }
}

impl DevContainerConfig {
    /// Resolve against the local folder and configuration path.
    pub fn resolve(&self, ctx: &LocalContext) -> Result<ResolvedConfig, String> {
        let id = config_id(ctx.folder, ctx.config_path);
        let config_dir = parent(ctx.config_path).to_string();
        let mut warnings = Vec::new();
        let compose_files = self.docker_compose_file.as_ref().map(OneOrMany::to_vec);
        let dockerfile = self
            .build
            .as_ref()
            .and_then(|b| b.dockerfile.clone())
            .or_else(|| self.docker_file.clone());
        let is_compose = compose_files.as_ref().is_some_and(|f| !f.is_empty());

        let default_folder = if is_compose {
            "/".to_string()
        } else {
            format!("/workspaces/{}", basename(ctx.folder))
        };
        let pre = |s: &str| substitute(s, ctx, &default_folder, &id);
        let workspace_folder = self
            .workspace_folder
            .as_deref()
            .map(pre)
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| default_folder.clone());
        let sub = |s: &str| substitute(s, ctx, &workspace_folder, &id);

        let source = if is_compose {
            let Some(service) = self.service.clone().filter(|s| !s.is_empty()) else {
                return Err("devcontainer.json: dockerComposeFile needs a service".into());
            };
            ContainerSource::Compose {
                files: compose_files
                    .unwrap_or_default()
                    .iter()
                    .map(|f| join(&config_dir, &sub(f)))
                    .collect(),
                service,
                run_services: self.run_services.clone(),
            }
        } else if let Some(dockerfile) = dockerfile {
            let context = self
                .build
                .as_ref()
                .and_then(|b| b.context.clone())
                .or_else(|| self.context.clone())
                .unwrap_or_else(|| ".".into());
            ContainerSource::Dockerfile {
                dockerfile: join(&config_dir, &sub(&dockerfile)),
                context: join(&config_dir, &sub(&context)),
                args: self
                    .build
                    .as_ref()
                    .map(|b| b.args.iter().map(|(k, v)| (k.clone(), sub(v))).collect())
                    .unwrap_or_default(),
                target: self.build.as_ref().and_then(|b| b.target.clone()),
            }
        } else if let Some(image) = self.image.clone().filter(|i| !i.trim().is_empty()) {
            ContainerSource::Image { image: sub(&image) }
        } else {
            return Err(
                "devcontainer.json needs an image, a build.dockerfile or a dockerComposeFile"
                    .into(),
            );
        };

        let workspace_mount = if is_compose {
            if self.workspace_mount.is_some() {
                warnings.push(
                    "workspaceMount is ignored for Docker Compose (the compose file mounts the sources)"
                        .into(),
                );
            }
            None
        } else {
            Some(match &self.workspace_mount {
                Some(m) if !m.trim().is_empty() => sub(m),
                _ => format!(
                    "type=bind,source={},target={workspace_folder},consistency=cached",
                    ctx.folder
                ),
            })
        };

        let mut forward_ports = Vec::new();
        for p in &self.forward_ports {
            match parse_port(p) {
                Some(port) => forward_ports.push(port),
                None => warnings.push(format!("forwardPorts: cannot read {p}")),
            }
        }

        let features: Vec<FeatureRef> = self
            .features
            .iter()
            .map(|(id, options)| FeatureRef {
                id: id.clone(),
                options: options.clone(),
            })
            .collect();
        if !features.is_empty() {
            warnings.push(format!(
                "Features are listed but not installed by Forge: {}",
                features
                    .iter()
                    .map(|f| f.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }

        let sub_cmd = |c: &LifecycleCommand| -> LifecycleCommand {
            match c {
                LifecycleCommand::Shell(s) => LifecycleCommand::Shell(sub(s)),
                LifecycleCommand::Exec(v) => {
                    LifecycleCommand::Exec(v.iter().map(|a| sub(a)).collect())
                }
                LifecycleCommand::Parallel(m) => LifecycleCommand::Parallel(
                    m.iter()
                        .map(|(k, v)| {
                            let v = match v {
                                LifecycleCommand::Shell(s) => LifecycleCommand::Shell(sub(s)),
                                LifecycleCommand::Exec(a) => {
                                    LifecycleCommand::Exec(a.iter().map(|x| sub(x)).collect())
                                }
                                other => other.clone(),
                            };
                            (k.clone(), v)
                        })
                        .collect(),
                ),
            }
        };

        let container_user = self.container_user.clone().filter(|u| !u.is_empty());
        Ok(ResolvedConfig {
            name: self
                .name
                .clone()
                .filter(|n| !n.trim().is_empty())
                .unwrap_or_else(|| basename(ctx.folder).to_string()),
            config_path: ctx.config_path.to_string(),
            local_folder: ctx.folder.to_string(),
            source,
            workspace_mount,
            mounts: self.mounts.iter().map(|m| sub(&m.to_mount_arg())).collect(),
            forward_ports,
            on_create_command: self.on_create_command.as_ref().map(sub_cmd),
            post_create_command: self.post_create_command.as_ref().map(sub_cmd),
            post_start_command: self.post_start_command.as_ref().map(sub_cmd),
            remote_user: self
                .remote_user
                .clone()
                .filter(|u| !u.is_empty())
                .or_else(|| container_user.clone()),
            container_user,
            container_env: self
                .container_env
                .iter()
                .map(|(k, v)| (k.clone(), sub(v)))
                .collect(),
            remote_env: self
                .remote_env
                .iter()
                .filter_map(|(k, v)| v.as_ref().map(|v| (k.clone(), sub(v))))
                .collect(),
            run_args: self.run_args.iter().map(|a| sub(a)).collect(),
            override_command: self.override_command.unwrap_or(!is_compose),
            features,
            warnings,
            workspace_folder,
            id,
        })
    }
}

/// Parse and resolve in one step.
pub fn load(text: &str, ctx: &LocalContext) -> Result<ResolvedConfig, String> {
    parse(text)?.resolve(ctx)
}

impl ResolvedConfig {
    /// "Image php:8.3", "Dockerfile .devcontainer/Dockerfile", "Compose service app".
    pub fn source_label(&self) -> String {
        match &self.source {
            ContainerSource::Image { image } => format!("Image {image}"),
            ContainerSource::Dockerfile { dockerfile, .. } => {
                format!("Dockerfile {}", relative_to(&self.local_folder, dockerfile))
            }
            ContainerSource::Compose { service, .. } => format!("Docker Compose service {service}"),
        }
    }

    /// The compose project name (`<folder>_devcontainer`, like the Dev Containers CLI).
    pub fn compose_project(&self) -> String {
        let base: String = basename(&self.local_folder)
            .to_lowercase()
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .collect();
        format!(
            "{}_devcontainer",
            if base.is_empty() { "forge" } else { &base }
        )
    }
}

/// `path` relative to `base` when it is inside it, else unchanged; `/` separators.
pub fn relative_to(base: &str, path: &str) -> String {
    let b = base.replace('\\', "/");
    let p = path.replace('\\', "/");
    match p.strip_prefix(b.trim_end_matches('/')) {
        Some(rest) if rest.starts_with('/') => rest[1..].to_string(),
        _ => p,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(name: &str) -> Option<String> {
        (name == "HOME").then(|| "/home/me".to_string())
    }

    fn ctx<'a>(folder: &'a str, config: &'a str) -> LocalContext<'a> {
        LocalContext {
            folder,
            config_path: config,
            env: &env,
        }
    }

    const IMAGE: &str = r#"{
        // Laravel on PHP 8.3
        "name": "Shop",
        "image": "mcr.microsoft.com/devcontainers/php:8.3",
        "forwardPorts": [8000, "db:5432", "x"],
        "postCreateCommand": "composer install",
        "postStartCommand": ["php", "artisan", "serve"],
        "remoteUser": "vscode",
        "containerEnv": { "APP_ENV": "local", "HOST_HOME": "${localEnv:HOME}" },
        "remoteEnv": { "PATH": "${containerEnv:PATH}:/opt/bin", "UNSET": null },
        "mounts": [
            "source=${localEnv:HOME}/.composer,target=/home/vscode/.composer,type=bind",
            { "source": "shop-node", "target": "${containerWorkspaceFolder}/node_modules", "type": "volume" }
        ],
        "features": {
            "ghcr.io/devcontainers/features/node:1": { "version": "20" },
            "ghcr.io/devcontainers/features/github-cli:1": {},
        },
    }"#;

    #[test]
    fn resolves_an_image_configuration() {
        let c = load(
            IMAGE,
            &ctx(
                "/home/me/shop",
                "/home/me/shop/.devcontainer/devcontainer.json",
            ),
        )
        .unwrap();
        assert_eq!(c.name, "Shop");
        assert_eq!(
            c.source,
            ContainerSource::Image {
                image: "mcr.microsoft.com/devcontainers/php:8.3".into()
            }
        );
        assert_eq!(c.workspace_folder, "/workspaces/shop");
        assert_eq!(
            c.workspace_mount.as_deref(),
            Some("type=bind,source=/home/me/shop,target=/workspaces/shop,consistency=cached")
        );
        assert_eq!(
            c.forward_ports,
            vec![
                ForwardPort {
                    host: None,
                    port: 8000
                },
                ForwardPort {
                    host: Some("db".into()),
                    port: 5432
                }
            ]
        );
        assert!(c.warnings.iter().any(|w| w.contains("forwardPorts")));
        assert_eq!(c.container_env["HOST_HOME"], "/home/me");
        assert_eq!(
            c.remote_env.get("PATH").unwrap(),
            "${containerEnv:PATH}:/opt/bin"
        );
        assert!(!c.remote_env.contains_key("UNSET"));
        assert_eq!(
            c.mounts,
            vec![
                "source=/home/me/.composer,target=/home/vscode/.composer,type=bind".to_string(),
                "type=volume,source=shop-node,target=/workspaces/shop/node_modules".to_string(),
            ]
        );
        assert_eq!(c.remote_user.as_deref(), Some("vscode"));
        assert!(c.override_command);
        assert_eq!(c.features.len(), 2);
        assert!(c.warnings.iter().any(|w| w.contains("github-cli")));
        assert_eq!(
            c.post_create_command.as_ref().unwrap().argvs(),
            vec![(
                None,
                vec![
                    "/bin/sh".to_string(),
                    "-c".into(),
                    "composer install".into()
                ]
            )]
        );
        assert_eq!(
            c.post_start_command.as_ref().unwrap().argvs()[0].1,
            vec!["php", "artisan", "serve"]
        );
        assert_eq!(
            c.id,
            config_id(
                "/home/me/shop",
                "/home/me/shop/.devcontainer/devcontainer.json"
            )
        );
        assert_eq!(c.id.len(), 12);
    }

    #[test]
    fn resolves_dockerfile_paths_against_the_config_folder() {
        let text = r#"{
            "build": { "dockerfile": "Dockerfile", "context": "..", "args": { "PHP": "8.3", "DIR": "${localWorkspaceFolderBasename}" }, "target": "dev" },
            "workspaceFolder": "/var/www/${localWorkspaceFolderBasename}",
            "workspaceMount": "source=${localWorkspaceFolder},target=${containerWorkspaceFolder},type=bind"
        }"#;
        let c = load(
            text,
            &ctx("/src/shop", "/src/shop/.devcontainer/php/devcontainer.json"),
        )
        .unwrap();
        assert_eq!(
            c.source,
            ContainerSource::Dockerfile {
                dockerfile: "/src/shop/.devcontainer/php/Dockerfile".into(),
                context: "/src/shop/.devcontainer".into(),
                args: [
                    ("DIR".to_string(), "shop".to_string()),
                    ("PHP".into(), "8.3".into())
                ]
                .into(),
                target: Some("dev".into()),
            }
        );
        assert_eq!(c.name, "shop");
        assert_eq!(c.workspace_folder, "/var/www/shop");
        assert_eq!(
            c.workspace_mount.as_deref(),
            Some("source=/src/shop,target=/var/www/shop,type=bind")
        );
        assert!(c.source_label().contains(".devcontainer/php/Dockerfile"));
    }

    #[test]
    fn old_spelling_and_windows_paths() {
        let text = r#"{ "dockerFile": "../docker/Dockerfile" }"#;
        let c = load(
            text,
            &ctx(
                r"C:\src\shop",
                r"C:\src\shop\.devcontainer\devcontainer.json",
            ),
        )
        .unwrap();
        match c.source {
            ContainerSource::Dockerfile {
                dockerfile,
                context,
                ..
            } => {
                assert_eq!(dockerfile, r"C:\src\shop\docker\Dockerfile");
                assert_eq!(context, r"C:\src\shop\.devcontainer");
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(c.workspace_folder, "/workspaces/shop");
    }

    #[test]
    fn resolves_compose_configurations() {
        let text = r#"{
            "name": "Shop (Compose)",
            "dockerComposeFile": ["../compose.yaml", "compose.dev.yaml"],
            "service": "app",
            "runServices": ["app", "db"],
            "workspaceFolder": "/var/www/html",
            "postCreateCommand": { "deps": "composer install", "assets": ["npm", "ci"] }
        }"#;
        let c = load(
            text,
            &ctx("/src/shop", "/src/shop/.devcontainer/devcontainer.json"),
        )
        .unwrap();
        assert_eq!(
            c.source,
            ContainerSource::Compose {
                files: vec![
                    "/src/shop/compose.yaml".into(),
                    "/src/shop/.devcontainer/compose.dev.yaml".into()
                ],
                service: "app".into(),
                run_services: vec!["app".into(), "db".into()],
            }
        );
        assert_eq!(c.workspace_mount, None);
        assert!(!c.override_command);
        assert_eq!(c.compose_project(), "shop_devcontainer");
        let cmds = c.post_create_command.unwrap().argvs();
        assert_eq!(
            cmds[0],
            (Some("assets".into()), vec!["npm".to_string(), "ci".into()])
        );
        assert_eq!(cmds[1].0.as_deref(), Some("deps"));
        let no_service = load(
            r#"{"dockerComposeFile": "c.yml"}"#,
            &ctx("/a", "/a/.devcontainer.json"),
        );
        assert!(no_service.unwrap_err().contains("service"));
    }

    #[test]
    fn reports_missing_sources_and_bad_json() {
        assert!(load("{}", &ctx("/a", "/a/.devcontainer.json"))
            .unwrap_err()
            .contains("image"));
        assert!(parse("{ \"image\": ").unwrap_err().contains("not valid"));
        assert!(parse("[1]").unwrap_err().contains("object"));
        assert!(parse(r#"{"forwardPorts": "8000"}"#).is_err());
    }

    #[test]
    fn config_locations() {
        assert!(is_config_rel(".devcontainer/devcontainer.json"));
        assert!(is_config_rel(".devcontainer.json"));
        assert!(is_config_rel(r".devcontainer\php\devcontainer.json"));
        assert!(!is_config_rel(".devcontainer/a/b/devcontainer.json"));
        assert!(!is_config_rel("devcontainer.json"));
        assert!(is_config_file("/src/shop/.devcontainer/devcontainer.json"));
        assert!(is_config_file(
            "/src/shop/.devcontainer/php/devcontainer.json"
        ));
        assert!(is_config_file(r"C:\src\.devcontainer.json"));
        assert!(!is_config_file("/src/shop/devcontainer.json"));
        assert_eq!(gutter_line("// header\n/* { */\n{\n}"), Some(2));
        assert_eq!(gutter_line("nothing"), None);
    }

    #[test]
    fn paths_and_substitution() {
        assert_eq!(join("/a/b", "../c/./d"), "/a/c/d");
        assert_eq!(join("/a", "/abs"), "/abs");
        assert_eq!(join(r"C:\a", r"..\..\x"), r"C:\x");
        assert_eq!(join(r"\\wsl$\Ubuntu\home", "me"), r"\\wsl$\Ubuntu\home\me");
        assert_eq!(parent("/a/b/c.json"), "/a/b");
        assert_eq!(parent("/c.json"), "/");
        assert_eq!(basename(r"C:\src\shop\"), "shop");
        let c = ctx("/src/shop", "/src/shop/.devcontainer.json");
        assert_eq!(
            substitute(
                "${localEnv:NOPE:fallback}-${localEnv:NOPE}-${containerEnv:PATH}-${unclosed",
                &c,
                "/w",
                "id"
            ),
            "fallback--${containerEnv:PATH}-${unclosed"
        );
        assert_eq!(
            relative_to("/src/shop", "/src/shop/.devcontainer/Dockerfile"),
            ".devcontainer/Dockerfile"
        );
    }
}
