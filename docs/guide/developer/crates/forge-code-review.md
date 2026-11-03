---
title: forge-code-review
description: "Forge IDE code review: viewed files, pending review comments and AI summaries and reviews of pull/merge requests."
order: 22
tags: [crate, rust, ai, code review]
---

# forge-code-review

`crates/forge-code-review` — review mode of pull/merge requests: viewed files, pending review
comments submitted as one review, and the AI features "Summarize PR", "AI Review" and
"Explain this change". Logic only; the host API is [`forge-forges`](forge-forges.md), the
models are [`forge-ai`](forge-ai.md) providers and the `.aiignore` policy comes from
[`forge-ai-editor`](forge-ai-editor.md).

## Purpose

- **Diff context** (`context.rs`): `plan` orders the changed files by `Priority` (source,
  tests, configuration, documentation; lock files, minified, vendored and generated files are
  only named), larger changes first, renders each hunk with new-file line numbers in front of
  added and context lines, cuts files to `Budget::max_file_chars` at hunk or line boundaries
  and fills chunks of `Budget::max_chars` (at most `max_chunks`; the rest is "omitted").
  Files the policy refuses (new or old path in `.aiignore`, AI off for the project) are
  `Excluded`: they appear in no text, not even by name.
- **Prompts** (`prompt.rs`): summary (JSON: overview, risks, files, testPlan), review (JSON
  findings with path, line, endLine, severity, title, explanation, suggestion; one request
  per chunk) and explain (one hunk with the start of its file's diff).
- **Parsing** (`parse.rs`): the first JSON value in the answer (fences and prose around it are
  ignored), field synonyms, severities mapped from the words models use (`critical`, `nit`, …),
  at most 40 findings; prose without JSON becomes the summary's overview.
- **Anchoring** (`anchor.rs`): findings matched to changed files (exact, old path or an
  unambiguous path suffix) and to lines shown in the diff; a line up to three lines away snaps
  to the nearest shown line, a range becomes a multi-line comment when both ends are in one
  hunk, anything else stays unplaced (it goes into the review body). Ids are stable hashes,
  so a dismissal survives a re-run.
- **Batching** (`batch.rs`): `Draft`s become one `forge_forges::NewReview`; drafts whose line
  left the diff and general drafts are added to the review body; `finding_body` writes a
  finding as a comment with a `suggestion` block on GitHub and GitLab.
- **Store** (`store.rs`): `ReviewRecord` per request in
  `<cache dir>/forge/code-review/<kind>-<server>-<repo>/<number>.json`: viewed files with a
  digest of their diff (a new push makes them unviewed), drafts, and the summary and findings
  of one head SHA.
- **Runs** (`run.rs`): `summarize`, `review` and `explain` check the policy, call the provider
  and return what was excluded and omitted. Nothing is posted to the host here.

## Main public items

```rust
pub use anchor::{anchor, Finding};
pub use batch::{build_review, draft_from_finding, finding_body, Draft};
pub use context::{plan, Budget, ContextFile, ContextPlan, DiffChunk, Inclusion, Priority};
pub use parse::{parse_findings, parse_summary, FileSummary, RawFinding, Risk, Severity, Summary};
pub use prompt::{Prompt, RequestMeta};
pub use run::{explain, review, summarize, AiRun, Explanation, ReviewError, ReviewOutcome};
pub use store::{digest, CachedFindings, CachedSummary, ReviewRecord, ReviewStore};
```

## Used by

- IPC: `review_session`, `review_set_viewed`, `review_save_draft`, `review_delete_draft`,
  `review_submit`, `review_summarize`, `review_ai`, `review_finding`, `review_explain`
  (`app/src-tauri/src/commands/code_review.rs`)
- UI: `app/ui/src/features/forges` (`MergeRequestTab.tsx`, `ReviewDiff.tsx`, `ReviewPanel.tsx`,
  `ReviewCards.tsx`), client `app/ui/src/ipc/codeReview.ts`
- Browser mode: not built to WebAssembly (it depends on the HTTP crates); `app/ui/src/mock/codeReviewMock.ts`
  serves the demo pull request #41 with canned AI answers and applies the same `ai.enabled`
  and `.aiignore` rules through the AI mocks.

## Dependencies

- Workspace crates: [`forge-ai`](forge-ai.md), [`forge-ai-editor`](forge-ai-editor.md), [`forge-forges`](forge-forges.md), [`forge-vcs`](forge-vcs.md)
- External: `dirs`, `serde`, `serde_json`, `thiserror`

## Tests

Unit tests with a fake provider (no network): prioritisation and chunking, `.aiignore`
exclusion from prompts, prompt contents, lenient parsing, anchoring and snapping, review
batching and the cache store. The hosts' `submit_review` is tested against a local mock
server in `crates/forge-forges/tests/`.

```sh
cargo test -p forge-code-review
```
