/**
 * Code With Me (`forge.codewithme`): the toolbar icon with the session popup, Tools | Code
 * With Me (manifest `code-with-me.toml`), Settings | Tools | Code With Me, and shared editor
 * tabs with everyone's carets. The backend is `crates/forge-collab`; the plan is
 * `docs/plans/code-with-me.md`.
 */
import { registerToolbarItem } from "@/features/toolbarItems";
import { registerStatusItem } from "@/features/statusItems";
import { registerSettingsPage } from "@/features/settingsPages";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { openSettings } from "@/features/settings/dialogState";
import { askConfirm, askName } from "@/features/contextMenu/dialogs";
import { registerCommand } from "@/state/commands";
import { getSetting } from "@/state/settings";
import { registerKeymapActions } from "@/keymap/store";
import { showBalloon } from "@/keymap";
import { isDialogWindow } from "@/shell/window/mode";
import { isCollabGuest } from "@/state/workspace";
import { collabApi, type PermissionLevel, type Permissions } from "@/ipc/collab";
import { applySession, inSession, isGuest, isHost, refreshSession, setPopupOpen, trackSession } from "./store";
import { trackEditorSharing } from "./session";
import { ToolbarButton } from "./ToolbarButton";
import { ConnectionStatusItem } from "./ConnectionStatus";
import { RelayServerTools } from "./RelaySettings";
import { GuestStatusItem } from "./GuestStatusItem";
import { trackGuestProject } from "./guest";
import { hideWaiting, trackApprovals } from "./approval";
import { editSessionPermissions, regenerateLink } from "./hostActions";
import { showSessionLog } from "./SessionLog";
import "./binding";
import "./codeWithMe.css";
import "./guest.css";

const message = (e: unknown) => String((e as Error)?.message ?? e);

async function copyLink(link: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(link);
    return true;
  } catch {
    return false;
  }
}

async function startSession(level: PermissionLevel, custom?: Permissions): Promise<void> {
  setPopupOpen(false);
  try {
    const s = await collabApi.start(level, custom);
    applySession(s);
    const copied = s.link ? await copyLink(s.link) : false;
    showBalloon(
      "Session started",
      copied ? "The invitation link is copied to the clipboard." : "Use Copy Session Link to invite guests.",
      8000,
      [
        { label: "Copy Session Link", run: () => void copySessionLink() },
        { label: "Settings", run: () => openSettings("tools.code-with-me") },
      ],
    );
  } catch (e) {
    showBalloon("Cannot start the session", message(e), 8000);
  }
}

async function copySessionLink(): Promise<void> {
  if (!isHost()) return void showBalloon("Code With Me", "No session is running.");
  const link = await collabApi.link();
  showBalloon("Code With Me", (await copyLink(link)) ? "The session link is copied to the clipboard." : link, 6000);
}

async function endSession(): Promise<void> {
  if (!isHost()) return void showBalloon("Code With Me", "No session is running.");
  if (getSetting<boolean>("codeWithMe.confirmEndSession", true)) {
    const ok = await askConfirm({ title: "End Session", message: "End the session for all participants?", confirm: "End Session" });
    if (!ok) return;
  }
  await collabApi.end().catch((e) => showBalloon("Code With Me", message(e)));
  await refreshSession();
}

const LINK = /^(forge:\/\/cwm\/join\?|cwm1:)/;

async function joinSession(): Promise<void> {
  setPopupOpen(false);
  if (isHost() || isGuest()) return void showBalloon("Code With Me", "A session is already running.");
  const link = await askName({
    title: "Join Session",
    label: "Session link",
    initial: "",
    confirm: "Join",
    validate: (v) => (LINK.test(v) ? null : "Paste the link from the host (forge://cwm/join?… or cwm1:…)"),
  });
  if (!link) return;
  try {
    applySession(await collabApi.join(link));
  } catch (e) {
    if (message(e) !== "Cancelled.") showBalloon("Cannot join the session", message(e), 8000);
  } finally {
    hideWaiting();
  }
}

async function leaveSession(): Promise<void> {
  if (!isGuest()) return void showBalloon("Code With Me", "You are not in a session.");
  await collabApi.leave().catch(() => undefined);
  await refreshSession();
}

registerToolbarItem({ id: "codeWithMe", order: 30, component: () => <ToolbarButton onStart={(l, c) => void startSession(l, c)} /> });

registerSettingsPage({
  id: "tools.code-with-me",
  title: "Code With Me",
  group: "Tools",
  order: 40,
  keywords: ["collaboration", "pair programming", "share", "session", "guest", "host", "invite"],
  component: () => <SchemaPage pageId="tools.code-with-me" sectionFooter={(s) => (s === "Relay Server" ? <RelayServerTools /> : null)} />,
});

// The path in use (direct or relayed) while a session runs.
registerStatusItem({ id: "codeWithMe.connection", align: "right", order: 20, when: inSession, component: ConnectionStatusItem });

// PHPStorm binds no shortcuts to Code With Me; the actions are listed under its plugin.
registerKeymapActions(
  [
    ["CodeWithMe.StartSession", "Start Session…"],
    ["CodeWithMe.CopySessionLink", "Copy Session Link"],
    ["CodeWithMe.RegenerateLink", "Regenerate Link"],
    ["CodeWithMe.Permissions", "Permissions…"],
    ["CodeWithMe.ShowSessionLog", "Session Log…"],
    ["CodeWithMe.EndSession", "End Session"],
    ["CodeWithMe.JoinSession", "Join Session…"],
    ["CodeWithMe.OpenSharedFile", "Open Shared File…"],
    ["CodeWithMe.LeaveSession", "Leave Session"],
  ].map(([id, title]) => ({ id, title, group: "Plugins/Code With Me" })),
);

const commands: [string, string, () => void | Promise<void>][] = [
  ["CodeWithMe.StartSession", "Start Session…", () => void setPopupOpen(true)],
  ["CodeWithMe.CopySessionLink", "Copy Session Link", copySessionLink],
  ["CodeWithMe.RegenerateLink", "Regenerate Link", regenerateLink],
  ["CodeWithMe.Permissions", "Permissions…", editSessionPermissions],
  ["CodeWithMe.ShowSessionLog", "Session Log…", showSessionLog],
  ["CodeWithMe.EndSession", "End Session", endSession],
  ["CodeWithMe.JoinSession", "Join Session…", joinSession],
  ["CodeWithMe.OpenSharedFile", "Open Shared File…", () => void setPopupOpen(true)],
  ["CodeWithMe.LeaveSession", "Leave Session", leaveSession],
  ["CodeWithMe.Settings", "Code With Me Settings…", () => openSettings("tools.code-with-me")],
];
for (const [id, title, run] of commands) registerCommand({ id, title, category: "Code With Me", run });

// A guest's window: "Code With Me: <host>" in the status bar, the host's project as the project.
registerStatusItem({ id: "codeWithMe.guest", align: "right", order: -40, component: GuestStatusItem, when: isCollabGuest });
registerCommand({ id: "CodeWithMe.LeaveSession", title: "Leave Session", category: "Code With Me", run: () => void collabApi.leave() });

if (!isDialogWindow()) {
  trackSession();
  trackEditorSharing();
  trackGuestProject();
  trackApprovals();
}
