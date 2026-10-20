/**
 * Opening the Run/Debug Configurations dialog (its own window in the desktop app) and the
 * open state of the Run… chooser popup.
 */
import { createSignal } from "solid-js";
import { closeDialog, openDialogWindow } from "@/shell/window/dialogs";

export const RUN_CONFIGURATIONS_DIALOG = "run-configurations";

export interface DialogRequest {
  /** Configuration to select initially. */
  select?: string;
}

const [chooser, setChooser] = createSignal<"run" | "debug" | null>(null);

export { chooser as configurationChooser };

export function openRunConfigurations(select?: string): void {
  void openDialogWindow(RUN_CONFIGURATIONS_DIALOG, { select } satisfies DialogRequest);
}

export function closeRunConfigurations(): void {
  closeDialog(RUN_CONFIGURATIONS_DIALOG);
}

export function openChooser(mode: "run" | "debug"): void {
  setChooser(mode);
}

export function closeChooser(): void {
  setChooser(null);
}
