/**
 * The ⋯ button of the left stripe (PHPStorm New UI "More Tool Windows"): every tool window
 * the project shows, alphabetically, including the ones removed from the sidebar. Picking
 * one opens it (and puts its button back on the sidebar).
 */
import { toolWindows } from "@/features/registry";
import { showResolvedMenu, type ResolvedEntry } from "@/features/contextMenu/state";
import { isRemovedFromSidebar, showToolWindow } from "@/state/layout";
import { t } from "@/i18n";

export function moreToolWindowsEntries(): ResolvedEntry[] {
  return toolWindows()
    .filter((tw) => tw.when?.() ?? true)
    .sort((a, b) => a.title.localeCompare(b.title))
    .map((tw) => ({
      kind: "action" as const,
      label: tw.title,
      icon: tw.icon,
      shortcut: tw.shortcut ?? "",
      detail: isRemovedFromSidebar(tw.id) ? t("toolwindow.notOnSidebar") : undefined,
      run: () => {
        showToolWindow(tw.id);
        queueMicrotask(() => document.querySelector<HTMLElement>(`[data-tool-window-panel="${tw.id}"] .tool-window-body`)?.focus?.());
      },
    }));
}

export function openMoreToolWindows(x: number, y: number): void {
  showResolvedMenu(moreToolWindowsEntries(), x, y, { class: "more-tool-windows" });
}
