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

const showBalloon = balloonsFor("Run");

/** Analyze `text` and show it in the Run tool window. */
export async function analyzeStackTrace(text: string): Promise<boolean> {
  try {
    const a = await stacktraceApi.analyze(text);
    openOutputTab({ name: "Stack Trace", type: "stacktrace", lines: a.lines, rerun: () => openAnalyzeDialog(text) });
    if (a.frames > 0 && a.resolved === 0) showBalloon("No frame of the stack trace is a project file", "Check the path mappings of the PHP servers or Docker Compose bind mounts.");
    return true;
  } catch (e) {
    showBalloon("Cannot analyze the stack trace", String(e instanceof Error ? e.message : e));
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
        aria-label="Analyze Stack Trace"
        onKeyDown={(e) => {
          if (e.key === "Escape") closeAnalyzeDialog();
          else if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) void analyze();
          else return;
          e.preventDefault();
          e.stopPropagation();
        }}
      >
        <div class="dialog-title">Analyze Stack Trace</div>
        <div class="dialog-body stacktrace-body">
          <label class="form-label" for="stacktrace-text">
            Put a stack trace here:
          </label>
          <textarea
            id="stacktrace-text"
            ref={area}
            class="stacktrace-text"
            spellcheck={false}
            wrap="off"
            value={text()}
            placeholder={"#0 /var/www/html/app/Http/Controllers/OrderController.php(42): App\\Models\\Order::findOrFail()\n    at Cart.add (/app/src/cart.ts:12:9)\nmain.lua:12: in function 'load'"}
            onInput={(e) => setText(e.currentTarget.value)}
          />
          <div class="muted stacktrace-hint">
            PHP, Laravel, Symfony and Xdebug traces, JavaScript / Node and Lua. Server and container paths are mapped with the
            PHP servers' path mappings and Docker Compose bind mounts.
          </div>
        </div>
        <div class="dialog-buttons">
          <button class="btn-secondary" onClick={closeAnalyzeDialog}>
            Cancel
          </button>
          <button class="btn-primary" disabled={!text().trim() || busy()} onClick={() => void analyze()}>
            Analyze
          </button>
        </div>
      </div>
    </div>
  );
}
