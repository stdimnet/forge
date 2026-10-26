//! Docker CLI command lines for dev containers (arguments after the `docker` program):
//! building the image, creating the container with the sources mounted, Docker Compose
//! with an override file, lifecycle commands and the Forge agent through `docker exec`,
//! and the list of Forge's dev containers from `docker ps`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::devcontainer::{ContainerSource, ResolvedConfig};

/// Container labels. The `devcontainer.*` ones are the Dev Containers CLI's, so other
/// tools recognise the container.
pub const LABEL_ID: &str = "forge.devcontainer.id";
pub const LABEL_NAME: &str = "forge.devcontainer.name";
pub const LABEL_WORKSPACE: &str = "forge.devcontainer.workspace";
pub const LABEL_USER: &str = "forge.devcontainer.user";
pub const LABEL_LOCAL_FOLDER: &str = "devcontainer.local_folder";
pub const LABEL_CONFIG_FILE: &str = "devcontainer.config_file";
pub const LABEL_COMPOSE_PROJECT: &str = "com.docker.compose.project";

/// Keeps a container alive when `overrideCommand` is on (the Dev Containers CLI's loop).
pub const SLEEP_LOOP: &str =
    "echo Container started; trap \"exit 0\" 15; while sleep 1 & wait $!; do :; done";

fn s(v: &str) -> String {
    v.to_string()
}

/// The labels of a dev container.
pub fn labels(cfg: &ResolvedConfig) -> Vec<(String, String)> {
    let mut l = vec![
        (s(LABEL_ID), cfg.id.clone()),
        (s(LABEL_NAME), cfg.name.clone()),
        (s(LABEL_WORKSPACE), cfg.workspace_folder.clone()),
        (s(LABEL_LOCAL_FOLDER), cfg.local_folder.clone()),
        (s(LABEL_CONFIG_FILE), cfg.config_path.clone()),
    ];
    if let Some(u) = &cfg.remote_user {
        l.push((s(LABEL_USER), u.clone()));
    }
    l
}

/// Container name: `forge-<folder>-<id>` (Docker allows `[a-zA-Z0-9][a-zA-Z0-9_.-]*`).
pub fn container_name(cfg: &ResolvedConfig) -> String {
    let base: String = crate::devcontainer::basename(&cfg.local_folder)
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let base = base.trim_matches(['-', '.', '_']);
    format!(
        "forge-{}-{}",
        if base.is_empty() { "dev" } else { base },
        &cfg.id[..8.min(cfg.id.len())]
    )
}

/// Tag of the image built from a Dockerfile.
pub fn image_tag(cfg: &ResolvedConfig) -> String {
    format!("forge-devcontainer-{}", cfg.id)
}

/// `docker build` for a Dockerfile configuration (`None` for the other kinds).
pub fn build_args(cfg: &ResolvedConfig, no_cache: bool) -> Option<Vec<String>> {
    let ContainerSource::Dockerfile {
        dockerfile,
        context,
        args,
        target,
    } = &cfg.source
    else {
        return None;
    };
    let mut a = vec![
        s("build"),
        s("-f"),
        dockerfile.clone(),
        s("-t"),
        image_tag(cfg),
    ];
    for (k, v) in args {
        a.push(s("--build-arg"));
        a.push(format!("{k}={v}"));
    }
    if let Some(t) = target {
        a.push(s("--target"));
        a.push(t.clone());
    }
    if no_cache {
        a.push(s("--no-cache"));
    }
    a.push(s("--label"));
    a.push(format!("{LABEL_ID}={}", cfg.id));
    a.push(context.clone());
    Some(a)
}

/// `docker pull` for an image configuration.
pub fn pull_args(cfg: &ResolvedConfig) -> Option<Vec<String>> {
    match &cfg.source {
        ContainerSource::Image { image } => Some(vec![s("pull"), image.clone()]),
        _ => None,
    }
}

