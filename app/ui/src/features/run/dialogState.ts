/** Open state of the Run/Debug Configurations dialog and the Run… chooser popup. */
import { createSignal } from "solid-js";

export interface DialogRequest {
  /** Configuration to select initially. */
  select?: string;
}

const [dialog, setDialog] = createSignal<DialogRequest | null>(null);
const [chooser, setChooser] = createSignal<"run" | "debug" | null>(null);

export { dialog as runConfigurationsDialog, chooser as configurationChooser };

export function openRunConfigurations(select?: string): void {
  setDialog({ select });
}

export function closeRunConfigurations(): void {
  setDialog(null);
}

export function openChooser(mode: "run" | "debug"): void {
  setChooser(mode);
}

export function closeChooser(): void {
  setChooser(null);
}
