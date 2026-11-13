/**
 * Window | Layouts, like PHPStorm: Save Current Layout as New…, the saved layouts (pick one
 * to restore it), Restore Default Layout (Shift+F12) and Delete Layout. Layouts keep the tool
 * window part of the workbench (open windows, places, view modes, sizes) and are stored by
 * Rust in the user setting `workbench.layouts` (`forge_core::project_state::NamedLayout`).
 */
import { projectStateApi, type ProjectLayout } from "@/ipc/projectState";
import { registerCommand } from "@/state/commands";
import { DEFAULT_SIZES, DEFAULT_SPLITS, setLayout } from "@/state/layout";
import { applyWindowLayout, windowLayout } from "@/state/layoutPersistence";
import { SLOTS, type ActiveWindows } from "@/state/toolWindowLayout";
import { showBalloon } from "@/keymap";
import { askName } from "@/features/contextMenu/dialogs";
import { registerMainMenuDynamic } from "@/features/mainMenu/model";
import { t } from "@/i18n";

/** Replace the tool window layout with a saved one (windows it does not open close). */
export function applyNamedLayout(saved: ProjectLayout): void {
  setLayout("active", Object.fromEntries(SLOTS.map((s) => [s, null])) as ActiveWindows);
  setLayout("size", { ...DEFAULT_SIZES });
  setLayout("splits", { ...DEFAULT_SPLITS });
  applyWindowLayout(saved);
}

export async function restoreNamedLayout(name: string): Promise<void> {
  const layouts = await projectStateApi.listLayouts();
  const found = layouts.find((l) => l.name.toLowerCase() === name.toLowerCase());
  if (!found) return void showBalloon(t("tools.windows.layoutNotFound", { name }));
  applyNamedLayout(found.layout);
}

/** Window | Layouts | Save Current Layout as New…. */
export async function saveCurrentLayoutAsNew(): Promise<void> {
  const taken = new Set((await projectStateApi.listLayouts().catch(() => [])).map((l) => l.name.toLowerCase()));
  const name = await askName({
    title: "Save Layout",
    label: "Name:",
    initial: "",
    confirm: "Save",
    validate: (n) => (n.length > 64 ? "The name is too long" : taken.has(n.toLowerCase()) ? "A layout with this name already exists" : null),
  });
  if (!name) return;
  try {
    await projectStateApi.saveNamedLayout(name, windowLayout());
    showBalloon(t("tools.windows.layoutSaved", { name }), t("tools.windows.windowLayoutsRestoresIt"));
  } catch (e) {
    showBalloon(t("tools.windows.cannotSaveTheLayout"), String(e));
  }
}

export async function deleteNamedLayout(name: string): Promise<void> {
  if (await projectStateApi.deleteNamedLayout(name).catch(() => false)) showBalloon(t("tools.windows.layoutDeleted", { name }));
}

registerCommand({ id: "StoreNewLayout", title: "Save Current Layout as New…", category: "Window", run: saveCurrentLayoutAsNew });

registerMainMenuDynamic("layouts", async () =>
  (await projectStateApi.listLayouts()).map((l) => ({
    key: l.name,
    label: l.name,
    run: () => applyNamedLayout(l.layout),
  })),
);
registerMainMenuDynamic("layoutsDelete", async () =>
  (await projectStateApi.listLayouts()).map((l) => ({
    key: l.name,
    label: l.name,
    run: () => void deleteNamedLayout(l.name),
  })),
);
