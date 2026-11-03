//! The diff context a model gets: which files go in, in which order and how much of each.
//!
//! Files are ordered by [`Priority`] (source code first, then tests, configuration and
//! documentation; lock files, minified and generated files are only named) and, within a
//! class, by the size of the change. Every file is cut to [`Budget::max_file_chars`]; files
//! fill chunks of [`Budget::max_chars`] until [`Budget::max_chunks`] are full, the rest is
//! listed as omitted. Files excluded by the AI policy never appear in any text.

use forge_ai_editor::{language_of, Feature, Policy};
use forge_forges::{ChangeKind, ChangedFile};
use forge_vcs::{Hunk, LineKind};
use serde::{Deserialize, Serialize};

/// Size limits of the context (characters, roughly four per token).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    /// Characters of diff per request.
    pub max_chars: usize,
    /// Characters of one file's diff before it is cut.
    pub max_file_chars: usize,
    /// Requests per run (AI Review splits big changes; the summary uses one).
    pub max_chunks: usize,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            max_chars: 48_000,
            max_file_chars: 12_000,
            max_chunks: 3,
        }
    }
}

impl Budget {
    /// One request only (Summarize PR).
    pub fn single(self) -> Self {
        Self {
            max_chunks: 1,
            ..self
        }
    }
}

/// How interesting a file is for a reviewer; lower sorts first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Priority {
    Source,
    Test,
    Config,
    Docs,
    /// Lock files, minified, vendored and generated files: named, never sent.
    Generated,
}

const LOCK_FILES: [&str; 12] = [
    "composer.lock",
    "package-lock.json",
    "pnpm-lock.yaml",
    "yarn.lock",
    "bun.lockb",
    "cargo.lock",
    "gemfile.lock",
    "poetry.lock",
    "pipfile.lock",
    "uv.lock",
    "go.sum",
    "packages.lock.json",
];

const GENERATED_DIRS: [&str; 7] = [
    "vendor/",
    "node_modules/",
    "dist/",
    "build/",
    "__snapshots__/",
    "generated/",
    ".next/",
];

const DOC_EXTS: [&str; 5] = ["md", "markdown", "rst", "txt", "adoc"];
const CONFIG_EXTS: [&str; 9] = [
    "json", "yaml", "yml", "toml", "ini", "xml", "neon", "env", "lock",
];

/// Classify a path by its name and folders.
pub fn classify(path: &str) -> Priority {
    let lower = path.to_ascii_lowercase();
    let name = lower.rsplit('/').next().unwrap_or(&lower);
    let in_dir = |d: &str| lower.starts_with(d) || lower.contains(&format!("/{d}"));
    if LOCK_FILES.contains(&name)
        || name.ends_with(".min.js")
        || name.ends_with(".min.css")
        || name.ends_with(".map")
        || name.ends_with(".snap")
        || name.ends_with(".g.dart")
        || name.ends_with(".pb.go")
        || name.ends_with(".designer.cs")
        || GENERATED_DIRS.iter().any(|d| in_dir(d))
    {
        return Priority::Generated;
    }
    let ext = name.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    let test = ["tests/", "test/", "spec/", "__tests__/"]
        .iter()
        .any(|d| in_dir(d))
        || name.contains(".test.")
        || name.contains(".spec.")
        || name.contains("_test.")
        || name.starts_with("test_")
        || name.ends_with("test.php")
        || name.ends_with("test.java")
        || name.ends_with("test.kt")
        || name.ends_with("tests.cs");
    if test {
        Priority::Test
    } else if DOC_EXTS.contains(&ext) || in_dir("docs/") {
        Priority::Docs
    } else if CONFIG_EXTS.contains(&ext) || name.starts_with('.') || name == "dockerfile" {
        Priority::Config
    } else {
        Priority::Source
    }
}

/// What happened to a file in the context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Inclusion {
    /// The whole diff was sent.
    Full,
    /// Cut to the per-file limit.
    Partial,
    /// Named only: generated, a lock file, deleted, binary or over the budget.
    Omitted,
    /// Excluded by `.aiignore`: not even named.
    Excluded,
}

