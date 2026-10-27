//! Running Forge's inspections over a project's files, shared by the Inspect Code commands
//! (`commands/inspections.rs`) and the command-line inspector (`cli.rs`): which files a scope
//! covers, the providers (the PHP index and data-flow inspections where the project lives,
//! duplicated code) and the profile with its scopes.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use forge_backend::Backend;
use forge_core::{Result, Settings, SettingsScope};
use forge_inspections::duplicates::DuplicateGroup;
use forge_inspections::engine::{DuplicatesProvider, RunContext};
use forge_inspections::{
    run, FileInput, InspectionProvider, Problem, ProfilesState, Registry, Report, ResolvedProfile,
    RunOptions, RunProgress,
};
use forge_scopes::{NamedScope, Scope, ScopeContext, ScopeFilter, ScopeSet};
use phpforge_lang_php::offsets::utf16_to_byte;
use serde::{Deserialize, Serialize};

/// Files larger than this are not inspected (generated code, bundles).
const MAX_FILE_SIZE: usize = 1024 * 1024;
/// Most files one run reads.
pub const MAX_FILES: usize = 20_000;

/// What Inspect Code covers.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct InspectScope {
    /// `project`, `module`, `directory`, `file`, `files` or `scope`.
    pub kind: String,
    /// The directory or file (project-relative).
    pub path: Option<String>,
    /// Several files (`files`: Open Files, Changed Files).
    pub paths: Vec<String>,
    /// A named or predefined scope (`scope`).
    pub filter: Option<ScopeFilter>,
    /// Include test sources (Project / Module / Directory).
    pub include_tests: Option<bool>,
    /// Shown in the results (`Directory 'src'`).
    pub label: String,
}

/// Whether a file is source code worth inspecting (not binary data, lock files or minified
/// bundles).
fn is_source(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let name = lower.rsplit('/').next().unwrap_or(&lower);
    if name.ends_with(".min.js")
        || name.ends_with(".min.css")
        || name.ends_with(".lock")
        || name.ends_with(".map")
    {
        return false;
    }
    let ext = name.rsplit_once('.').map_or("", |(_, e)| e);
    matches!(
        ext,
        "php"
            | "phtml"
            | "inc"
            | "js"
            | "jsx"
            | "ts"
            | "tsx"
            | "mjs"
            | "cjs"
            | "vue"
            | "py"
            | "rs"
            | "go"
            | "java"
            | "kt"
            | "kts"
            | "scala"
            | "groovy"
            | "c"
            | "h"
            | "cc"
            | "cpp"
            | "hpp"
            | "cs"
            | "lua"
            | "rb"
            | "swift"
            | "dart"
            | "css"
            | "scss"
            | "less"
            | "twig"
            | "sql"
    )
}

/// The project files `scope` covers (paths only).
pub fn scope_files(
    backend: &dyn Backend,
    scope: &InspectScope,
    test_roots: &[String],
) -> Result<Vec<String>> {
    let all = backend.list_files(200_000)?;
    let ctx = ScopeContext {
        test_roots: test_roots.to_vec(),
        ..ScopeContext::default()
    };
    let compiled = scope.filter.as_ref().and_then(|f| Scope::new(f).ok());
    let include_tests = scope.include_tests.unwrap_or(true);
    let mut out: Vec<String> = all
        .into_iter()
        .filter(|p| is_source(p) && !forge_scopes::is_library_path(p))
        .filter(|p| match scope.kind.as_str() {
            "file" => scope.path.as_deref() == Some(p.as_str()),
            "files" => scope.paths.iter().any(|x| x == p),
            "directory" => scope.path.as_deref().is_some_and(|d| {
                let d = d.trim_matches('/');
                d.is_empty() || p.starts_with(&format!("{d}/"))
            }),
            "scope" => compiled.as_ref().is_some_and(|s| s.contains(p)),
            _ => true,
        })
        .filter(|p| {
            include_tests
                || matches!(scope.kind.as_str(), "file" | "files" | "scope")
                || !ctx.is_test(p)
        })
        .collect();
    out.truncate(MAX_FILES);
    Ok(out)
}

/// Read the files (skipping unreadable and huge ones), polling `cancelled`.
pub fn read_files(
    backend: &dyn Backend,
    paths: &[String],
    progress: &mut dyn FnMut(RunProgress),
    cancelled: &dyn Fn() -> bool,
) -> Vec<FileInput> {
    let mut out = Vec::new();
    for (i, p) in paths.iter().enumerate() {
        if cancelled() {
            break;
        }
        if i % 50 == 0 {
            progress(RunProgress {
                done: i,
                total: paths.len(),
                text: format!("Reading {p}"),
            });
        }
        if let Ok(text) = backend.read_file(p) {
            if text.len() <= MAX_FILE_SIZE && !text.contains('\0') {
                out.push(FileInput {
                    path: p.clone(),
                    text,
                });
            }
        }
    }
    out
}

fn is_php(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    (lower.ends_with(".php") || lower.ends_with(".phtml") || lower.ends_with(".inc"))
        && !lower.ends_with(".blade.php")
}

/// The PHP index inspections and PHP data flow, computed where the project lives.
pub struct PhpProvider {
    pub backend: Arc<dyn Backend>,
}

