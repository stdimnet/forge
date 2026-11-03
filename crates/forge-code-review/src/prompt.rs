//! Prompts of the review features. The diff comes from [`crate::context`], so `.aiignore`
//! files are never in it; the request's title and description are sent as context.

use forge_forges::ChangedFile;

use crate::context::{render_file, render_hunk, ContextPlan, DiffChunk};

/// The pull/merge request around the diff.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RequestMeta {
    /// "pull request" or "merge request".
    pub request_name: String,
    pub title: String,
    pub description: String,
    pub source_branch: String,
    pub target_branch: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    pub system: String,
    pub user: String,
    pub max_tokens: u32,
}

const MAX_DESCRIPTION: usize = 4_000;

const DIFF_FORMAT: &str = "The diff lists each file under a `###` heading. In front of every \
added (`+`) and unchanged line is its line number in the NEW version of the file; removed \
(`-`) lines have no number.";

fn header(meta: &RequestMeta) -> String {
    let name = if meta.request_name.is_empty() {
        "pull request"
    } else {
        &meta.request_name
    };
    let mut description: String = meta
        .description
        .trim()
        .chars()
        .take(MAX_DESCRIPTION)
        .collect();
    if description.is_empty() {
        description = "(none)".into();
    }
    format!(
        "{}: \"{}\" ({} → {})\n\nDescription:\n{description}\n",
        capitalize(name),
        meta.title.trim(),
        meta.source_branch,
        meta.target_branch,
    )
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}

/// "Summarize PR": overview, risk areas, per-file summary and a test plan as JSON.
pub fn summary_prompt(meta: &RequestMeta, plan: &ContextPlan) -> Prompt {
    let diff = plan.chunks.first().map(|c| c.text.as_str()).unwrap_or("");
    Prompt {
        system: format!(
            "You are a senior software engineer summarizing a {} for its reviewers. Be concise \
and concrete; mention class, function and file names. {DIFF_FORMAT} Answer with one JSON \
object and nothing else:\n\
{{\"overview\": \"2-5 sentences of Markdown: what changes and why\",\n \
\"risks\": [{{\"title\": \"short\", \"detail\": \"why it is risky\", \"severity\": \"high|medium|low\", \"files\": [\"path\"]}}],\n \
\"files\": [{{\"path\": \"path\", \"summary\": \"one sentence\"}}],\n \
\"testPlan\": [\"one manual or automated check per item\"]}}\n\
List only real risks (an empty list is fine). Summarize every file whose diff is shown.",
            if meta.request_name.is_empty() {
                "pull request"
            } else {
                &meta.request_name
            }
        ),
        user: format!(
            "{}\nChanged files:\n{}\nDiff:\n{diff}",
            header(meta),
            plan.overview()
        ),
        max_tokens: 2048,
    }
}

/// "AI Review": inline findings for one chunk of the diff, as JSON.
pub fn review_prompt(
    meta: &RequestMeta,
    plan: &ContextPlan,
    chunk: &DiffChunk,
    index: usize,
) -> Prompt {
    let part = if plan.chunks.len() > 1 {
        format!(" (part {} of {})", index + 1, plan.chunks.len())
    } else {
        String::new()
    };
    Prompt {
        system: format!(
            "You are a careful code reviewer. Report bugs, security problems, missing error \
handling, race conditions, wrong edge cases and clearly confusing code in the CHANGED lines. \
Do not report style preferences, formatting or praise. {DIFF_FORMAT} Answer with one JSON \
object and nothing else:\n\
{{\"findings\": [{{\"path\": \"file path as in the heading\", \"line\": <new line number>, \
\"endLine\": <optional last line of a range>, \"severity\": \"error|warning|info\", \
\"title\": \"short\", \"explanation\": \"what is wrong and why, Markdown\", \
\"suggestion\": \"optional replacement code for exactly those lines, no fences\"}}]}}\n\
Use only line numbers shown in the diff. Return {{\"findings\": []}} when there is nothing \
worth a comment. At most 15 findings."
        ),
        user: format!(
            "{}\nChanged files:\n{}\nDiff{part}:\n{}",
            header(meta),
            plan.overview(),
            chunk.text
        ),
        max_tokens: 4096,
    }
}

/// "Explain this change": one hunk with the start of its file's diff for context.
pub fn explain_prompt(meta: &RequestMeta, file: &ChangedFile, hunk: usize) -> Option<Prompt> {
    let h = file.hunks.get(hunk)?;
    let (file_diff, _) = render_file(file, 6_000);
    Some(Prompt {
        system: format!(
            "You explain code changes to a reviewer. Say what the selected change does, why it \
was probably made and what could break. Use short Markdown paragraphs or bullets; no \
headings. {DIFF_FORMAT}"
        ),
        user: format!(
            "{}\nFile diff (for context):\n{file_diff}\nExplain this change in {}:\n{}",
            header(meta),
            file.path,
            render_hunk(h)
        ),
        max_tokens: 1024,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::tests::{file, hunk};
    use crate::context::{plan, Budget};
    use forge_ai_editor::Policy;

    fn meta() -> RequestMeta {
        RequestMeta {
            request_name: "merge request".into(),
            title: "Add roles".into(),
            description: "x".repeat(10_000),
            source_branch: "feature/roles".into(),
            target_branch: "main".into(),
        }
    }

    #[test]
    fn prompts_carry_meta_overview_and_diff() {
        let files = vec![file(
            "app/User.php",
            vec![hunk(3, &[("+", "public $role;")])],
        )];
        let p = plan(&files, &Policy::default(), Budget::default());
        let s = summary_prompt(&meta(), &p);
        assert!(s
            .user
            .starts_with("Merge request: \"Add roles\" (feature/roles → main)"));
        assert!(s.user.contains("- app/User.php (modified, +1 -0)"));
        assert!(s.user.contains("    3 +public $role;"));
        assert!(s.user.len() < 5_000, "description is cut");
        assert!(s.system.contains("\"testPlan\""));
        let r = review_prompt(&meta(), &p, &p.chunks[0], 0);
        assert!(r.system.contains("\"findings\""));
        assert!(!r.user.contains("part 1"));
        let e = explain_prompt(&meta(), &files[0], 0).unwrap();
        assert!(e.user.contains("Explain this change in app/User.php"));
        assert!(explain_prompt(&meta(), &files[0], 3).is_none());
    }
}
