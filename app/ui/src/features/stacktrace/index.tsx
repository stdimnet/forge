/**
 * Analyze External Stack Trace: Code | Analyze Stack Trace or Thread Dump… (PHPStorm's
 * `AnalyzeStacktraceOnError`). The analysis is Rust (`forge_output::stacktrace`); the result
 * is a "Stack Trace" tab of the Run tool window with clickable frames.
 */
import { createRoot } from "solid-js";
import { render, Show } from "solid-js/web";
import { registerCommand } from "@/state/commands";
import { actionById } from "@/keymap";
import { isDialogWindow } from "@/shell/window/mode";
import { AnalyzeDialog } from "./AnalyzeDialog";
import { analyzeDialog, openAnalyzeDialog } from "./state";
import "./stacktrace.css";

registerCommand({
  id: "AnalyzeStacktraceOnError",
  title: actionById("AnalyzeStacktraceOnError")?.title ?? "Analyze Stack Trace or Thread Dump…",
  category: "Code",
  run: () => void openAnalyzeDialog(),
});

if (typeof document !== "undefined" && !isDialogWindow()) {
  const host = document.createElement("div");
  host.id = "stacktrace-overlay";
  document.body.appendChild(host);
  createRoot(() =>
    render(
      () => (
        <Show when={analyzeDialog()} keyed>
          {(d) => <AnalyzeDialog text={d.text} />}
        </Show>
      ),
      host,
    ),
  );
}