/// `docker run -d …` creating the container of an image or Dockerfile configuration.
pub fn run_args(cfg: &ResolvedConfig) -> Option<Vec<String>> {
    let image = match &cfg.source {
        ContainerSource::Image { image } => image.clone(),
        ContainerSource::Dockerfile { .. } => image_tag(cfg),
        ContainerSource::Compose { .. } => return None,
    };
    let mut a = vec![s("run"), s("-d"), s("--name"), container_name(cfg)];
    for (k, v) in labels(cfg) {
        a.push(s("--label"));
        a.push(format!("{k}={v}"));
    }
    if let Some(m) = &cfg.workspace_mount {
        a.push(s("--mount"));
        a.push(m.clone());
    }
    for m in &cfg.mounts {
        a.push(s("--mount"));
        a.push(m.clone());
    }
    for (k, v) in &cfg.container_env {
        a.push(s("-e"));
        a.push(format!("{k}={v}"));
    }
    for p in cfg.forward_ports.iter().filter(|p| p.host.is_none()) {
        a.push(s("-p"));
        a.push(format!("127.0.0.1:{0}:{0}", p.port));
    }
    if let Some(u) = &cfg.container_user {
        a.push(s("-u"));
        a.push(u.clone());
    }
    a.extend(cfg.run_args.iter().cloned());
    if cfg.override_command {
        a.extend([
            s("--entrypoint"),
            s("/bin/sh"),
            image,
            s("-c"),
            s(SLEEP_LOOP),
        ]);
    } else {
        a.push(image);
    }
    Some(a)
}

fn yaml_str(v: &str) -> String {
    serde_json::to_string(v).unwrap_or_else(|_| format!("\"{v}\""))
}

/// The compose override file Forge adds for a Compose configuration: labels, the extra
/// mounts and environment, and the sleep loop when `overrideCommand` is on.
pub fn compose_override(cfg: &ResolvedConfig) -> Option<String> {
    let ContainerSource::Compose { service, .. } = &cfg.source else {
        return None;
    };
    let mut y = String::from("# Written by Forge for the dev container; do not edit.\nservices:\n");
    y.push_str(&format!("  {}:\n", yaml_str(service)));
    y.push_str("    labels:\n");
    for (k, v) in labels(cfg) {
        y.push_str(&format!("      {}: {}\n", yaml_str(&k), yaml_str(&v)));
    }
    if !cfg.container_env.is_empty() {
        y.push_str("    environment:\n");
        for (k, v) in &cfg.container_env {
            y.push_str(&format!("      {}: {}\n", yaml_str(k), yaml_str(v)));
        }
    }
    let volumes: Vec<String> = cfg
        .mounts
        .iter()
        .filter_map(|m| mount_to_volume(m))
        .collect();
    if !volumes.is_empty() {
        y.push_str("    volumes:\n");
        for v in volumes {
            y.push_str(&format!("      - {}\n", yaml_str(&v)));
        }
    }
    if let Some(u) = &cfg.container_user {
        y.push_str(&format!("    user: {}\n", yaml_str(u)));
    }
    if cfg.override_command {
        y.push_str("    entrypoint: [\"/bin/sh\", \"-c\"]\n");
        y.push_str(&format!("    command: [{}]\n", yaml_str(SLEEP_LOOP)));
    }
    Some(y)
}

/// A `--mount` string as a compose short volume (`source:target[:ro]`).
fn mount_to_volume(mount: &str) -> Option<String> {
    let mut fields: BTreeMap<&str, &str> = BTreeMap::new();
    let mut readonly = false;
    for part in mount.split(',') {
        match part.split_once('=') {
            Some((k, v)) => {
                fields.insert(k.trim(), v.trim());
            }
            None if matches!(part.trim(), "readonly" | "ro") => readonly = true,
            None => {}
        }
    }
    let source = fields.get("source").or_else(|| fields.get("src"))?;
    let target = fields
        .get("target")
        .or_else(|| fields.get("dst"))
        .or_else(|| fields.get("destination"))?;
    readonly |= fields.get("readonly").is_some_and(|v| *v != "false");
    Some(format!(
        "{source}:{target}{}",
        if readonly { ":ro" } else { "" }
    ))
}

fn compose_base(cfg: &ResolvedConfig, override_file: Option<&str>) -> Option<Vec<String>> {
    let ContainerSource::Compose { files, .. } = &cfg.source else {
        return None;
    };
    let mut a = vec![s("compose"), s("-p"), cfg.compose_project()];
    for f in files {
        a.push(s("-f"));
        a.push(f.clone());
    }
    if let Some(o) = override_file {
        a.push(s("-f"));
        a.push(o.to_string());
    }
    Some(a)
}

/// `docker compose … up -d --build <service> <runServices…>`.
pub fn compose_up_args(
    cfg: &ResolvedConfig,
    override_file: &str,
    rebuild: bool,
) -> Option<Vec<String>> {
    let ContainerSource::Compose {
        service,
        run_services,
        ..
    } = &cfg.source
    else {
        return None;
    };
    let mut a = compose_base(cfg, Some(override_file))?;
    a.extend([s("up"), s("-d")]);
    a.push(s("--build"));
    if rebuild {
        a.push(s("--force-recreate"));
    }
    a.push(service.clone());
    for r in run_services.iter().filter(|r| *r != service) {
        a.push(r.clone());
    }
    Some(a)
}

