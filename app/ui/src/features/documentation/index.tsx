/**
 * The documentation actions of PHPStorm's View menu: Quick Documentation (Ctrl+Q, a second
 * Ctrl+Q moves it to the Documentation tool window), External Documentation (Shift+F1),
 * Quick Definition (Ctrl+Shift+I), Type Info (Ctrl+Shift+P), Context Info (Alt+Q) and Error
 * Description (Ctrl+F1). Documentation comes from the language servers' hover and the PHP
 * index; URLs, definition snippets, context headers and types are computed in Rust
 * (`code_external_doc`, `code_quick_definition`, `code_context_info`, `code_type_info`).
 */
import { ViewPlugin, type EditorView, type ViewUpdate } from "@codemirror/view";
import { BookOpen, ChevronLeft, ChevronRight, ExternalLink, Pencil, Pin } from "lucide-solid";
import { registerToolWindow } from "@/features/registry";
import { registerEditorExtension } from "@/editor/extensions";
import { codeApi, type QuickDefinition } from "@/ipc/codeintel";
import { registerCommand } from "@/state/commands";
import { isToolWindowVisible, showToolWindow } from "@/state/layout";
import { actionById, shortcutLabel } from "@/keymap";
import { activeView, revealLocation, viewFor } from "@/features/lsp/navigation";
import { diagnosticsAt } from "@/features/lsp/diagnostics";
import { highlightInto } from "@/features/lsp/highlightCode";
import { showHint } from "@/features/lsp/hint";
import { severityIcon, severityLabel } from "@/features/lsp/severity";
import { syncNow } from "@/features/lsp/sync";
import { baseName, h, iconNode, toLsp } from "@/features/lsp/util";
import { gotoDeclaration } from "@/features/lsp/goto";
import { documentationAt, signatureLanguage, type Documentation } from "./docs";
import { closePopup, docPopups, popupOf, showPopup } from "./popup";
import { DocumentationActionsBar, DocumentationView, autoUpdate, setDocumentationActions, setShown } from "./DocumentationView";
import "./documentation.css";

const TOOL_WINDOW = "documentation";

const toolWindowVisible = () => isToolWindowVisible(TOOL_WINDOW);

const withShortcut = (title: string, actionId: string) => {
  const s = shortcutLabel(actionId);
  return s ? `${title} (${s})` : title;
};

/** A toolbar button for popups (plain DOM; keeps the editor focused). */
function popupButton(icon: Parameters<typeof iconNode>[0], title: string, run: () => void): HTMLButtonElement {
  const b = h("button", "icon-btn doc-popup-btn", iconNode(icon, 14));
  b.title = title;
  b.addEventListener("mousedown", (e) => e.preventDefault());
  b.addEventListener("click", (e) => {
    e.stopPropagation();
    run();
  });
  return b;
}

// ---- Quick Documentation / Documentation tool window ----

/** The documentation the Quick Documentation popup shows. */
let popupDoc: Documentation | null = null;

function openInToolWindow(doc: Documentation, view?: EditorView): void {
  setShown(doc);
  showToolWindow(TOOL_WINDOW);
  if (view) closePopup(view);
}

function quickDocDom(doc: Documentation, view: EditorView): HTMLElement {
  const content = h("div", "doc-quick-content", doc.render());
  const toolbar = h(
    "div",
    "doc-quick-toolbar",
    popupButton(Pin, withShortcut("Open in Documentation Tool Window", "QuickJavaDoc"), () => openInToolWindow(doc, view)),
    popupButton(ExternalLink, withShortcut("External Documentation", "ExternalJavaDoc"), () => void externalDocAt(doc.path, view, doc.pos)),
    popupButton(Pencil, withShortcut("Edit Source", "EditSource"), () => {
      closePopup(view);
      void gotoDeclaration(doc.path, view, doc.pos);
    }),
  );
  return h("div", "doc-quick", content, toolbar);
}

