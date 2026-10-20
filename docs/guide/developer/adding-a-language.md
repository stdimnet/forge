---
title: Adding a language
description: Add a file type, a syntax mode, editor features, a language server and template data languages to Forge.
order: 90
tags: [languages, syntax highlighting, file types, codemirror]
---

# Adding a language

Recipe: [`add-language-support`](../../../.claude/skills/add-language-support/SKILL.md).

A language has up to four parts:

| Part | Where |
| --- | --- |
| **File type** (names, extensions, shebangs, label, indentation) | Rust: `crates/forge-filetypes/src/builtin.rs` (bundled) or a manifest's `[[contributions.fileTypes]]` |
| **Syntax mode** (highlighting) | UI: `app/ui/src/editor/languages.ts` or `registerLanguage` |
| **Editor features** without a server (completion, hover, folding, checks) | UI: `app/ui/src/features/languages/<name>.ts` |
| **Language server** | catalog: `crates/forge-langservers/catalog.toml` |

## 1. File type

```toml
[[plugin.contributions.fileTypes]]
id = "neon"
name = "NEON"                      # status bar label
extensions = ["neon"]              # longest match wins: "html.twig" beats "html"
fileNames = ["phpstan.neon*"]      # exact names, `*` anywhere, `/` for path suffixes
interpreters = ["neon"]            # optional: `#!` lines of extensionless scripts
indentSize = 2                     # optional, default 4
```

A plugin type with the id of a bundled type replaces it while the plugin is active. After
changing a manifest run `FORGE_UPDATE_SNAPSHOTS=1 cargo test -p forge-app`. Add detection tests
to `crates/forge-filetypes/src/tests.rs` (bundled) or `file_types_detect_like_phpstorm` in
`app/src-tauri/src/plugins.rs` (manifests).

The UI asks Rust for a file's type (`filetypes_detect`) and caches it per path; use
`languageIdFor`, `languageLabel` and `indentFor` from `editor/languages.ts`, and
`await ensureFileType(path)` before building something synchronous.

## 2. Syntax mode

Pick, in this order: an official Lezer package (`@codemirror/lang-<x>`), a legacy mode
(`@codemirror/legacy-modes`) wrapped with `StreamLanguage.define`, or your own `StreamParser`
(use `streamLanguage` from `src/editor/tokens.ts` to map token names to Lezer tags). Then:

```ts
registerLanguage({ id: "neon", load: () => yaml() });
```

Token colors come from `forgeHighlight` in `src/editor/forgeTheme.ts` (Lezer tags → `--syn-*`
CSS variables). Add a style there instead of hard-coding colors. Add a file icon in
`src/shell/FileIcon.tsx`.

## 3. Editor features without a server

Language packs for config and script formats live in `app/ui/src/features/languages/` (see
`bash.ts`, `compose.ts`, `postfix.ts`). Each file calls `registerLanguage` and
`registerEditorExtension`; `shared.ts` provides hover docs, folding and `syntaxDiagnostics`,
which feeds the Problems view unless a listed language server runs for the file. Parsers are
unit-tested without an editor (`testkit.ts`).

## 4. Language server

Add a `[[language]]` and a `[[server]]` entry to `crates/forge-langservers/catalog.toml`:
command and args, `languages`, `group`, `install` options (npm commands are rewritten to
Forge's own prefix), `license`, `homepage`, optional `alternative`, `plugin` and `when`. Server
settings and initialization options go into the entry; project-specific values are added in
`crates/forge-langservers/src/config.rs`. If a file uses a feature only the server provides, add
an install hint to `SERVER_HINTS` (`features/langservers/serverHint.ts`).

## Template languages and data languages

HTML-based template engines (Twig, Smarty, Latte, Jinja, Handlebars, Go templates) are a
`TemplateSyntax` for the shared template parser in `src/editor/templates.ts`, drawn as a
decoration layer (`templateOverlay.ts`) over the data language's real editor support. The data
language is chosen in Rust (`crates/forge-filetypes/src/data.rs`): the
`templates.dataLanguages` setting, then compound names (`invoice.tex.twig`), then HTML.

- Another data language: add it to `DATA_LANGUAGES` and `COMPOUND` in `data.rs`, and to
  `DATA_LANGUAGES` in `src/editor/templateData.ts` with a `parser` (and a `support` factory if
  one exists).
- Another template engine: describe it as a `TemplateSyntax` (delimiters, keywords, builtins)
  and register it with the `template(...)` helper in `languages.ts`; mark its type as a template
  in `builtin.rs` (`TEMPLATES`).

## Check

Add a sample to `app/ui/src/mock/demoConfigFiles.ts`, a scenario to
`app/ui/scripts/scenarios/syntax.mjs`, capture it and look at it in both themes.
