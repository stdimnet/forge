import { createMemo, For, onCleanup, Show } from "solid-js";
import { Portal } from "solid-js/web";
import { ExternalLink, X } from "lucide-solid";
import { openFile } from "@/state/editor";
import { FileIcon } from "@/shell/FileIcon";
import { diffLines, type DiffLine } from "./diff";
import { highlight, langOfPath } from "./highlight";
import { HighlightedCode } from "./Markdown";

export interface Hunk {
  oldText: string;
  newText: string;
}

/** Keep changed lines plus `context` lines around them; gaps become separators. */
function trimContext(lines: DiffLine[], context: number): (DiffLine | null)[] {
  const keep = lines.map(() => false);
  lines.forEach((l, i) => {
    if (l.type === "ctx") return;
    for (let k = Math.max(0, i - context); k <= Math.min(lines.length - 1, i + context); k++) keep[k] = true;
  });
  const out: (DiffLine | null)[] = [];
  lines.forEach((l, i) => {
    if (keep[i]) out.push(l);
    else if (out.length && out[out.length - 1] !== null) out.push(null);
  });
  if (out[out.length - 1] === null) out.pop();
  return out;
}

export function DiffView(props: { hunks: Hunk[]; path: string; maxLines?: number; context?: number }) {
  const lang = () => langOfPath(props.path);
  const rows = createMemo(() => {
    const all: (DiffLine | null)[] = [];
    props.hunks.forEach((h, i) => {
      if (i > 0) all.push(null);
      all.push(...trimContext(diffLines(h.oldText, h.newText), props.context ?? 2));
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
export function DiffDialog(props: { path: string; hunks: Hunk[]; onClose: () => void }) {
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
            <DiffView hunks={props.hunks} path={props.path} context={4} />
          </div>
        </div>
      </div>
    </Portal>
  );
}
