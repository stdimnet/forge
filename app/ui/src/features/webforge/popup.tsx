/**
 * Popups of the WebForge editor features: the "Choose Declaration" / "Usages of …" list for
 * several targets, and small menus (Run/Debug of a package.json script).
 */
import { createSignal, For, onCleanup, onMount, Show, type JSX } from "solid-js";
import { render } from "solid-js/web";
import { ContextMenu, type MenuItem } from "@/features/database/ContextMenu";
import { openFileAt } from "@/editor/views";
import type { WebLocation } from "@/ipc/web";

type Popup =
  | { kind: "targets"; x: number; y: number; title: string; targets: WebLocation[] }
  | { kind: "menu"; x: number; y: number; items: MenuItem[] };

const [popup, setPopup] = createSignal<Popup | null>(null);

export const revealLocation = (t: WebLocation) => void openFileAt(t.path, t.line + 1, t.column + 1);

/** Open the only target, or let the user choose between several at screen position x/y. */
export function chooseTarget(targets: WebLocation[], title: string, x: number, y: number): void {
  if (targets.length === 1) revealLocation(targets[0]);
  else if (targets.length > 1) setPopup({ kind: "targets", x, y, title, targets });
}

export function showMenu(items: MenuItem[], x: number, y: number): void {
  setPopup({ kind: "menu", x, y, items });
}

export const closePopup = () => setPopup(null);

function TargetList(props: { x: number; y: number; title: string; targets: WebLocation[] }) {
  const [index, setIndex] = createSignal(0);
  let el!: HTMLDivElement;
  const away = (e: MouseEvent) => {
    if (!el.contains(e.target as Node)) closePopup();
  };
  onMount(() => {
    window.addEventListener("mousedown", away, true);
    const r = el.getBoundingClientRect();
    if (r.bottom > window.innerHeight - 8) el.style.top = `${Math.max(8, props.y - r.height - 24)}px`;
    if (r.right > window.innerWidth - 8) el.style.left = `${Math.max(8, window.innerWidth - r.width - 8)}px`;
    el.focus();
  });
  onCleanup(() => window.removeEventListener("mousedown", away, true));
  const open = (t: WebLocation) => {
    closePopup();
    revealLocation(t);
  };
  // List navigation while the popup has focus (not a shortcut: the popup owns these keys).
  const onKey: JSX.EventHandler<HTMLDivElement, KeyboardEvent> = (e) => {
    const n = props.targets.length;
    if (e.key === "ArrowDown") setIndex((i) => (i + 1) % n);
    else if (e.key === "ArrowUp") setIndex((i) => (i + n - 1) % n);
    else if (e.key === "Enter") open(props.targets[index()]);
    else if (e.key === "Escape") closePopup();
    else return;
    e.preventDefault();
    e.stopPropagation();
  };
  return (
    <div class="wf-chooser" ref={el} tabIndex={-1} role="listbox" style={{ left: `${props.x}px`, top: `${props.y}px` }} onKeyDown={onKey}>
      <div class="wf-chooser-title">{props.title}</div>
      <For each={props.targets}>
        {(t, i) => (
          <button
            class="wf-chooser-row"
            classList={{ selected: i() === index() }}
            role="option"
            onMouseEnter={() => setIndex(i())}
            onClick={() => open(t)}
          >
            <span class="wf-chooser-name">
              {t.path.split("/").pop()}:{t.line + 1}
            </span>
            <span class="wf-chooser-path">{t.path.split("/").slice(0, -1).join("/")}</span>
          </button>
        )}
      </For>
    </div>
  );
}

function PopupHost() {
  return (
    <Show when={popup()}>
      {(p) => {
        const current = p();
        return current.kind === "targets" ? (
          <TargetList x={current.x} y={current.y} title={current.title} targets={current.targets} />
        ) : (
          <ContextMenu x={current.x} y={current.y} items={current.items} onClose={closePopup} />
        );
      }}
    </Show>
  );
}

if (typeof document !== "undefined") {
  const host = document.createElement("div");
  host.id = "webforge-overlay";
  document.body.appendChild(host);
  render(() => <PopupHost />, host);
}