export async function quickDocumentationAt(path: string, view: EditorView, pos: number): Promise<void> {
  const open = popupOf(view);
  if (open?.kind === "doc" && popupDoc) {
    // Ctrl+Q a second time: move the documentation to the tool window, like PHPStorm.
    openInToolWindow(popupDoc, view);
    return;
  }
  const doc = await documentationAt(path, view, pos);
  if (!doc) {
    showHint(view, pos, "No documentation found");
    return;
  }
  if (toolWindowVisible()) {
    setShown(doc);
    return;
  }
  popupDoc = doc;
  showPopup(view, { kind: "doc", pos, above: false, build: (v) => quickDocDom(doc, v) });
}

// ---- External Documentation ----

export async function externalDocAt(path: string, view: EditorView, pos: number): Promise<void> {
  await syncNow(path);
  const answer = await codeApi.externalDoc(path, toLsp(view.state.doc, pos), view.state.doc.toString()).catch(() => null);
  if (answer?.url) {
    window.open(answer.url, "_blank", "noopener");
    return;
  }
  showHint(view, pos, "No external documentation found");
}

setDocumentationActions({
  external: (doc) => {
    const view = viewFor(doc.path);
    if (view) void externalDocAt(doc.path, view, doc.pos);
  },
  editSource: (doc) => {
    const view = viewFor(doc.path);
    if (view) void gotoDeclaration(doc.path, view, doc.pos);
  },
});

// ---- Quick Definition ----

function definitionCode(def: QuickDefinition): HTMLElement {
  const lines = def.text.split("\n");
  const gutter = h("div", "doc-def-gutter");
  gutter.textContent = lines.map((_, i) => String(def.startLine + i + 1)).join("\n");
  const code = h("pre", "doc-def-code");
  highlightInto(code, def.text, signatureLanguage(def.path));
  if (def.truncated) code.append(h("span", "doc-def-more", "\n…"));
  return h("div", "doc-def-body", gutter, code);
}

function quickDefinitionDom(defs: QuickDefinition[], view: EditorView): HTMLElement {
  const root = h("div", "doc-def");
  let index = 0;
  const render = () => {
    const def = defs[index];
    const location = h("span", "doc-def-location", `${baseName(def.path)}:${def.line + 1}`);
    location.title = def.path;
    const name = def.name ? h("span", "doc-def-name", def.name) : null;
    const nav =
      defs.length > 1
        ? h(
            "span",
            "doc-def-nav",
            popupButton(ChevronLeft, "Previous Definition", () => ((index = (index + defs.length - 1) % defs.length), render())),
            h("span", "doc-def-count", `${index + 1} of ${defs.length}`),
            popupButton(ChevronRight, "Next Definition", () => ((index = (index + 1) % defs.length), render())),
          )
        : null;
    const edit = popupButton(Pencil, withShortcut("Edit Source", "EditSource"), () => {
      closePopup(view);
      void revealLocation(def.path, { line: def.line, character: def.column });
    });
    const header = h("div", "doc-def-header", name, location, h("span", "doc-spacer"), nav, edit);
    root.replaceChildren(header, definitionCode(def));
  };
  render();
  return root;
}

export async function quickDefinitionAt(path: string, view: EditorView, pos: number): Promise<void> {
  await syncNow(path);
  const answer = await codeApi.quickDefinition(path, toLsp(view.state.doc, pos), view.state.doc.toString()).catch((e) => ({
    definitions: [],
    message: String(e instanceof Error ? e.message : e),
  }));
  if (!answer.definitions.length) {
    showHint(view, pos, answer.message ?? "No definition found");
    return;
  }
  showPopup(view, { kind: "definition", pos, above: false, build: (v) => quickDefinitionDom(answer.definitions, v) });
}

// ---- Type Info, Context Info, Error Description ----

export async function typeInfoAt(path: string, view: EditorView, pos: number): Promise<void> {
  await syncNow(path);
  const type = await codeApi.typeInfo(path, toLsp(view.state.doc, pos), view.state.doc.toString()).catch(() => null);
  if (!type) {
    showHint(view, pos, "No type information available");
    return;
  }
  showPopup(view, { kind: "info", pos, above: true, build: () => h("div", "doc-type", type) });
}

