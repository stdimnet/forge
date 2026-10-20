//! ▶ gutter markers: on `services:` (up all) and every service of a compose file (up one),
//! and on the first `FROM` of a Dockerfile (build the image and run a container), plus the
//! command lines those markers run.

use serde::Serialize;

use crate::compose;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MarkerKind {
    ComposeAll,
    ComposeService,
    Dockerfile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GutterMarker {
    /// 0-based line.
    pub line: u32,
    pub kind: MarkerKind,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service: Option<String>,
}

/// `Dockerfile`, `Dockerfile.dev`, `app.dockerfile`, `Containerfile`.
pub fn is_dockerfile(path: &str) -> bool {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let lower = name.to_ascii_lowercase();
    lower == "dockerfile"
        || lower == "containerfile"
        || lower.starts_with("dockerfile.")
        || lower.starts_with("containerfile.")
        || lower.ends_with(".dockerfile")
        || lower.ends_with(".containerfile")
}

pub fn markers(path: &str, content: &str) -> Vec<GutterMarker> {
    if compose::is_compose_file(path) {
        let file = compose::parse(path, content, &Default::default());
        let mut out = Vec::new();
        if let Some(line) = file.services_line {
            out.push(GutterMarker {
                line,
                kind: MarkerKind::ComposeAll,
                title: "Run all services (docker compose up)".into(),
                service: None,
            });
        }
        for s in &file.services {
            out.push(GutterMarker {
                line: s.line,
                kind: MarkerKind::ComposeService,
                title: format!("Run service '{}' (docker compose up {})", s.name, s.name),
                service: Some(s.name.clone()),
            });
        }
        return out;
    }
    if is_dockerfile(path) {
        return content
            .lines()
            .position(|l| l.trim_start().to_ascii_uppercase().starts_with("FROM "))
            .map(|line| GutterMarker {
                line: line as u32,
                kind: MarkerKind::Dockerfile,
                title: "Build image and run container".into(),
                service: None,
            })
            .into_iter()
            .collect();
    }
    Vec::new()
}

/// Ports from `EXPOSE 80 443/tcp` instructions.
pub fn exposed_ports(content: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in content.lines() {
        let t = line.trim();
        if t.len() > 7 && t[..7].eq_ignore_ascii_case("EXPOSE ") {
            for p in t[7..].split_whitespace() {
                let port = p.split('/').next().unwrap_or(p);
                if port.chars().all(|c| c.is_ascii_digit()) && !out.iter().any(|x| x == p) {
                    out.push(p.to_string());
                }
            }
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerfileRun {
    pub image: String,
    pub container: String,
    /// Arguments after `docker` (`build -t … -f … .`).
    pub build_args: Vec<String>,
    /// Arguments after `docker` (`run -d --name … -p … image`).
    pub run_args: Vec<String>,
}

/// Build-and-run plan for a Dockerfile at `path` (relative to the project root named
/// `project`): image `<project>-<dir or suffix>:latest`, container of the same name, every
/// `EXPOSE`d port published on the same host port.
pub fn dockerfile_run(path: &str, project: &str, content: &str) -> DockerfileRun {
    let normalized = path.replace('\\', "/");
    let (dir, file) = match normalized.rsplit_once('/') {
        Some((d, f)) => (d.to_string(), f.to_string()),
        None => (".".to_string(), normalized.clone()),
    };
    let suffix = file
        .split_once('.')
        .map(|(a, b)| {
            if a.eq_ignore_ascii_case("dockerfile") {
                b
            } else {
                a
            }
        })
        .unwrap_or("");
    let mut name = compose::normalize_project_name(project);
    if name.is_empty() {
        name = "forge".into();
    }
    let extra = if !suffix.is_empty() {
        compose::normalize_project_name(suffix)
    } else if dir != "." {
        compose::normalize_project_name(dir.rsplit('/').next().unwrap_or(""))
    } else {
        String::new()
    };
    if !extra.is_empty() {
        name = format!("{name}-{extra}");
    }
    let image = format!("{name}:latest");
    let build_args = ["build", "-t", &image, "-f", &normalized, &dir]
        .map(String::from)
        .to_vec();
    let mut run_args = vec![
        "run".to_string(),
        "-d".into(),
        "--name".into(),
        name.clone(),
    ];
    for p in exposed_ports(content) {
        let (port, proto) = p.split_once('/').unwrap_or((&p, ""));
        run_args.push("-p".into());
        run_args.push(if proto.is_empty() {
            format!("{port}:{port}")
        } else {
            format!("{port}:{port}/{proto}")
        });
    }
    run_args.push(image.clone());
    DockerfileRun {
        image,
        container: name,
        build_args,
        run_args,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compose_markers() {
        let m = markers(
            "docker-compose.yml",
            "# demo\nservices:\n  app:\n    image: x\n  mysql:\n    image: y\nvolumes:\n  data: {}\n",
        );
        assert_eq!(m.len(), 3);
        assert_eq!((m[0].line, m[0].kind), (1, MarkerKind::ComposeAll));
        assert_eq!((m[1].line, m[1].service.as_deref()), (2, Some("app")));
        assert_eq!((m[2].line, m[2].service.as_deref()), (4, Some("mysql")));
    }

    #[test]
    fn dockerfile_markers_and_plan() {
        assert!(is_dockerfile("docker/Dockerfile.dev"));
        assert!(is_dockerfile("api.Dockerfile"));
        assert!(!is_dockerfile("Dockerfiles/readme.md"));
        let content =
            "# syntax=docker/dockerfile:1\nFROM php:8.3-fpm-alpine\nEXPOSE 8000 9003/tcp\n";
        let m = markers("Dockerfile", content);
        assert_eq!(m[0].line, 1);
        assert_eq!(exposed_ports(content), ["8000", "9003/tcp"]);
        let plan = dockerfile_run("Dockerfile", "Acme Shop", content);
        assert_eq!(plan.image, "acmeshop:latest");
        assert_eq!(
            plan.build_args.join(" "),
            "build -t acmeshop:latest -f Dockerfile ."
        );
        assert_eq!(
            plan.run_args.join(" "),
            "run -d --name acmeshop -p 8000:8000 -p 9003:9003/tcp acmeshop:latest"
        );
        let plan = dockerfile_run("docker/Dockerfile.dev", "acme-shop", "FROM x\n");
        assert_eq!(plan.container, "acme-shop-dev");
        assert_eq!(
            plan.build_args.join(" "),
            "build -t acme-shop-dev:latest -f docker/Dockerfile.dev docker"
        );
        assert_eq!(
            dockerfile_run("worker/Dockerfile", "acme", "").container,
            "acme-worker"
        );
        assert!(markers("README.md", "FROM x").is_empty());
    }
}
