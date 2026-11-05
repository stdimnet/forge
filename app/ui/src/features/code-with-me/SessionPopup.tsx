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
import { navKeys } from "./a11y";
import { t } from "@/i18n";

export type StartHandler = (level: PermissionLevel, custom?: Permissions) => void;

export { describePermissions };

export const LEVELS: { level: PermissionLevel; readonly title: string; readonly hint: string }[] = [
  {
    level: "readOnly",
    get title() {
      return t("tools.codeWithMe.level.readOnly");
    },
    get hint() {
      return t("tools.codeWithMe.levelHint.readOnly");
    },
  },
  {
    level: "editFiles",
    get title() {
      return t("tools.codeWithMe.level.editFiles");
    },
    get hint() {
      return t("tools.codeWithMe.levelHint.editFiles");
    },
  },
  {
    level: "fullAccess",
    get title() {
      return t("tools.codeWithMe.level.fullAccess");
    },
    get hint() {
      return t("tools.codeWithMe.levelHint.fullAccess");
    },
  },
];

const excludeCount = () =>
  getSetting<string[]>("codeWithMe.excludedFiles", []).filter((p) => p.trim() && !p.trim().startsWith("#") && !p.trim().startsWith("!")).length;

function StartView(props: { onStart: StartHandler }) {
  const initial = getSetting<PermissionLevel>("codeWithMe.defaultPermission", "readOnly");
  const [level, setLevel] = createSignal<PermissionLevel>(initial === "custom" ? "readOnly" : initial);
  const [custom, setCustom] = createSignal<Permissions | null>(null);
  const chooseCustom = async () => {
    const base = custom() ?? (level() === "custom" ? PRESETS.readOnly : PRESETS[level() as Exclude<PermissionLevel, "custom">]);
    const p = await askCustomPermissions(t("tools.codeWithMe.customPermissions"), base);
    if (!p) return;
    // Values that match a preset are that preset.
    const l = levelOf(p);
    setCustom(l === "custom" ? p : null);
    setLevel(l);
  };
  return (
    <>
      <div class="cwm-popup-title">{t("tools.codeWithMe.startSession")}</div>
      <div class="cwm-popup-label muted">{t("tools.codeWithMe.permissionsForGuests")}</div>
      <div class="cwm-levels" role="radiogroup" aria-label={t("tools.codeWithMe.permissionsForGuests")}>
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
            <span class="cwm-level-title">{t("tools.codeWithMe.customEllipsis")}</span>
            <Show when={custom()}>{(c) => <span class="cwm-level-hint muted">{describePermissions(c())}</span>}</Show>
          </span>
        </label>
      </div>
      <div class="cwm-popup-note muted">{t("tools.codeWithMe.excludedFilesNote", { count: excludeCount() })}</div>
      <Show when={session().hostUnavailable}>
        <div class="cwm-popup-warning">{session().hostUnavailable}</div>
      </Show>
      <div class="cwm-popup-actions">
        <button class="btn-primary" disabled={!session().canHost} onClick={() => props.onStart(level(), custom() ?? undefined)}>
          {t("tools.codeWithMe.startSession")}
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
      tabIndex={-1}
      data-nav
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
      <div class="cwm-popup-title">{t("tools.codeWithMe.title")}</div>
      <Show when={session().notice}>
        <div class="cwm-popup-warning cwm-notice">{session().notice}</div>
      </Show>
      <div class="cwm-popup-label muted">{t("tools.codeWithMe.participants")}</div>
      <Participants />
      <Show when={session().participants.length < 2}>
        <div class="cwm-popup-note muted">{t("tools.codeWithMe.inviteNote")}</div>
      </Show>
      <div class="cwm-sep" />
      <div role="menu" aria-label={t("tools.codeWithMe.session")}>
      <Show when={followState().leader !== null}>
        <MenuItem label={t("tools.codeWithMe.stopFollowing")} action="CodeWithMe.StopFollowing" />
      </Show>
      <MenuItem label={t("tools.codeWithMe.copySessionLink")} action="CodeWithMe.CopySessionLink" />
      <MenuItem label={t("tools.codeWithMe.regenerateLink")} action="CodeWithMe.RegenerateLink" />
      <div
        class="cwm-menu-item cwm-submenu-item"
        role="menuitem"
        tabIndex={-1}
        data-nav
        aria-haspopup="menu"
        data-action="CodeWithMe.Permissions"
        title={describePermissions(sessionPermissions())}
        onClick={(e) => {
          const r = e.currentTarget.getBoundingClientRect();
          showSessionPermissionsMenu(r.right, r.top);
        }}
      >
        <span>{t("tools.codeWithMe.permissions")}</span>
        <span class="cwm-submenu-value muted">{LEVEL_TITLES[levelOf(sessionPermissions())]}</span>
        <ChevronRight size={14} />
      </div>
      <div
        class="cwm-menu-item cwm-check-item"
        role="menuitemcheckbox"
        tabIndex={-1}
        data-nav
        aria-checked={!!session().followEveryone}
        data-action="CodeWithMe.MakeEveryoneFollow"
        onClick={() => {
          setPopupOpen(false);
          void runCommand("CodeWithMe.MakeEveryoneFollow");
        }}
      >
        <span>{t("tools.codeWithMe.makeEveryoneFollow")}</span>
        <Show when={session().followEveryone && session().participants.length > 1}>
          <span class="cwm-submenu-value muted">
            {t("tools.codeWithMe.followersOf", { followers: hostFollowers(), others: session().participants.length - 1 })}
          </span>
        </Show>
        <span class="cwm-check">{session().followEveryone ? <Check size={14} /> : null}</span>
      </div>
      <MenuItem label={t("tools.codeWithMe.sessionLogEllipsis")} action="CodeWithMe.ShowSessionLog" />
      <MenuItem label={t("tools.codeWithMe.settingsEllipsis")} action="CodeWithMe.Settings" />
      <div class="cwm-sep" />
      <MenuItem label={t("tools.codeWithMe.endSession")} action="CodeWithMe.EndSession" danger />
      </div>
    </>
  );
}

