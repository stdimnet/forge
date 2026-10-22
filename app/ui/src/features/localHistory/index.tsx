/**
 * Local History (PHPStorm's File | Local History, VCS | Local History, Recent Changes): the
 * actions, the Project view and editor menu entries, and the Recent Changes popup. Recording
 * happens in the backend (`forge-localhistory`); refactorings and VCS operations mark their
 * changes with `withLocalHistory` (`@/ipc/localHistory`).
 */
import { render } from "solid-js/web";
import { Show } from "solid-js";
import { registerCommand } from "@/state/commands";
import { item, registerContextMenuProvider, separator, submenu } from "@/features/contextMenu/state";
import { actionById } from "@/keymap";
import type { FileEntry } from "@/ipc/workspace";
import { workspace } from "@/state/workspace";
import {
  openHistoryTab,
  putLabel,
  recentChangesOpen,
  setRecentChangesOpen,
  showHistoryOfTarget,
  showSelectionHistory,
} from "./actions";
import { RecentChangesPopup } from "./RecentChangesPopup";
import "./LocalHistoryView";

function command(id: string, run: () => void | Promise<void>): void {
  registerCommand({ id, title: actionById(id)?.title ?? id, category: "Local History", run });
}

command("LocalHistory.ShowHistory", showHistoryOfTarget);
command("LocalHistory.ShowSelectionHistory", showSelectionHistory);
command("LocalHistory.PutLabel", putLabel);
command("RecentChanges", () => {
  if (workspace()) setRecentChangesOpen((open) => !open);
});

// Project view: Local History next to the Git submenu, like PHPStorm.
registerContextMenuProvider<FileEntry | null>("project", (entry) => {
  if (!workspace()) return [];
  const target = entry ? { kind: entry.kind === "dir" ? ("folder" as const) : ("file" as const), path: entry.path } : { kind: "folder" as const, path: "" };
  return [
    submenu("Local History", [
      item("Show History", () => openHistoryTab(target), "LocalHistory.ShowHistory"),
      separator,
      item("Put Label…", () => void putLabel(), "LocalHistory.PutLabel"),
    ]),
  ];
});

if (typeof window !== "undefined") {
  const host = document.createElement("div");
  host.id = "local-history-overlays";
  document.body.append(host);
  render(
    () => (
      <Show when={recentChangesOpen()}>
        <RecentChangesPopup />
      </Show>
    ),
    host,
  );
}