export async function contextInfoAt(path: string, view: EditorView): Promise<void> {
  const head = view.state.selection.main.head;
  const line = view.state.doc.lineAt(head).number - 1;
  const headers = await codeApi.contextInfo(path, line, view.state.doc.toString()).catch(() => []);
  if (!headers.length) {
    showHint(view, head, "No context information available");
    return;
  }
  // Over the top of the editor, like PHPStorm.
  const top = view.lineBlockAtHeight(view.scrollDOM.scrollTop).from;
  const lang = signatureLanguage(path);
  const build = () => {
    const root = h("div", "doc-context");
    for (const header of headers) {
      const code = h("span", "doc-context-code");
      highlightInto(code, header.text, lang);
      root.append(h("div", "doc-context-row", h("span", "doc-context-line", String(header.line + 1)), code));
    }
    return root;
  };
  showPopup(view, { kind: "info", pos: Math.max(top, 0), above: false, build });
}

export function errorDescriptionAt(view: EditorView, pos: number): void {
  const problems = diagnosticsAt(view.state, pos);
  if (!problems.length) {
    showHint(view, pos, "No problems found at the caret");
    return;
  }
  const build = () => {
    const root = h("div", "doc-error");
    for (const p of problems) {
      const origin = [p.source, p.code ? `(${p.code})` : ""].filter(Boolean).join(" ");
      root.append(
        h(
          "div",
          `doc-error-row sev-${p.severity}`,
          iconNode(severityIcon[p.severity], 14, "sev-icon"),
          h(
            "div",
            "doc-error-text",
            h("div", "doc-error-message", p.message),
            h("div", "doc-error-origin", [severityLabel[p.severity], origin].filter(Boolean).join(" · ")),
          ),
        ),
      );
    }
    return root;
  };
  showPopup(view, { kind: "info", pos, above: true, build });
}

// ---- Registration ----

const inEditor = (fn: (path: string, view: EditorView, pos: number) => unknown) => () => {
  const a = activeView();
  if (a) void fn(a.path, a.view, a.view.state.selection.main.head);
};

function action(id: string, fallbackTitle: string, run: () => void) {
  registerCommand({ id, title: actionById(id)?.title ?? fallbackTitle, category: "View", run });
}

action("QuickJavaDoc", "Quick Documentation", inEditor(quickDocumentationAt));
action("ExternalJavaDoc", "External Documentation", inEditor(externalDocAt));
action("QuickImplementations", "Quick Definition", inEditor(quickDefinitionAt));
action("ExpressionTypeInfo", "Type Info", inEditor(typeInfoAt));
action("EditorContextInfo", "Context Info", inEditor((path, view) => contextInfoAt(path, view)));
action("ShowErrorDescription", "Error Description", inEditor((_path, view, pos) => errorDescriptionAt(view, pos)));

/** Auto-Update from Source: the tool window follows the caret of the focused editor. */
function followCaret(path: string) {
  return ViewPlugin.define((view) => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    let seq = 0;
    return {
      update(u: ViewUpdate) {
        if (!u.selectionSet || !u.view.hasFocus || !autoUpdate() || !toolWindowVisible()) return;
        clearTimeout(timer);
        const id = ++seq;
        timer = setTimeout(async () => {
          const doc = await documentationAt(path, view, view.state.selection.main.head).catch(() => null);
          if (doc && id === seq) setShown(doc);
        }, 400);
      },
      destroy() {
        clearTimeout(timer);
      },
    };
  });
}

registerEditorExtension(({ path }) => [docPopups(), followCaret(path)]);

registerToolWindow({
  id: TOOL_WINDOW,
  title: "Documentation",
  icon: BookOpen,
  anchor: "right",
  order: 30,
  component: DocumentationView,
  actions: () => <DocumentationActionsBar />,
});