/// `docker compose … down` (Delete for a Compose dev container).
pub fn compose_down_args(cfg: &ResolvedConfig, override_file: &str) -> Option<Vec<String>> {
    let mut a = compose_base(cfg, Some(override_file))?;
    a.push(s("down"));
    Some(a)
}

/// `docker ps` finding the container of a configuration (`ID\tState` lines).
pub fn find_args(id: &str) -> Vec<String> {
    vec![
        s("ps"),
        s("-a"),
        s("--filter"),
        format!("label={LABEL_ID}={id}"),
        s("--format"),
        s("{{.ID}}\t{{.State}}"),
    ]
}

/// Parse [`find_args`] output: the first container and whether it runs.
pub fn parse_find(output: &str) -> Option<(String, bool)> {
    output.lines().find_map(|l| {
        let (id, state) = l.trim().split_once('\t').unwrap_or((l.trim(), ""));
        (!id.is_empty()).then(|| (id.to_string(), state.trim() == "running"))
    })
}

/// `docker exec` running `argv` in the container (shared with the run targets of
/// `forge-runtargets`).
pub use forge_runtargets::target::docker_exec_args as exec_args;

/// `docker exec -it … <shell>` for a terminal in the container.
pub fn terminal_args(container: &str, user: Option<&str>, workdir: &str) -> Vec<String> {
    let mut a = vec![s("exec"), s("-it")];
    if let Some(u) = user.filter(|u| !u.is_empty()) {
        a.push(s("-u"));
        a.push(u.to_string());
    }
    a.extend([s("-w"), workdir.to_string(), container.to_string()]);
    a.extend([s("/bin/sh"), s("-c"), s("exec \"${SHELL:-/bin/sh}\" -l")]);
    a
}

pub fn start_args(container: &str) -> Vec<String> {
    vec![s("start"), container.to_string()]
}

pub fn stop_args(container: &str) -> Vec<String> {
    vec![s("stop"), container.to_string()]
}

pub fn remove_args(container: &str) -> Vec<String> {
    vec![s("rm"), s("-f"), container.to_string()]
}

/// The fields [`list_args`] prints, tab-separated.
const LIST_FIELDS: &[&str] = &[
    "{{.ID}}",
    "{{.Names}}",
    "{{.Image}}",
    "{{.State}}",
    "{{.Status}}",
    "{{.CreatedAt}}",
];

/// `docker ps -a` listing Forge's dev containers.
pub fn list_args() -> Vec<String> {
    let mut fields: Vec<String> = LIST_FIELDS.iter().map(|f| f.to_string()).collect();
    for l in [
        LABEL_ID,
        LABEL_NAME,
        LABEL_WORKSPACE,
        LABEL_USER,
        LABEL_LOCAL_FOLDER,
        LABEL_CONFIG_FILE,
        LABEL_COMPOSE_PROJECT,
    ] {
        fields.push(format!("{{{{.Label \"{l}\"}}}}"));
    }
    vec![
        s("ps"),
        s("-a"),
        s("--filter"),
        format!("label={LABEL_ID}"),
        s("--format"),
        fields.join("\t"),
    ]
}

/// A dev container known to Docker (Welcome screen > Dev Containers).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevContainerInstance {
    /// Container id (short).
    pub container_id: String,
    /// Container name.
    pub container_name: String,
    pub image: String,
    /// `running`, `exited`, `created`, `paused`, …
    pub state: String,
    /// `Up 2 hours`, `Exited (0) 3 days ago`.
    pub status: String,
    pub created: String,
    /// Configuration id (label).
    pub id: String,
    /// Display name (`name` of devcontainer.json).
    pub name: String,
    pub workspace_folder: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_user: Option<String>,
    pub local_folder: String,
    pub config_file: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compose_project: Option<String>,
}

impl DevContainerInstance {
    pub fn running(&self) -> bool {
        self.state == "running"
    }
}

