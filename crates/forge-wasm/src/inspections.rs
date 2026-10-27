//! Inspections (`forge-inspections`), called synchronously by the UI in the desktop app and
//! in browser mode: the Settings tree of a profile and its edits, which diagnostics the
//! selected profile keeps (and how severe they are) and which suppression comments hide,
//! the Suppress actions of Alt+Enter, Run Inspection by Name, the Inspection Results tree,
//! and, for browser mode, a whole run over the demo files.

use std::cell::RefCell;
use std::collections::BTreeMap;

use forge_inspections::io::{self as profile_io, ProfileFormat};
use forge_inspections::profile::{self, ProfileOp};
use forge_inspections::report::{self, Grouping};
use forge_inspections::suppress::{self, Suppressions};
use forge_inspections::text::LineIndex;
use forge_inspections::{
    cleanup, severity, FileInput, InspectionSpec, Problem, Profile, ProfilesState, Registry, Report,
    ResolvedProfile, RunOptions, SeverityDef, ToolMapping,
};
use forge_scopes::{ScopeContext, ScopeSet};
use serde::Deserialize;
use serde_json::Value;
use wasm_bindgen::prelude::*;

use crate::to_json;

fn parse<T: for<'de> Deserialize<'de>>(json: &str) -> Result<T, String> {
    serde_json::from_str(json).map_err(|e| e.to_string())
}

fn error(e: String) -> String {
    to_json(&serde_json::json!({ "error": e }))
}

fn answer<T: serde::Serialize>(r: Result<T, String>) -> String {
    match r {
        Ok(v) => to_json(&v),
        Err(e) => error(e),
    }
}

/// `{ inspections, mappings }` (contributions of the enabled plugins) → `Registry` with the
/// framework's own inspections.
#[wasm_bindgen]
pub fn inspections_registry(request_json: &str) -> String {
    #[derive(Deserialize)]
    struct Req {
        #[serde(default)]
        inspections: Vec<InspectionSpec>,
        #[serde(default)]
        mappings: Vec<ToolMapping>,
    }
    answer(parse::<Req>(request_json).map(|r| Registry::new(r.inspections, r.mappings)))
}

/// `{ registry, profile, query }` → `ViewGroup` (the Settings tree with the profile's state).
#[wasm_bindgen]
pub fn inspections_view(request_json: &str) -> String {
    #[derive(Deserialize)]
    struct Req {
        registry: Registry,
        profile: Profile,
        #[serde(default)]
        query: String,
    }
    answer(parse::<Req>(request_json).map(|r| profile::view(&r.registry, &r.profile, &r.query)))
}

/// `{ registry, state, op }` → the changed `ProfilesState` (or `{ error }`).
#[wasm_bindgen]
pub fn inspections_profile_op(request_json: &str) -> String {
    #[derive(Deserialize)]
    struct Req {
        registry: Registry,
        state: ProfilesState,
        op: ProfileOp,
    }
    answer(parse::<Req>(request_json).and_then(|mut r| {
        r.state.apply(&r.registry, r.op)?;
        Ok(r.state)
    }))
}

/// `{ custom }` → every severity (built-in, then custom).
#[wasm_bindgen]
pub fn inspections_severities(request_json: &str) -> String {
    #[derive(Deserialize)]
    struct Req {
        #[serde(default)]
        custom: Vec<SeverityDef>,
    }
    answer(parse::<Req>(request_json).map(|r| severity::all(&r.custom)))
}

/// `{ registry, profile, format }` → the profile as PHPStorm XML or JSON.
#[wasm_bindgen]
pub fn inspections_export_profile(request_json: &str) -> String {
    #[derive(Deserialize)]
    struct Req {
        registry: Registry,
        profile: Profile,
        format: ProfileFormat,
    }
    answer(parse::<Req>(request_json).map(|r| profile_io::export(&r.profile, &r.registry, r.format)))
}

/// `{ registry, text }` → `Profile` (XML or JSON).
#[wasm_bindgen]
pub fn inspections_import_profile(request_json: &str) -> String {
    #[derive(Deserialize)]
    struct Req {
        registry: Registry,
        text: String,
    }
    answer(parse::<Req>(request_json).and_then(|r| profile_io::import(&r.text, &r.registry)))
}

/// `{ registry, query, limit }` → matching `InspectionSpec`s (Run Inspection by Name).
#[wasm_bindgen]
pub fn inspections_find(request_json: &str) -> String {
    #[derive(Deserialize)]
    struct Req {
        registry: Registry,
        #[serde(default)]
        query: String,
        #[serde(default)]
        limit: Option<usize>,
    }
    answer(parse::<Req>(request_json).map(|r| {
        r.registry
            .find_by_name(&r.query, r.limit.unwrap_or(50))
            .into_iter()
            .cloned()
            .collect::<Vec<_>>()
    }))
}

