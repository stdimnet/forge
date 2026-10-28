/**
 * Status bar widget of AI in the editor (like JetBrains AI Assistant's): a sparkle that
 * shows whether inline completion is on for the file and when a suggestion is being asked.
 * Click: enable or disable inline completion (globally or for the file's language) and
 * open the settings.
 */
import { Show } from "solid-js";
import { LoaderCircle, Sparkles } from "lucide-solid";
import { showContextMenu, item, separator, type MenuEntry } from "@/features/contextMenu/state";
import { openSettings } from "@/features/settings/dialogState";
import { getSetting, saveSetting } from "@/state/settings";
import { inlineActivity, inlineError } from "./inline";
import { aiStatus, refreshAiStatus } from "./status";

function describe(): { on: boolean; text: string } {
  const s = aiStatus();
  if (!s) return { on: false, text: "AI Assistant" };
  if (!s.projectEnabled) return { on: false, text: "AI features are turned off for this project" };
  if (!s.completionEnabled) return { on: false, text: "Inline completion is turned off" };
  if (!s.completion) return { on: false, text: `Inline completion: ${s.completionProblem ?? "no provider"}` };
  if (s.file && s.file.status !== "allowed") return { on: false, text: s.file.message };
  const p = s.completion;
  const where = p.local ? "local" : "hosted";
  return { on: true, text: `Inline completion: ${p.name}${p.model ? ` · ${p.model}` : ""} (${where}, ${p.fim ? "fill-in-the-middle" : "chat"})` };
}

async function openMenu(el: HTMLElement): Promise<void> {
  const s = (await refreshAiStatus()) ?? aiStatus();
  const enabled = getSetting<boolean>("ai.completion.enabled", false);
  const disabled = getSetting<string[]>("ai.completion.disabledLanguages", []);
  const lang = s?.language && s.language !== "text" ? s.language : null;
  const entries: MenuEntry[] = [
    item(describe().text, () => undefined, undefined, { disabled: true }),
    separator,
    item("Enable Inline Completion", () => void saveSetting("ai.completion.enabled", !enabled), undefined, { checked: enabled }),
  ];
  if (lang) {
    const off = disabled.includes(lang);
    entries.push(
      item(
        `Inline Completion for ${lang}`,
        () => void saveSetting("ai.completion.disabledLanguages", off ? disabled.filter((l) => l !== lang) : [...disabled, lang]),
        undefined,
        { checked: !off, disabled: !enabled },
      ),
    );
  }
  if (s?.fileActions?.status === "ignored") entries.push(item("Excluded by .aiignore", () => undefined, undefined, { disabled: true }));
  const err = inlineError();
  if (err) entries.push(item(`Last error: ${err}`, () => undefined, undefined, { disabled: true }));
  entries.push(
    separator,
    item("AI Assistant Settings…", () => openSettings("tools.ai-assistant")),
    item("AI Providers…", () => openSettings("tools.ai-providers")),
  );
  const r = el.getBoundingClientRect();
  showContextMenu(entries, r.left, r.top - 8 - entries.length * 26);
}

export function AiStatusWidget() {
  const d = () => describe();
  return (
    <button
      class="status-item ai-status"
      classList={{ off: !d().on, error: inlineActivity() === "error" }}
      title={inlineActivity() === "error" ? `${d().text}\n${inlineError() ?? ""}` : d().text}
      aria-label="AI Assistant"
      onClick={(e) => void openMenu(e.currentTarget)}
    >
      <Show when={inlineActivity() === "working"} fallback={<Sparkles size={14} />}>
        <LoaderCircle size={14} class="spin" />
      </Show>
    </button>
  );
}
