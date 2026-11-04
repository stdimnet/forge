/**
 * PHPStorm's Analyze Stack Trace dialog (Code | Analyze Stack Trace or Thread Dump…): paste a
 * trace from a log, a bug report or a server; "Analyze" opens it in a "Stack Trace" tab of
 * the Run tool window with its frames linked to project files (`stacktrace_analyze`).
 */
import { createSignal, onMount } from "solid-js";
import { balloonsFor } from "@/keymap";
import { stacktraceApi } from "@/ipc/stacktrace";
import { openOutputTab } from "@/features/run/runStore";
import { closeAnalyzeDialog, openAnalyzeDialog } from "./state";
import { t } from "@/i18n";

const showBalloon = balloonsFor("Run");

/** Analyze `text` and show it in the Run tool window. */
export async function analyzeStackTrace(text: string): Promise<boolean> {
  try {
    const a = await stacktraceApi.analyze(text);
    openOutputTab({ name: "Stack Trace", type: "stacktrace", lines: a.lines, rerun: () => openAnalyzeDialog(text) });
    if (a.frames > 0 && a.resolved === 0) showBalloon(t("tools.stacktrace.noFrameOfTheStack"), t("tools.stacktrace.checkThePathMappingsOf"));
    return true;
  } catch (e) {
    showBalloon(t("tools.stacktrace.cannotAnalyzeTheStackTrace"), String(e instanceof Error ? e.message : e));
    return false;
  }
}

export function AnalyzeDialog(props: { text: string }) {
  let area!: HTMLTextAreaElement;
  const [text, setText] = createSignal(props.text);
  const [busy, setBusy] = createSignal(false);
  onMount(() => {
    area.focus();
    area.select();
  });
  const analyze = async () => {
    if (!text().trim() || busy()) return;
    setBusy(true);
    try {
      if (await analyzeStackTrace(text())) closeAnalyzeDialog();
    } finally {
      setBusy(false);
    }
  };
  return (
    <div class="popup-backdrop modal" onMouseDown={(e) => e.target === e.currentTarget && closeAnalyzeDialog()}>
      <div
        class="dialog stacktrace-dialog"
        role="dialog"
        aria-label={t("tools.stacktrace.analyzeStackTrace")}
        onKeyDown={(e) => {
          if (e.key === "Escape") closeAnalyzeDialog();
          else if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) void analyze();
          else return;
          e.preventDefault();
          e.stopPropagation();
        }}
      >
        <div class="dialog-title">{t("tools.stacktrace.analyzeStackTrace")}</div>
        <div class="dialog-body stacktrace-body">
          <label class="form-label" for="stacktrace-text">
            {t("tools.stacktrace.putAStackTraceHereLabel")}
          </label>
          <textarea
            id="stacktrace-text"
            ref={area}
            class="stacktrace-text"
            spellcheck={false}
            wrap="off"
            value={text()}
            placeholder={t("tools.stacktrace.n0VarWwwHtmlApp")}
            onInput={(e) => setText(e.currentTarget.value)}
          />
          <div class="muted stacktrace-hint">
            {t("tools.stacktrace.phpLaravelSymfonyAndXdebug")}
          </div>
        </div>
        <div class="dialog-buttons">
          <button class="btn-secondary" onClick={closeAnalyzeDialog}>
            {t("common.cancel")}
          </button>
          <button class="btn-primary" disabled={!text().trim() || busy()} onClick={() => void analyze()}>
            {t("tools.stacktrace.analyze")}
          </button>
        </div>
      </div>
    </div>
  );
}
