/**
 * Fallback completion for every file: words already in the document, offered when completion
 * is invoked explicitly (Ctrl+Space / Shift+Space). Languages with their own sources keep
 * them; this adds to them instead of replacing them, and never pops up while typing.
 */
import { EditorState } from "@codemirror/state";
import { completeAnyWord, type CompletionContext, type CompletionResult } from "@codemirror/autocomplete";

export const explicitWords = (ctx: CompletionContext): CompletionResult | null | Promise<CompletionResult | null> =>
  ctx.explicit ? completeAnyWord(ctx) : null;

export const wordCompletion = EditorState.languageData.of(() => [{ autocomplete: explicitWords }]);
