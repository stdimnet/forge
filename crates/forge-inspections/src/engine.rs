//! Running inspections over a set of files (Inspect Code, Run Inspection by Name, the
//! command-line inspector): providers report problems, the profile decides which count and
//! how severe they are, suppression comments drop theirs.
//!
//! Three kinds of providers plug in:
//! - [`InspectionProvider::inspect_file`]: checks of one file (the PHP index inspections,
//!   PHP data flow);
//! - [`InspectionProvider::inspect_project`]: checks across files (duplicated code);
//! - [`ProblemSource`]: an external tool that yields problems by position and code (PHPStan,
//!   Psalm, linters); [`Registry::inspection_for`] maps them onto inspection ids.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::duplicates::{self, DuplicateOptions};
use crate::problem::{stamp, Problem};
use crate::profile::{Effective, ResolvedProfile};
use crate::registry::{Registry, DUPLICATED_CODE};
use crate::report::{Report, ReportProblem};
use crate::severity;
use crate::suppress::Suppressions;
use crate::text::LineIndex;

/// A shared cancel flag (the Cancel button of the progress, a new run).
pub type CancelFlag = Arc<AtomicBool>;

/// A file to inspect (project-relative path).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileInput {
    pub path: String,
    pub text: String,
}

/// What providers may ask during a run.
pub struct RunContext<'a> {
    pub profile: &'a ResolvedProfile,
    only: Option<&'a [String]>,
    cancelled: &'a dyn Fn() -> bool,
}

impl RunContext<'_> {
    /// The effective settings of `id` for `path` (`None`: do not report it there).
    pub fn effective(&self, id: &str, path: &str) -> Option<Effective> {
        if self.only.is_some_and(|o| !o.iter().any(|x| x == id)) {
            return None;
        }
        self.profile.effective(id, path)
    }

    /// Whether `id` runs at all in this run (some scope or everywhere).
    pub fn wanted(&self, id: &str) -> bool {
        if self.only.is_some_and(|o| !o.iter().any(|x| x == id)) {
            return false;
        }
        self.profile.enabled_anywhere(id)
    }

    pub fn cancelled(&self) -> bool {
        (self.cancelled)()
    }

    pub fn registry(&self) -> &Registry {
        &self.profile.registry
    }
}

/// Forge's own checks.
pub trait InspectionProvider: Send + Sync {
    /// The inspections it reports; it is skipped when none of them runs.
    fn inspections(&self) -> Vec<String>;

    /// Problems of one file.
    fn inspect_file(&self, _file: &FileInput, _ctx: &RunContext) -> Vec<Problem> {
        Vec::new()
    }

    /// Problems across all files, after the files were inspected one by one.
    fn inspect_project(&self, _files: &[FileInput], _ctx: &RunContext) -> Vec<Problem> {
        Vec::new()
    }
}

/// A problem as an external tool reports it (0-based line, UTF-16 columns).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalProblem {
    /// The tool (`phpstan`, `clippy`): matched by the registry's mappings.
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    pub path: String,
    pub line: u32,
    pub column: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_column: Option<u32>,
    pub message: String,
    /// The tool's level (`error`, `warning`, `WEAK WARNING`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,
}

/// An external tool that yields problems for the inspected files.
pub trait ProblemSource: Send + Sync {
    /// Shown in progress and errors (`PHPStan`).
    fn name(&self) -> String;
    fn problems(
        &self,
        files: &[FileInput],
        ctx: &RunContext,
    ) -> Result<Vec<ExternalProblem>, String>;
}

/// Duplicated code over all inspected files.
pub struct DuplicatesProvider;

impl InspectionProvider for DuplicatesProvider {
    fn inspections(&self) -> Vec<String> {
        vec![DUPLICATED_CODE.into()]
    }

