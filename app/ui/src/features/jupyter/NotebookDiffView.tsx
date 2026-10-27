/**
 * Notebook diffs in the diff viewer (Git, Local History, Compare): cell by cell instead of
 * JSON, with the source and the outputs of a changed cell compared separately. The diff is
 * computed in Rust (`jupyter_diff`); runs of unchanged cells fold away.
 */
import { createResource, createSignal, For, Show } from "solid-js";
import { ChevronDown, ChevronRight } from "lucide-solid";
import { jupyterApi, type CellDiff, type CellSide, type TextChange } from "@/ipc/jupyter";
import type { DiffRequest } from "@/features/git/overlays";

const STATUS_LABEL: Record<CellDiff["status"], string> = {
  unchanged: "Unchanged",
  modified: "Modified",
  added: "Added",
  removed: "Deleted",
};

function sideLabel(s: CellSide | undefined): string {
  if (!s) return "";
  const kind = s.cellType === "code" ? "Code" : s.cellType === "markdown" ? "Markdown" : "Raw";
  const count = s.cellType === "code" && s.executionCount != null ? ` · In [${s.executionCount}]` : "";
  return `Cell ${s.index + 1} · ${kind}${count}`;
}

function Lines(props: { change: TextChange; title: string; showEqual?: boolean }) {
  return (
    <div class="nbd-section">
      <div class="nbd-section-title">
        {props.title}
        <Show when={props.change.changed}>
          <span class="nbd-counts">
            <span class="nbd-plus">+{props.change.inserted}</span> <span class="nbd-minus">−{props.change.deleted}</span>
          </span>
        </Show>
      </div>
      <div class="nbd-lines">
        <For each={props.change.lines}>
          {(l) => (
            <div class={`nbd-line ${l.kind}`}>
              <span class="nbd-no">{l.left ?? ""}</span>
              <span class="nbd-no">{l.right ?? ""}</span>
              <span class="nbd-sign">{l.kind === "insert" ? "+" : l.kind === "delete" ? "−" : " "}</span>
              <span class="nbd-text">{l.text || "​"}</span>
            </div>
          )}
        </For>
      </div>
    </div>
  );
}

function CellCard(props: { cell: CellDiff }) {
  const c = () => props.cell;
  const notes = () =>
    [c().typeChanged && "cell type changed", c().executionCountChanged && "execution count changed", c().metadataChanged && "metadata changed"].filter(
      Boolean,
    ) as string[];
  return (
    <div class={`nbd-cell ${c().status}`} data-testid="notebook-diff-cell">
      <div class="nbd-head">
        <span class={`nbd-badge ${c().status}`}>{STATUS_LABEL[c().status]}</span>
        <span class="nbd-side">{sideLabel(c().left)}</span>
        <Show when={c().left && c().right}>
          <span class="muted">→</span>
        </Show>
        <span class="nbd-side">{sideLabel(c().right)}</span>
        <span class="spacer" />
        <span class="muted nbd-notes">{notes().join(", ")}</span>
      </div>
      <Show when={c().status !== "modified" || c().source.changed}>
        <Lines change={c().source} title="Source" />
      </Show>
      <Show when={c().outputs.changed}>
        <Lines change={c().outputs} title="Outputs" />
      </Show>
    </div>
  );
}

/** A run of unchanged cells, folded. */
function Unchanged(props: { cells: CellDiff[] }) {
  const [open, setOpen] = createSignal(false);
  return (
    <div class="nbd-fold">
      <button class="nbd-fold-btn" onClick={() => setOpen(!open())}>
        {open() ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
        {props.cells.length} unchanged {props.cells.length === 1 ? "cell" : "cells"}
      </button>
      <Show when={open()}>
        <For each={props.cells}>{(c) => <CellCard cell={c} />}</For>
      </Show>
    </div>
  );
}

/** Cells grouped into changed cells and folded runs of unchanged ones. */
export function groupCells(cells: CellDiff[]): (CellDiff | CellDiff[])[] {
  const out: (CellDiff | CellDiff[])[] = [];
  for (const c of cells) {
    const last = out[out.length - 1];
    if (c.status === "unchanged") {
      if (Array.isArray(last)) last.push(c);
      else out.push([c]);
    } else out.push(c);
  }
  return out;
}

export function NotebookDiffView(props: { request: DiffRequest }) {
  const [diff] = createResource(
    () => [props.request.left.content, props.request.right.content] as const,
    ([l, r]) => jupyterApi.diff(l, r),
  );
  return (
    <div class="nbd" data-testid="notebook-diff">
      <Show when={diff.error}>
        <div class="nbd-error">Cannot compare the notebooks cell by cell: {String(diff.error).replace(/^Error: /, "")}. Switch to Text.</div>
      </Show>
      <Show when={diff.latest}>
        {(d) => (
          <>
            <div class="nbd-summary">
              <span class="nbd-badge modified">{d().stats.modified} modified</span>
              <span class="nbd-badge added">{d().stats.added} added</span>
              <span class="nbd-badge removed">{d().stats.removed} deleted</span>
              <span class="muted">{d().stats.unchanged} unchanged</span>
              <Show when={d().metadataChanged}>
                <span class="muted">· notebook metadata changed</span>
              </Show>
            </div>
            <For each={groupCells(d().cells)}>{(g) => (Array.isArray(g) ? <Unchanged cells={g} /> : <CellCard cell={g} />)}</For>
          </>
        )}
      </Show>
    </div>
  );
}