function GuestView() {
  return (
    <>
      <div class="cwm-popup-title">{t("tools.codeWithMe.titleWith", { value: session().hostLabel ?? "" })}</div>
      <Show when={session().notice}>
        <div class="cwm-popup-warning cwm-notice">{session().notice}</div>
      </Show>
      <Participants />
      <div class="cwm-popup-label muted">{t("tools.codeWithMe.sharedFiles")}</div>
      <div class="cwm-files" role="list" aria-label={t("tools.codeWithMe.sharedFiles")}>
        <For each={session().files} fallback={<div class="cwm-popup-note muted">{t("tools.codeWithMe.hostHasNoFilesOpen")}</div>}>
          {(f) => (
            <div
              class="cwm-menu-item cwm-file"
              role="listitem"
              tabIndex={-1}
              data-nav
              aria-label={t("tools.codeWithMe.openFile", { path: f.path })}
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
      <div role="menu" aria-label={t("tools.codeWithMe.session")}>
        <Show when={followState().leader !== null}>
          <div class="cwm-sep" />
          <MenuItem label={t("tools.codeWithMe.stopFollowing")} action="CodeWithMe.StopFollowing" />
        </Show>
        <div class="cwm-sep" />
        <MenuItem label={t("tools.codeWithMe.leaveSession")} action="CodeWithMe.LeaveSession" danger />
      </div>
    </>
  );
}

export function SessionPopup(props: { anchor: HTMLElement; onStart: StartHandler }) {
  let el!: HTMLDivElement;
  const rect = props.anchor.getBoundingClientRect();
  const onDoc = (e: MouseEvent) => {
    const target = e.target as Element;
    // The participant and permission menus, and the Custom Permissions dialog, belong to the popup.
    if (!target.closest(".cwm-popup") && !target.closest(".cwm-toolbar-btn") && !target.closest(".context-menu") && !target.closest(".cwm-custom-dialog")) setPopupOpen(false);
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
        aria-label={t("tools.codeWithMe.title")}
        style={{ top: `${rect.bottom + 4}px`, left: `${Math.max(8, Math.min(rect.left, window.innerWidth - 380))}px` }}
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            e.stopPropagation();
            setPopupOpen(false);
          } else navKeys(e);
        }}
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
          <div role="menu" aria-label={t("tools.codeWithMe.title")}>
            <MenuItem label={t("tools.codeWithMe.joinSessionEllipsis")} action="CodeWithMe.JoinSession" />
          </div>
        </Show>
      </div>
    </Portal>
  );
}