    fn inspect_project(&self, files: &[FileInput], ctx: &RunContext) -> Vec<Problem> {
        let spec = ctx.registry().get(DUPLICATED_CODE);
        let options = spec
            .map(|s| ctx.profile.profile.options(s))
            .unwrap_or_default();
        let opts = DuplicateOptions::from_options(&options);
        let list: Vec<(&str, &str)> = files
            .iter()
            .filter(|f| ctx.effective(DUPLICATED_CODE, &f.path).is_some())
            .map(|f| (f.path.as_str(), f.text.as_str()))
            .collect();
        duplicates::problems(&duplicates::find(&list, &opts, &|| ctx.cancelled()))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RunOptions {
    /// Only these inspections (Run Inspection by Name); `None`: the profile's.
    pub only: Option<Vec<String>>,
    /// Stop collecting after this many problems (0: no limit).
    pub max_problems: usize,
}

/// Progress of a run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunProgress {
    pub done: usize,
    pub total: usize,
    /// The file or step being processed.
    pub text: String,
}

/// Per-file data the runner needs to post-process problems.
struct FileInfo<'a> {
    text: &'a str,
    lines: LineIndex<'a>,
    suppressions: Option<Suppressions>,
    stamp: u64,
}

/// Run the providers and sources over `files` with `profile`.
pub fn run(
    files: &[FileInput],
    profile: &ResolvedProfile,
    providers: &[&dyn InspectionProvider],
    sources: &[&dyn ProblemSource],
    options: &RunOptions,
    progress: &mut dyn FnMut(RunProgress),
    cancelled: &dyn Fn() -> bool,
) -> Report {
    let ctx = RunContext {
        profile,
        only: options.only.as_deref(),
        cancelled,
    };
    let active: Vec<&dyn InspectionProvider> = providers
        .iter()
        .copied()
        .filter(|p| p.inspections().iter().any(|id| ctx.wanted(id)))
        .collect();
    let total = files.len() + 1 + sources.len();
    let mut raw: Vec<Problem> = Vec::new();
    let mut errors = Vec::new();
    let mut was_cancelled = false;
    for (k, f) in files.iter().enumerate() {
        if ctx.cancelled() {
            was_cancelled = true;
            break;
        }
        progress(RunProgress {
            done: k,
            total,
            text: f.path.clone(),
        });
        for p in &active {
            raw.extend(p.inspect_file(f, &ctx));
        }
    }
    if !was_cancelled {
        progress(RunProgress {
            done: files.len(),
            total,
            text: "Looking for duplicates and cross-file problems".into(),
        });
        for p in &active {
            raw.extend(p.inspect_project(files, &ctx));
        }
        was_cancelled = ctx.cancelled();
    }
    let by_path: HashMap<&str, &FileInput> = files.iter().map(|f| (f.path.as_str(), f)).collect();
    if !was_cancelled {
        for (k, s) in sources.iter().enumerate() {
            progress(RunProgress {
                done: files.len() + 1 + k,
                total,
                text: format!("Running {}", s.name()),
            });
            match s.problems(files, &ctx) {
                Ok(list) => raw.extend(
                    list.into_iter()
                        .filter_map(|e| external(e, &by_path, ctx.registry())),
                ),
                Err(e) => errors.push(format!("{}: {e}", s.name())),
            }
            if ctx.cancelled() {
                was_cancelled = true;
                break;
            }
        }
    }
    let mut info: BTreeMap<&str, FileInfo> = BTreeMap::new();
    let mut out: Vec<ReportProblem> = Vec::new();
    for p in raw {
        let Some(eff) = ctx.effective(&p.inspection, &p.path) else {
            continue;
        };
        let Some(file) = by_path.get(p.path.as_str()) else {
            continue;
        };
        let fi = info.entry(file.path.as_str()).or_insert_with(|| FileInfo {
            text: &file.text,
            lines: LineIndex::new(&file.text),
            suppressions: None,
            stamp: stamp(&file.text),
        });
        let sup = fi
            .suppressions
            .get_or_insert_with(|| Suppressions::parse(&file.path, fi.text));
        if sup.is_suppressed(&p.inspection, p.start) {
            continue;
        }
        let spec = ctx.registry().get(&p.inspection);
        let level = eff
            .severity
            .clone()
            .or_else(|| p.severity.as_deref().map(severity::from_editor))
            .or_else(|| spec.map(|s| s.severity.clone()))
            .unwrap_or_else(|| severity::WARNING.into());
        let (line, column) = fi.lines.position(p.start);
        let (end_line, end_column) = fi.lines.position(p.end.max(p.start));
        out.push(ReportProblem {
            inspection: p.inspection.clone(),
            inspection_name: spec.map_or_else(|| p.inspection.clone(), |s| s.name.clone()),
            group: spec.map_or_else(String::new, |s| s.group.clone()),
            path: p.path.clone(),
            line,
            column,
            end_line,
            end_column,
            start: p.start,
            end: p.end,
            message: p.message,
            severity_name: severity::display_name(&level, &profile.severities),
            editor: severity::editor_of(&level, &profile.severities),
            severity: level,
            snippet: fi
                .lines
                .line_text(line as usize)
                .trim()
                .chars()
                .take(200)
                .collect(),
            fixes: p.fixes,
            related: p.related,
            stamp: fi.stamp,
            unnecessary: p.unnecessary,
            deprecated: p.deprecated,
        });
        if options.max_problems > 0 && out.len() >= options.max_problems {
            break;
        }
    }
    out.sort_by(|a, b| {
        a.path
            .cmp(&b.path)
            .then(a.start.cmp(&b.start))
            .then(a.inspection.cmp(&b.inspection))
    });
    out.dedup_by(|a, b| {
        a.path == b.path
            && a.start == b.start
            && a.end == b.end
            && a.inspection == b.inspection
            && a.message == b.message
    });
    progress(RunProgress {
        done: total,
        total,
        text: String::new(),
    });
    Report {
        problems: out,
        files: files.len(),
        profile: profile.profile.name.clone(),
        cancelled: was_cancelled,
        errors,
        ..Report::default()
    }
}

