//! Docker Compose files (`compose.yaml`, `docker-compose.yml`, overrides): services with
//! their images, builds, ports, profiles and env files, the project name, `${VAR}`
//! interpolation from the project's `.env`, and the `docker compose` command lines.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use yaml_rust2::{Yaml, YamlLoader};

/// File names `docker compose` picks up in a directory, in its order of preference.
pub const COMPOSE_FILE_NAMES: &[&str] = &[
    "compose.yaml",
    "compose.yml",
    "docker-compose.yaml",
    "docker-compose.yml",
];

/// Override files merged into the main file.
pub const OVERRIDE_FILE_NAMES: &[&str] = &[
    "compose.override.yaml",
    "compose.override.yml",
    "docker-compose.override.yaml",
    "docker-compose.override.yml",
];

/// Whether a file name is a compose file (`compose.yaml`, `docker-compose.prod.yml`, ...).
pub fn is_compose_file(name: &str) -> bool {
    let name = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let Some(stem) = name
        .strip_suffix(".yaml")
        .or_else(|| name.strip_suffix(".yml"))
    else {
        return false;
    };
    stem == "compose"
        || stem == "docker-compose"
        || stem.starts_with("compose.")
        || stem.starts_with("docker-compose.")
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildSpec {
    pub context: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dockerfile: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvVar {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComposeService {
    pub name: String,
    /// 0-based line of the service key.
    pub line: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build: Option<BuildSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container_name: Option<String>,
    pub ports: Vec<String>,
    pub profiles: Vec<String>,
    pub depends_on: Vec<String>,
    pub env_files: Vec<String>,
    pub environment: Vec<EnvVar>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    pub volumes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restart: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replicas: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComposeFile {
    /// Path as given (workspace relative in the app).
    pub path: String,
    /// Project name: the top-level `name:` or the normalized directory name.
    pub project: String,
    pub services: Vec<ComposeService>,
    pub volumes: Vec<String>,
    pub networks: Vec<String>,
    /// All profiles used by services.
    pub profiles: Vec<String>,
    /// 0-based line of the top-level `services:` key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub services_line: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// The override file merged into it (`compose.override.yaml`), same base as `path`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub override_path: Option<String>,
}

impl ComposeFile {
    pub fn service(&self, name: &str) -> Option<&ComposeService> {
        self.services.iter().find(|s| s.name == name)
    }
}

/// Compose's project name rules: lowercase letters, digits, `-` and `_`, starting with a
/// letter or digit.
pub fn normalize_project_name(name: &str) -> String {
    let lower: String = name
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-' || *c == '_')
        .collect();
    lower.trim_start_matches(['-', '_']).to_string()
}

/// Directory name of a path (`/home/dev/acme-shop/compose.yaml` → `acme-shop`).
pub fn dir_name(path: &str) -> String {
    let mut parts: Vec<&str> = path.split(['/', '\\']).filter(|p| !p.is_empty()).collect();
    parts.pop();
    parts.last().copied().unwrap_or_default().to_string()
}

/// `KEY=value` lines of an env file (`#` comments, `export`, quotes).
pub fn parse_env_file(content: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let v = v.trim();
        let v = if (v.starts_with('"') && v.ends_with('"') && v.len() >= 2)
            || (v.starts_with('\'') && v.ends_with('\'') && v.len() >= 2)
        {
            v[1..v.len() - 1].to_string()
        } else {
            // Unquoted values end at an inline comment.
            v.split(" #").next().unwrap_or(v).trim().to_string()
        };
        out.insert(k.trim().to_string(), v);
    }
    out
}

/// Compose interpolation: `$VAR`, `${VAR}`, `${VAR:-default}`, `${VAR-default}`,
/// `${VAR:?err}` (empty) and `$$` for a literal `$`.
pub fn interpolate(text: &str, env: &BTreeMap<String, String>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find('$') {
        out.push_str(&rest[..pos]);
        let after = &rest[pos + 1..];
        if let Some(r) = after.strip_prefix('$') {
            out.push('$');
            rest = r;
        } else if let Some(r) = after.strip_prefix('{') {
            let Some(end) = r.find('}') else {
                out.push_str(&rest[pos..]);
                return out;
            };
            let expr = &r[..end];
            let (name, op, default) = match expr.find([':', '-', '?', '+']) {
                Some(i) => {
                    let op_len = if expr[i..].starts_with(':') { 2 } else { 1 };
                    let op_end = (i + op_len).min(expr.len());
                    (&expr[..i], &expr[i..op_end], &expr[op_end..])
                }
                None => (expr, "", ""),
            };
            let value = env.get(name);
            let resolved = match op {
                ":-" => value
                    .filter(|v| !v.is_empty())
                    .cloned()
                    .unwrap_or_else(|| interpolate(default, env)),
                "-" => value.cloned().unwrap_or_else(|| interpolate(default, env)),
                ":+" => {
                    if value.is_some_and(|v| !v.is_empty()) {
                        interpolate(default, env)
                    } else {
                        String::new()
                    }
                }
                "+" => {
                    if value.is_some() {
                        interpolate(default, env)
                    } else {
                        String::new()
                    }
                }
                _ => value.cloned().unwrap_or_default(),
            };
            out.push_str(&resolved);
            rest = &r[end + 1..];
        } else {
            let len = after
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .unwrap_or(after.len());
            if len == 0 {
                out.push('$');
            } else {
                out.push_str(env.get(&after[..len]).map(String::as_str).unwrap_or(""));
            }
            rest = &after[len..];
        }
    }
    out.push_str(rest);
    out
}

fn scalar(y: &Yaml) -> Option<String> {
    match y {
        Yaml::String(s) | Yaml::Real(s) => Some(s.clone()),
        Yaml::Integer(i) => Some(i.to_string()),
        Yaml::Boolean(b) => Some(b.to_string()),
        _ => None,
    }
}

fn list(y: &Yaml) -> Vec<String> {
    match y {
        Yaml::Array(a) => a
            .iter()
            .filter_map(|item| match item {
                // Long syntax: `{ target: 80, published: 8080 }`, `{ path: .env }`,
                // `{ source: x, target: /y }`.
                Yaml::Hash(h) => {
                    let get = |k: &str| h.get(&Yaml::String(k.into())).and_then(scalar);
                    if let Some(p) = get("path") {
                        Some(p)
                    } else if let (Some(published), Some(target)) =
                        (get("published"), get("target"))
                    {
                        Some(format!("{published}:{target}"))
                    } else if let (Some(source), Some(target)) = (get("source"), get("target")) {
                        Some(format!("{source}:{target}"))
                    } else {
                        get("target")
                    }
                }
                other => scalar(other),
            })
            .collect(),
        Yaml::Hash(h) => h.keys().filter_map(scalar).collect(),
        other => scalar(other).into_iter().collect(),
    }
}

/// 0-based lines of the top-level `services:` key and of each service key below it.
fn service_lines(content: &str) -> (Option<u32>, BTreeMap<String, u32>) {
    let mut services_line = None;
    let mut lines = BTreeMap::new();
    let mut child_indent: Option<usize> = None;
    for (n, raw) in content.lines().enumerate() {
        let trimmed = raw.trim_end();
        let body = trimmed.trim_start();
        if body.is_empty() || body.starts_with('#') {
            continue;
        }
        let indent = trimmed.len() - body.len();
        if indent == 0 {
            if services_line.is_some() && child_indent.is_some() {
                // The next top-level key ends the services block.
                break;
            }
            if body.starts_with("services:") {
                services_line = Some(n as u32);
            }
            continue;
        }
        if services_line.is_none() {
            continue;
        }
        let ci = *child_indent.get_or_insert(indent);
        if indent == ci {
            if let Some((key, _)) = body.split_once(':') {
                let key = key.trim().trim_matches(['"', '\'']);
                lines.insert(key.to_string(), n as u32);
            }
        }
    }
    (services_line, lines)
}

/// Parse one compose file. `env` is used for `${VAR}` interpolation (the project's `.env`
/// merged with the process environment by the caller).
pub fn parse(path: &str, content: &str, env: &BTreeMap<String, String>) -> ComposeFile {
    let fallback_name = normalize_project_name(&dir_name(path));
    let mut file = ComposeFile {
        path: path.to_string(),
        project: fallback_name,
        ..Default::default()
    };
    let docs = match YamlLoader::load_from_str(content) {
        Ok(d) => d,
        Err(e) => {
            file.error = Some(format!("Invalid YAML: {e}"));
            let (services_line, lines) = service_lines(content);
            file.services_line = services_line;
            file.services = lines
                .into_iter()
                .map(|(name, line)| ComposeService {
                    name,
                    line,
                    ..Default::default()
                })
                .collect();
            return file;
        }
    };
    let Some(root) = docs.first() else {
        return file;
    };
    let ip = |s: String| interpolate(&s, env);
    if let Some(name) = root["name"].as_str() {
        let n = normalize_project_name(&ip(name.to_string()));
        if !n.is_empty() {
            file.project = n;
        }
    }
    let (services_line, lines) = service_lines(content);
    file.services_line = services_line;
    if let Yaml::Hash(services) = &root["services"] {
        for (name, svc) in services {
            let Some(name) = scalar(name) else { continue };
            let get = |k: &str| &svc[k];
            let build = match get("build") {
                Yaml::Hash(_) => Some(BuildSpec {
                    context: scalar(&svc["build"]["context"])
                        .map(ip)
                        .unwrap_or_else(|| ".".into()),
                    dockerfile: scalar(&svc["build"]["dockerfile"]).map(ip),
                    target: scalar(&svc["build"]["target"]).map(ip),
                }),
                y => scalar(y).map(|context| BuildSpec {
                    context: ip(context),
                    ..Default::default()
                }),
            };
            let environment = match get("environment") {
                Yaml::Hash(h) => h
                    .iter()
                    .filter_map(|(k, v)| {
                        Some(EnvVar {
                            name: scalar(k)?,
                            value: scalar(v).map(ip),
                        })
                    })
                    .collect(),
                Yaml::Array(a) => a
                    .iter()
                    .filter_map(scalar)
                    .map(|e| match e.split_once('=') {
                        Some((k, v)) => EnvVar {
                            name: k.to_string(),
                            value: Some(ip(v.to_string())),
                        },
                        None => EnvVar {
                            name: e,
                            value: None,
                        },
                    })
                    .collect(),
                _ => Vec::new(),
            };
            let command = match get("command") {
                Yaml::Array(a) => Some(a.iter().filter_map(scalar).collect::<Vec<_>>().join(" ")),
                y => scalar(y),
            };
            let replicas = svc["deploy"]["replicas"]
                .as_i64()
                .and_then(|n| u32::try_from(n).ok());
            file.services.push(ComposeService {
                line: lines.get(&name).copied().unwrap_or(0),
                image: scalar(get("image")).map(ip),
                build,
                container_name: scalar(get("container_name")).map(ip),
                ports: list(get("ports")).into_iter().map(ip).collect(),
                profiles: list(get("profiles")),
                depends_on: list(get("depends_on")),
                env_files: list(get("env_file")).into_iter().map(ip).collect(),
                environment,
                command,
                volumes: list(get("volumes")).into_iter().map(ip).collect(),
                restart: scalar(get("restart")),
                replicas,
                name,
            });
        }
    }
    file.volumes = list(&root["volumes"]);
    file.networks = list(&root["networks"]);
    let mut profiles: Vec<String> = file
        .services
        .iter()
        .flat_map(|s| s.profiles.iter().cloned())
        .collect();
    profiles.sort();
    profiles.dedup();
    file.profiles = profiles;
    file
}

/// Merge an override file into a main file (services added or extended, like compose).
pub fn merge(mut base: ComposeFile, over: ComposeFile) -> ComposeFile {
    for svc in over.services {
        match base.services.iter_mut().find(|s| s.name == svc.name) {
            Some(existing) => {
                if svc.image.is_some() {
                    existing.image = svc.image;
                }
                if svc.build.is_some() {
                    existing.build = svc.build;
                }
                if svc.container_name.is_some() {
                    existing.container_name = svc.container_name;
                }
                if svc.command.is_some() {
                    existing.command = svc.command;
                }
                for p in svc.ports {
                    if !existing.ports.contains(&p) {
                        existing.ports.push(p);
                    }
                }
                existing.environment.extend(svc.environment);
                existing.env_files.extend(svc.env_files);
                existing.volumes.extend(svc.volumes);
                existing.depends_on.extend(svc.depends_on);
                if !svc.profiles.is_empty() {
                    existing.profiles = svc.profiles;
                }
            }
            None => base.services.push(svc),
        }
    }
    for v in over.volumes {
        if !base.volumes.contains(&v) {
            base.volumes.push(v);
        }
    }
    for n in over.networks {
        if !base.networks.contains(&n) {
            base.networks.push(n);
        }
    }
    for p in over.profiles {
        if !base.profiles.contains(&p) {
            base.profiles.push(p);
        }
    }
    base
}

/// What to run with `docker compose`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ComposeAction {
    Up,
    Down,
    Start,
    Stop,
    Restart,
    Pull,
    Build,
    Ps,
    Logs,
}

impl ComposeAction {
    pub fn title(self) -> &'static str {
        match self {
            Self::Up => "Up",
            Self::Down => "Down",
            Self::Start => "Start",
            Self::Stop => "Stop",
            Self::Restart => "Restart",
            Self::Pull => "Pull",
            Self::Build => "Build",
            Self::Ps => "Ps",
            Self::Logs => "Logs",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComposeCommand {
    /// Compose files (`-f`), absolute or relative to the working directory.
    pub files: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(default)]
    pub profiles: Vec<String>,
    #[serde(default)]
    pub env_file: Option<String>,
    pub action: ComposeAction,
    /// Only these services (all when empty).
    #[serde(default)]
    pub services: Vec<String>,
    /// `up --build`.
    #[serde(default)]
    pub build: bool,
}

/// Arguments after `docker` for a compose command (`compose -f … up -d app`).
pub fn compose_args(cmd: &ComposeCommand) -> Vec<String> {
    let mut args = vec!["compose".to_string()];
    for f in &cmd.files {
        args.push("-f".into());
        args.push(f.clone());
    }
    if let Some(p) = &cmd.project {
        args.push("-p".into());
        args.push(p.clone());
    }
    for p in &cmd.profiles {
        args.push("--profile".into());
        args.push(p.clone());
    }
    if let Some(e) = &cmd.env_file {
        args.push("--env-file".into());
        args.push(e.clone());
    }
    let services = cmd.services.iter().cloned();
    match cmd.action {
        ComposeAction::Up => {
            args.extend(["up", "-d"].map(String::from));
            if cmd.build {
                args.push("--build".into());
            }
            args.extend(services);
        }
        ComposeAction::Down if cmd.services.is_empty() => args.push("down".into()),
        // `down` takes no services; stop and remove only the selected ones.
        ComposeAction::Down => {
            args.extend(["rm", "--stop", "--force"].map(String::from));
            args.extend(services);
        }
        ComposeAction::Logs => {
            args.extend(["logs", "--follow", "--tail", "200"].map(String::from));
            args.extend(services);
        }
        ComposeAction::Ps => {
            args.extend(["ps", "--all"].map(String::from));
            args.extend(services);
        }
        other => {
            args.push(other.title().to_lowercase());
            args.extend(services);
        }
    }
    args
}

/// Shell-like display of a command line (`docker compose -f "my file.yml" up -d`).
pub fn display_command(program: &str, args: &[String]) -> String {
    std::iter::once(program.to_string())
        .chain(args.iter().map(|a| {
            if a.is_empty() || a.contains([' ', '"', '\'', '$']) {
                format!("\"{}\"", a.replace('"', "\\\""))
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

    const ACME: &str = r#"name: ${PROJECT:-Acme-Shop}
services:
  app:
    build:
      context: .
      dockerfile: Dockerfile
    ports:
      - "${APP_PORT:-8000}:8000"
    env_file: .env
    environment:
      DB_HOST: mysql
      XDEBUG_MODE: debug
    depends_on:
      - mysql
      - redis

  mysql:
    image: mysql:${MYSQL_VERSION}
    environment:
      - MYSQL_DATABASE=shop
      - MYSQL_ROOT_PASSWORD
    volumes:
      - db-data:/var/lib/mysql

  redis:
    image: "redis:7-alpine"
    profiles: [cache]
    command: ["redis-server", "--appendonly", "yes"]
    deploy:
      replicas: 2

volumes:
  db-data: {}
"#;

    fn env() -> BTreeMap<String, String> {
        parse_env_file("# comment\nMYSQL_VERSION=8.4\nexport APP_PORT=\"8080\"\nEMPTY=\n")
    }

    #[test]
    fn recognizes_compose_file_names() {
        assert!(is_compose_file("compose.yaml"));
        assert!(is_compose_file("deploy/docker-compose.prod.yml"));
        assert!(is_compose_file("compose.override.yml"));
        assert!(!is_compose_file("composer.json"));
        assert!(!is_compose_file("compose.json"));
        assert!(!is_compose_file("my-compose.yaml"));
    }

    #[test]
    fn parses_services() {
        let f = parse("/home/dev/acme/compose.yaml", ACME, &env());
        assert_eq!(f.error, None);
        assert_eq!(f.project, "acme-shop");
        assert_eq!(f.services_line, Some(1));
        let names: Vec<_> = f.services.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["app", "mysql", "redis"]);
        let app = f.service("app").unwrap();
        assert_eq!(app.line, 2);
        assert_eq!(
            app.build.as_ref().unwrap().dockerfile.as_deref(),
            Some("Dockerfile")
        );
        assert_eq!(app.ports, ["8080:8000"]);
        assert_eq!(app.env_files, [".env"]);
        assert_eq!(app.depends_on, ["mysql", "redis"]);
        assert_eq!(app.environment[0].name, "DB_HOST");
        let mysql = f.service("mysql").unwrap();
        assert_eq!(mysql.line, 16);
        assert_eq!(mysql.image.as_deref(), Some("mysql:8.4"));
        assert_eq!(mysql.environment[1].value, None);
        let redis = f.service("redis").unwrap();
        assert_eq!(redis.profiles, ["cache"]);
        assert_eq!(
            redis.command.as_deref(),
            Some("redis-server --appendonly yes")
        );
        assert_eq!(redis.replicas, Some(2));
        assert_eq!(f.profiles, ["cache"]);
        assert_eq!(f.volumes, ["db-data"]);
    }

    #[test]
    fn project_name_falls_back_to_directory() {
        let f = parse(
            "/x/My Shop/docker-compose.yml",
            "services:\n  web:\n    image: nginx\n",
            &BTreeMap::new(),
        );
        assert_eq!(f.project, "myshop");
        assert_eq!(f.services[0].line, 1);
    }

    #[test]
    fn invalid_yaml_keeps_service_lines() {
        let f = parse(
            "compose.yaml",
            "services:\n  web:\n    image: [nginx\n",
            &BTreeMap::new(),
        );
        assert!(f.error.is_some());
        assert_eq!(f.services[0].name, "web");
    }

    #[test]
    fn interpolation() {
        let env = env();
        assert_eq!(interpolate("${MYSQL_VERSION}", &env), "8.4");
        assert_eq!(interpolate("$MYSQL_VERSION-x", &env), "8.4-x");
        assert_eq!(
            interpolate("${EMPTY:-d}|${EMPTY-d}|${NOPE-d}", &env),
            "d||d"
        );
        assert_eq!(interpolate("${APP_PORT:+set}${NOPE:+set}", &env), "set");
        assert_eq!(interpolate("$$HOME", &env), "$HOME");
        assert_eq!(interpolate("${UNSET}", &env), "");
    }

    #[test]
    fn merges_overrides() {
        let base = parse("compose.yaml", ACME, &env());
        let over = parse(
            "compose.override.yaml",
            "services:\n  app:\n    ports: [\"9003:9003\"]\n  mailpit:\n    image: axllent/mailpit\n",
            &env(),
        );
        let merged = merge(base, over);
        assert_eq!(
            merged.service("app").unwrap().ports,
            ["8080:8000", "9003:9003"]
        );
        assert!(merged.service("mailpit").is_some());
    }

    #[test]
    fn command_lines() {
        let mut cmd = ComposeCommand {
            files: vec!["compose.yaml".into()],
            project: Some("acme-shop".into()),
            profiles: vec!["cache".into()],
            env_file: None,
            action: ComposeAction::Up,
            services: vec!["app".into()],
            build: true,
        };
        assert_eq!(
            compose_args(&cmd).join(" "),
            "compose -f compose.yaml -p acme-shop --profile cache up -d --build app"
        );
        cmd.action = ComposeAction::Down;
        assert_eq!(
            compose_args(&cmd).join(" "),
            "compose -f compose.yaml -p acme-shop --profile cache rm --stop --force app"
        );
        cmd.services.clear();
        assert!(compose_args(&cmd).ends_with(&["down".to_string()]));
        cmd.action = ComposeAction::Restart;
        assert!(compose_args(&cmd).ends_with(&["restart".to_string()]));
        assert_eq!(
            display_command(
                "docker",
                &["compose".into(), "-f".into(), "my file.yml".into()]
            ),
            "docker compose -f \"my file.yml\""
        );
    }
}
