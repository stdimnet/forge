import { For, Show } from "solid-js";
import { applyReplace, canUndoReplace, closeReplacePreview, openReplacePreview, previewTotals, replaceBusy, replaceMessage, replacePreview, replaceWith, setReplaceWith, undoReplace } from "./replace";

export function ReplaceBar() {
  const totals = () => previewTotals(replacePreview() ?? []);
  return (
    <>
      <div class="fw-replace">
        <input
          class="fw-pattern"
          placeholder="Replace with"
          value={replaceWith()}
          spellcheck={false}
          onInput={(e) => setReplaceWith(e.currentTarget.value)}
          onKeyDown={(e) => e.key === "Enter" && void openReplacePreview()}
        />
        <button class="btn" disabled={replaceBusy()} onClick={() => void openReplacePreview()}>
          Replace…
        </button>
        <button class="btn" disabled={replaceBusy() || !canUndoReplace()} title="Put back the files the last replace changed" onClick={() => void undoReplace()}>
          Undo replace
        </button>
        <Show when={replaceMessage()}>
          <span class="fw-note">{replaceMessage()}</span>
        </Show>
      </div>
      <Show when={replacePreview()?.length}>
        <div class="fw-preview" data-testid="replace-preview">
          <div class="fw-preview-head">
            <span>
              Replace {totals().matches} matches in {totals().files} files
            </span>
            <button class="btn primary" disabled={replaceBusy()} onClick={() => void applyReplace()}>
              Replace all
            </button>
            <button class="btn" onClick={closeReplacePreview}>
              Cancel
            </button>
          </div>
          <div class="fw-preview-body">
            <For each={replacePreview() ?? []}>
              {(file) => (
                <div class="fw-preview-file">
                  <div class="fw-preview-path">
                    {file.path} <span class="muted">{file.matches}</span>
                  </div>
                  <For each={file.lines}>
                    {(l) => (
                      <div class="fw-preview-line">
                        <span class="fw-line">{l.line + 1}</span>
                        <span class="fw-old">{l.before}</span>
                        <span class="fw-new">{l.after}</span>
                      </div>
                    )}
                  </For>
                </div>
              )}
            </For>
          </div>
        </div>
      </Show>
    </>
  );
}
