//! WebForge (`packs/webforge/*`) for the browser-mode mock: npm packages and scripts,
//! package manager detection, and React/Vue/Angular component navigation over the demo
//! project's files, mirroring the `npm_*` and `web_*` IPC commands.

use serde::Deserialize;
use wasm_bindgen::prelude::*;
use webforge_lang_web::{npm, FrameworkIndex, SourceFile, Sources};

use crate::parsers::parse;
use crate::to_json;

/// The project as the mock knows it: files with content plus other known paths.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Project {
    #[serde(default)]
    files: Vec<SourceFile>,
    #[serde(default)]
    paths: Vec<String>,
}

impl Project {
    fn sources(self) -> Sources {
        Sources::new(self.files, self.paths)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PackagesRequest {
    #[serde(flatten)]
    project: Project,
    #[serde(default)]
    package_manager: Option<String>,
}

/// `{ files, paths, packageManager? }` -> `NpmPackage[]`.
#[wasm_bindgen]
pub fn web_npm_packages(request_json: &str) -> String {
    match parse::<PackagesRequest>(request_json) {
        Ok(r) => {
            let setting = r.package_manager.unwrap_or_else(|| "auto".into());
            to_json(&npm::packages(&r.project.sources(), &setting))
        }
        Err(e) => e,
    }
}

#[derive(Deserialize)]
struct FileRequest {
    path: String,
    content: String,
}

/// `{ path, content }` -> `GutterMark[]` (run icons of a package.json).
#[wasm_bindgen]
pub fn web_npm_gutter(request_json: &str) -> String {
    match parse::<FileRequest>(request_json) {
        Ok(r) => to_json(&npm::gutter(&r.path, &r.content)),
        Err(e) => e,
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FrameworkRequest {
    framework: String,
    #[serde(flatten)]
    project: Project,
    #[serde(default)]
    path: String,
    #[serde(default)]
    content: String,
    #[serde(default)]
    offset: usize,
}

fn with_index(
    request_json: &str,
    f: impl FnOnce(&dyn FrameworkIndex, &FrameworkRequest) -> String,
) -> String {
    let mut r = match parse::<FrameworkRequest>(request_json) {
        Ok(r) => r,
        Err(e) => return e,
    };
    let sources = std::mem::replace(
        &mut r.project,
        Project {
            files: Vec::new(),
            paths: Vec::new(),
        },
    )
    .sources();
    match r.framework.as_str() {
        "react" => f(&webforge_fw_react::ReactIndex::new(&sources), &r),
        "vue" => f(&webforge_fw_vue::VueIndex::new(&sources), &r),
        "angular" => f(&webforge_fw_angular::AngularIndex::new(&sources), &r),
        other => to_json(&serde_json::json!({ "error": format!("unknown framework {other}") })),
    }
}

/// `{ framework, files, paths }` -> `Component[]`.
#[wasm_bindgen]
pub fn web_components(request_json: &str) -> String {
    with_index(request_json, |index, _| to_json(&index.components()))
}

/// `{ framework, files, paths, path, content, offset }` -> `Location[]`.
#[wasm_bindgen]
pub fn web_goto(request_json: &str) -> String {
    with_index(request_json, |index, r| {
        to_json(&index.goto(&r.path, &r.content, r.offset))
    })
}

/// `{ framework, files, paths, path, content }` -> `GutterMark[]`.
#[wasm_bindgen]
pub fn web_gutter(request_json: &str) -> String {
    with_index(request_json, |index, r| {
        to_json(&index.gutter(&r.path, &r.content))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridges_answer_json() {
        let files = r#"[{"path":"package.json","content":"{\"scripts\":{\"dev\":\"vite\"}}"},
            {"path":"src/App.tsx","content":"import Card from './Card';\nexport function App() { return <Card />; }\n"},
            {"path":"src/Card.tsx","content":"export default function Card() { return <div/>; }\n"}]"#;
        let pkgs = web_npm_packages(&format!(r#"{{"files":{files},"paths":["yarn.lock"]}}"#));
        assert!(pkgs.contains("\"manager\":\"yarn\"") && pkgs.contains("\"dev\""));
        let goto = web_goto(&format!(
            r#"{{"framework":"react","files":{files},"path":"src/App.tsx","content":"import Card from './Card';\nexport function App() {{ return <Card />; }}\n","offset":60}}"#
        ));
        assert!(goto.contains("src/Card.tsx"), "{goto}");
        assert!(web_components(r#"{"framework":"svelte"}"#).contains("error"));
    }
}
