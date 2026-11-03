---
title: forge-ai-editor
description: "AI in the editor: inline completion context and post-processing, AI actions (explain, fix, tests, docs) and the .aiignore policy (pure, WebAssembly-friendly)."
order: 21
tags: [crate, rust, ai]
---

# forge-ai-editor

`crates/forge-ai-editor` — AI in the editor: inline completion context and post-processing,
AI actions (explain, fix, tests, docs) and the `.aiignore` policy. Pure logic (no network, no
file system), also built to WebAssembly for browser mode.

## Purpose

- **Inline (ghost text) completion** (`inline.rs`): `build_context` cuts the code around the
  caret (UTF-16 offsets from CodeMirror) to a prefix and suffix window, collects the file's
  imports outside the window and the heads of other open files (same language first), and
  decides single or multi-line (`MultilinePolicy`). `build_prompt` makes both request forms: a
  fill-in-the-middle prompt with a commented context header and stop sequences, and a chat
  prompt with a `<CURSOR>` marker. `postprocess` cleans the answer: end tokens and markers,
  Markdown fences, code echoed from before the caret, re-indented first lines, the
  single-line cut, the end of the block for multi-line suggestions (indentation and bracket
  depth), and overlap with the text after the caret (closing brackets, a duplicated `}` line).
- **AI actions** (`actions.rs`): `prompt` builds the request of "Explain with AI", "Fix with
  AI" (the problems at the caret, the region with context before and after), "Generate
  Documentation with AI" (the declaration at the caret, the language's doc format) and
  "Generate Tests with AI" (the code under test, the framework and the conventional test
  file, `test_path`). `proposal` turns the reply into a `Proposal`: the new text with a diff
  (`forge_agent_cli::diff::text_diff`) and, for the requesting file, the minimal `TextEdit`.
  Doc comments go above attributes and replace an existing one; Python docstrings go into the
  body; Rust tests are appended as `#[cfg(test)] mod tests`.
- **Policy** (`policy.rs`): `Policy::check` decides whether code of a file may be sent:
  `ai.enabled` for the project, `.aiignore` / `.aiexclude` (gitignore syntax: anchored and
  unanchored patterns, `**`, character classes, `!`, folder rules) and, for inline completion,
  the languages turned off.
- `lang.rs`: language ids, comment and doc styles, declaration and import lines, test
  frameworks.

The network calls are made by the app with [`forge-ai`](forge-ai.md) providers.

## Modules

| Module | Summary |
| --- | --- |
| `actions.rs` | AI actions in the editor: prompts and proposals with diff previews. |
| `inline.rs` | Inline completion: context, fill-in-the-middle / chat prompts, post-processing. |
| `lang.rs` | What the AI features need to know about a language. |
| `policy.rs` | Whether code may be sent: project switch, `.aiignore`, per-language switch. |
| `text.rs` | UTF-16 / byte offsets, lines and indentation, code from fenced replies. |

## Main public items

```rust
pub use actions::{explain_prompt, prompt, proposal, test_path, ActionKind, ActionPrompt, ActionRequest, Problem, Proposal, TextEdit};
pub use inline::{build_context, build_prompt, finish, postprocess, CompletionMode, InlineContext, InlinePrompt, InlineRequest, MultilinePolicy, RelatedFile};
pub use lang::{language_of, Language};
pub use policy::{AiIgnore, Feature, Policy, Verdict};
```

## Used by

- IPC: `ai_inline_complete`, `ai_inline_cancel`, `ai_editor_status`, `ai_action`, `ai_explain` (`app/src-tauri/src/commands/ai_editor.rs`)
- [`forge-code-review`](forge-code-review.md): the `.aiignore` policy for pull request diffs
- UI: `app/ui/src/features/ai-editor`
- Browser mode: [`forge-wasm`](forge-wasm.md) `ai_editor.rs` (`ai_inline_finish`, `ai_action_prompt`, `ai_action_proposal`, `ai_policy_check`) for `app/ui/src/mock/aiEditorMock.ts`

## Dependencies

- Workspace crates: [`forge-agent-cli`](forge-agent-cli.md) (without the `runtime` feature, for the diff)
- External: `serde`, `serde_json`

## Tests

6 source files; 21 unit tests (context and trimming, multi-line policy, post-processing,
region detection, fix / docs / tests proposals in PHP, Rust, Python and Lua, test paths,
`.aiignore` matching and the policy).

```sh
cargo test -p forge-ai-editor
```