impl PhpProvider {
    /// The inspections the PHP index reports.
    pub fn ids() -> Vec<String> {
        [
            "PhpDeprecation",
            "PhpIllegalPsrClassPath",
            "PhpExpectedValuesShouldBeUsed",
            "PhpImmutablePropertyIsChanged",
            "PhpExpressionResultUnused",
            "PhpUnreachableStatement",
            phpforge_lang_php::dataflow::ALWAYS_NULL,
            phpforge_lang_php::dataflow::UNDEFINED,
            phpforge_lang_php::dataflow::CONSTANT_CONDITION,
            phpforge_lang_php::dataflow::UNUSED_ASSIGNMENT,
        ]
        .into_iter()
        .map(String::from)
        .collect()
    }
}

impl InspectionProvider for PhpProvider {
    fn inspections(&self) -> Vec<String> {
        Self::ids()
    }

    fn inspect_file(&self, file: &FileInput, _ctx: &RunContext) -> Vec<Problem> {
        if !is_php(&file.path) {
            return Vec::new();
        }
        let Ok(problems) = self
            .backend
            .php()
            .inspect(&file.path, Some(file.text.clone()))
        else {
            return Vec::new();
        };
        let text = &file.text;
        problems
            .into_iter()
            .map(|mut p| {
                // The backend answers in UTF-16 (for the editor); the runner works in bytes.
                p.start = utf16_to_byte(text, p.start);
                p.end = utf16_to_byte(text, p.end);
                if let Some(f) = p.fix.as_mut() {
                    for e in &mut f.edits {
                        e.start = utf16_to_byte(text, e.start);
                        e.end = utf16_to_byte(text, e.end);
                    }
                }
                phpforge_lang_php::inspections::to_problem(&file.path, p)
            })
            .collect()
    }
}

/// Duplicated code that also keeps its groups for the Duplicates view.
#[derive(Default)]
pub struct GroupingDuplicates {
    groups: Mutex<Vec<DuplicateGroup>>,
}

impl InspectionProvider for GroupingDuplicates {
    fn inspections(&self) -> Vec<String> {
        DuplicatesProvider.inspections()
    }

    fn inspect_project(&self, files: &[FileInput], ctx: &RunContext) -> Vec<Problem> {
        let spec = ctx
            .registry()
            .get(forge_inspections::registry::DUPLICATED_CODE);
        let options = spec
            .map(|s| ctx.profile.profile.options(s))
            .unwrap_or_default();
        let opts = forge_inspections::duplicates::DuplicateOptions::from_options(&options);
        let list: Vec<(&str, &str)> = files
            .iter()
            .filter(|f| {
                ctx.effective(forge_inspections::registry::DUPLICATED_CODE, &f.path)
                    .is_some()
            })
            .map(|f| (f.path.as_str(), f.text.as_str()))
            .collect();
        let groups = forge_inspections::duplicates::find(&list, &opts, &|| ctx.cancelled());
        let problems = forge_inspections::duplicates::problems(&groups);
        if let Ok(mut g) = self.groups.lock() {
            *g = groups;
        }
        problems
    }
}

/// The named scopes of the settings (`scopes.shared` in the project, `scopes.local`).
pub fn scope_set(settings: &Settings) -> ScopeSet {
    ScopeSet {
        shared: settings
            .get_as::<Vec<NamedScope>>("scopes.shared")
            .unwrap_or_default(),
        local: settings
            .get_as::<Vec<NamedScope>>("scopes.local")
            .unwrap_or_default(),
    }
}

/// The profiles from the settings layers and the project's profile files.
pub fn load_profiles(
    settings: &Settings,
    backend: Option<&dyn Backend>,
    registry: &Registry,
) -> ProfilesState {
    let user = settings.layer_values(SettingsScope::User).clone();
    let project = settings
        .has_project()
        .then(|| settings.layer_values(SettingsScope::Project).clone());
    let read = |rel: &str| backend.and_then(|b| b.read_optional(rel).ok().flatten());
    forge_inspections::store::load(&user, project.as_ref(), &read, registry)
}

/// Run every enabled inspection of `profile` over `files`.
#[allow(clippy::too_many_arguments)]
pub fn inspect(
    backend: Arc<dyn Backend>,
    registry: Registry,
    profiles: &ProfilesState,
    profile_name: Option<&str>,
    scopes: &ScopeSet,
    test_roots: &[String],
    files: &[FileInput],
    options: &RunOptions,
    progress: &mut dyn FnMut(RunProgress),
    cancelled: &dyn Fn() -> bool,
) -> Report {
    let profile = profile_name
        .and_then(|n| profiles.get(n))
        .unwrap_or_else(|| profiles.selected())
        .clone();
    let ctx = ScopeContext {
        test_roots: test_roots.to_vec(),
        ..ScopeContext::default()
    };
    let resolved =
        ResolvedProfile::new(registry, profile, profiles.severities.clone(), scopes, &ctx);
    let php = PhpProvider { backend };
    let dups = GroupingDuplicates::default();
    let mut report = run(
        files,
        &resolved,
        &[&php, &dups],
        &[],
        options,
        progress,
        cancelled,
    );
    if let Ok(mut g) = dups.groups.lock() {
        report.duplicates = std::mem::take(&mut *g);
    }
    report
}

/// The texts of `files` by path (Code Cleanup).
pub fn texts(files: &[FileInput]) -> BTreeMap<String, String> {
    files
        .iter()
        .map(|f| (f.path.clone(), f.text.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_files_only() {
        assert!(is_source("src/A.php"));
        assert!(is_source("ui/app.tsx"));
        assert!(!is_source("public/app.min.js"));
        assert!(!is_source("composer.lock"));
        assert!(!is_source("logo.png"));
    }
}
