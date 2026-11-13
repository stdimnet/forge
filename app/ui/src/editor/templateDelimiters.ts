/**
 * Template delimiters per (template language, data language) pair, like Smarty's
 * `left_delimiter` or Twig's `tag_variable` / `tag_block` / `tag_comment` lexer options.
 *
 * Rust (`crates/forge-filetypes`, `delimiters.rs`) knows the configurable template languages
 * and their default delimiters, and resolves the delimiters of each template file from the
 * `templates.delimiters` setting (`FileTypeInfo.delimiters`). This module only turns those
 * values into the syntax the per-keystroke template tokenizer uses.
 */
import { fileTypeOf, isFileTypeResolved, type FileTypeInfo } from "./fileTypes";
import { goTemplateSyntaxWith, jinjaSyntaxWith, smartySyntaxWith, twigSyntaxWith, type TemplateSyntax } from "./templates";

/** The setting with the delimiter entries. */
export const DELIMITERS_KEY = "templates.delimiters";

/** Delimiter values by field key, as resolved by Rust. */
export type DelimiterValues = Record<string, string>;

/** The template syntax of each language with configurable delimiters, by file type id. */
const SYNTAX: Record<string, (v: DelimiterValues) => TemplateSyntax> = {
  smarty: (v) => smartySyntaxWith(v.left, v.right),
  twig: (v) => twigSyntaxWith(v),
  jinja: (v) => jinjaSyntaxWith(v),
  gotemplate: (v) => goTemplateSyntaxWith(v.left, v.right),
};

/** The template syntax of `templateId` with `values`; `undefined` for fixed-syntax languages. */
export function syntaxWithDelimiters(templateId: string, values: DelimiterValues): TemplateSyntax | undefined {
  return SYNTAX[templateId]?.(values);
}

/** The template syntax of a file from its resolved file type (the defaults without delimiters). */
export function templateSyntaxOf(templateId: string, info: Pick<FileTypeInfo, "delimiters">): TemplateSyntax | undefined {
  return syntaxWithDelimiters(templateId, info.delimiters ?? {});
}

/**
 * The live template syntax of a file. Reactive: reading it in an effect tracks the file type,
 * which Rust resolves again when the delimiter settings change. While a new answer is pending
 * (desktop app), the last known syntax stays, so open editors do not flicker.
 */
export function liveTemplateSyntax(templateId: string, path: string, initial: FileTypeInfo): () => TemplateSyntax | undefined {
  let last = templateSyntaxOf(templateId, initial);
  return () => {
    const info = fileTypeOf(path);
    if (isFileTypeResolved(path)) last = templateSyntaxOf(templateId, info);
    return last;
  };
}
