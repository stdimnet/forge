/**
 * The Code With Me popup of the main toolbar, as in PHPStorm: without a session it offers
 * the permission for guests and Start Session; during a session it lists the participants
 * and the session actions.
 */
import { createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { Portal } from "solid-js/web";
import { Check, ChevronRight } from "lucide-solid";
import { levelOf, PRESETS, type PermissionLevel, type Permissions } from "@/ipc/collab";
import { getSetting } from "@/state/settings";
import { runCommand } from "@/state/commands";
import { FileIcon } from "@/shell/FileIcon";
import { inSession, session, setPopupOpen } from "./store";
import { openSharedFile } from "./session";
import { SessionConnections } from "./ConnectionStatus";
import { askCustomPermissions } from "./CustomPermissions";
import { LEVEL_TITLES, sessionPermissions, showSessionPermissionsMenu } from "./hostActions";
import { describePermissions } from "./permissionText";
import { hostFollowers, Participants } from "./Participants";
import { followState } from "./follow";

export type StartHandler = (level: PermissionLevel, custom?: Permissions) => void;

export { describePermissions };

export const LEVELS: { level: PermissionLevel; title: string; hint: string }[] = [
  { level: "readOnly", title: "Read-only", hint: "Guests can view files, the terminal and run output." },
  { level: "editFiles", title: "Edit files", hint: "Guests can edit files, but cannot use the terminal or run code." },
  { level: "fullAccess", title: "Full access", hint: "Guests can edit files, use the terminal and run code on your machine." },
];

const excludeCount = () =>
  getSetting<string[]>("codeWithMe.excludedFiles", []).filter((p) => p.trim() && !p.trim().startsWith("#") && !p.trim().startsWith("!")).length;

function StartView(props: { onStart: StartHandler }) {
  const initial = getSetting<PermissionLevel>("codeWithMe.defaultPermission", "readOnly");
  const [level, setLevel] = createSignal<PermissionLevel>(initial === "custom" ? "readOnly" : initial);
  const [custom, setCustom] = createSignal<Permissions | null>(null);
  const chooseCustom = async () => {
    const base = custom() ?? (level() === "custom" ? PRESETS.readOnly : PRESETS[level() as Exclude<PermissionLevel, "custom">]);
    const p = await askCustomPermissions("Custom Permissions", base);
    if (!p) return;
    // Values that match a preset are that preset.
    const l = levelOf(p);
    setCustom(l === "custom" ? p : null);
    setLevel(l);
  };
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
        <label class="cwm-level" classList={{ selected: level() === "custom" }} data-level="custom">
          <input
            type="radio"
            name="cwm-level"
            checked={level() === "custom"}
            onClick={(e) => {
              e.preventDefault();
              void chooseCustom();
            }}
          />
          <span class="cwm-level-text">
            <span class="cwm-level-title">Custom…</span>
            <Show when={custom()}>{(c) => <span class="cwm-level-hint muted">{describePermissions(c())}</span>}</Show>
          </span>
        </label>
      </div>
      <div class="cwm-popup-note muted">Files matching {excludeCount()} exclude patterns are hidden from guests.</div>
      <Show when={session().hostUnavailable}>
        <div class="cwm-popup-warning">{session().hostUnavailable}</div>
      </Show>
      <div class="cwm-popup-actions">
        <button class="btn-primary" disabled={!session().canHost} onClick={() => props.onStart(level(), custom() ?? undefined)}>
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

function HostView() {
  return (
    <>
      <div class="cwm-popup-title">Code With Me</div>
      <Show when={session().notice}>
        <div class="cwm-popup-warning cwm-notice">{session().notice}</div>
      </Show>
      <div class="cwm-popup-label muted">Participants</div>
      <Participants />
      <Show when={session().participants.length < 2}>
        <div class="cwm-popup-note muted">Send the session link to invite guests. The files open in your editor are shared.</div>
      </Show>
      <div class="cwm-sep" />
      <Show when={followState().leader !== null}>
        <MenuItem label="Stop Following" action="CodeWithMe.StopFollowing" />
      </Show>
      <MenuItem label="Copy Session Link" action="CodeWithMe.CopySessionLink" />
      <MenuItem label="Regenerate Link" action="CodeWithMe.RegenerateLink" />
      <div
        class="cwm-menu-item cwm-submenu-item"
        role="menuitem"
        aria-haspopup="menu"
        data-action="CodeWithMe.Permissions"
        title={describePermissions(sessionPermissions())}
        onClick={(e) => {
          const r = e.currentTarget.getBoundingClientRect();
          showSessionPermissionsMenu(r.right, r.top);
        }}
      >
        <span>Permissions</span>
        <span class="cwm-submenu-value muted">{LEVEL_TITLES[levelOf(sessionPermissions())]}</span>
        <ChevronRight size={14} />
      </div>
      <div
        class="cwm-menu-item cwm-check-item"
        role="menuitemcheckbox"
        aria-checked={!!session().followEveryone}
        data-action="CodeWithMe.MakeEveryoneFollow"
        onClick={() => {
          setPopupOpen(false);
          void runCommand("CodeWithMe.MakeEveryoneFollow");
        }}
      >
        <span>Make Everyone Follow You</span>
        <Show when={session().followEveryone && session().participants.length > 1}>
          <span class="cwm-submenu-value muted">
            {hostFollowers()} of {session().participants.length - 1} following
          </span>
        </Show>
        <span class="cwm-check">{session().followEveryone ? <Check size={14} /> : null}</span>
      </div>
      <MenuItem label="Session Log…" action="CodeWithMe.ShowSessionLog" />
      <MenuItem label="Settings…" action="CodeWithMe.Settings" />
      <div class="cwm-sep" />
      <MenuItem label="End Session" action="CodeWithMe.EndSession" danger />
    </>
  );
}

function GuestView() {
  return (
    <>
      <div class="cwm-popup-title">Code With Me: {session().hostLabel}</div>
      <Show when={session().notice}>
        <div class="cwm-popup-warning cwm-notice">{session().notice}</div>
      </Show>
      <Participants />
      <div class="cwm-popup-label muted">Shared files</div>
      <div class="cwm-files" role="list">
        <For each={session().files} fallback={<div class="cwm-popup-note muted">The host has no files open.</div>}>
          {(f) => (
            <div
              class="cwm-menu-item cwm-file"
              role="listitem"
              data-path={f.path}
              title={f.path}
              onClick={() => {
                setPopupOpen(false);
                void openSharedFile(f.path);
              }}
            >
              <FileIcon name={f.path.split("/").pop() ?? f.path} />
              <span class="cwm-file-name">{f.path.split("/").pop()}</span>
              <span class="cwm-file-dir muted">{f.path.split("/").slice(0, -1).join("/")}</span>
            </div>
          )}
        </For>
      </div>
      <Show when={followState().leader !== null}>
        <div class="cwm-sep" />
        <MenuItem label="Stop Following" action="CodeWithMe.StopFollowing" />
      </Show>
      <div class="cwm-sep" />
      <MenuItem label="Leave Session" action="CodeWithMe.LeaveSession" danger />
    </>
  );
}

export function SessionPopup(props: { anchor: HTMLElement; onStart: StartHandler }) {
  let el!: HTMLDivElement;
  const rect = props.anchor.getBoundingClientRect();
  const onDoc = (e: MouseEvent) => {
    const t = e.target as Element;
    // The participant and permission menus, and the Custom Permissions dialog, belong to the popup.
    if (!t.closest(".cwm-popup") && !t.closest(".cwm-toolbar-btn") && !t.closest(".context-menu") && !t.closest(".cwm-custom-dialog")) setPopupOpen(false);
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
        <Show when={inSession()}>
          <SessionConnections />
          <div class="cwm-sep" />
        </Show>
        <Show when={session().role === "host"}>
          <HostView />
        </Show>
        <Show when={session().role === "guest"}>
          <GuestView />
        </Show>
        <Show when={session().role === "none"}>
          <StartView onStart={props.onStart} />
          <div class="cwm-sep" />
          <MenuItem label="Join Session…" action="CodeWithMe.JoinSession" />
        </Show>
      </div>
    </Portal>
  );
}
