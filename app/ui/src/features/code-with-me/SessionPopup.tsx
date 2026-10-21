/**
 * The Code With Me popup of the main toolbar, as in PHPStorm: without a session it offers
 * the permission for guests and Start Session; during a session it lists the participants
 * and the session actions.
 */
import { createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { Portal } from "solid-js/web";
import type { PermissionLevel } from "@/ipc/collab";
import { getSetting } from "@/state/settings";
import { runCommand } from "@/state/commands";
import { session, setPopupOpen } from "./store";

export const LEVELS: { level: PermissionLevel; title: string; hint: string }[] = [
  { level: "readOnly", title: "Read-only", hint: "Guests can view files, the terminal and run output." },
  { level: "editFiles", title: "Edit files", hint: "Guests can edit files, but cannot use the terminal or run code." },
  { level: "fullAccess", title: "Full access", hint: "Guests can edit files, use the terminal and run code on your machine." },
];

const excludeCount = () =>
  getSetting<string[]>("codeWithMe.excludedFiles", []).filter((p) => p.trim() && !p.trim().startsWith("#") && !p.trim().startsWith("!")).length;

function StartView(props: { onStart: (level: PermissionLevel) => void }) {
  const [level, setLevel] = createSignal<PermissionLevel>(getSetting<PermissionLevel>("codeWithMe.defaultPermission", "readOnly"));
  return (
    <>
      <div class="cwm-popup-title">Start Session</div>
      <div class="cwm-popup-label muted">Permissions for guests</div>
      <div class="cwm-levels" role="radiogroup">
        <For each={LEVELS}>
          {(l) => (
            <label class="cwm-level" classList={{ selected: level() === l.level }}>
              <input type="radio" name="cwm-level" checked={level() === l.level} onChange={() => setLevel(l.level)} />
              <span class="cwm-level-text">
                <span class="cwm-level-title">{l.title}</span>
                <span class="cwm-level-hint muted">{l.hint}</span>
              </span>
            </label>
          )}
        </For>
        <label class="cwm-level disabled" title="Custom permissions come with host approval">
          <input type="radio" name="cwm-level" disabled />
          <span class="cwm-level-text">
            <span class="cwm-level-title">Custom…</span>
          </span>
        </label>
      </div>
      <div class="cwm-popup-note muted">Files matching {excludeCount()} exclude patterns are hidden from guests.</div>
      <Show when={session().hostUnavailable}>
        <div class="cwm-popup-warning">{session().hostUnavailable}</div>
      </Show>
      <div class="cwm-popup-actions">
        <button class="btn-primary" disabled={!session().canHost} onClick={() => props.onStart(level())}>
          Start Session
        </button>
      </div>
    </>
  );
}

function MenuItem(props: { label: string; action: string; danger?: boolean }) {
  return (
    <div
      class="cwm-menu-item"
      classList={{ danger: props.danger }}
      role="menuitem"
      data-action={props.action}
      onClick={() => {
        setPopupOpen(false);
        void runCommand(props.action);
      }}
    >
      {props.label}
    </div>
  );
}

export function SessionPopup(props: { anchor: HTMLElement; onStart: (level: PermissionLevel) => void }) {
  let el!: HTMLDivElement;
  const rect = props.anchor.getBoundingClientRect();
  const onDoc = (e: MouseEvent) => {
    const t = e.target as Element;
    if (!t.closest(".cwm-popup") && !t.closest(".cwm-toolbar-btn")) setPopupOpen(false);
  };
  onMount(() => {
    document.addEventListener("mousedown", onDoc, true);
    el.focus();
  });
  onCleanup(() => document.removeEventListener("mousedown", onDoc, true));
  return (
    <Portal>
      <div
        ref={el}
        class="popup cwm-popup"
        tabIndex={-1}
        role="dialog"
        aria-label="Code With Me"
        style={{ top: `${rect.bottom + 4}px`, left: `${Math.max(8, Math.min(rect.left, window.innerWidth - 380))}px` }}
        onKeyDown={(e) => e.key === "Escape" && (e.stopPropagation(), setPopupOpen(false))}
      >
        <StartView onStart={props.onStart} />
        <div class="cwm-sep" />
        <MenuItem label="Join Session…" action="CodeWithMe.JoinSession" />
      </div>
    </Portal>
  );
}
