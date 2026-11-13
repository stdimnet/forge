/**
 * Popups of the WebForge editor features: small menus (Run/Debug of a package.json script).
 * Several declaration or usage targets open the shared "Choose Declaration" popup.
 */
import { createSignal, Show } from "solid-js";
import { render } from "solid-js/web";
import { ContextMenu, type MenuItem } from "@/features/database/ContextMenu";
import { chooseDeclaration } from "@/features/navigation/declarationChooser";
import type { WebLocation } from "@/ipc/web";

type Popup = { kind: "menu"; x: number; y: number; items: MenuItem[] };

const [popup, setPopup] = createSignal<Popup | null>(null);

/** Open the only target, or let the user choose between several at screen position x/y. */
export function chooseTarget(targets: WebLocation[], title: string, x: number, y: number): void {
  chooseDeclaration(targets, { x, y, title });
}

export function showMenu(items: MenuItem[], x: number, y: number): void {
  setPopup({ kind: "menu", x, y, items });
}

export const closePopup = () => setPopup(null);

function PopupHost() {
  return (
    <Show when={popup()}>
      {(p) => <ContextMenu x={p().x} y={p().y} items={p().items} onClose={closePopup} />}
    </Show>
  );
}

if (typeof document !== "undefined") {
  const host = document.createElement("div");
  host.id = "webforge-overlay";
  document.body.appendChild(host);
  render(() => <PopupHost />, host);
}
