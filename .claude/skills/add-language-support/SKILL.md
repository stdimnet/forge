---
name: add-language-support
description: Add or improve syntax highlighting / file type support for a language or template engine in the Forge editor (CodeMirror 6). Use when a file type shows as "Plain text", when adding a new template language, or when a pack needs its own syntax mode.
---

# Add language support

Which file type a file has (names, extensions, shebangs, labels, indentation, template data
languages) is decided in Rust by `crates/forge-filetypes`: bundled types in its `builtin.rs`,
plus the `[[plugin.contributions.fileTypes]]` of the plugin manifests. The UI only maps a file
type id to a CodeMirror syntax mode in `app/ui/src/editor/languages.ts`.

## Steps

1. Pick the best source, in this order:
   - an official Lezer package `@codemirror/lang-<x>` (full parser, folding, indentation);
   - a legacy mode from `@codemirror/legacy-modes/mode/<x>` wrapped with `StreamLanguage.define`;
   - your own `StreamParser` (see `app/ui/src/editor/dotenv.ts`), or for HTML-based templates
     a `TemplateSyntax` for the shared template parser in `app/ui/src/editor/templates.ts`
     (Twig, Smarty, Latte, Jinja, Handlebars, Go templates use it; it is drawn as a decoration
     layer over the data language, see `templateOverlay.ts`).
