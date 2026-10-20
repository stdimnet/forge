import { createMemo, For, onCleanup, Show } from "solid-js";
import { Portal } from "solid-js/web";
import { ExternalLink, X } from "lucide-solid";
import { openFile } from "@/state/editor";
import { FileIcon } from "@/shell/FileIcon";
import type { DiffLine, EditDiff } from "@/ipc/agent";
import { highlight, langOfPath } from "./highlight";
import { HighlightedCode } from "./Markdown";

/** Lines within `context` of a change (computed by the backend); skipped runs become gaps. */
function visibleLines(lines: DiffLine[], context: number): (DiffLine | null)[] {
  const out: (DiffLine | null)[] = [];
  for (const l of lines) {
    if (l.near <= context) out.push(l);
    else if (out.length && out[out.length - 1] !== null) out.push(null);
  }
  if (out[out.length - 1] === null) out.pop();
  return out;
}

export function DiffView(props: { diff: EditDiff; path: string; maxLines?: number; context?: number }) {
  const lang = () => langOfPath(props.path);
  const rows = createMemo(() => {
    const all: (DiffLine | null)[] = [];
    props.diff.hunks.forEach((h, i) => {
      if (i > 0) all.push(null);
      all.push(...visibleLines(h, props.context ?? 2));
    });
    return all;
  });
  const shown = () => (props.maxLines ? rows().slice(0, props.maxLines) : rows());
  const hidden = () => rows().length - shown().length;
  return (
    <div class="chat-diff">
      <For each={shown()}>
        {(row) =>
          row === null ? (
            <div class="chat-diff-gap">⋯</div>
          ) : (
            <div class={`chat-diff-line ${row.type}`}>
              <span class="chat-diff-sign">{row.type === "add" ? "+" : row.type === "del" ? "−" : " "}</span>
              <span class="chat-diff-text">
                <HighlightedCode lines={highlight(row.text, lang()).slice(0, 1)} />
              </span>
            </div>
          )
        }
      </For>
      <Show when={hidden() > 0}>
        <div class="chat-diff-more">{hidden()} more lines</div>
      </Show>
    </div>
  );
}

/** Full-size diff dialog ("Open diff"). */
export function DiffDialog(props: { path: string; diff: EditDiff; onClose: () => void }) {
  const name = () => props.path.split("/").pop() ?? props.path;
  const onKey = (e: KeyboardEvent) => e.key === "Escape" && props.onClose();
  document.addEventListener("keydown", onKey);
  onCleanup(() => document.removeEventListener("keydown", onKey));
  return (
    <Portal>
      <div class="chat-dialog-backdrop" onClick={props.onClose}>
        <div class="chat-dialog" role="dialog" aria-label={`Diff of ${props.path}`} onClick={(e) => e.stopPropagation()}>
          <header class="chat-dialog-header">
            <FileIcon name={name()} size={15} />
            <span class="chat-dialog-title">{name()}</span>
            <span class="muted chat-dialog-path">{props.path}</span>
            <span class="chat-flex" />
            <button
              class="btn-secondary"
              onClick={() => {
                void openFile(props.path).catch(() => undefined);
                props.onClose();
              }}
            >
              <ExternalLink size={13} /> Open file
            </button>
            <button class="icon-btn" title="Close (Esc)" onClick={props.onClose}>
              <X size={15} />
            </button>
          </header>
          <div class="chat-dialog-body">
            <DiffView diff={props.diff} path={props.path} context={4} />
          </div>
        </div>
      </div>
    </Portal>
  );
}