/// An external problem as a [`Problem`] of the inspection its mapping names (or of an
/// inspection named after the tool when no mapping matches).
fn external(
    e: ExternalProblem,
    files: &HashMap<&str, &FileInput>,
    registry: &Registry,
) -> Option<Problem> {
    let file = files.get(e.path.as_str())?;
    let lines = LineIndex::new(&file.text);
    let start = lines.offset(e.line, e.column);
    let end = match (e.end_line, e.end_column) {
        (Some(l), Some(c)) => lines.offset(l, c),
        _ => lines.line_end(e.line as usize).max(start),
    };
    let id = registry
        .inspection_for(&e.source, e.code.as_deref())
        .map_or_else(|| e.source.clone(), str::to_string);
    let mut p = Problem::new(&id, &e.path, start, end, e.message);
    p.severity = e.severity;
    Some(p)
}

/// A cancel flag that is not set.
pub fn new_cancel_flag() -> CancelFlag {
    Arc::new(AtomicBool::new(false))
}

/// Whether a flag is set.
pub fn is_set(flag: &CancelFlag) -> bool {
    flag.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::{Profile, ProfileLevel, ToolOp};
    use crate::spec::{InspectionSpec, ToolMapping};
    use forge_scopes::{ScopeContext, ScopeSet};

    struct Todo;

    impl InspectionProvider for Todo {
        fn inspections(&self) -> Vec<String> {
            vec!["Todo".into()]
        }

        fn inspect_file(&self, f: &FileInput, _ctx: &RunContext) -> Vec<Problem> {
            f.text
                .match_indices("TODO")
                .map(|(i, _)| Problem::new("Todo", &f.path, i, i + 4, "A TODO"))
                .collect()
        }
    }

    struct Tool;

    impl ProblemSource for Tool {
        fn name(&self) -> String {
            "Tool".into()
        }

        fn problems(
            &self,
            _files: &[FileInput],
            _ctx: &RunContext,
        ) -> Result<Vec<ExternalProblem>, String> {
            Ok(vec![ExternalProblem {
                source: "phpstan".into(),
                code: Some("argument.type".into()),
                path: "a.php".into(),
                line: 1,
                column: 0,
                end_line: None,
                end_column: None,
                message: "Bad argument".into(),
                severity: Some("error".into()),
            }])
        }
    }

    fn registry() -> Registry {
        let todo = InspectionSpec {
            id: "Todo".into(),
            name: "TODO comment".into(),
            group: "General".into(),
            language: String::new(),
            severity: "WEAK WARNING".into(),
            enabled: true,
            description: String::new(),
            options: vec![],
            cleanup: false,
            external: false,
            batch: true,
        };
        let stan = InspectionSpec {
            id: "PhpStan".into(),
            name: "PHPStan".into(),
            group: "PHP/Quality tools".into(),
            external: true,
            ..todo.clone()
        };
        Registry::new(
            vec![todo, stan],
            vec![ToolMapping {
                source: "phpstan".into(),
                code: None,
                inspection: "PhpStan".into(),
            }],
        )
    }

    fn files() -> Vec<FileInput> {
        vec![
            FileInput {
                path: "a.php".into(),
                text: "<?php // TODO one\nfoo();\n/** @noinspection Todo */\n$a = 1; // TODO two\n"
                    .into(),
            },
            FileInput {
                path: "b.js".into(),
                text: "// TODO three\n".into(),
            },
        ]
    }

    fn resolved(profile: Profile) -> ResolvedProfile {
        ResolvedProfile::new(
            registry(),
            profile,
            vec![],
            &ScopeSet::default(),
            &ScopeContext::default(),
        )
    }

    #[test]
    fn runs_providers_and_sources_with_suppressions() {
        let profile = resolved(Profile::new("Default", ProfileLevel::Ide));
        let mut steps = Vec::new();
        let report = run(
            &files(),
            &profile,
            &[&Todo, &DuplicatesProvider],
            &[&Tool],
            &RunOptions::default(),
            &mut |p| steps.push(p),
            &|| false,
        );
        let ids: Vec<(&str, u32)> = report
            .problems
            .iter()
            .map(|p| (p.inspection.as_str(), p.line))
            .collect();
        assert_eq!(
            ids,
            [("Todo", 0), ("PhpStan", 1), ("Todo", 0)],
            "{report:#?}"
        );
        let stan = &report.problems[1];
        assert_eq!(stan.severity, "ERROR", "the tool's level");
        assert_eq!(stan.snippet, "foo();");
        assert_eq!(report.problems[0].severity_name, "Weak Warning");
        assert!(steps.last().is_some_and(|s| s.done == s.total));
        assert!(!report.cancelled);
    }

    #[test]
    fn profile_and_only_filter() {
        let r = registry();
        let mut p = Profile::new("Default", ProfileLevel::Ide);
        p.edit(
            &r,
            &ToolOp::SetEnabled {
                ids: vec!["Todo".into()],
                enabled: false,
            },
        );
        let report = run(
            &files(),
            &resolved(p),
            &[&Todo],
            &[&Tool],
            &RunOptions::default(),
            &mut |_| {},
            &|| false,
        );
        assert_eq!(report.problems.len(), 1);
        let only = RunOptions {
            only: Some(vec!["Todo".into()]),
            max_problems: 0,
        };
        let report = run(
            &files(),
            &resolved(Profile::new("D", ProfileLevel::Ide)),
            &[&Todo],
            &[&Tool],
            &only,
            &mut |_| {},
            &|| false,
        );
        assert!(report.problems.iter().all(|p| p.inspection == "Todo"));
        let report = run(
            &files(),
            &resolved(Profile::new("D", ProfileLevel::Ide)),
            &[&Todo],
            &[],
            &RunOptions::default(),
            &mut |_| {},
            &|| true,
        );
        assert!(report.cancelled && report.problems.is_empty());
    }
}