2. Declare the file type: a bundled one in `crates/forge-filetypes/src/builtin.rs`, a pack's
   in its manifest (`app/src-tauri/plugins/*.toml`):

   ```toml
   [[plugin.contributions.fileTypes]]
   id = "neon"
   name = "NEON"                      # shown in the status bar
   extensions = ["neon"]              # longest match wins: "html.twig" beats "html"
   fileNames = ["phpstan.neon*"]      # exact names, `*` wildcards, `/` for path patterns
   interpreters = ["neon"]            # optional: `#!` lines of extensionless scripts
   indentSize = 2                     # optional, default 4
   ```

   A plugin type with the same id as a bundled one replaces it while the plugin is active.
   After changing a manifest run `FORGE_UPDATE_SNAPSHOTS=1 cargo test -p forge-app`.
3. Map the id to its syntax mode: add it to the `loaders` in `languages.ts`, or call
   `registerLanguage` from a pack:

   ```ts
   registerLanguage({ id: "neon", load: () => yaml() });
   ```

   A later `registerLanguage` with the same id replaces the mode; the Blade pack does this.
4. Token colors come from `forgeHighlight` in `app/ui/src/editor/forgeTheme.ts`, which maps
   Lezer tags to `--syn-*` CSS variables. Stream parsers return tag names such as `"keyword"`,
   `"string"`, `"comment"`, `"variableName.special"` or `"propertyName.definition"`. If a tag has
   no style, add one there instead of hard-coding colors.
5. Add a file icon in `app/ui/src/shell/FileIcon.tsx` (`byExt`).
6. Add a detection case to the tests in `crates/forge-filetypes/src/tests.rs` (bundled) or
   `file_types_detect_like_phpstorm` in `app/src-tauri/src/plugins.rs` (manifests), and a sample file to
   `app/ui/src/mock/demoConfigFiles.ts`. Then add a scenario to
   `app/ui/scripts/scenarios/syntax.mjs` and check the screenshot.

## Packs with editor features

Languages that need more than colors (completion, hover docs, folding, checks) live in
`app/ui/src/features/languages/<name>.ts`; see `bash.ts`, `postfix.ts` or `compose.ts` for
examples. Each file calls `registerLanguage` and `registerEditorExtension`.

- Shebangs (`interpreters`) and indentation (`indentSize`) are part of the manifest's file type;
  `fileNames` may contain `*` anywhere and `/` to match a path suffix (`config/services*.yaml`).
- Code that needs a file's type calls `languageIdFor` / `languageLabel` / `indentFor`
  (`editor/languages.ts`), which read the per-path cache of `editor/fileTypes.ts`. In the
  desktop app answers come over IPC (`filetypes_detect`); await `ensureFileType(path)` before
  building something that needs it synchronously (the editor does).
- Use `streamLanguage` from `src/editor/tokens.ts` for stream parsers: it maps the shared token
  names (`keyword`, `def`, `builtin`, `variable`, `option`, ...) to Lezer tags.
- `syntaxDiagnostics` (`features/languages/shared.ts`) feeds checks into the Problems view and is
  skipped while a language server for the file is running.
- Declare the file type in `app/src-tauri/plugins/langs.toml` (`[[contributions.fileTypes]]`),
  then run `FORGE_UPDATE_SNAPSHOTS=1 cargo test -p forge-app`.
- Parsers are unit-tested without an editor, see `features/languages/testkit.ts`.

## Template data languages

Template languages (Twig, Smarty, Latte, Jinja, Handlebars, Go templates) highlight the content between their
tags in a *data language*, as PHPStorm does. File types marked `template` get one, chosen in
Rust (`crates/forge-filetypes/src/data.rs`) from the first of these that applies:

1. the `templates.dataLanguages` setting (Settings → Languages & Frameworks → Template Data
   Languages), with entries like `legacy/latex/*=latex`;
2. a compound file name: `invoice.tex.twig` is LaTeX, `report.sql.twig` is SQL,
   `style.css.tpl` is CSS;
3. HTML.

The data language keeps its own highlighting: a file is loaded as the data language's real
editor support (`support` in `DATA_LANGUAGES`: the Lezer grammar for HTML, CSS, SQL, YAML, ...,
or a `StreamLanguage` from its `parser`), and the template engine is only a decoration layer
on top (`templateOverlay` in `app/ui/src/editor/templateOverlay.ts`). The UI side of data
languages (`DATA_LANGUAGES` in `app/ui/src/editor/templateData.ts`) only maps ids to highlighting. The overlay runs the
template parser with no data language, so it colors delimiters, keywords, variables, filters,
strings and comments inside the tags and leaves everything else to the data language. Tags
may therefore sit anywhere, also where the data language would not expect them.

To support another data language, add it to `DATA_LANGUAGES` in `data.rs` (id, label) and to
`COMPOUND` there, and to `DATA_LANGUAGES` in `templateData.ts` with a `parser` and, if there is
one, a `support` factory. To support another
template engine, describe it as a `TemplateSyntax` (delimiters, keywords, builtins) and
register it with the `template(...)` helper in `languages.ts`.

### Configurable delimiters

Engines whose delimiters can change (Smarty's `left_delimiter`, Twig's and Jinja's
`tag_variable` / `tag_block` / `tag_comment`, Go's `Template.Delims`) are listed in Rust, in
`CONFIGURABLE_TEMPLATES` (`crates/forge-filetypes/src/delimiters.rs`). Each declares its
delimiter `fields` with their built-in defaults (`left`/`right`, or `variableStart`,
`variableEnd`, `blockStart`, ...). Rust resolves the delimiters of every template file
(`FileTypeInfo.delimiters`); the UI only maps them to a syntax with the builder in `SYNTAX`
(`app/ui/src/editor/templateDelimiters.ts`), which derives everything else (comments, end
tags, literal blocks).

One setting, `templates.delimiters` (type `objects`, page `lang.templates`, declared in
`app/src-tauri/plugins/core.toml`), holds the differences: one entry per (template language,
data language) pair, e.g. `{ template = "smarty", language = "latex", left = "[[", right = "]]" }`.
The LaTeX entries ship as default *data*; never add a code path for a specific pair. Files
without an entry use the language's defaults. The settings page edits the entries with
`TemplateDelimitersEditor` (a table plus a form with the template's fields, both from the
`filetypes_template_delimiters` command); duplicate pairs are rejected by the `unique` fields
of the schema, checked in Rust (`settings_validate`). To make another engine configurable, add
it to `CONFIGURABLE_TEMPLATES` in Rust and its syntax builder to `SYNTAX`; its
`template()` registration in `languages.ts` then follows the setting.

`objects` settings (lists of tables) are generic: declare `type = "objects"` with optional
`unique` fields (validated in `forge-plugin-api`), and register an editor with
`registerObjectSettingEditor(key, Component)`.

Everything tag-related reads the resolved syntax from `templateSyntaxField`: the overlay,
comment toggling, folding, matching delimiters, auto-closing and tag completion
(`templateEditing.ts`). The syntax is live: a settings change re-highlights open editors.

Smarter features for a language (completion, hover, diagnostics) belong to a language server
(`forge-lsp`, see the PHP pack) or a pack-specific editor extension (`registerEditorExtension`).
