//! CForge v2 (`cforge-analyzers`, `cforge-embedded`, `cforge_cmake::packages`) for the
//! browser-mode mock: sanitizer reports, Valgrind XML and `perf script` output with project
//! paths, `platformio.ini` and the PlatformIO task commands, SVD files with decoded register
//! values, and vcpkg port completion — the same code as `cpp_sanitizer_reports`,
//! `cpp_valgrind_open`, the `perf` run event, `embedded_info`, `pio_command`,
//! `cpp_svd_peripherals` / `cpp_svd_read` and `cpp_vcpkg_complete`.

use std::collections::BTreeMap;

use cforge_analyzers::{perf, project_path, sanitizer, valgrind};
use cforge_embedded::platformio::{self, PioTask};
use cforge_embedded::svd;
use serde::Deserialize;
use serde_json::json;
use wasm_bindgen::prelude::*;

use crate::to_json;

fn error(e: impl ToString) -> String {
    to_json(&json!({ "error": e.to_string() }))
}

fn parse<T: for<'de> Deserialize<'de>>(json: &str) -> Result<T, String> {
    serde_json::from_str(json).map_err(|e| e.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OutputRequest {
    text: String,
    /// Absolute project root.
    root: String,
    #[serde(default)]
    build_dir: String,
    /// Project files (relative), for relative paths.
    #[serde(default)]
    files: Vec<String>,
}

impl OutputRequest {
    fn resolver(&self) -> impl Fn(&str) -> Option<String> + '_ {
        move |f: &str| {
            let exists = |p: &str| self.files.iter().any(|x| x == p);
            project_path(f, &self.root, &self.build_dir, &exists)
        }
    }
}

/// `{ text, root, buildDir, files }` → sanitizer reports with project paths.
#[wasm_bindgen]
pub fn cpp_sanitizer_reports(request_json: &str) -> String {
    match parse::<OutputRequest>(request_json) {
        Ok(r) => {
            let mut reports = sanitizer::parse(&r.text);
            sanitizer::resolve(&mut reports, &r.resolver());
            to_json(&reports)
        }
        Err(e) => error(e),
    }
}

/// `{ text (Valgrind XML), root, files }` → the Valgrind report.
#[wasm_bindgen]
pub fn cpp_valgrind_report(request_json: &str) -> String {
    match parse::<OutputRequest>(request_json).and_then(|r| {
        let mut report = valgrind::parse(&r.text)?;
        valgrind::resolve(&mut report, &r.resolver());
        Ok(report)
    }) {
        Ok(report) => to_json(&report),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PerfRequest {
    text: String,
    #[serde(default)]
    sample_index: Option<usize>,
}

/// `{ text (perf script output), sampleIndex? }` → the Profiler's views.
#[wasm_bindgen]
pub fn cpp_perf_view(request_json: &str) -> String {
    match parse::<PerfRequest>(request_json).and_then(|r| perf::open(&r.text, r.sample_index)) {
        Ok(view) => to_json(&view),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct TextRequest {
    text: String,
}

/// `{ text }` → the parsed `platformio.ini`.
#[wasm_bindgen]
pub fn pio_project(request_json: &str) -> String {
    match parse::<TextRequest>(request_json) {
        Ok(r) => to_json(&platformio::parse(&r.text)),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct PioCommandRequest {
    pio: String,
    env: String,
    task: PioTask,
    cwd: String,
}

/// `{ pio, env, task, cwd }` → the task's command line.
#[wasm_bindgen]
pub fn pio_command(request_json: &str) -> String {
    match parse::<PioCommandRequest>(request_json) {
        Ok(r) => to_json(&platformio::task_command(&r.pio, r.task, &r.env, &r.cwd)),
        Err(e) => error(e),
    }
}

/// `{ text }` → the SVD file's peripherals.
#[wasm_bindgen]
pub fn svd_summary(request_json: &str) -> String {
    match parse::<TextRequest>(request_json).and_then(|r| svd::parse(&r.text)) {
        Ok(d) => to_json(&d.summary()),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct SvdValuesRequest {
    text: String,
    peripheral: String,
    /// Register values by name (others read as their reset value): a simulated target.
    #[serde(default)]
    values: BTreeMap<String, u64>,
}

/// `{ text, peripheral, values }` → decoded registers, as if read from a target whose
/// registers hold `values` (else their reset values).
#[wasm_bindgen]
pub fn svd_peripheral_values(request_json: &str) -> String {
    let result = parse::<SvdValuesRequest>(request_json).and_then(|r| {
        let device = svd::parse(&r.text)?;
        let p = device
            .peripheral(&r.peripheral)
            .ok_or_else(|| format!("no peripheral {}", r.peripheral))?;
        let blocks: Vec<(u64, Vec<u8>)> = svd::read_ranges(p)
            .into_iter()
            .map(|(offset, len)| {
                let mut bytes = vec![0u8; len as usize];
                for reg in &p.registers {
                    let size = reg.size.div_ceil(8) as u64;
                    if reg.offset < offset || reg.offset + size > offset + len as u64 {
                        continue;
                    }
                    let v = r.values.get(&reg.name).copied().unwrap_or(reg.reset_value);
                    for i in 0..size {
                        bytes[(reg.offset - offset + i) as usize] = (v >> (8 * i)) as u8;
                    }
                }
                (offset, bytes)
            })
            .collect();
        Ok(svd::decode(p, &blocks))
    });
    match result {
        Ok(v) => to_json(&v),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct VcpkgCompleteRequest {
    text: String,
    offset: usize,
    ports: Vec<String>,
}

/// `{ text, offset, ports }` → completion or null (`cpp_vcpkg_complete`).
#[wasm_bindgen]
pub fn cpp_vcpkg_complete(request_json: &str) -> String {
    match parse::<VcpkgCompleteRequest>(request_json) {
        Ok(r) => to_json(&cforge_cmake::packages::complete_vcpkg(
            &r.text, r.offset, &r.ports,
        )),
        Err(e) => error(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mirrors_the_commands() {
        let out = "src/a.cpp:3:5: runtime error: division by zero\n";
        let r: serde_json::Value = serde_json::from_str(&cpp_sanitizer_reports(
            &json!({ "text": out, "root": "/p", "files": ["src/a.cpp"] }).to_string(),
        ))
        .unwrap();
        assert_eq!(r[0]["location"]["path"], "src/a.cpp");
        let pio: serde_json::Value = serde_json::from_str(&pio_project(
            &json!({ "text": "[env:uno]\nplatform = atmelavr\nboard = uno\n" }).to_string(),
        ))
        .unwrap();
        assert_eq!(pio["envs"][0]["board"], "uno");
        let svd = "<device><name>X</name><peripherals><peripheral><name>P</name><baseAddress>0x10</baseAddress><registers><register><name>R</name><addressOffset>0</addressOffset><resetValue>0x5</resetValue><fields><field><name>F</name><bitOffset>0</bitOffset><bitWidth>4</bitWidth></field></fields></register></registers></peripheral></peripherals></device>";
        let v: serde_json::Value = serde_json::from_str(&svd_peripheral_values(
            &json!({ "text": svd, "peripheral": "P", "values": { "R": 9 } }).to_string(),
        ))
        .unwrap();
        assert_eq!(v[0]["value"], 9);
        assert_eq!(v[0]["fields"][0]["display"], "0x9 (9)");
        assert!(cpp_perf_view(r#"{"text":""}"#).contains("error"));
    }
}
