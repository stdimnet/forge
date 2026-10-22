/**
 * Structural Search and Replace: Edit | Find | Search Structurally… and Replace
 * Structurally… (PHPStorm's `StructuralSearchPlugin.*` actions, bindable in the keymap).
 */
import { render, Show } from "solid-js/web";
import { createRoot } from "solid-js";
import { registerCommand } from "@/state/commands";
import { actionById } from "@/keymap";
import { isDialogWindow } from "@/shell/window/mode";
import { SsrDialog } from "./SsrDialog";
import { openSsrDialog, ssrDialog } from "./state";
import "./ssr.css";

registerCommand({
  id: "StructuralSearchPlugin.StructuralSearchAction",
  title: actionById("StructuralSearchPlugin.StructuralSearchAction")?.title ?? "Search Structurally…",
  category: "Find",
  run: () => openSsrDialog("search"),
});

registerCommand({
  id: "StructuralSearchPlugin.StructuralReplaceAction",
  title: actionById("StructuralSearchPlugin.StructuralReplaceAction")?.title ?? "Replace Structurally…",
  category: "Find",
  run: () => openSsrDialog("replace"),
});

if (typeof document !== "undefined" && !isDialogWindow()) {
  const host = document.createElement("div");
  host.id = "ssr-overlay";
  document.body.appendChild(host);
  createRoot(() =>
    render(
      () => (
        <Show when={ssrDialog()} keyed>
          {(mode) => <SsrDialog mode={mode} />}
        </Show>
      ),
      host,
    ),
  );
}
