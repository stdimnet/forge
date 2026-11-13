/**
 * Postfix completion (PHPStorm's postfix templates): after `expr.` the completion popup offers
 * templates such as `.if`, `.foreach` or `.log`, and typing a full key followed by the expand
 * key (Tab by default) expands it directly. The expression and the expansion come from Rust
 * (`forge-editing`); this file only asks for them and applies the edit.
 */
import { EditorSelection, EditorState, Prec, type Extension } from "@codemirror/state";
import { keymap, type Command, type EditorView } from "@codemirror/view";
import { indentUnit, syntaxTree } from "@codemirror/language";
import type { Completion, CompletionContext, CompletionResult } from "@codemirror/autocomplete";
import { indentMore, insertNewlineAndIndent } from "@codemirror/commands";
import { editingApi, type PostfixCompletion, type PostfixItem, type PostfixLanguage, type PostfixTemplateInfo } from "@/ipc/editing";
import { getSetting } from "@/state/settings";

/** Forge file types with postfix templates, by template language. */
const FAMILIES: Record<string, PostfixLanguage> = {
  php: "php",
  javascript: "js",
  jsx: "js",
  typescript: "js",
  tsx: "js",
  lua: "lua",
};

export const postfixFamily = (fileType: string | undefined): PostfixLanguage | undefined =>
  fileType ? FAMILIES[fileType] : undefined;

let templates: PostfixTemplateInfo[] = [];
let loading: Promise<PostfixTemplateInfo[]> | null = null;

/** All templates (loaded once from Rust). */
export function loadPostfixTemplates(): Promise<PostfixTemplateInfo[]> {
  loading ??= editingApi
    .postfixTemplates()
    .then((t) => (templates = t))
    .catch(() => {
      loading = null;
      return [];
    });
  return loading;
}

const enabled = () => getSetting<boolean>("editor.postfix.enabled", true);
const disabledIds = () => getSetting<string[]>("editor.postfix.disabled", []) ?? [];

/** Not inside a string or comment (multi-line ones included). */
function inCode(state: EditorState, pos: number): boolean {
  for (let node: { name: string; parent: unknown } | null = syntaxTree(state).resolveInner(pos, -1); node; ) {
    if (/string|comment/i.test(node.name)) return false;
    node = node.parent as typeof node;
  }
  return true;
}

/** Ask Rust for the templates that apply at `pos`. */
async function complete(fileType: string, state: EditorState, pos: number): Promise<PostfixCompletion | null> {
  const line = state.doc.lineAt(pos);
  return editingApi
    .postfixComplete({
      language: fileType,
      line: line.text.slice(0, pos - line.from),
      indent: state.facet(indentUnit),
      disabled: disabledIds(),
    })
    .catch(() => null);
}

/** Replace the expression and key with the expansion. */
export function applyPostfix(view: EditorView, lineFrom: number, item: PostfixItem, to: number): void {
  const from = lineFrom + item.from;
  view.dispatch({
    changes: { from, to, insert: item.text },
    selection: EditorSelection.single(from + item.selectionFrom, from + item.selectionTo),
    scrollIntoView: true,
    userEvent: "input.complete",
  });
}

/** `expr.key` right before the caret (no space before the dot). */
const POSTFIX_BEFORE = /[^\s.]\.[A-Za-z]*$/;

function source(fileType: string) {
  return async (ctx: CompletionContext): Promise<CompletionResult | null> => {
    if (!enabled()) return null;
    if (!ctx.explicit && !getSetting<boolean>("editor.postfix.showInCompletion", true)) return null;
    if (!ctx.matchBefore(POSTFIX_BEFORE) || !inCode(ctx.state, ctx.pos)) return null;
    const result = await complete(fileType, ctx.state, ctx.pos);
    if (!result || ctx.aborted) return null;
    const lineFrom = ctx.state.doc.lineAt(ctx.pos).from;
    return {
      from: lineFrom + result.keyFrom,
      options: result.items.map(
        (item): Completion => ({
          label: item.key,
          detail: item.description,
          type: "postfix",
          // Below the members of the expression, as in PHPStorm.
          boost: -20,
          apply: (view, _c, _from, to) => applyPostfix(view, lineFrom, item, to),
        }),
      ),
      validFor: /^[A-Za-z]*$/,
    };
  };
}

type ExpandKey = "tab" | "space" | "enter";

const fallbacks: Record<ExpandKey, Command> = {
  tab: indentMore,
  space: (view) => {
    view.dispatch(view.state.replaceSelection(" "), { userEvent: "input.type" });
    return true;
  },
  enter: insertNewlineAndIndent,
};

/**
 * The expand key: when the caret follows `expr.key` with a full template key, expand it;
 * otherwise the key does what it always does.
 */
function expandWith(fileType: string, key: ExpandKey): Command {
  return (view) => {
    if (!enabled() || getSetting<string>("editor.postfix.expandKey", "tab") !== key) return false;
    const { state } = view;
    const sel = state.selection.main;
    if (state.selection.ranges.length > 1 || !sel.empty) return false;
    const typed = state.sliceDoc(state.doc.lineAt(sel.head).from, sel.head).match(/[^\s.]\.([A-Za-z]+)$/)?.[1];
    const family = postfixFamily(fileType);
    const off = disabledIds();
    const known = templates.some((t) => t.language === family && t.key === typed && !off.includes(t.id));
    if (!typed || !known || !inCode(state, sel.head)) return false;
    // The key press is taken now; Rust answers asynchronously.
    void complete(fileType, state, sel.head).then((result) => {
      if (view.state.doc !== state.doc || view.state.selection.main.head !== sel.head) return;
      const item = result?.items.find((i) => i.key === typed);
      if (item) applyPostfix(view, state.doc.lineAt(sel.head).from, item, sel.head);
      else fallbacks[key](view);
    });
    return true;
  };
}

/** Postfix completion for a file of type `fileType`; `null` for languages without templates. */
export function postfixCompletion(fileType: string | undefined): Extension | null {
  if (!fileType || !postfixFamily(fileType)) return null;
  void loadPostfixTemplates();
  const data = [{ autocomplete: source(fileType) }];
  return [
    EditorState.languageData.of(() => data),
    Prec.highest(
      keymap.of([
        { key: "Tab", run: expandWith(fileType, "tab") },
        { key: "Space", run: expandWith(fileType, "space") },
        { key: "Enter", run: expandWith(fileType, "enter") },
      ]),
    ),
  ];
}
