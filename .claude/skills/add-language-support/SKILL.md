---
name: add-language-support
description: Add or improve syntax highlighting / file type support for a language or template engine in the Forge editor (CodeMirror 6). Use when a file type shows as "Plain text", when adding a new template language, or when a pack needs its own syntax mode.
---

# Add language support

File types are mapped to CodeMirror languages in `app/ui/src/editor/languages.ts`.

## Steps

1. Pick the best source, in this order:
   - an official Lezer package `@codemirror/lang-<x>` (full parser, folding, indentation);
   - a legacy mode from `@codemirror/legacy-modes/mode/<x>` wrapped with `StreamLanguage.define`;
   - your own `StreamParser` (see `app/ui/src/editor/dotenv.ts`), or for HTML-based templates
     a `TemplateSyntax` for the shared template parser in `app/ui/src/editor/templates.ts`
     (Twig, Smarty, Latte, Jinja, Handlebars, Go templates use it).
2. Add an entry to the `languages` list, or call `registerLanguage` from a pack:

   ```ts
   registerLanguage({
     id: "neon",
     label: "NEON",                 // shown in the status bar
     extensions: ["neon"],          // longest match wins: "html.twig" beats "html"
     fileNames: ["phpstan.neon*"],  // exact names, "prefix*" or "*suffix"
     load: () => yaml(),
   });
   ```

   A later `registerLanguage` with the same id replaces the built-in entry; a pack's Blade or
   Twig mode overrides the generic one this way.
3. Token colors come from `forgeHighlight` in `app/ui/src/editor/forgeTheme.ts`, which maps
   Lezer tags to `--syn-*` CSS variables. Stream parsers return tag names such as `"keyword"`,
   `"string"`, `"comment"`, `"variableName.special"` or `"propertyName.definition"`. If a tag has
   no style, add one there instead of hard-coding colors.
4. Add a file icon in `app/ui/src/shell/FileIcon.tsx` (`byExt`).
5. Add a case to `app/ui/src/editor/languages.test.ts` and a sample file to
   `app/ui/src/mock/demoConfigFiles.ts`. Then add a scenario to
   `app/ui/scripts/scenarios/syntax.mjs` and check the screenshot.

## Template data languages

Template languages (Twig, Smarty, Latte, Jinja, Handlebars, Go templates) highlight the content between their
tags in a *data language*, as PHPStorm does. The data language is chosen in
`app/ui/src/editor/templateData.ts`, from the first of these that applies:

1. the `templates.dataLanguages` setting (Settings → Languages & Frameworks → Template Data
   Languages), with entries like `legacy/latex/*=latex`;
2. a compound file name: `invoice.tex.twig` is LaTeX, `report.sql.twig` is SQL,
   `style.css.tpl` is CSS;
3. HTML.

To support another data language, add it to `DATA_LANGUAGES` with a stream parser, and add its
inner extension to `COMPOUND`. The template parser (`templateParser(syntax, dataParser)`) cuts
each line at the next template opener, so template tags also work inside strings of the data
language. Smarty delimiters are configurable (`templates.smarty.leftDelimiter`), which helps for
LaTeX, where `{` is part of the content.

Smarter features for a language (completion, hover, diagnostics) belong to a language server
(`forge-lsp`, see the PHP pack) or a pack-specific editor extension (`registerEditorExtension`).
