/**
 * Read-only side-by-side diff inside a panel (the diff preview of a history tab), with the
 * same merge view, line alignment and colors as the diff viewer.
 */
import { createEffect, onCleanup, Show } from "solid-js";
import { EditorState, type Extension } from "@codemirror/state";
import { EditorView, lineNumbers, highlightSpecialChars } from "@codemirror/view";
import { MergeView } from "@codemirror/merge";
import { forgeEditorTheme, forgeHighlight } from "@/editor/forgeTheme";
import { languageFor } from "@/editor/languages";
import { ensureFileType } from "@/editor/fileTypes";
import { lineDiffConfig } from "./lineDiff";
import type { DiffSide } from "./overlays";

const panelTheme = EditorView.theme({
  "&": { height: "auto" },
  ".cm-scroller": { overflow: "visible" },
});

export function InlineDiff(props: { path: string; left: DiffSide; right: DiffSide; empty?: string }) {
  let host!: HTMLDivElement;
  let mv: MergeView | undefined;

  const base = (path: string): Extension[] => [
    lineNumbers(),
    highlightSpecialChars(),
    forgeEditorTheme,
    forgeHighlight,
    panelTheme,
    ...languageFor(path),
    EditorView.editable.of(false),
    EditorState.readOnly.of(true),
  ];

  createEffect(() => {
    const { path, left, right } = props;
    let stale = false;
    onCleanup(() => {
      stale = true;
    });
    void ensureFileType(path).then(() => {
      if (stale) return;
      mv?.destroy();
      mv = new MergeView({
        parent: host,
        a: { doc: left.content, extensions: base(path) },
        b: { doc: right.content, extensions: base(path) },
        highlightChanges: true,
        gutter: true,
        diffConfig: lineDiffConfig,
        collapseUnchanged: { margin: 3, minSize: 6 },
      });
    });
  });
  onCleanup(() => mv?.destroy());

  return (
    <div class="inline-diff">
      <div class="diff-headers">
        <div class="diff-header">{props.left.title}</div>
        <div class="diff-header">{props.right.title}</div>
      </div>
      <Show when={props.empty}>
        <div class="commit-empty">{props.empty}</div>
      </Show>
      <div class="diff-body" ref={host} />
    </div>
  );
}