thread_local! {
    /// The selected profile, set whenever the profiles, plugins or scopes change.
    static ACTIVE: RefCell<Option<ResolvedProfile>> = const { RefCell::new(None) };
}

/// `{ registry, profile, severities, scopes, context }`: the profile the editor's
/// diagnostics go through from now on → `true`.
#[wasm_bindgen]
pub fn inspections_configure(request_json: &str) -> String {
    #[derive(Deserialize)]
    struct Req {
        registry: Registry,
        profile: Profile,
        #[serde(default)]
        severities: Vec<SeverityDef>,
        #[serde(default)]
        scopes: ScopeSet,
        #[serde(default)]
        context: ScopeContext,
    }
    answer(parse::<Req>(request_json).map(|r| {
        let resolved = ResolvedProfile::new(r.registry, r.profile, r.severities, &r.scopes, &r.context);
        ACTIVE.with(|a| *a.borrow_mut() = Some(resolved));
        true
    }))
}

fn position(d: &Value, which: &str) -> Option<(u32, u32)> {
    let p = d.get("range")?.get(which)?;
    Some((p.get("line")?.as_u64()? as u32, p.get("character")?.as_u64()? as u32))
}

fn editor_name(e: forge_inspections::EditorSeverity) -> &'static str {
    match e {
        forge_inspections::EditorSeverity::Error => "error",
        forge_inspections::EditorSeverity::Warning => "warning",
        forge_inspections::EditorSeverity::Weak => "weak",
        forge_inspections::EditorSeverity::Typo => "typo",
        forge_inspections::EditorSeverity::Info => "info",
    }
}

/// `{ path, source, text?, diagnostics }` → the diagnostics the profile keeps, with their
/// severity and `inspection` / `inspectionName` / `severityName` set; suppression comments
/// in `text` hide theirs. Without a configured profile everything is kept.
#[wasm_bindgen]
pub fn inspections_apply(request_json: &str) -> String {
    #[derive(Deserialize)]
    struct Req {
        path: String,
        #[serde(default)]
        source: String,
        #[serde(default)]
        text: Option<String>,
        diagnostics: Vec<Value>,
    }
    let r = match parse::<Req>(request_json) {
        Ok(r) => r,
        Err(e) => return error(e),
    };
    ACTIVE.with(|a| {
        let active = a.borrow();
        let Some(profile) = active.as_ref() else {
            return to_json(&r.diagnostics);
        };
        let suppressions = r
            .text
            .as_deref()
            .filter(|t| t.contains("noinspection"))
            .map(|t| (Suppressions::parse(&r.path, t), LineIndex::new(t)));
        let mut out = Vec::new();
        for mut d in r.diagnostics {
            let code = d.get("code").and_then(Value::as_str).map(str::to_string);
            let Some(id) = profile
                .registry
                .inspection_for(&r.source, code.as_deref())
                .map(str::to_string)
            else {
                out.push(d);
                continue;
            };
            let Some(eff) = profile.effective(&id, &r.path) else {
                continue;
            };
            if let (Some((sup, lines)), Some((line, col))) = (&suppressions, position(&d, "start")) {
                if sup.is_suppressed(&id, lines.offset(line, col)) {
                    continue;
                }
            }
            let spec = profile.registry.get(&id);
            if let Some(obj) = d.as_object_mut() {
                obj.insert("inspection".into(), Value::String(id.clone()));
                if let Some(s) = spec {
                    obj.insert("inspectionName".into(), Value::String(s.name.clone()));
                }
                if let Some(level) = &eff.severity {
                    let editor = severity::editor_of(level, &profile.severities);
                    obj.insert("severity".into(), Value::String(editor_name(editor).into()));
                    obj.insert(
                        "severityName".into(),
                        Value::String(severity::display_name(level, &profile.severities)),
                    );
                }
            }
            out.push(d);
        }
        to_json(&out)
    })
}

/// `{ path, text, line, character, id, bytes? }` → `SuppressAction[]` (changes in UTF-16
/// offsets for the editor, in bytes with `bytes: true` for the backend).
#[wasm_bindgen]
pub fn inspections_suppress_actions(request_json: &str) -> String {
    #[derive(Deserialize)]
    struct Req {
        path: String,
        text: String,
        line: u32,
        character: u32,
        id: String,
        #[serde(default)]
        bytes: bool,
    }
    answer(parse::<Req>(request_json).map(|r| {
        let offset = LineIndex::new(&r.text).offset(r.line, r.character);
        let mut actions = suppress::actions(&r.path, &r.text, offset, &r.id);
        if !r.bytes {
            for a in &mut actions {
                suppress::to_utf16(&r.text, &mut a.changes);
            }
        }
        actions
    }))
}

