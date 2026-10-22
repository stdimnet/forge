/**
 * A text editor with a preview next to it, like PHPStorm's `TextEditorWithPreview` (Markdown,
 * SVG): the three layout buttons in the top-right corner switch between "Editor Only",
 * "Editor and Preview" and "Preview Only". The layout is remembered per file for the session
 * and starts from the default the feature passes (a setting). The actions
 * `TextEditorWithPreview.Layout.*` (Find Action, keymap) switch the active file's layout.
 */
import { createSignal, onCleanup, onMount, Show, type JSX } from "solid-js";
import type { EditorView } from "@codemirror/view";
import { Columns2, Eye, TextAlignStart } from "lucide-solid";
import { activeTab, type EditorTab } from "@/state/editor";
import { registerCommand } from "@/state/commands";
import { registerKeymapActions } from "@/keymap/store";
import { CodeEditor } from "./CodeEditor";
import { visibleView, viewFor } from "./views";
import "./textEditorWithPreview.css";

export type PreviewLayout = "editor" | "split" | "preview";

const [layouts, setLayouts] = createSignal<Record<string, PreviewLayout>>({});

/** Files shown in an editor with preview, with their default layout. */
const withPreview = new Map<string, { count: number; fallback: () => PreviewLayout }>();

export const layoutOf = (path: string, fallback: PreviewLayout): PreviewLayout => layouts()[path] ?? fallback;

export function setPreviewLayout(path: string, layout: PreviewLayout): void {
  setLayouts((l) => ({ ...l, [path]: layout }));
}

/** Whether the active file is shown in an editor with preview. */
export const activeHasPreview = (): boolean => {
  const path = activeTab()?.path;
  return !!path && withPreview.has(path);
};

const LAYOUT_ACTIONS: { id: string; title: string; layout: PreviewLayout }[] = [
  { id: "TextEditorWithPreview.Layout.EditorOnly", title: "Editor Only", layout: "editor" },
  { id: "TextEditorWithPreview.Layout.EditorAndPreview", title: "Editor and Preview", layout: "split" },
  { id: "TextEditorWithPreview.Layout.PreviewOnly", title: "Preview Only", layout: "preview" },
];

registerKeymapActions(LAYOUT_ACTIONS.map((a) => ({ id: a.id, title: a.title, group: "Editor Layout", shortcuts: {} })));
for (const a of LAYOUT_ACTIONS) {
  registerCommand({
    id: a.id,
    title: a.title,
    category: "Editor Layout",
    run: () => {
      const path = activeTab()?.path;
      if (path && withPreview.has(path)) setPreviewLayout(path, a.layout);
    },
  });
}

export interface PreviewContext {
  /** The CodeMirror view of this editor (undefined until it is mounted). */
  editorView: () => EditorView | undefined;
  /** The current layout. */
  layout: () => PreviewLayout;
}

export function TextEditorWithPreview(props: {
  tab: EditorTab;
  group?: number;
  /** Layout of files the user has not switched yet. */
  defaultLayout: () => PreviewLayout;
  preview: (ctx: PreviewContext) => JSX.Element;
  /** Extra buttons left of the layout buttons. */
  toolbar?: JSX.Element;
}) {
  const path = props.tab.path;
  const layout = () => layoutOf(path, props.defaultLayout());
  const [ratio, setRatio] = createSignal(0.5);
  const [view, setView] = createSignal<EditorView | undefined>();
  let container!: HTMLDivElement;

  onMount(() => {
    const entry = withPreview.get(path);
    if (entry) entry.count++;
    else withPreview.set(path, { count: 1, fallback: props.defaultLayout });
    void visibleView(path).then(() => setView(viewFor(path, props.group)));
  });
  onCleanup(() => {
    const entry = withPreview.get(path);
    if (entry && --entry.count <= 0) withPreview.delete(path);
  });

  const editorView = () => view() ?? viewFor(path, props.group);

  const onSplitterDown = (e: PointerEvent) => {
    e.preventDefault();
    const box = container.getBoundingClientRect();
    const move = (ev: PointerEvent) => setRatio(Math.min(0.85, Math.max(0.15, (ev.clientX - box.left) / Math.max(1, box.width))));
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  const button = (l: PreviewLayout, title: string, icon: JSX.Element) => (
    <button
      class="twp-btn"
      classList={{ active: layout() === l }}
      title={title}
      aria-pressed={layout() === l}
      data-layout={l}
      onClick={() => {
        setPreviewLayout(path, l);
        if (l !== "preview") requestAnimationFrame(() => editorView()?.focus());
      }}
    >
      {icon}
    </button>
  );

  return (
    <div class="twp" ref={container} data-layout={layout()}>
      <div
        class="twp-editor"
        classList={{ hidden: layout() === "preview" }}
        style={{ "flex-basis": layout() === "split" ? `${ratio() * 100}%` : undefined }}
      >
        <CodeEditor tab={props.tab} group={props.group} />
      </div>
      <Show when={layout() === "split"}>
        <div class="twp-splitter" onPointerDown={onSplitterDown} />
      </Show>
      <Show when={layout() !== "editor"}>
        <div class="twp-preview">{props.preview({ editorView, layout })}</div>
      </Show>
      <div class="twp-toolbar" role="toolbar" aria-label="Editor layout">
        {props.toolbar}
        {button("editor", "Editor Only", <TextAlignStart size={14} />)}
        {button("split", "Editor and Preview", <Columns2 size={14} />)}
        {button("preview", "Preview Only", <Eye size={14} />)}
      </div>
    </div>
  );
}
