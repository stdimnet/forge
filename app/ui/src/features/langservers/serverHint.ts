/**
 * The banner at the top of an editor that offers to install a missing language server
 * (like PHPStorm's yellow "plugin is not installed" bar). It follows the server list,
 * the settings and the document, and goes away as soon as the server is installed.
 */
import { createEffect, createRoot } from "solid-js";
import { showPanel, type EditorView, type Panel, type ViewUpdate } from "@codemirror/view";
import type { Extension } from "@codemirror/state";
import { Download } from "lucide-solid";
import { languageIdFor } from "@/editor/languages";
import { h, iconNode } from "@/features/lsp/util";
import { activeHint, HINT_HEAD_CHARS, SERVER_HINTS } from "./hints";
import { dismissedHints, dismissHint, hintsEnabled, refreshRows, requestInstall, rows } from "./store";

/** Servers whose banner was closed with "Not now": hidden until Forge restarts. */
const closedForNow = new Set<string>();

const hintLanguages = new Set(SERVER_HINTS.flatMap((x) => x.languageIds));

/** Editor extension for `path`, or null for file types no hint applies to. */
export function serverHintPanel(path: string): Extension | null {
  const languageId = languageIdFor(path);
  if (!languageId || !hintLanguages.has(languageId)) return null;

  return showPanel.of((view: EditorView): Panel => {
    const dom = h("div", "ls-hint");
    dom.hidden = true;
    const text = h("span", "ls-hint-text");
    const install = h("button", "ls-hint-btn primary", "Install…");
    const later = h("button", "ls-hint-btn", "Not now");
    const never = h("button", "ls-hint-btn", "Don't show again");
    dom.append(iconNode(Download, 14, "ls-hint-icon"), text, install, later, never);

    let current: ReturnType<typeof activeHint> = null;
    const refresh = () => {
      const head = view.state.sliceDoc(0, HINT_HEAD_CHARS);
      const found = activeHint({
        languageId,
        head,
        rows: rows(),
        dismissed: [...dismissedHints(), ...closedForNow],
        enabled: hintsEnabled(),
      });
      current = found;
      dom.hidden = !found;
      if (!found) return;
      text.replaceChildren(h("strong", undefined, found.hint.title), ` ${found.hint.detail}`);
    };

    install.onclick = () => current && requestInstall(current.row);
    later.onclick = () => {
      if (!current) return;
      closedForNow.add(current.hint.serverId);
      refresh();
    };
    never.onclick = () => {
      if (current) void dismissHint(current.hint.serverId);
    };

    let dispose = () => {};
    createRoot((d) => {
      dispose = d;
      // Re-run whenever the server list or the settings change.
      createEffect(refresh);
    });
    if (rows().length === 0) void refreshRows();

    return {
      dom,
      top: true,
      update(u: ViewUpdate) {
        if (u.docChanged) refresh();
      },
      destroy: () => dispose(),
    };
  });
}
