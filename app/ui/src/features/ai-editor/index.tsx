/**
 * AI in the editor (JetBrains AI Assistant style), built on the AI providers of
 * Settings | Tools | AI Providers:
 *
 * - inline (ghost text) completion with its keymap actions and status bar widget,
 * - Alt+Enter: "Explain with AI" (answer in the AI Chat) and "Fix with AI" for problems at
 *   the caret,
 * - Generate (Alt+Insert) and the editor menu's "AI Actions": documentation and tests,
 * - Settings | Tools | AI Assistant.
 *
 * Every request first passes the backend's policy (provider configured and enabled, AI on
 * for the project, file not in `.aiignore`).
 */
import { Show } from "solid-js";
import { render } from "solid-js/web";
import type { EditorView } from "@codemirror/view";
import type { ContextAction } from "@/ipc/codeintel";
import { registerContextActionHandler, registerContextActionProvider } from "@/features/codeintel/intentions";
import { item, registerContextMenuProvider, separator, submenu } from "@/features/contextMenu/state";
import { registerSettingsPage } from "@/features/settingsPages";
import { registerStatusItem } from "@/features/statusItems";
import { registerEditorExtension } from "@/editor/extensions";
import { registerGenerateItems } from "@/editor/generateItems";
import { viewFor } from "@/editor/views";
import { registerKeymapActions, runAction } from "@/keymap";
import { registerCommand } from "@/state/commands";
import { editor } from "@/state/editor";
import { ACTION_TITLES, explainWithAi, problemsAt, proposalState, runProposal } from "./actions";
import { AiAssistantPage } from "./AiAssistantPage";
import {
  acceptLine,
  acceptSuggestion,
  acceptWord,
  CALL,
  callInlineCompletion,
  cycleSuggestion,
  dismissSuggestion,
  inlineCompletion,
  INSERT,
  INSERT_LINE,
  INSERT_WORD,
  NEXT,
  PREV,
} from "./inline";
import { ProposalDialog } from "./ProposalDialog";
import { actionsAllowed } from "./status";
import { AiStatusWidget } from "./StatusWidget";
import "./aiEditor.css";

const isFile = (path: string) => !path.includes("://");

registerEditorExtension(({ path }) => (isFile(path) ? inlineCompletion(path) : null));

/** The focused file editor, for commands run from Find Action or menus. */
function active(): { path: string; view: EditorView } | undefined {
  const path = editor.active;
  const view = path && isFile(path) ? viewFor(path) : undefined;
  return path && view ? { path, view } : undefined;
}

const withEditor = (fn: (path: string, view: EditorView) => unknown) => () => {
  const a = active();
  if (a) void fn(a.path, a.view);
};

// ---- inline completion keys (native: the editor handles them while a suggestion shows)

registerKeymapActions([
  { id: INSERT, title: "Insert Inline Proposal", group: "Editor Actions/Inline Completion", context: "editor", native: true, shortcuts: { $default: ["Tab"] } },
  {
    id: INSERT_WORD,
    title: "Insert Inline Proposal Word",
    group: "Editor Actions/Inline Completion",
    context: "editor",
    native: true,
    shortcuts: { $default: ["Ctrl+Right"], macOS: ["Alt+Right"] },
  },
  { id: INSERT_LINE, title: "Insert Inline Proposal Line", group: "Editor Actions/Inline Completion", context: "editor", native: true, shortcuts: { $default: [] } },
  { id: NEXT, title: "Show Next Inline Proposal", group: "Editor Actions/Inline Completion", context: "editor", native: true, shortcuts: { $default: ["Alt+]"] } },
  { id: PREV, title: "Show Previous Inline Proposal", group: "Editor Actions/Inline Completion", context: "editor", native: true, shortcuts: { $default: ["Alt+["] } },
  { id: CALL, title: "Call Inline Completion", group: "Editor Actions/Inline Completion", context: "editor", native: true, shortcuts: { $default: ["Alt+Shift+\\"] } },
  { id: "ai.explain", title: "Explain with AI", group: "AI Actions" },
  { id: "ai.fix", title: "Fix with AI", group: "AI Actions" },
  { id: "ai.generateDocs", title: "Generate Documentation with AI", group: "AI Actions" },
  { id: "ai.generateTests", title: "Generate Tests with AI", group: "AI Actions" },
]);

