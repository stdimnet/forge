/**
 * Unified diff of one merge request file with inline review threads, pending review
 * comments and AI Review findings. Hovering a line shows a "+" in the gutter that starts a
 * comment on it; Shift+click on a second "+" extends it to a range. Hunk headers offer
 * "Explain this change".
 */
import { createMemo, createSignal, For, Show } from "solid-js";
import { Plus, Sparkles, X } from "lucide-solid";
import type { ChangedFile, Discussion, LineKind, Position } from "@/ipc/forges";
import type { Draft, ExplainResult, Finding } from "@/ipc/codeReview";
import { CommentEditor, Markdown, ThreadView } from "./Thread";
import { DraftCard, FindingCard, ProviderLine } from "./ReviewCards";
import { highlightLines, parserFor, type Segment } from "./highlight";

interface Row {
  kind: LineKind | "hunk";
  text: string;
  oldNo: number | null;
  newNo: number | null;
  segs?: Segment[];
  /** Index of the hunk (also set on its header row). */
  hunk: number;
}

/** Table rows of a file diff; with `highlight`, code lines carry syntax-colored segments. */
export function diffRows(file: ChangedFile, highlight = false): Row[] {
  const rows: Row[] = [];
  const parser = highlight ? parserFor(file.path) : null;
  file.hunks.forEach((h, hunk) => {
    rows.push({ kind: "hunk", text: `@@ -${h.oldStart},${h.oldLines} +${h.newStart},${h.newLines} @@ ${h.header}`.trimEnd(), oldNo: null, newNo: null, hunk });
    let o = h.oldStart;
    let n = h.newStart;
    const oldSide = highlightLines(h.lines.filter((l) => l.kind !== "added").map((l) => l.text), parser);
    const newSide = highlightLines(h.lines.filter((l) => l.kind !== "removed").map((l) => l.text), parser);
    let oi = 0;
    let ni = 0;
    for (const l of h.lines) {
      if (l.kind === "added") rows.push({ kind: l.kind, text: l.text, oldNo: null, newNo: n++, segs: newSide[ni++], hunk });
      else if (l.kind === "removed") rows.push({ kind: l.kind, text: l.text, oldNo: o++, newNo: null, segs: oldSide[oi++], hunk });
      else {
        rows.push({ kind: l.kind, text: l.text, oldNo: o++, newNo: n++, segs: newSide[ni++], hunk });
        oi++;
      }
    }
  });
  return rows;
}

const onFile = (p: Position | null | undefined, f: ChangedFile) => !!p && (p.path === f.path || (!!f.oldPath && p.path === f.oldPath));

/** Is `p` the line of this diff row? */
export function positionOnRow(p: Position | null | undefined, row: Row): boolean {
  if (!p || row.kind === "hunk") return false;
  if (p.newLine != null) return row.newNo === p.newLine;
  return row.kind !== "added" && row.oldNo === p.oldLine;
}

/** Does a thread belong under this diff row? */
export const threadOnRow = (d: Discussion, row: Row) => positionOnRow(d.position, row);

function positionOf(file: ChangedFile, row: Row): Position {
  return {
    path: file.path,
    oldPath: file.oldPath,
    newLine: row.newNo,
    oldLine: row.kind === "added" ? null : row.oldNo,
  };
}

/** The first line of a range from row `from` to row `to` (same side), or null. */
export function rangeStart(rows: Row[], from: number | null, to: number): number | null {
  if (from == null || from === to) return null;
  const a = rows[Math.min(from, to)];
  const b = rows[Math.max(from, to)];
  if (a.hunk !== b.hunk) return null;
  const line = (r: Row) => (b.newNo != null ? r.newNo : r.oldNo);
  return line(a) ?? null;
}

export interface ReviewDiffProps {
  file: ChangedFile;
  discussions: Discussion[];
  canComment: boolean;
  onComment: (body: string, position: Position) => Promise<boolean>;
  onReply: (d: Discussion, body: string) => Promise<boolean>;
  onResolve?: (d: Discussion, resolved: boolean) => Promise<void>;
  /** Review mode: pending comments and AI findings of this request. */
  drafts?: Draft[];
  findings?: Finding[];
  onDraft?: (body: string, position: Position, startLine: number | null) => Promise<boolean>;
  onEditDraft?: (d: Draft, body: string) => Promise<boolean>;
  onDeleteDraft?: (d: Draft) => void;
  onFinding?: (f: Finding, action: "draft" | "dismiss") => void;
  /** "Explain this change"; absent when AI cannot be used for this file. */
  onExplain?: (hunk: number) => Promise<ExplainResult | null>;
}

