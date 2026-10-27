//! The vulnerable dependency check (`forge-vulns`) for the browser-mode mocks of the
//! `vulnerabilities_*` IPC commands: the mocks hand in recorded audit reports of the demo
//! projects, everything else (detection, parsing, problems, the Upgrade edit) is computed
//! here exactly like in the desktop app.

use forge_vulns::report::{self, TargetResult};
use forge_vulns::{problems, tool, DetectOptions, Notice, Target, Vulnerability};
use serde::Deserialize;
use serde_json::json;
use wasm_bindgen::prelude::*;

use crate::to_json;

fn parse<T: for<'de> Deserialize<'de>>(json: &str) -> Result<T, String> {
    serde_json::from_str(json).map_err(|e| e.to_string())
}

fn answer<T: serde::Serialize>(r: Result<T, String>) -> String {
    match r {
        Ok(v) => to_json(&v),
        Err(e) => to_json(&json!({ "error": e })),
    }
}

#[derive(Deserialize)]
struct File {
    path: String,
    #[serde(default)]
    text: String,
}

/// `{ files: [{ path, text }], options }` → `Target[]`.
#[wasm_bindgen]
pub fn vulnerabilities_detect(request_json: &str) -> String {
    #[derive(Deserialize)]
    struct Req {
        files: Vec<File>,
        #[serde(default)]
        options: DetectOptions,
    }
    answer(parse::<Req>(request_json).map(|r| {
        let paths: Vec<String> = r.files.iter().map(|f| f.path.clone()).collect();
        let read = |p: &str| r.files.iter().find(|f| f.path == p).map(|f| f.text.clone());
        tool::detect(&paths, &read, &r.options)
    }))
}

/// `{ target, stdout, stderr?, lock?, checkedAt }` → `TargetResult` (what the desktop app
/// makes of a tool run; `lock` is the lock file's text for the installed versions).
#[wasm_bindgen]
pub fn vulnerabilities_result(request_json: &str) -> String {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Req {
        target: Target,
        stdout: String,
        #[serde(default)]
        stderr: String,
        #[serde(default)]
        lock: Option<String>,
        #[serde(default)]
        checked_at: u64,
    }
    answer(parse::<Req>(request_json).map(|r| -> TargetResult {
        let mut out = report::from_output(r.target, &r.stdout, &r.stderr, r.checked_at);
        report::fill_installed(&mut out, r.lock.as_deref());
        out
    }))
}

/// `{ path, text, vulnerabilities, notices }` → `FileProblem[]`.
#[wasm_bindgen]
pub fn vulnerabilities_problems(request_json: &str) -> String {
    #[derive(Deserialize)]
    struct Req {
        path: String,
        text: String,
        #[serde(default)]
        vulnerabilities: Vec<Vulnerability>,
        #[serde(default)]
        notices: Vec<Notice>,
    }
    answer(
        parse::<Req>(request_json)
            .map(|r| problems::problems(&r.path, &r.text, &r.vulnerabilities, &r.notices)),
    )
}

/// `{ path, text, package, fixed }` → the new text, or `null`.
#[wasm_bindgen]
pub fn vulnerabilities_upgrade(request_json: &str) -> String {
    #[derive(Deserialize)]
    struct Req {
        path: String,
        text: String,
        package: String,
        fixed: String,
    }
    answer(
        parse::<Req>(request_json)
            .map(|r| problems::upgrade(&r.path, &r.text, &r.package, &r.fixed)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn checks_a_recorded_report() {
        let targets: Value = serde_json::from_str(&vulnerabilities_detect(
            r#"{"files": [{"path": "package.json", "text": "{\"dependencies\": {\"lodash\": \"^4.17.20\"}}"}, {"path": "package-lock.json"}]}"#,
        ))
        .unwrap();
        assert_eq!(targets[0]["tool"], "npm");
        let report = r#"{"auditReportVersion": 2, "vulnerabilities": {"lodash": {"name": "lodash", "severity": "high", "isDirect": true, "via": [{"source": 1, "name": "lodash", "title": "Command Injection", "url": "https://github.com/advisories/GHSA-35jh-r3h4-6jhm", "severity": "high", "range": "<4.17.21"}], "effects": [], "fixAvailable": true}}}"#;
        let result: Value = serde_json::from_str(&vulnerabilities_result(
            &json!({"target": targets[0], "stdout": report, "checkedAt": 5}).to_string(),
        ))
        .unwrap();
        assert_eq!(result["status"], "ok");
        let text = "{\"dependencies\": {\"lodash\": \"^4.17.20\"}}";
        let problems: Value = serde_json::from_str(&vulnerabilities_problems(
            &json!({"path": "package.json", "text": text, "vulnerabilities": result["vulnerabilities"]})
                .to_string(),
        ))
        .unwrap();
        assert_eq!(problems[0]["upgrade"]["constraint"], "^4.17.21");
        let upgraded: Value = serde_json::from_str(&vulnerabilities_upgrade(
            &json!({"path": "package.json", "text": text, "package": "lodash", "fixed": "4.17.21"})
                .to_string(),
        ))
        .unwrap();
        assert_eq!(upgraded, "{\"dependencies\": {\"lodash\": \"^4.17.21\"}}");
    }
}