registerCommand({ id: INSERT, title: "Insert Inline Proposal", category: "AI", run: withEditor((_p, v) => acceptSuggestion(v)) });
registerCommand({ id: INSERT_WORD, title: "Insert Inline Proposal Word", category: "AI", run: withEditor((_p, v) => acceptWord(v)) });
registerCommand({ id: INSERT_LINE, title: "Insert Inline Proposal Line", category: "AI", run: withEditor((_p, v) => acceptLine(v)) });
registerCommand({ id: NEXT, title: "Show Next Inline Proposal", category: "AI", run: withEditor((_p, v) => cycleSuggestion(v, 1)) });
registerCommand({ id: PREV, title: "Show Previous Inline Proposal", category: "AI", run: withEditor((_p, v) => cycleSuggestion(v, -1)) });
registerCommand({ id: CALL, title: "Call Inline Completion", category: "AI", run: withEditor((_p, v) => callInlineCompletion(v)) });
registerCommand({ id: "ai.dismissInline", title: "Dismiss Inline Proposal", category: "AI", run: withEditor((_p, v) => dismissSuggestion(v)) });

// ---- actions

const fix = (path: string, view: EditorView) => {
  const problems = problemsAt(path, view);
  return runProposal("fix", path, view, problems);
};

registerCommand({ id: "ai.explain", title: ACTION_TITLES.explain, category: "AI", run: withEditor(explainWithAi) });
registerCommand({ id: "ai.fix", title: ACTION_TITLES.fix, category: "AI", run: withEditor(fix) });
registerCommand({ id: "ai.generateDocs", title: ACTION_TITLES.docs, category: "AI", run: withEditor((p, v) => runProposal("docs", p, v)) });
registerCommand({ id: "ai.generateTests", title: ACTION_TITLES.tests, category: "AI", run: withEditor((p, v) => runProposal("tests", p, v)) });

function entry(title: string, kind: ContextAction["kind"], action: string, data: unknown): ContextAction {
  return { title, kind, source: "ai", isPreferred: false, needsResolve: false, action, data };
}

// Alt+Enter. Offered everywhere, so the entries do not light the bulb.
registerContextActionProvider(
  async (path, view) => {
    if (!isFile(path) || !(await actionsAllowed(path))) return [];
    const problems = problemsAt(path, view);
    const out: ContextAction[] = [];
    if (problems.length) out.push(entry(ACTION_TITLES.fix, "quickFix", "ai.fixAt", { problems }));
    out.push(entry(ACTION_TITLES.explain, "intention", "ai.explainAt", {}));
    return out;
  },
  { bulb: false },
);

registerContextActionHandler("ai.fixAt", (data, ctx) => runProposal("fix", ctx.path, ctx.view, (data as { problems: ReturnType<typeof problemsAt> }).problems));
registerContextActionHandler("ai.explainAt", (_data, ctx) => explainWithAi(ctx.path, ctx.view));

// Editor context menu: AI Actions.
registerContextMenuProvider<EditorView>("editor", (view) => {
  const path = editor.active;
  if (!path || !isFile(path)) return [];
  const problems = problemsAt(path, view).length > 0;
  return [
    separator,
    submenu("AI Actions", [
      item(ACTION_TITLES.explain, () => void explainWithAi(path, view), "ai.explain"),
      item(ACTION_TITLES.fix, () => void fix(path, view), "ai.fix", { disabled: !problems, hint: problems ? undefined : "No problems at the caret" }),
      separator,
      item(ACTION_TITLES.docs, () => void runProposal("docs", path, view), "ai.generateDocs"),
      item(ACTION_TITLES.tests, () => void runProposal("tests", path, view), "ai.generateTests"),
      separator,
      item("Open AI Chat", () => void runAction("AIChat.Focus"), "AIChat.Focus"),
    ]),
  ];
});

// Generate (Alt+Insert).
registerGenerateItems((path, view) =>
  isFile(path)
    ? [
        { label: "Documentation with AI…", mnemonic: 0, enabled: true, run: () => void runProposal("docs", path, view) },
        { label: "Tests with AI…", mnemonic: 0, enabled: true, run: () => void runProposal("tests", path, view) },
      ]
    : [],
);

registerSettingsPage({
  id: "tools.ai-assistant",
  title: "AI Assistant",
  group: "Tools",
  order: 8,
  keywords: ["AI", "inline completion", "ghost text", "completion", "explain", "fix", "tests", "documentation", "aiignore", "local model", "Ollama"],
  component: AiAssistantPage,
});

registerStatusItem({ id: "ai.status", align: "right", order: 90, component: AiStatusWidget });

if (typeof document !== "undefined") {
  const host = document.createElement("div");
  host.id = "ai-editor-overlay";
  document.body.appendChild(host);
  render(
    () => (
      <Show when={proposalState()}>
        <ProposalDialog />
      </Show>
    ),
    host,
  );
}