/// `{ report, levels }` → `ResultNode[]` (Inspection Results tree).
#[wasm_bindgen]
pub fn inspections_report_tree(request_json: &str) -> String {
    #[derive(Deserialize)]
    struct Req {
        report: Report,
        levels: Vec<Grouping>,
    }
    answer(parse::<Req>(request_json).map(|r| report::tree(&r.report, &r.levels)))
}

/// `{ report, format: "html" | "xml" | "json" }` → the exported text.
#[wasm_bindgen]
pub fn inspections_export_report(request_json: &str) -> String {
    #[derive(Deserialize)]
    struct Req {
        report: Report,
        format: String,
    }
    answer(parse::<Req>(request_json).map(|r| match r.format.as_str() {
        "html" => report::to_html(&r.report),
        "json" => report::to_json(&r.report),
        _ => report::to_xml(&r.report),
    }))
}

/// A provider that hands over problems computed elsewhere (browser mode's PHP problems).
struct Given(Vec<Problem>);

impl forge_inspections::InspectionProvider for Given {
    fn inspections(&self) -> Vec<String> {
        self.0.iter().map(|p| p.inspection.clone()).collect()
    }

    fn inspect_project(&self, _files: &[FileInput], _ctx: &forge_inspections::RunContext) -> Vec<Problem> {
        self.0.clone()
    }
}

/// Browser mode's Inspect Code: `{ registry, profiles, profile?, files, problems, scopes,
/// context, only?, scope }` → `Report` (duplicated code is found here; `problems` are the
/// other providers' results).
#[wasm_bindgen]
pub fn inspections_run(request_json: &str) -> String {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Req {
        registry: Registry,
        profiles: ProfilesState,
        #[serde(default)]
        profile: Option<String>,
        files: Vec<FileInput>,
        #[serde(default)]
        problems: Vec<Problem>,
        #[serde(default)]
        scopes: ScopeSet,
        #[serde(default)]
        context: ScopeContext,
        #[serde(default)]
        only: Option<Vec<String>>,
        #[serde(default)]
        scope: String,
        #[serde(default)]
        cleanup: bool,
    }
    answer(parse::<Req>(request_json).map(|mut r| {
        let profile = r
            .profile
            .as_deref()
            .and_then(|n| r.profiles.get(n))
            .unwrap_or_else(|| r.profiles.selected())
            .clone();
        let only = if r.cleanup {
            Some(r.registry.inspections.iter().filter(|s| s.cleanup).map(|s| s.id.clone()).collect())
        } else {
            r.only
        };
        r.registry
            .inspections
            .retain(|s| s.batch || only.as_ref().is_some_and(|o| o.contains(&s.id)));
        let resolved = ResolvedProfile::new(r.registry, profile, r.profiles.severities.clone(), &r.scopes, &r.context);
        let given = Given(r.problems);
        let dups = forge_inspections::DuplicatesProvider;
        let options = RunOptions { only, max_problems: 20_000 };
        let mut report = forge_inspections::run(&r.files, &resolved, &[&given, &dups], &[], &options, &mut |_| {}, &|| false);
        report.scope = r.scope;
        let opts = forge_inspections::duplicates::DuplicateOptions::from_options(
            &resolved
                .registry
                .get(forge_inspections::registry::DUPLICATED_CODE)
                .map(|s| resolved.profile.options(s))
                .unwrap_or_default(),
        );
        if report.problems.iter().any(|p| p.inspection == forge_inspections::registry::DUPLICATED_CODE) {
            let list: Vec<(&str, &str)> = r.files.iter().map(|f| (f.path.as_str(), f.text.as_str())).collect();
            report.duplicates = forge_inspections::duplicates::find(&list, &opts, &|| false);
        }
        report
    }))
}

/// `{ registry, problems, texts }` → `CleanupPlan` (browser mode's Code Cleanup).
#[wasm_bindgen]
pub fn inspections_cleanup_plan(request_json: &str) -> String {
    #[derive(Deserialize)]
    struct Req {
        registry: Registry,
        problems: Vec<forge_inspections::ReportProblem>,
        texts: BTreeMap<String, String>,
    }
    answer(parse::<Req>(request_json).map(|r| cleanup::plan(&r.problems, &r.texts, &r.registry)))
}