export function ReviewDiff(props: ReviewDiffProps) {
  const rows = createMemo(() => diffRows(props.file, true));
  const threads = createMemo(() => props.discussions.filter((d) => onFile(d.position, props.file)));
  const placed = createMemo(() => new Set(threads().filter((d) => rows().some((r) => threadOnRow(d, r))).map((d) => d.id)));
  const outside = () => threads().filter((d) => !placed().has(d.id));
  const drafts = createMemo(() => (props.drafts ?? []).filter((d) => onFile(d.position, props.file)));
  const findings = createMemo(() => (props.findings ?? []).filter((f) => f.path === props.file.path));
  const looseFindings = () => findings().filter((f) => !rows().some((r) => positionOnRow(f.position, r)));
  const [draftAt, setDraftAt] = createSignal<number | null>(null);
  const [rangeFrom, setRangeFrom] = createSignal<number | null>(null);
  const [explained, setExplained] = createSignal<Record<number, ExplainResult | "loading">>({});

  const inRange = (i: number) => {
    const at = draftAt();
    const from = rangeFrom();
    return at != null && from != null && i >= Math.min(at, from) && i <= Math.max(at, from) && rows()[i].hunk === rows()[at].hunk;
  };

  const startComment = (i: number, e: MouseEvent) => {
    if (e.shiftKey && draftAt() != null && rows()[draftAt()!].hunk === rows()[i].hunk) {
      setRangeFrom(draftAt());
      setDraftAt(i);
      return;
    }
    setRangeFrom(null);
    setDraftAt(i);
  };
  const closeEditor = () => {
    setDraftAt(null);
    setRangeFrom(null);
  };
  const editorTarget = () => {
    const at = draftAt()!;
    const r = rows();
    const lo = Math.min(at, rangeFrom() ?? at);
    const hi = Math.max(at, rangeFrom() ?? at);
    return { row: r[hi], start: rangeStart(r, lo === hi ? null : lo, hi), label: lo === hi ? `line ${r[hi].newNo ?? r[hi].oldNo}` : `lines ${r[lo].newNo ?? r[lo].oldNo}–${r[hi].newNo ?? r[hi].oldNo}` };
  };

  const explain = async (hunk: number) => {
    if (!props.onExplain) return;
    setExplained((m) => ({ ...m, [hunk]: "loading" }));
    const res = await props.onExplain(hunk);
    setExplained((m) => {
      const n = { ...m };
      if (res) n[hunk] = res;
      else delete n[hunk];
      return n;
    });
  };
  const closeExplain = (hunk: number) =>
    setExplained((m) => {
      const n = { ...m };
      delete n[hunk];
      return n;
    });

  const thread = (d: Discussion) => (
    <ThreadView
      discussion={d}
      onReply={(body) => props.onReply(d, body)}
      onResolve={props.onResolve ? (r) => props.onResolve!(d, r) : undefined}
    />
  );
  const draftCard = (d: Draft) => <DraftCard draft={d} onSave={props.onEditDraft} onDelete={props.onDeleteDraft} />;
  const findingCard = (f: Finding) => <FindingCard finding={f} onAction={props.onFinding} />;

  return (
    <div class="fg-diff">
      <Show when={outside().length || looseFindings().length}>
        <div class="fg-diff-outside">
          <div class="muted fg-small">Comments on lines outside the diff</div>
          <For each={outside()}>{thread}</For>
          <For each={looseFindings()}>{findingCard}</For>
        </div>
      </Show>
      <Show when={props.file.hunks.length} fallback={<div class="fg-empty muted">{props.file.status === "renamed" ? "File renamed without changes" : "No textual changes (binary file or too large to show)"}</div>}>
        <table class="fg-diff-table">
          <tbody>
            <For each={rows()}>
              {(row, i) => (
                <>
                  <tr class={`fg-dl fg-dl-${row.kind}`} classList={{ "fg-dl-range": inRange(i()) }} data-new-line={row.newNo ?? undefined} data-old-line={row.oldNo ?? undefined}>
                    <Show
                      when={row.kind !== "hunk"}
                      fallback={
                        <td class="fg-dl-hunk" colSpan={4}>
                          <div class="fg-hunk-row">
                            <span class="fg-hunk-text">{row.text}</span>
                            <Show when={props.onExplain}>
                              <button class="fg-hunk-explain" title="Explain this change with AI" data-action="explain-hunk" data-hunk={row.hunk} onClick={() => void explain(row.hunk)}>
                                <Sparkles size={12} />
                                Explain
                              </button>
                            </Show>
                          </div>
                        </td>
                      }
                    >
                      <td class="fg-dl-no">{row.oldNo ?? ""}</td>
                      <td class="fg-dl-no">{row.newNo ?? ""}</td>
                      <td class="fg-dl-gutter">
                        <Show when={props.canComment}>
                          <button class="fg-dl-add" title="Add a comment (Shift+click: extend to a range)" onClick={(e) => startComment(i(), e)}>
                            <Plus size={12} />
                          </button>
                        </Show>
                        <span class="fg-dl-sign">{row.kind === "added" ? "+" : row.kind === "removed" ? "-" : " "}</span>
                      </td>
                      <td class="fg-dl-code">
                        <Show when={row.segs && row.text} fallback={row.text || " "}>
                          <For each={row.segs}>{(s) => (s.cls ? <span class={s.cls}>{s.text}</span> : s.text)}</For>
                        </Show>
                      </td>
                    </Show>
                  </tr>
                  <Show when={row.kind === "hunk" && explained()[row.hunk]}>
                    {(res) => (
                      <tr class="fg-dl-thread">
                        <td colSpan={4}>
                          <div class="fg-explain" data-explain={row.hunk}>
                            <div class="fg-explain-head">
                              <Sparkles size={13} />
                              <span>Explain this change</span>
                              <span class="spacer" />
                              <button class="icon-btn fg-tiny-btn" title="Close" onClick={() => closeExplain(row.hunk)}>
                                <X size={13} />
                              </button>
                            </div>
                            <Show when={res() !== "loading"} fallback={<div class="muted fg-small">Asking the AI…</div>}>
                              <Markdown text={(res() as ExplainResult).text} />
                              <ProviderLine provider={(res() as ExplainResult).provider} model={(res() as ExplainResult).model} />
                            </Show>
                          </div>
                        </td>
                      </tr>
                    )}
                  </Show>
                  <For each={threads().filter((d) => threadOnRow(d, row))}>
                    {(d) => (
                      <tr class="fg-dl-thread">
                        <td colSpan={4}>{thread(d)}</td>
                      </tr>
                    )}
                  </For>
                  <For each={drafts().filter((d) => positionOnRow(d.position, row))}>
                    {(d) => (
                      <tr class="fg-dl-thread">
                        <td colSpan={4}>{draftCard(d)}</td>
                      </tr>
                    )}
                  </For>
                  <For each={findings().filter((f) => positionOnRow(f.position, row))}>
                    {(f) => (
                      <tr class="fg-dl-thread">
                        <td colSpan={4}>{findingCard(f)}</td>
                      </tr>
                    )}
                  </For>
                  <Show when={draftAt() === i()}>
                    <tr class="fg-dl-thread">
                      <td colSpan={4}>
                        <div class="fg-thread fg-thread-new">
                          <CommentEditor
                            placeholder={`Comment on ${editorTarget().label}…`}
                            submitLabel={props.onDraft ? ((props.drafts ?? []).length ? "Add Review Comment" : "Start Review") : "Add Comment"}
                            hint={props.onDraft ? "Shift+click another + for a range · Ctrl+Enter adds to the review" : undefined}
                            rows={3}
                            autofocus
                            onCancel={closeEditor}
                            secondary={
                              props.onDraft
                                ? {
                                    label: "Add Single Comment",
                                    onSubmit: async (body) => {
                                      const ok = await props.onComment(body, positionOf(props.file, editorTarget().row));
                                      if (ok) closeEditor();
                                      return ok;
                                    },
                                  }
                                : undefined
                            }
                            onSubmit={async (body) => {
                              const t = editorTarget();
                              const ok = props.onDraft ? await props.onDraft(body, positionOf(props.file, t.row), t.start) : await props.onComment(body, positionOf(props.file, t.row));
                              if (ok) closeEditor();
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
