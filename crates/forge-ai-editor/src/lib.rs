//! AI in the editor, as pure logic (no network, also built to WebAssembly for browser mode):
//!
//! - [`inline`]: inline (ghost text) completion: the context around the caret (prefix,
//!   suffix, imports, other open files), the fill-in-the-middle or chat prompt, and the
//!   post-processing of the model's answer (fences, echoed code, suffix overlap, the
//!   single/multi-line policy).
//! - [`actions`]: "Explain with AI", "Fix with AI", "Generate Tests with AI" and "Generate
//!   Documentation with AI": prompts and the proposals (with diff previews) built from the
//!   model's reply.
//! - [`policy`]: whether code of a file may be sent at all: the per-project switch, languages
//!   turned off for completion and the `.aiignore` exclusion file.
//!
//! Network calls happen in the app through `forge-ai` providers.

pub mod actions;
pub mod inline;
pub mod lang;
pub mod policy;
pub mod text;

pub use actions::{
    explain_prompt, prompt, proposal, test_path, ActionKind, ActionPrompt, ActionRequest, Problem,
    Proposal, TextEdit,
};
pub use inline::{
    build_context, build_prompt, finish, postprocess, CompletionMode, InlineContext, InlinePrompt,
    InlineRequest, MultilinePolicy, RelatedFile,
};
pub use lang::{language_of, Language};
pub use policy::{AiIgnore, Feature, Policy, Verdict};