/// Parse [`list_args`] output (one container per line).
pub fn parse_list(output: &str) -> Vec<DevContainerInstance> {
    let opt = |v: &str| (!v.trim().is_empty()).then(|| v.trim().to_string());
    output
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|line| {
            let f: Vec<&str> = line.trim_end_matches('\r').split('\t').collect();
            if f.len() < LIST_FIELDS.len() + 6 {
                return None;
            }
            let id = f[6].trim().to_string();
            (!id.is_empty()).then(|| DevContainerInstance {
                container_id: f[0].trim().to_string(),
                container_name: f[1].trim().to_string(),
                image: f[2].trim().to_string(),
                state: f[3].trim().to_string(),
                status: f[4].trim().to_string(),
                created: f[5].trim().to_string(),
                id,
                name: f[7].trim().to_string(),
                workspace_folder: f[8].trim().to_string(),
                remote_user: opt(f[9]),
                local_folder: f[10].trim().to_string(),
                config_file: f[11].trim().to_string(),
                compose_project: f.get(12).and_then(|v| opt(v)),
            })
        })
        .collect()
}

/// A command line as the build log shows it (`docker run -d --name …`), quoted where needed.
pub fn display_command(program: &str, args: &[String]) -> String {
    std::iter::once(program.to_string())
        .chain(args.iter().map(|a| {
            if a.is_empty() || a.contains([' ', '"', '\'', '$', ';', '&']) {
                format!("'{}'", a.replace('\'', "'\\''"))
            } else {
                a.clone()
            }
        }))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::devcontainer::{load, LocalContext};

    fn no_env(_: &str) -> Option<String> {
        None
    }

    fn resolved(text: &str) -> ResolvedConfig {
        load(
            text,
            &LocalContext {
                folder: "/src/My Shop",
                config_path: "/src/My Shop/.devcontainer/devcontainer.json",
                env: &no_env,
            },
        )
        .unwrap()
    }

    #[test]
    fn image_container_command_line() {
        let c = resolved(
            r#"{ "image": "php:8.3", "forwardPorts": [8000, "db:5432"], "containerEnv": {"A": "1"},
                "mounts": ["type=volume,source=cache,target=/cache"], "runArgs": ["--cap-add=SYS_PTRACE"],
                "containerUser": "www-data" }"#,
        );
        assert_eq!(pull_args(&c), Some(vec![s("pull"), s("php:8.3")]));
        assert_eq!(build_args(&c, false), None);
        let a = run_args(&c).unwrap();
        let name = container_name(&c);
        assert!(name.starts_with("forge-my-shop-"), "{name}");
        assert_eq!(&a[..4], &[s("run"), s("-d"), s("--name"), name]);
        assert!(a.contains(&format!("{LABEL_ID}={}", c.id)));
        assert!(a.contains(&format!("{LABEL_LOCAL_FOLDER}=/src/My Shop")));
        assert!(a.contains(&s(
            "type=bind,source=/src/My Shop,target=/workspaces/My Shop,consistency=cached"
        )));
        assert!(a.contains(&s("type=volume,source=cache,target=/cache")));
        assert!(a.contains(&s("A=1")));
        assert!(a.contains(&s("127.0.0.1:8000:8000")));
        assert!(!a.iter().any(|x| x.contains("5432")));
        assert!(a.contains(&s("--cap-add=SYS_PTRACE")));
        let u = a.iter().position(|x| x == "-u").unwrap();
        assert_eq!(a[u + 1], "www-data");
        let tail: Vec<&str> = a[a.len() - 5..].iter().map(String::as_str).collect();
        assert_eq!(
            tail,
            ["--entrypoint", "/bin/sh", "php:8.3", "-c", SLEEP_LOOP]
        );
    }

    #[test]
    fn dockerfile_build_command_line() {
        let c = resolved(
            r#"{ "build": { "dockerfile": "Dockerfile", "context": "..", "args": {"PHP": "8.3"}, "target": "dev" },
                "overrideCommand": false }"#,
        );
        assert_eq!(
            build_args(&c, true).unwrap(),
            vec![
                s("build"),
                s("-f"),
                s("/src/My Shop/.devcontainer/Dockerfile"),
                s("-t"),
                image_tag(&c),
                s("--build-arg"),
                s("PHP=8.3"),
                s("--target"),
                s("dev"),
                s("--no-cache"),
                s("--label"),
                format!("{LABEL_ID}={}", c.id),
                s("/src/My Shop"),
            ]
        );
        let run = run_args(&c).unwrap();
        assert_eq!(run.last(), Some(&image_tag(&c)));
        assert!(!run.contains(&s("--entrypoint")));
    }

    #[test]
    fn compose_command_lines_and_override() {
        let c = resolved(
            r#"{ "dockerComposeFile": "../compose.yaml", "service": "app", "runServices": ["db", "app"],
                "mounts": ["source=/home/me/.ssh,target=/root/.ssh,type=bind,readonly"],
                "containerEnv": {"XDEBUG_MODE": "debug"}, "overrideCommand": true }"#,
        );
        assert_eq!(run_args(&c), None);
        let up = compose_up_args(&c, "/cache/override.yml", false).unwrap();
        assert_eq!(
            up,
            vec![
                s("compose"),
                s("-p"),
                s("myshop_devcontainer"),
                s("-f"),
                s("/src/My Shop/compose.yaml"),
                s("-f"),
                s("/cache/override.yml"),
                s("up"),
                s("-d"),
                s("--build"),
                s("app"),
                s("db"),
            ]
        );
        assert!(compose_up_args(&c, "/o.yml", true)
            .unwrap()
            .contains(&s("--force-recreate")));
        assert_eq!(
            compose_down_args(&c, "/o.yml").unwrap().last(),
            Some(&s("down"))
        );
        let o = compose_override(&c).unwrap();
        assert!(o.contains("  \"app\":\n    labels:\n"), "{o}");
        assert!(o.contains(&format!("      \"{LABEL_ID}\": \"{}\"", c.id)));
        assert!(o.contains("\"XDEBUG_MODE\": \"debug\""));
        assert!(o.contains("- \"/home/me/.ssh:/root/.ssh:ro\""));
        assert!(o.contains("entrypoint: [\"/bin/sh\", \"-c\"]"));
    }

    #[test]
    fn exec_and_terminal_command_lines() {
        let env: BTreeMap<String, String> = [
            (s("PATH"), s("${containerEnv:PATH}:/x")),
            (s("APP"), s("1")),
        ]
        .into();
        assert_eq!(
            exec_args(
                "abc",
                Some("vscode"),
                Some("/workspaces/shop"),
                &env,
                true,
                &[s("sh"), s("-c"), s("id")]
            ),
            vec![
                s("exec"),
                s("-i"),
                s("-u"),
                s("vscode"),
                s("-w"),
                s("/workspaces/shop"),
                s("-e"),
                s("APP=1"),
                s("abc"),
                s("sh"),
                s("-c"),
                s("id")
            ]
        );
        assert_eq!(
            exec_args("abc", None, None, &BTreeMap::new(), false, &[]),
            vec![s("exec"), s("abc")]
        );
        let t = terminal_args("abc", None, "/w");
        assert_eq!(&t[..5], &[s("exec"), s("-it"), s("-w"), s("/w"), s("abc")]);
        assert_eq!(remove_args("abc"), vec![s("rm"), s("-f"), s("abc")]);
        assert_eq!(stop_args("abc")[0], "stop");
        assert_eq!(start_args("abc")[0], "start");
    }

    #[test]
    fn parses_the_container_list() {
        let args = list_args();
        assert_eq!(args[3], format!("label={LABEL_ID}"));
        assert!(args[5].contains("{{.Label \"devcontainer.local_folder\"}}"));
        let out = "a1b2c3\tforge-shop-1234\tphp:8.3\trunning\tUp 2 hours\t2026-10-20 10:00:00 +0200 CEST\tid1\tShop\t/workspaces/shop\tvscode\t/src/shop\t/src/shop/.devcontainer/devcontainer.json\t\n\
                   d4e5f6\tforge-api-9876\tforge-devcontainer-id2\texited\tExited (0) 3 days ago\t2026-10-01\tid2\tAPI\t/var/www\t\t/src/api\t/src/api/.devcontainer.json\tapi_devcontainer\r\n\
                   short\tline\n";
        let list = parse_list(out);
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "Shop");
        assert!(list[0].running());
        assert_eq!(list[0].remote_user.as_deref(), Some("vscode"));
        assert_eq!(list[0].compose_project, None);
        assert_eq!(list[1].state, "exited");
        assert_eq!(list[1].remote_user, None);
        assert_eq!(list[1].compose_project.as_deref(), Some("api_devcontainer"));
        assert_eq!(parse_find("abc\trunning\n"), Some((s("abc"), true)));
        assert_eq!(parse_find("abc\texited\n"), Some((s("abc"), false)));
        assert_eq!(parse_find("\n"), None);
    }

    #[test]
    fn displays_command_lines() {
        assert_eq!(
            display_command("docker", &[s("run"), s("-v"), s("/a b:/c"), s("it's")]),
            "docker run -v '/a b:/c' 'it'\\''s'"
        );
    }
}
