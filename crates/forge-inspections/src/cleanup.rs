//! Applying quick fixes: one fix from Inspection Results, or every safe fix of a run (Code
//! Cleanup) with a preview of each changed file.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::problem::{stamp, FileEdit};
use crate::registry::Registry;
use crate::report::ReportProblem;

/// Apply edits (bytes) to `text`; overlapping or out-of-range edits are an error.
pub fn apply_edits(text: &str, edits: &[FileEdit]) -> Result<String, String> {
    let mut sorted: Vec<&FileEdit> = edits.iter().collect();
    sorted.sort_by_key(|e| (e.start, e.end));
    for pair in sorted.windows(2) {
        if pair[1].start < pair[0].end {
            return Err("the changes overlap".into());
        }
    }
    let mut out = text.to_string();
    for e in sorted.into_iter().rev() {
        if e.start > e.end
            || e.end > out.len()
            || !out.is_char_boundary(e.start)
            || !out.is_char_boundary(e.end)
        {
            return Err(format!("the change at {} does not fit the file", e.start));
        }
        out.replace_range(e.start..e.end, &e.text);
    }
    Ok(out)
}

/// A file Code Cleanup changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupFile {
    pub path: String,
    /// The fingerprint of the text the fixes were computed for.
    pub stamp: u64,
    pub before: String,
    pub after: String,
    /// The fixes applied (`Remove unused assignment` x 2).
    pub fixes: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupPlan {
    pub files: Vec<CleanupFile>,
    /// Fixes left out because they overlap others.
    pub skipped: usize,
}

/// The safe fixes of `problems` applied to the files' texts (`texts`: path -> text). Only
/// inspections marked `cleanup` contribute; overlapping fixes are skipped.
pub fn plan(
    problems: &[ReportProblem],
    texts: &BTreeMap<String, String>,
    registry: &Registry,
) -> CleanupPlan {
    let mut per_file: BTreeMap<String, (Vec<FileEdit>, Vec<String>)> = BTreeMap::new();
    let mut skipped = 0;
    for p in problems {
        if !registry.get(&p.inspection).is_some_and(|s| s.cleanup) {
            continue;
        }
        let Some(fix) = p.fixes.iter().find(|f| f.safe) else {
            continue;
        };
        let fits = fix.edits.iter().all(|e| {
            let current = per_file
                .get(&e.path)
                .map(|(v, _)| v.as_slice())
                .unwrap_or(&[]);
            texts.contains_key(&e.path)
                && !current
                    .iter()
                    .any(|o| e.start < o.end.max(o.start + 1) && o.start < e.end.max(e.start + 1))
        });
        if !fits {
            skipped += 1;
            continue;
        }
        for e in &fix.edits {
            per_file
                .entry(e.path.clone())
                .or_default()
                .0
                .push(e.clone());
        }
        if let Some(e) = fix.edits.first() {
            per_file
                .entry(e.path.clone())
                .or_default()
                .1
                .push(fix.title.clone());
        }
    }
    let mut files = Vec::new();
    for (path, (edits, fixes)) in per_file {
        let before = &texts[&path];
        match apply_edits(before, &edits) {
            Ok(after) if &after != before => files.push(CleanupFile {
                stamp: stamp(before),
                path,
                before: before.clone(),
                after,
                fixes,
            }),
            Ok(_) => {}
            Err(_) => skipped += fixes.len(),
        }
    }
    CleanupPlan { files, skipped }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::problem::Fix;
    use crate::severity::EditorSeverity;
    use crate::spec::InspectionSpec;

    fn edit(start: usize, end: usize, text: &str) -> FileEdit {
        FileEdit {
            path: "a.php".into(),
            start,
            end,
            text: text.into(),
        }
    }

    #[test]
    fn edits_apply_in_order_and_reject_overlaps() {
        assert_eq!(
            apply_edits("abcdef", &[edit(4, 5, "E"), edit(0, 1, "A")]).unwrap(),
            "AbcdEf"
        );
        assert!(apply_edits("abc", &[edit(0, 2, ""), edit(1, 3, "")]).is_err());
        assert!(apply_edits("abc", &[edit(2, 9, "")]).is_err());
        assert!(apply_edits("ä", &[edit(1, 1, "x")]).is_err());
    }

    #[test]
    fn plans_safe_fixes_of_cleanup_inspections() {
        let spec = |id: &str, cleanup: bool| InspectionSpec {
            id: id.into(),
            name: id.into(),
            group: "PHP".into(),
            language: "php".into(),
            severity: "WARNING".into(),
            enabled: true,
            description: String::new(),
            options: vec![],
            cleanup,
            external: false,
            batch: true,
        };
        let registry = Registry::new(vec![spec("Clean", true), spec("Manual", false)], vec![]);
        let problem = |id: &str, e: FileEdit, safe: bool| ReportProblem {
            inspection: id.into(),
            inspection_name: id.into(),
            group: "PHP".into(),
            path: e.path.clone(),
            line: 0,
            column: 0,
            end_line: 0,
            end_column: 0,
            start: e.start,
            end: e.end,
            message: String::new(),
            severity: "WARNING".into(),
            severity_name: "Warning".into(),
            editor: EditorSeverity::Warning,
            snippet: String::new(),
            fixes: vec![Fix {
                title: format!("Fix {id}"),
                edits: vec![e],
                safe,
            }],
            related: vec![],
            stamp: 0,
            unnecessary: false,
            deprecated: false,
        };
        let texts: BTreeMap<String, String> =
            [("a.php".to_string(), "0123456789".to_string())].into();
        let problems = vec![
            problem("Clean", edit(0, 2, ""), true),
            problem("Clean", edit(1, 3, "x"), true),
            problem("Clean", edit(5, 6, "five"), true),
            problem("Clean", edit(8, 9, ""), false),
            problem("Manual", edit(9, 10, ""), true),
        ];
        let plan = plan(&problems, &texts, &registry);
        assert_eq!(plan.skipped, 1);
        assert_eq!(plan.files.len(), 1);
        assert_eq!(plan.files[0].after, "234five6789");
        assert_eq!(plan.files[0].fixes.len(), 2);
        assert_eq!(plan.files[0].stamp, stamp("0123456789"));
    }
}
