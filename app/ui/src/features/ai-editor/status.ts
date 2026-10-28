/**
 * The AI state of the active editor (`ai_editor_status`): whether AI is on for the project,
 * the providers of inline completion and of the actions, and whether the file may be sent
 * (`.aiignore`, per-language switch). Refreshed when the active file or an AI setting changes.
 */
import { createEffect, createRoot, createSignal, on } from "solid-js";
import { aiEditorApi, type AiEditorStatus } from "@/ipc/aiEditor";
import { editor } from "@/state/editor";
import { getSetting } from "@/state/settings";
import { workspace } from "@/state/workspace";

const [status, setStatus] = createSignal<AiEditorStatus | null>(null);
export { status as aiStatus };

let seq = 0;

export async function refreshAiStatus(): Promise<AiEditorStatus | null> {
  const id = ++seq;
  const path = editor.active && !editor.active.includes("://") ? editor.active : undefined;
  try {
    const s = await aiEditorApi.status(path);
    if (id === seq) setStatus(s);
    return s;
  } catch {
    if (id === seq) setStatus(null);
    return null;
  }
}

/** Whether the AI actions may run for `path` (they still fail with a hint without a provider). */
export async function actionsAllowed(path: string): Promise<boolean> {
  const s = await aiEditorApi.status(path).catch(() => null);
  return !!s && s.projectEnabled && s.fileActions?.status === "allowed";
}

if (typeof window !== "undefined") {
  createRoot(() =>
    createEffect(
      on(
        () => [
          editor.active,
          workspace()?.root,
          getSetting<boolean>("ai.enabled", true),
          getSetting<boolean>("ai.completion.enabled", false),
          getSetting<string>("ai.completion.mode", "auto"),
          JSON.stringify(getSetting<string[]>("ai.completion.disabledLanguages", [])),
        ],
        () => void refreshAiStatus(),
      ),
    ),
  );
}
