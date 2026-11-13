---
title: forge-injections
description: "Language injections: SQL, HTML, CSS, JSON and RegExp inside PHP and JavaScript/TypeScript strings, `// language=` comments, fragment escaping and the injection intentions (pure, WebAssembly-friendly)."
order: 26
tags: [crate, rust]
---

# forge-injections

`crates/forge-injections` — Language injections: SQL, HTML, CSS, JSON and RegExp inside PHP and JavaScript/TypeScript strings, `// language=` comments, fragment escaping and the injection intentions (pure, WebAssembly-friendly).

## Purpose

Everything behind PHPStorm's IntelliLang that is not per-keystroke highlighting: which string
literals carry another language, the text ranges that belong to it, the value of a literal for
Edit Fragment and Run SQL, escaping typed text back into a literal, and the Alt+Enter
intentions. The editor only colors the ranges with the injected language's CodeMirror parser
(`app/ui/src/features/injections`).

- `lexer`: string literals and comments of PHP (only inside `<?php … ?>`; single and double
  quotes, heredoc and nowdoc with their label, `$var` / `{$expr}` interpolations as gaps) and
  of JavaScript/TypeScript (quotes, template literals with nested `${…}` code, regular
  expression literals skipped). `Host::of_path` picks the lexer from the file name (Blade is
  not a host).
- `rules`: the built-in rules (`RULES`, ids such as `php-sql-calls` or `js-css`) and
  `detect`: literals joined with `.` / `+` are grouped, then the first of a `language=` comment
  (`// language=SQL`, `/* language=JSON */`, `# language=HTML`, `/** @lang SQL */`, first
  literal after it in the same statement), a heredoc label (`<<<SQL`), a call, tagged template
  or assignment rule (`PDO::query()`, `DB::select()`, `preg_match()`, `` sql`…` ``,
  `innerHTML =`), and the text rules (strings starting with a SQL statement or an HTML tag)
  applies. `createQuery()` and `<<<DQL` are left to `phpforge-dql`. PHP patterns are injected
  without their delimiters and flags.
- `escape`: `unescape` (value of a literal plus a value-to-source offset map) and `escape`
  (text typed into a fragment, written for the literal's kind: `\'` in single-quoted PHP,
  `\$` in double-quoted PHP and heredocs, `\n` in JavaScript quotes, `` \` `` and `\$` in
  templates).
- `languages`: the injectable languages (`sql`, `html`, `css`, `json`, `regexp`, `javascript`,
  `xml`) and their aliases (`MySQL`, `PostgreSQL`, `JS`, `Regex`, …).
- `lib`: the API with UTF-16 offsets (CodeMirror's): `find`, `injection_at`, `fragment`,
  `inject` (the `language=` comment to insert, inline `/* … */` when other code precedes the
  string on its line, or the replaced comment), `context_actions` ("Inject language or
  reference", "Edit SQL Fragment", "Un-inject Language" as `forge_codeintel::ContextAction`s),
  `rule_infos`, `languages`. Disabled rules and un-injected places (`path#key`, `key` a hash
  of the literals' source) come with each request.

## Main public items

```rust
pub fn find(req: &FindRequest) -> Vec<Injection> { … }
pub fn injection_at(req: &OffsetRequest) -> Option<Injection> { … }
pub fn fragment(req: &OffsetRequest) -> Option<Fragment> { … }
pub fn inject(req: &InjectRequest) -> Option<Edit> { … }
pub fn context_actions(req: &OffsetRequest) -> Vec<ContextAction> { … }
pub fn escape(kind: LiteralKind, text: &str) -> String { … }
pub fn unescape(kind: LiteralKind, src: &str, from: usize, to: usize) -> (String, Vec<usize>) { … }
pub fn rule_infos() -> Vec<RuleInfo> { … }
pub fn languages() -> Vec<Language> { … }
```

## Used by

- IPC: `injections_find`, `injections_at`, `injections_fragment`, `injections_escape`,
  `injections_inject`, `injections_rules`, `injections_languages`
  (`app/src-tauri/src/commands/injections.rs`); `code_context_actions` adds
  `context_actions` with the `editor.injections.*` settings.
- UI: `app/ui/src/features/injections` (highlighting, Edit Fragment panel, language chooser,
  Run SQL, Settings | Editor | Language Injections). The editor calls `injections_find`,
  `injections_fragment` and `injections_escape` in-process through `forge-wasm` in both modes,
  because highlighting and the fragment editor follow every keystroke.
- Browser mode: built into `forge-wasm` (`injections.rs`), also for `code_context_actions`.

## Dependencies

- Workspace crates: `forge-codeintel` (context actions and workspace edits)
- External: `serde`, `serde_json`, `regex-lite`

## Tests

Unit tests for host detection, the PHP and JavaScript lexers (PHP tags, heredocs,
interpolations, templates, regex literals), every rule, comment and heredoc precedence,
concatenation, disabled rules and un-injected places, UTF-16 offsets, escaping round trips per
literal kind, fragment offset maps, the inserted and replaced comments and the intentions.

```sh
cargo test -p forge-injections
```
