import { For, Show } from "solid-js";
import { applyReplace, canUndoReplace, closeReplacePreview, openReplacePreview, previewTotals, replaceBusy, replaceMessage, replacePreview, replaceWith, setReplaceWith, undoReplace } from "./replace";
import { t } from "@/i18n";

export function ReplaceBar() {
  const totals = () => previewTotals(replacePreview() ?? []);
  return (
    <>
      <div class="fw-replace">
        <input
          class="fw-pattern"
          placeholder={t("tools.find.replaceWith")}
          value={replaceWith()}
          spellcheck={false}
          onInput={(e) => setReplaceWith(e.currentTarget.value)}
          onKeyDown={(e) => e.key === "Enter" && void openReplacePreview()}
        />
        <button class="btn" disabled={replaceBusy()} onClick={() => void openReplacePreview()}>
          {t("tools.find.replaceEllipsis")}
        </button>
        <button class="btn" disabled={replaceBusy() || !canUndoReplace()} title={t("tools.find.putBackTheFilesThe")} onClick={() => void undoReplace()}>
          {t("tools.find.undoReplace")}
        </button>
        <Show when={replaceMessage()}>
          <span class="fw-note">{replaceMessage()}</span>
        </Show>
      </div>
      <Show when={replacePreview()?.length}>
        <div class="fw-preview" data-testid="replace-preview">
          <div class="fw-preview-head">
            <span>
              {t("tools.find.replaceMatchesInFiles", { matches: totals().matches, files: totals().files })}
            </span>
            <button class="btn primary" disabled={replaceBusy()} onClick={() => void applyReplace()}>
              {t("tools.find.replaceAll")}
            </button>
            <button class="btn" onClick={closeReplacePreview}>
              {t("common.cancel")}
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
