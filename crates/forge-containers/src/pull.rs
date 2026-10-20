//! Image pulls: `POST /images/create?fromImage=…` answers with one JSON message per line
//! (`{"status":"Downloading","id":"a1b2","progressDetail":{"current":1,"total":9}}`).
//! [`PullProgress`] folds them into per-layer states, an overall percentage and the console
//! text the UI shows.

use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Layer {
    pub id: String,
    pub status: String,
    pub current: u64,
    pub total: u64,
}

impl Layer {
    fn done(&self) -> bool {
        matches!(
            self.status.as_str(),
            "Pull complete" | "Already exists" | "Download complete"
        ) || self.status.starts_with("Pull complete")
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullState {
    pub image: String,
    pub layers: Vec<Layer>,
    /// 0–100 over all layers with known sizes.
    pub percent: u32,
    /// Status lines without a layer (`Digest: …`, `Status: Downloaded newer image …`).
    pub messages: Vec<String>,
    pub done: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Default)]
pub struct PullProgress {
    state: PullState,
    partial: String,
}

/// `mysql` → (`mysql`, `latest`); `ghcr.io/acme/app:1.2` → (`ghcr.io/acme/app`, `1.2`);
/// digests stay on the name.
pub fn split_reference(image: &str) -> (String, String) {
    if image.contains('@') {
        return (image.to_string(), String::new());
    }
    let slash = image.rfind('/').map(|i| i + 1).unwrap_or(0);
    match image[slash..].rfind(':') {
        Some(i) => (
            image[..slash + i].to_string(),
            image[slash + i + 1..].to_string(),
        ),
        None => (image.to_string(), "latest".to_string()),
    }
}

/// Query of `POST /images/create`.
pub fn pull_query(image: &str) -> String {
    let (name, tag) = split_reference(image);
    let enc = crate::kube::encode_query;
    if tag.is_empty() {
        format!("fromImage={}", enc(&name))
    } else {
        format!("fromImage={}&tag={}", enc(&name), enc(&tag))
    }
}

impl PullProgress {
    pub fn new(image: &str) -> Self {
        PullProgress {
            state: PullState {
                image: image.to_string(),
                ..Default::default()
            },
            partial: String::new(),
        }
    }

    pub fn state(&self) -> &PullState {
        &self.state
    }

    /// Feed body text; returns the console lines to print for complete messages.
    pub fn push(&mut self, text: &str) -> Vec<String> {
        self.partial.push_str(text);
        let mut lines = Vec::new();
        while let Some(nl) = self.partial.find('\n') {
            let line: String = self.partial.drain(..=nl).collect();
            if let Some(l) = self.message(line.trim()) {
                lines.push(l);
            }
        }
        lines
    }

    /// End of the body.
    pub fn finish(&mut self) -> Vec<String> {
        let rest = std::mem::take(&mut self.partial);
        let mut lines: Vec<String> = self.message(rest.trim()).into_iter().collect();
        if self.state.error.is_none() {
            self.state.done = true;
            self.state.percent = 100;
        } else if let Some(e) = &self.state.error {
            lines.push(format!("Error: {e}"));
        }
        lines
    }