/// One changed file as the context saw it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextFile {
    pub path: String,
    pub status: ChangeKind,
    pub additions: u32,
    pub deletions: u32,
    pub priority: Priority,
    pub inclusion: Inclusion,
    /// Index of the chunk that holds the diff (`None` when not sent).
    pub chunk: Option<usize>,
}

/// The text of one request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffChunk {
    pub text: String,
    /// Paths whose diff is in this chunk.
    pub paths: Vec<String>,
}

/// All chunks plus what happened to every file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextPlan {
    pub chunks: Vec<DiffChunk>,
    /// Every changed file, in priority order.
    pub files: Vec<ContextFile>,
}

impl ContextPlan {
    /// Paths excluded by the policy.
    pub fn excluded(&self) -> Vec<String> {
        self.paths(Inclusion::Excluded)
    }

    /// Paths that were only named (generated, over the budget, …).
    pub fn omitted(&self) -> Vec<String> {
        self.paths(Inclusion::Omitted)
    }

    fn paths(&self, which: Inclusion) -> Vec<String> {
        self.files
            .iter()
            .filter(|f| f.inclusion == which)
            .map(|f| f.path.clone())
            .collect()
    }

    /// A short list of the files the model may know about (not the excluded ones).
    pub fn overview(&self) -> String {
        let mut out = String::new();
        for f in self
            .files
            .iter()
            .filter(|f| f.inclusion != Inclusion::Excluded)
        {
            let note = match f.inclusion {
                Inclusion::Full => "",
                Inclusion::Partial => ", diff cut",
                _ => ", diff not shown",
            };
            out.push_str(&format!(
                "- {} ({}, +{} -{}{note})\n",
                f.path,
                status_word(f.status),
                f.additions,
                f.deletions
            ));
        }
        out
    }
}

/// Whether the policy lets this file's code leave the machine (new and old path).
pub fn allowed(policy: &Policy, file: &ChangedFile) -> bool {
    let ok = |p: &str| {
        policy
            .check(p, language_of(p, None).id(), Feature::Action)
            .is_allowed()
    };
    ok(&file.path) && file.old_path.as_deref().is_none_or(ok)
}

fn status_word(s: ChangeKind) -> &'static str {
    match s {
        ChangeKind::Added => "added",
        ChangeKind::Modified => "modified",
        ChangeKind::Deleted => "deleted",
        ChangeKind::Renamed => "renamed",
    }
}

const MAX_LINE_CHARS: usize = 400;

/// One hunk with the new file's line numbers in front of added and context lines.
pub fn render_hunk(h: &Hunk) -> String {
    let mut out = format!(
        "@@ -{},{} +{},{} @@ {}\n",
        h.old_start, h.old_lines, h.new_start, h.new_lines, h.header
    )
    .replace(" \n", "\n");
    let mut n = h.new_start;
    for l in &h.lines {
        let text: String = if l.text.chars().count() > MAX_LINE_CHARS {
            l.text
                .chars()
                .take(MAX_LINE_CHARS)
                .chain("…".chars())
                .collect()
        } else {
            l.text.clone()
        };
        match l.kind {
            LineKind::Added => {
                out.push_str(&format!("{n:>5} +{text}\n"));
                n += 1;
            }
            LineKind::Context => {
                out.push_str(&format!("{n:>5}  {text}\n"));
                n += 1;
            }
            LineKind::Removed => out.push_str(&format!("{:>5} -{text}\n", "")),
        }
    }
    out
}

/// A file's diff cut to `max_chars` at hunk (or line) boundaries; `true` when cut.
pub fn render_file(file: &ChangedFile, max_chars: usize) -> (String, bool) {
    let mut out = format!(
        "### {}{} ({}, +{} -{})\n",
        file.old_path
            .as_deref()
            .map(|o| format!("{o} → "))
            .unwrap_or_default(),
        file.path,
        status_word(file.status),
        file.additions,
        file.deletions
    );
    for (i, h) in file.hunks.iter().enumerate() {
        let text = render_hunk(h);
        if out.len() + text.len() <= max_chars {
            out.push_str(&text);
            continue;
        }
        // Keep the beginning of the first hunk that does not fit, then stop.
        let mut used = out.len();
        for line in text.lines() {
            if used + line.len() + 1 > max_chars {
                break;
            }
            out.push_str(line);
            out.push('\n');
            used += line.len() + 1;
        }
        let rest = file.hunks.len() - i;
        out.push_str(&format!(
            "… diff cut here ({rest} hunk{} not fully shown)\n",
            if rest == 1 { "" } else { "s" }
        ));
        return (out, true);
    }
    (out, false)
}

