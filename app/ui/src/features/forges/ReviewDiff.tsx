/**
 * Unified diff of one merge request file with inline review threads. Hovering a line shows
 * a "+" in the gutter that starts a new thread on it.
 */
import { createMemo, createSignal, For, Show } from "solid-js";
import { Plus } from "lucide-solid";
import type { ChangedFile, Discussion, LineKind, Position } from "@/ipc/forges";
import { CommentEditor, ThreadView } from "./Thread";

interface Row {
  kind: LineKind | "hunk";
  text: string;
  oldNo: number | null;
  newNo: number | null;
}

export function diffRows(file: ChangedFile): Row[] {
  const rows: Row[] = [];
  for (const h of file.hunks) {
    rows.push({ kind: "hunk", text: `@@ -${h.oldStart},${h.oldLines} +${h.newStart},${h.newLines} @@ ${h.header}`.trimEnd(), oldNo: null, newNo: null });
    let o = h.oldStart;
    let n = h.newStart;
    for (const l of h.lines) {
      if (l.kind === "added") rows.push({ kind: l.kind, text: l.text, oldNo: null, newNo: n++ });
      else if (l.kind === "removed") rows.push({ kind: l.kind, text: l.text, oldNo: o++, newNo: null });
      else rows.push({ kind: l.kind, text: l.text, oldNo: o++, newNo: n++ });
    }
  }
  return rows;
}

const onFile = (d: Discussion, f: ChangedFile) => !!d.position && (d.position.path === f.path || (!!f.oldPath && d.position.path === f.oldPath));

/** Does a thread belong under this diff row? */
export function threadOnRow(d: Discussion, row: Row): boolean {
  const p = d.position;
  if (!p || row.kind === "hunk") return false;
  if (p.newLine != null) return row.newNo === p.newLine;
  return row.kind !== "added" && row.oldNo === p.oldLine;
}

function positionOf(file: ChangedFile, row: Row): Position {
  return {
    path: file.path,
    oldPath: file.oldPath,
    newLine: row.newNo,
    oldLine: row.kind === "added" ? null : row.oldNo,
  };
}

export function ReviewDiff(props: {
  file: ChangedFile;
  discussions: Discussion[];
  canComment: boolean;
  onComment: (body: string, position: Position) => Promise<boolean>;
  onReply: (d: Discussion, body: string) => Promise<boolean>;
  onResolve?: (d: Discussion, resolved: boolean) => Promise<void>;
}) {
  const rows = createMemo(() => diffRows(props.file));
  const threads = createMemo(() => props.discussions.filter((d) => onFile(d, props.file)));
  const placed = createMemo(() => new Set(threads().filter((d) => rows().some((r) => threadOnRow(d, r))).map((d) => d.id)));
  const outside = () => threads().filter((d) => !placed().has(d.id));
  const [draftAt, setDraftAt] = createSignal<number | null>(null);

  const thread = (d: Discussion) => (
    <ThreadView
      discussion={d}
      onReply={(body) => props.onReply(d, body)}
      onResolve={props.onResolve ? (r) => props.onResolve!(d, r) : undefined}
    />
  );

  return (
    <div class="fg-diff">
      <Show when={outside().length}>
        <div class="fg-diff-outside">
          <div class="muted fg-small">Comments on lines outside the diff</div>
          <For each={outside()}>{thread}</For>
        </div>
      </Show>
      <Show when={props.file.hunks.length} fallback={<div class="fg-empty muted">{props.file.status === "renamed" ? "File renamed without changes" : "No textual changes (binary file or too large to show)"}</div>}>
        <table class="fg-diff-table">
          <tbody>
            <For each={rows()}>
              {(row, i) => (
                <>
                  <tr class={`fg-dl fg-dl-${row.kind}`} data-new-line={row.newNo ?? undefined} data-old-line={row.oldNo ?? undefined}>
                    <Show
                      when={row.kind !== "hunk"}
                      fallback={
                        <td class="fg-dl-hunk" colSpan={4}>
                          {row.text}
                        </td>
                      }
                    >
                      <td class="fg-dl-no">{row.oldNo ?? ""}</td>
                      <td class="fg-dl-no">{row.newNo ?? ""}</td>
                      <td class="fg-dl-gutter">
                        <Show when={props.canComment}>
                          <button class="fg-dl-add" title="Add a comment" onClick={() => setDraftAt(i())}>
                            <Plus size={12} />
                          </button>
                        </Show>
                        <span class="fg-dl-sign">{row.kind === "added" ? "+" : row.kind === "removed" ? "-" : " "}</span>
                      </td>
                      <td class="fg-dl-code">{row.text || " "}</td>
                    </Show>
                  </tr>
                  <For each={threads().filter((d) => threadOnRow(d, row))}>
                    {(d) => (
                      <tr class="fg-dl-thread">
                        <td colSpan={4}>{thread(d)}</td>
                      </tr>
                    )}
                  </For>
                  <Show when={draftAt() === i()}>
                    <tr class="fg-dl-thread">
                      <td colSpan={4}>
                        <div class="fg-thread fg-thread-new">
                          <CommentEditor
                            placeholder={`Comment on line ${row.newNo ?? row.oldNo}…`}
                            submitLabel="Add Comment"
                            rows={3}
                            autofocus
                            onCancel={() => setDraftAt(null)}
                            onSubmit={async (body) => {
                              const ok = await props.onComment(body, positionOf(props.file, row));
                              if (ok) setDraftAt(null);
                              return ok;
                            }}
                          />
                        </div>
                      </td>
                    </tr>
                  </Show>
                </>
              )}
            </For>
          </tbody>
        </table>
      </Show>
    </div>
  );
}