    fn message(&mut self, line: &str) -> Option<String> {
        if line.is_empty() {
            return None;
        }
        let v: Value = serde_json::from_str(line).ok()?;
        if let Some(e) = v.get("error").and_then(Value::as_str) {
            self.state.error = Some(e.to_string());
            return None;
        }
        let status = v
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let Some(id) = v.get("id").and_then(Value::as_str) else {
            self.state.messages.push(status.clone());
            return Some(status);
        };
        // `Pulling from library/mysql` carries the tag as id.
        if status.starts_with("Pulling from") {
            let line = format!("{id}: {status}");
            self.state.messages.push(line.clone());
            return Some(line);
        }
        let current = v.pointer("/progressDetail/current").and_then(Value::as_u64);
        let total = v.pointer("/progressDetail/total").and_then(Value::as_u64);
        let idx = match self.state.layers.iter().position(|l| l.id == id) {
            Some(i) => i,
            None => {
                self.state.layers.push(Layer {
                    id: id.to_string(),
                    status: String::new(),
                    current: 0,
                    total: 0,
                });
                self.state.layers.len() - 1
            }
        };
        let layer = &mut self.state.layers[idx];
        let changed = layer.status != status;
        layer.status = status.clone();
        if status == "Downloading" {
            if let Some(t) = total {
                layer.total = t;
            }
            if let Some(c) = current {
                layer.current = c;
            }
        } else if layer.done() || status == "Verifying Checksum" || status == "Extracting" {
            layer.current = layer.total;
        }
        self.state.percent = self.percent();
        // Progress messages repeat many times per layer; print status changes only.
        changed.then(|| format!("{id}: {status}"))
    }

    fn percent(&self) -> u32 {
        let layers = &self.state.layers;
        if layers.is_empty() {
            return 0;
        }
        let total: u64 = layers.iter().map(|l| l.total).sum();
        if total == 0 {
            let done = layers.iter().filter(|l| l.done()).count();
            return (done * 100 / layers.len()) as u32;
        }
        let current: u64 = layers
            .iter()
            .map(|l| {
                if l.done() {
                    l.total
                } else {
                    l.current.min(l.total)
                }
            })
            .sum();
        ((current * 100) / total).min(99) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn references() {
        assert_eq!(split_reference("mysql"), ("mysql".into(), "latest".into()));
        assert_eq!(split_reference("mysql:8.4"), ("mysql".into(), "8.4".into()));
        assert_eq!(
            split_reference("localhost:5000/app"),
            ("localhost:5000/app".into(), "latest".into())
        );
        assert_eq!(
            split_reference("ghcr.io/acme/app:1.2"),
            ("ghcr.io/acme/app".into(), "1.2".into())
        );
        assert_eq!(pull_query("redis:7-alpine"), "fromImage=redis&tag=7-alpine");
        assert_eq!(pull_query("a@sha256:00"), "fromImage=a%40sha256%3A00");
    }

    #[test]
    fn folds_progress() {
        let mut p = PullProgress::new("redis:7-alpine");
        let body = concat!(
            "{\"status\":\"Pulling from library/redis\",\"id\":\"7-alpine\"}\n",
            "{\"status\":\"Pulling fs layer\",\"id\":\"aaa\"}\n",
            "{\"status\":\"Already exists\",\"id\":\"bbb\"}\n",
            "{\"status\":\"Downloading\",\"progressDetail\":{\"current\":50,\"total\":100},\"id\":\"aaa\"}\n",
            "{\"status\":\"Downloading\",\"progressDetail\":{\"current\":75,\"total\":100},\"id\":\"aaa\"}\n",
        );
        let lines = p.push(body);
        assert_eq!(
            lines,
            [
                "7-alpine: Pulling from library/redis",
                "aaa: Pulling fs layer",
                "bbb: Already exists",
                "aaa: Downloading"
            ]
        );
        assert_eq!(p.state().percent, 75);
        let lines = p.push("{\"status\":\"Pull complete\",\"id\":\"aaa\"}\n{\"status\":\"Status: Downloaded newer image for redis:7-alpine\"}");
        assert_eq!(lines, ["aaa: Pull complete"]);
        let lines = p.finish();
        assert_eq!(lines, ["Status: Downloaded newer image for redis:7-alpine"]);
        assert!(p.state().done);
        assert_eq!(p.state().percent, 100);
    }

    #[test]
    fn reports_errors() {
        let mut p = PullProgress::new("nope");
        p.push("{\"error\":\"pull access denied for nope\"}\n");
        let lines = p.finish();
        assert_eq!(
            p.state().error.as_deref(),
            Some("pull access denied for nope")
        );
        assert!(!p.state().done);
        assert_eq!(lines, ["Error: pull access denied for nope"]);
    }
}