/// Plan the context for `files` under `policy` and `budget`.
pub fn plan(files: &[ChangedFile], policy: &Policy, budget: Budget) -> ContextPlan {
    let mut order: Vec<&ChangedFile> = files.iter().collect();
    order.sort_by_key(|f| {
        (
            classify(&f.path),
            std::cmp::Reverse(f.additions + f.deletions),
            f.path.clone(),
        )
    });
    let mut chunks: Vec<DiffChunk> = Vec::new();
    let mut out = Vec::new();
    for f in order {
        let priority = classify(&f.path);
        let mut cf = ContextFile {
            path: f.path.clone(),
            status: f.status,
            additions: f.additions,
            deletions: f.deletions,
            priority,
            inclusion: Inclusion::Omitted,
            chunk: None,
        };
        if !allowed(policy, f) {
            cf.inclusion = Inclusion::Excluded;
            out.push(cf);
            continue;
        }
        let sendable = priority != Priority::Generated
            && f.status != ChangeKind::Deleted
            && !f.hunks.is_empty();
        if sendable {
            let cap = budget.max_file_chars.min(budget.max_chars);
            let (text, cut) = render_file(f, cap);
            let fits_last = chunks
                .last()
                .is_some_and(|c| c.text.len() + text.len() <= budget.max_chars);
            if !fits_last && chunks.len() < budget.max_chunks {
                chunks.push(DiffChunk {
                    text: String::new(),
                    paths: Vec::new(),
                });
            }
            if let Some(c) = chunks
                .last_mut()
                .filter(|c| c.text.len() + text.len() <= budget.max_chars)
            {
                c.text.push_str(&text);
                c.text.push('\n');
                c.paths.push(f.path.clone());
                cf.inclusion = if cut {
                    Inclusion::Partial
                } else {
                    Inclusion::Full
                };
                cf.chunk = Some(chunks.len() - 1);
            }
        }
        out.push(cf);
    }
    chunks.retain(|c| !c.paths.is_empty());
    ContextPlan { chunks, files: out }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use forge_ai_editor::AiIgnore;
    use forge_vcs::DiffLine;

    pub fn hunk(new_start: u32, lines: &[(&str, &str)]) -> Hunk {
        let lines: Vec<DiffLine> = lines
            .iter()
            .map(|(k, t)| DiffLine {
                kind: match *k {
                    "+" => LineKind::Added,
                    "-" => LineKind::Removed,
                    _ => LineKind::Context,
                },
                text: t.to_string(),
            })
            .collect();
        Hunk {
            old_start: new_start,
            old_lines: lines.iter().filter(|l| l.kind != LineKind::Added).count() as u32,
            new_start,
            new_lines: lines.iter().filter(|l| l.kind != LineKind::Removed).count() as u32,
            header: String::new(),
            lines,
        }
    }

    pub fn file(path: &str, hunks: Vec<Hunk>) -> ChangedFile {
        let (additions, deletions) = forge_forges::host::count_changes(&hunks);
        ChangedFile {
            path: path.into(),
            old_path: None,
            status: ChangeKind::Modified,
            additions,
            deletions,
            hunks,
        }
    }

    fn big(path: &str, lines: usize) -> ChangedFile {
        let text: Vec<String> = (0..lines)
            .map(|i| format!("line {i} {}", "x".repeat(60)))
            .collect();
        let pairs: Vec<(&str, &str)> = text.iter().map(|t| ("+", t.as_str())).collect();
        file(path, vec![hunk(1, &pairs)])
    }

    #[test]
    fn classifies_paths() {
        assert_eq!(classify("app/Models/User.php"), Priority::Source);
        assert_eq!(classify("tests/Unit/UserTest.php"), Priority::Test);
        assert_eq!(classify("src/cart.spec.ts"), Priority::Test);
        assert_eq!(classify("config/app.yaml"), Priority::Config);
        assert_eq!(classify("README.md"), Priority::Docs);
        assert_eq!(classify("composer.lock"), Priority::Generated);
        assert_eq!(classify("public/build/app.min.js"), Priority::Generated);
        assert_eq!(classify("web/vendor/x/y.php"), Priority::Generated);
    }

    #[test]
    fn numbers_new_lines() {
        let h = hunk(10, &[(" ", "a"), ("-", "b"), ("+", "c"), (" ", "d")]);
        assert_eq!(
            render_hunk(&h),
            "@@ -10,3 +10,3 @@\n   10  a\n      -b\n   11 +c\n   12  d\n"
        );
    }

    #[test]
    fn orders_by_priority_and_skips_generated_and_excluded() {
        let files = vec![
            file("README.md", vec![hunk(1, &[("+", "docs")])]),
            file("composer.lock", vec![hunk(1, &[("+", "{}")])]),
            file("tests/AppTest.php", vec![hunk(1, &[("+", "test")])]),
            file("app/A.php", vec![hunk(1, &[("+", "a")])]),
            file("app/B.php", vec![hunk(1, &[("+", "b"), ("+", "b2")])]),
            file("config/secrets.php", vec![hunk(1, &[("+", "API_KEY=1")])]),
        ];
        let policy = Policy {
            ignore: AiIgnore::parse("config/secrets.php"),
            ..Policy::default()
        };
        let p = plan(&files, &policy, Budget::default());
        let order: Vec<&str> = p.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(
            order,
            [
                "app/B.php",
                "app/A.php",
                "config/secrets.php",
                "tests/AppTest.php",
                "README.md",
                "composer.lock"
            ]
        );
        assert_eq!(p.chunks.len(), 1);
        let text = &p.chunks[0].text;
        assert!(text.find("app/B.php").unwrap() < text.find("tests/AppTest.php").unwrap());
        assert!(!text.contains("API_KEY") && !text.contains("secrets.php"));
        assert!(!p.overview().contains("secrets.php"));
        assert_eq!(p.excluded(), ["config/secrets.php"]);
        assert_eq!(p.omitted(), ["composer.lock"]);
        assert!(p
            .overview()
            .contains("composer.lock (modified, +1 -0, diff not shown)"));
    }

    #[test]
    fn excludes_renames_from_ignored_paths() {
        let mut f = file("app/Config.php", vec![hunk(1, &[("+", "x")])]);
        f.old_path = Some(".env.php".into());
        let policy = Policy {
            ignore: AiIgnore::parse(".env*"),
            ..Policy::default()
        };
        assert!(!allowed(&policy, &f));
    }

    #[test]
    fn cuts_long_files_and_splits_into_chunks() {
        let files = vec![
            big("app/A.php", 300),
            big("app/B.php", 30),
            big("app/C.php", 30),
        ];
        let budget = Budget {
            max_chars: 4_000,
            max_file_chars: 3_000,
            max_chunks: 2,
        };
        let p = plan(&files, &Policy::default(), budget);
        assert!(p.chunks.iter().all(|c| c.text.len() <= budget.max_chars));
        let a = &p.files[0];
        assert_eq!(
            (a.path.as_str(), a.inclusion, a.chunk),
            ("app/A.php", Inclusion::Partial, Some(0))
        );
        assert!(p.chunks[0].text.contains("diff cut here"));
        // B and C need a chunk each; only two chunks are allowed.
        let incl: Vec<Inclusion> = p.files.iter().map(|f| f.inclusion).collect();
        assert_eq!(
            incl,
            [Inclusion::Partial, Inclusion::Full, Inclusion::Omitted]
        );
        assert_eq!(p.chunks.len(), 2);
        let single = plan(&files, &Policy::default(), budget.single());
        assert_eq!(single.chunks.len(), 1);
    }

    #[test]
    fn nothing_when_ai_is_off_for_the_project() {
        let files = vec![file("app/A.php", vec![hunk(1, &[("+", "a")])])];
        let policy = Policy {
            project_enabled: false,
            ..Policy::default()
        };
        let p = plan(&files, &policy, Budget::default());
        assert!(p.chunks.is_empty());
        assert_eq!(p.excluded(), ["app/A.php"]);
    }
}
