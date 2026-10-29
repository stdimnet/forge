/**
 * Show Coverage Data (PHPStorm: "Choose Coverage Suite to Display", Ctrl+Alt+F6): the
 * session's suites with checkboxes. One checked suite is shown as it is; several are merged
 * into one suite in Rust (`coverage_merge`); No Coverage hides the marks. Built on the native
 * <dialog> (Escape cancels, no key listeners).
 */
import { createSignal, For, onMount, Show } from "solid-js";
import { render } from "solid-js/web";
import { FileInput } from "lucide-solid";
import type { CoverageSuite } from "@/ipc/testing";
import { activeSuite, setCoverageVisible, showSuites, suites, visible } from "./store";
import { importReport } from "./actions";

const [open, setOpen] = createSignal(false);
let mounted = false;

/** "Oct 26, 10:41". */
export const when = (at: number) => new Date(at).toLocaleString(undefined, { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });

function Dialog() {
  let dialog!: HTMLDialogElement;
  let ok!: HTMLButtonElement;
  const shown = activeSuite();
  const [checked, setChecked] = createSignal<Set<string>>(new Set(shown && visible() ? [shown.id] : []));
  const toggle = (s: CoverageSuite) =>
    setChecked((c) => {
      const next = new Set(c);
      if (next.has(s.id)) next.delete(s.id);
      else next.add(s.id);
      return next;
    });
  const close = () => setOpen(false);
  const showChecked = async () => {
    // Keep the list's order (newest first), like the dialog shows it.
    const ids = suites()
      .map((s) => s.id)
      .filter((id) => checked().has(id));
    close();
    await showSuites(ids);
  };
  onMount(() => {
    dialog.showModal();
    ok.focus();
  });
  return (
    <dialog
      ref={dialog}
      class="coverage-suites-dialog"
      aria-label="Choose Coverage Suite to Display"
      onCancel={(e) => (e.preventDefault(), close())}
      onMouseDown={(e) => e.target === dialog && close()}
    >
      <form method="dialog" onSubmit={(e) => (e.preventDefault(), void showChecked())}>
        <header class="coverage-dialog-title">Choose Coverage Suite to Display</header>
        <div class="coverage-suites-list" role="list">
          <Show when={suites().length} fallback={<div class="coverage-empty muted">No coverage data yet. Run tests with coverage or import a report.</div>}>
            <For each={suites()}>
              {(s) => (
                <label class="coverage-suite-item" role="listitem" data-suite={s.id}>
                  <input type="checkbox" checked={checked().has(s.id)} onChange={() => toggle(s)} />
                  <span class="coverage-suite-name">
                    <span>{s.name}</span>
                    <span class="muted">
                      {s.format} · {s.files} file{s.files === 1 ? "" : "s"}
                      {s.perTest ? " · per test" : ""}
                    </span>
                  </span>
                  <span class="coverage-suite-pct">{s.lineText}</span>
                  <span class="muted coverage-suite-when">{when(s.at)}</span>
                </label>
              )}
            </For>
          </Show>
        </div>
        <Show when={checked().size > 1}>
          <div class="coverage-suites-note muted">The {checked().size} suites are merged into one.</div>
        </Show>
        <footer class="coverage-dialog-actions">
          <button type="button" class="btn-secondary coverage-dialog-import" onClick={() => (close(), void importReport())}>
            <FileInput size={14} /> Import…
          </button>
          <span class="coverage-spacer" />
          <button type="button" class="btn-secondary" onClick={() => (close(), setCoverageVisible(false))}>
            No Coverage
          </button>
          <button type="button" class="btn-secondary" onClick={close}>
            Cancel
          </button>
          <button type="submit" class="btn-primary" ref={ok} disabled={!checked().size}>
            Show Selected
          </button>
        </footer>
      </form>
    </dialog>
  );
}

function Host() {
  return (
    <Show when={open()}>
      <Dialog />
    </Show>
  );
}

/** Open Show Coverage Data. */
export function showSuitesDialog(): void {
  if (!mounted && typeof document !== "undefined") {
    mounted = true;
    const host = document.createElement("div");
    host.id = "forge-coverage-suites";
    document.body.appendChild(host);
    render(() => <Host />, host);
  }
  setOpen(true);
}
