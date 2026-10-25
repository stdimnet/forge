//! Dev Containers and WSL (`forge-devenv` without its runtime): `devcontainer.json`
//! resolving and the gutter line, the `docker ps` list and `wsl.exe -l -v` parsing and the
//! path mapping, mirroring `devcontainer_configs`, `devcontainer_gutter`,
//! `devcontainer_list`, `wsl_distributions`, `wsl_map_path` and `wsl_windows_path`.

use forge_devenv::devcontainer::{self, LocalContext};
use forge_devenv::{docker, wsl};
use serde::Deserialize;
use wasm_bindgen::prelude::*;

use crate::to_json;

fn error(e: impl std::fmt::Display) -> String {
    to_json(&serde_json::json!({ "error": e.to_string() }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResolveRequest {
    text: String,
    folder: String,
    config_path: String,
}

/// `{ text, folder, configPath }` → `{ config, source } | { error }` (the browser mock has no
/// local environment, so `${localEnv:…}` is empty).
#[wasm_bindgen]
pub fn devcontainer_resolve(request_json: &str) -> String {
    let r: ResolveRequest = match serde_json::from_str(request_json) {
        Ok(r) => r,
        Err(e) => return error(e),
    };
    let env = |_: &str| None;
    match devcontainer::load(
        &r.text,
        &LocalContext {
            folder: &r.folder,
            config_path: &r.config_path,
            env: &env,
        },
    ) {
        Ok(c) => to_json(&serde_json::json!({ "source": c.source_label(), "config": c })),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct GutterRequest {
    path: String,
    content: String,
}

/// `{ path, content }` → `number | null`.
#[wasm_bindgen]
pub fn devcontainer_gutter(request_json: &str) -> String {
    match serde_json::from_str::<GutterRequest>(request_json) {
        Ok(r) => to_json(
            &devcontainer::is_config_file(&r.path)
                .then(|| devcontainer::gutter_line(&r.content))
                .flatten(),
        ),
        Err(e) => error(e),
    }
}

/// `docker ps` output (the format of `docker::list_args`) → `DevContainerInstance[]`.
#[wasm_bindgen]
pub fn devcontainer_parse_list(output: &str) -> String {
    to_json(&docker::parse_list(output))
}

/// `wsl.exe -l -v` output bytes → `Distro[]` (without Docker Desktop's distributions).
#[wasm_bindgen]
pub fn wsl_parse_list(bytes: &[u8]) -> String {
    to_json(
        &wsl::parse_list_bytes(bytes)
            .into_iter()
            .filter(|d| !d.is_internal())
            .collect::<Vec<_>>(),
    )
}

/// A Windows path → `LinuxPath | null`.
#[wasm_bindgen]
pub fn wsl_map_path(path: &str) -> String {
    to_json(&wsl::to_linux(path))
}

#[derive(Deserialize)]
struct WindowsPathRequest {
    distro: String,
    path: String,
}

/// `{ distro, path }` → the Windows view of a Linux path.
#[wasm_bindgen]
pub fn wsl_windows_path(request_json: &str) -> String {
    match serde_json::from_str::<WindowsPathRequest>(request_json) {
        Ok(r) => to_json(&wsl::to_windows(&r.distro, &r.path)),
        Err(e) => error(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_and_maps() {
        let req = serde_json::json!({
            "text": "{ // c\n \"image\": \"php:8.3\", }",
            "folder": "/home/demo/Projects/shop",
            "configPath": "/home/demo/Projects/shop/.devcontainer/devcontainer.json",
        });
        let out: serde_json::Value =
            serde_json::from_str(&devcontainer_resolve(&req.to_string())).unwrap();
        assert_eq!(out["source"], "Image php:8.3");
        assert_eq!(out["config"]["workspaceFolder"], "/workspaces/shop");
        let gutter = serde_json::json!({ "path": "/a/.devcontainer.json", "content": "\n{}" });
        assert_eq!(devcontainer_gutter(&gutter.to_string()), "1");
        assert_eq!(wsl_map_path("C:\\src"), r#"{"path":"/mnt/c/src"}"#);
        let utf16: Vec<u8> = "* Ubuntu Running 2\r\n  docker-desktop Stopped 2\r\n"
            .encode_utf16()
            .flat_map(|u| u.to_le_bytes())
            .collect();
        let list: serde_json::Value = serde_json::from_str(&wsl_parse_list(&utf16)).unwrap();
        assert_eq!(list.as_array().unwrap().len(), 1);
    }
}