/// `{ text, edits }` → the text with the edits applied (browser mode's Apply Fix).
#[wasm_bindgen]
pub fn inspections_apply_edits(request_json: &str) -> String {
    #[derive(Deserialize)]
    struct Req {
        text: String,
        edits: Vec<forge_inspections::FileEdit>,
    }
    answer(parse::<Req>(request_json).and_then(|r| cleanup::apply_edits(&r.text, &r.edits)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn registry() -> Value {
        json!({
            "inspections": [
                {"id": "PhpUnusedAssignment", "name": "Unused assignment", "group": "PHP/Data flow", "severity": "WEAK WARNING", "cleanup": true},
                {"id": "RsClippy", "name": "Clippy", "group": "Rust", "external": true, "batch": false}
            ],
            "mappings": [{"source": "clippy", "inspection": "RsClippy"}]
        })
    }

    #[test]
    fn configure_and_apply_profiles_to_diagnostics() {
        let reg: Value = serde_json::from_str(&inspections_registry(&registry().to_string())).unwrap();
        let mut profile = json!({"name": "Default", "level": "ide", "tools": {"RsClippy": {"severity": "ERROR"}}});
        let ok = inspections_configure(&json!({"registry": reg, "profile": profile}).to_string());
        assert_eq!(ok, "true");
        let text = "fn f() {\n    let a = 1; // noinspection RsClippy\n    let b = 2;\n}\n";
        let diag = |line: u32| json!({"range": {"start": {"line": line, "character": 8}, "end": {"line": line, "character": 9}}, "severity": "warning", "message": "m", "code": "x"});
        let out: Vec<Value> = serde_json::from_str(&inspections_apply(
            &json!({"path": "src/a.rs", "source": "clippy", "text": text, "diagnostics": [diag(1), diag(2)]}).to_string(),
        ))
        .unwrap();
        assert_eq!(out.len(), 1, "{out:?}");
        assert_eq!(out[0]["severity"], "error");
        assert_eq!(out[0]["inspection"], "RsClippy");
        assert_eq!(out[0]["severityName"], "Error");
        // Off in the profile: gone; unknown sources pass through.
        profile["tools"]["RsClippy"] = json!({"enabled": false});
        inspections_configure(&json!({"registry": reg, "profile": profile}).to_string());
        let out: Vec<Value> = serde_json::from_str(&inspections_apply(
            &json!({"path": "src/a.rs", "source": "clippy", "diagnostics": [diag(2)]}).to_string(),
        ))
        .unwrap();
        assert!(out.is_empty());
        let out: Vec<Value> = serde_json::from_str(&inspections_apply(
            &json!({"path": "src/a.rs", "source": "lsp", "diagnostics": [diag(2)]}).to_string(),
        ))
        .unwrap();
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn suppress_actions_and_runs() {
        let actions: Vec<Value> = serde_json::from_str(&inspections_suppress_actions(
            &json!({"path": "a.php", "text": "<?php\nfunction f() {\n    $a = 1;\n}\n", "line": 2, "character": 4, "id": "PhpUnusedAssignment"}).to_string(),
        ))
        .unwrap();
        let titles: Vec<&str> = actions.iter().map(|a| a["title"].as_str().unwrap()).collect();
        assert_eq!(titles, ["Suppress for statement", "Suppress for function", "Suppress for file"]);
        let reg: Value = serde_json::from_str(&inspections_registry(&registry().to_string())).unwrap();
        let body = "    $total = 0;\n    foreach ($items as $item) {\n        $total += $item->price * $item->quantity;\n    }\n    return round($total, 2);\n";
        let files = json!([
            {"path": "a.php", "text": format!("<?php\nfunction a($items) {{\n{body}}}\n")},
            {"path": "b.php", "text": format!("<?php\nfunction b($items) {{\n{body}}}\n")}
        ]);
        let profiles = json!({"profiles": [{"name": "Default", "level": "ide", "tools": {"DuplicatedCode": {"options": {"minTokens": 20}}}}], "selected": "Default", "hasProject": false});
        let problems = json!([{"inspection": "PhpUnusedAssignment", "path": "a.php", "start": 6, "end": 14, "message": "unused",
            "fixes": [{"title": "Remove", "edits": [{"path": "a.php", "start": 0, "end": 0, "text": "x"}], "safe": true}]}]);
        let report: Report = serde_json::from_str(&inspections_run(
            &json!({"registry": reg, "profiles": profiles, "files": files, "problems": problems, "scope": "Project"}).to_string(),
        ))
        .unwrap();
        assert!(report.problems.iter().any(|p| p.inspection == "DuplicatedCode"), "{report:?}");
        assert!(report.problems.iter().any(|p| p.inspection == "PhpUnusedAssignment"));
        assert_eq!(report.duplicates.len(), 1);
        let tree: Vec<Value> = serde_json::from_str(&inspections_report_tree(
            &json!({"report": report, "levels": ["severity", "inspection"]}).to_string(),
        ))
        .unwrap();
        assert!(!tree.is_empty());
        let plan: Value = serde_json::from_str(&inspections_cleanup_plan(
            &json!({"registry": reg, "problems": report.problems, "texts": {"a.php": "<?php"}}).to_string(),
        ))
        .unwrap();
        assert_eq!(plan["files"][0]["after"], "x<?php");
    }
}
