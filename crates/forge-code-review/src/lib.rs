//! Code review of pull/merge requests inside Forge, as pure logic on top of
//! [`forge_forges`] (the host API) and [`forge_ai`] (the providers):
//!
//! - [`context`]: the size-bounded diff context sent to a model: files ordered by
//!   importance (source, tests, config, docs; lock files and generated code only by name),
//!   long files and hunks cut, big requests split into chunks, `.aiignore` files left out.
//! - [`prompt`]: the prompts of "Summarize PR", "AI Review" and "Explain this change".
//! - [`parse`]: the model's answers turned into a [`Summary`] and raw findings.
//! - [`anchor`]: findings mapped to lines of the diff, so they can become inline comments.
//! - [`batch`]: pending review comments (drafts) turned into one [`forge_forges::NewReview`].
//! - [`store`]: per-request state in the cache folder: viewed files (by a digest of their
//!   diff, so a new push marks them unviewed), drafts, and AI results cached per head SHA.
//! - [`run`]: the three AI calls through a [`forge_ai::Provider`].
//!
//! Nothing is posted to a code host without an explicit user action; nothing leaves the
//! machine when AI is off for the project or a file is excluded by `.aiignore`.

pub mod anchor;
pub mod batch;
pub mod context;
pub mod parse;
pub mod prompt;
pub mod run;
pub mod store;

pub use anchor::{anchor, Finding};
pub use batch::{build_review, draft_from_finding, finding_body, Draft};
pub use context::{plan, Budget, ContextFile, ContextPlan, DiffChunk, Inclusion, Priority};
pub use parse::{parse_findings, parse_summary, FileSummary, RawFinding, Risk, Severity, Summary};
pub use prompt::{Prompt, RequestMeta};
pub use run::{explain, review, summarize, AiRun, Explanation, ReviewError, ReviewOutcome};
pub use store::{digest, CachedFindings, CachedSummary, ReviewRecord, ReviewStore};
