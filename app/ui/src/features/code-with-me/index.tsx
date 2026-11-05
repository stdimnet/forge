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
import { askConfirm } from "@/features/contextMenu/dialogs";
import { registerCommand } from "@/state/commands";
import { getSetting } from "@/state/settings";
import { registerKeymapActions } from "@/keymap/store";
import { showBalloon } from "@/keymap";
import { isDialogWindow } from "@/shell/window/mode";
import { isCollabGuest } from "@/state/workspace";
import { collabApi, type PermissionLevel, type Permissions } from "@/ipc/collab";
import { applySession, inSession, isGuest, isHost, refreshSession, session, setPopupOpen, trackSession } from "./store";
import { trackEditorSharing } from "./session";
import { ToolbarButton } from "./ToolbarButton";
import { ConnectionStatusItem } from "./ConnectionStatus";
import { RelayServerTools } from "./RelaySettings";
import { GuestStatusItem } from "./GuestStatusItem";
import { trackGuestProject } from "./guest";
import { trackApprovals } from "./approval";
import { editSessionPermissions, regenerateLink } from "./hostActions";
import { showSessionLog } from "./SessionLog";
import { registerToolWindow } from "@/features/registry";
import { showToolWindow } from "@/state/layout";
import { Users } from "lucide-solid";
import { follow, followState, jumpTo, setFollowEveryone, stopFollowing, trackFollow } from "./follow";
import { locationLabel } from "./followState";
import { presence, trackPresence } from "./presence";
import { CodeWithMeTabs, CodeWithMeToolWindow } from "./ToolWindow";
import { showResolvedMenu } from "@/features/contextMenu/state";
import type { Participant } from "@/ipc/collab";
import { activeTab as activeTerminalTab, tabs as terminalTabs } from "@/features/terminal/store";
import { setTerminalShared, trackTerminalsAndRuns } from "./terminalsRuns";
import { isReconnecting, trackReconnects } from "./reconnect";
import { joinSession } from "./join";
import "./binding";
import "./codeWithMe.css";
import "./guest.css";
import { t } from "@/i18n";
import { writeClipboardText } from "@/features/clipboard/system";

const message = (e: unknown) => String((e as Error)?.message ?? e);

async function copyLink(link: string): Promise<boolean> {
  try {
    await writeClipboardText(link);
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
      t("tools.codeWithMe.sessionStarted"),
      copied ? t("tools.codeWithMe.invitationLinkCopied") : t("tools.codeWithMe.useCopySessionLink"),
      8000,
      [
        { label: t("tools.codeWithMe.copySessionLink"), run: () => void copySessionLink() },
        { label: t("tools.codeWithMe.settings"), run: () => openSettings("tools.code-with-me") },
      ],
    );
  } catch (e) {
    showBalloon(t("tools.codeWithMe.cannotStartSession"), message(e), 8000);
  }
}

async function copySessionLink(): Promise<void> {
  if (!isHost()) return void showBalloon(t("tools.codeWithMe.title"), t("tools.codeWithMe.noSessionRunning"));
  const link = await collabApi.link();
  showBalloon(t("tools.codeWithMe.title"), (await copyLink(link)) ? t("tools.codeWithMe.sessionLinkCopied") : link, 6000);
}

async function endSession(): Promise<void> {
  if (!isHost()) return void showBalloon(t("tools.codeWithMe.title"), t("tools.codeWithMe.noSessionRunning"));
  if (getSetting<boolean>("codeWithMe.confirmEndSession", true)) {
    const ok = await askConfirm({
      title: t("tools.codeWithMe.endSession"),
      message: t("tools.codeWithMe.endSessionQuestion"),
      confirm: t("tools.codeWithMe.endSession"),
    });
    if (!ok) return;
  }
  await collabApi.end().catch((e) => showBalloon(t("tools.codeWithMe.title"), message(e)));
  await refreshSession();
}


async function leaveSession(): Promise<void> {
  if (!isGuest()) return void showBalloon(t("tools.codeWithMe.title"), t("tools.codeWithMe.notInSession"));
  await collabApi.leave().catch(() => undefined);
  await refreshSession();
}

registerToolbarItem({ id: "codeWithMe", order: 30, component: () => <ToolbarButton onStart={(l, c) => void startSession(l, c)} /> });

registerSettingsPage({
  id: "tools.code-with-me",
  title: t("settings.page.tools.code-with-me"),
  group: "Tools",
  order: 40,
  keywords: ["collaboration", "pair programming", "share", "session", "guest", "host", "invite"],
  component: () => <SchemaPage pageId="tools.code-with-me" sectionFooter={(s) => (s === "Relay Server" ? <RelayServerTools /> : null)} />,
});

// The path in use (direct or relayed) while a session runs.
// A guest that is reconnecting sees that in its own status item instead.
registerStatusItem({ id: "codeWithMe.connection", align: "right", order: 20, when: () => inSession() && !isReconnecting(), component: ConnectionStatusItem });

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
    ["CodeWithMe.FollowParticipant", "Follow Participant…"],
    ["CodeWithMe.StopFollowing", "Stop Following"],
    ["CodeWithMe.MakeEveryoneFollow", "Make Everyone Follow You"],
    ["CodeWithMe.JumpToParticipant", "Jump To Participant…"],
    ["CodeWithMe.ShowToolWindow", "Code With Me"],
    ["CodeWithMe.ShareTerminal", "Share Terminal with Guests"],
  ].map(([id, title]) => ({ id, title, group: "Plugins/Code With Me" })),
);

/** Share With Guests for the active terminal tab (host). */
function shareActiveTerminal(): void {
  if (!isHost()) return void showBalloon(t("tools.codeWithMe.title"), t("tools.codeWithMe.startSessionToShareTerminal"));
  const tab = terminalTabs().find((x) => x.id === activeTerminalTab());
  if (!tab || tab.source) return void showBalloon(t("tools.codeWithMe.title"), t("tools.codeWithMe.openTerminalTabToShare"));
  void setTerminalShared(tab, !tab.shared);
}

/** A chooser of the other participants under the toolbar icon (Follow Participant…, Jump To Participant…). */
function chooseParticipant(title: string, pick: (p: Participant) => void): void {
  if (!inSession()) return void showBalloon(t("tools.codeWithMe.title"), t("tools.codeWithMe.noSessionRunning"));
  const others = session().participants.filter((p) => p.id !== session().me);
  if (!others.length) return void showBalloon(t("tools.codeWithMe.title"), t("tools.codeWithMe.nobodyElseInSession"));
  const anchor = document.querySelector(".cwm-toolbar-btn")?.getBoundingClientRect();
  showResolvedMenu(
    others.map((p) => ({
      kind: "action" as const,
      label: p.role === "host" ? t("tools.codeWithMe.participantHost", { name: p.name }) : p.name,
      detail: locationLabel(presence().get(p.id)),
      checkable: true,
      checked: followState().leader === p.id,
      hint: title,
      run: () => pick(p),
    })),
    anchor ? anchor.left : window.innerWidth / 2 - 120,
    anchor ? anchor.bottom + 4 : 80,
  );
}

function makeEveryoneFollow(): void {
  if (!isHost()) return void showBalloon(t("tools.codeWithMe.title"), t("tools.codeWithMe.onlyHostCanAskToFollow"));
  void setFollowEveryone(!session().followEveryone);
}

const commands: [string, string, () => void | Promise<void>][] = [
  ["CodeWithMe.StartSession", "Start Session…", () => void setPopupOpen(true)],
  ["CodeWithMe.CopySessionLink", "Copy Session Link", copySessionLink],
  ["CodeWithMe.RegenerateLink", "Regenerate Link", regenerateLink],
  ["CodeWithMe.Permissions", "Permissions…", editSessionPermissions],
  ["CodeWithMe.ShowSessionLog", "Session Log…", showSessionLog],
  ["CodeWithMe.EndSession", "End Session", endSession],
  ["CodeWithMe.JoinSession", "Join Session…", () => joinSession()],
  ["CodeWithMe.OpenSharedFile", "Open Shared File…", () => void setPopupOpen(true)],
  ["CodeWithMe.LeaveSession", "Leave Session", leaveSession],
  ["CodeWithMe.Settings", "Code With Me Settings…", () => openSettings("tools.code-with-me")],
  ["CodeWithMe.FollowParticipant", "Follow Participant…", () => chooseParticipant(t("tools.codeWithMe.follow"), (p) => follow(p.id))],
  ["CodeWithMe.JumpToParticipant", "Jump To Participant…", () => chooseParticipant(t("tools.codeWithMe.jumpTo"), (p) => void jumpTo(p))],
  ["CodeWithMe.StopFollowing", "Stop Following", stopFollowing],
  ["CodeWithMe.MakeEveryoneFollow", "Make Everyone Follow You", makeEveryoneFollow],
  ["CodeWithMe.ShowToolWindow", "Code With Me", () => (inSession() ? showToolWindow("code-with-me") : setPopupOpen(true))],
  ["CodeWithMe.ShareTerminal", "Share Terminal with Guests", shareActiveTerminal],
];
for (const [id, title, run] of commands) registerCommand({ id, title, category: "Code With Me", run });

// Participants (Follow, Jump To) and the host's Session Log, while a session runs.
registerToolWindow({
  id: "code-with-me",
  title: t("toolwindow.code-with-me.title"),
  icon: Users,
  anchor: "right",
  order: 60,
  component: CodeWithMeToolWindow,
  headerContent: CodeWithMeTabs,
  when: inSession,
});

// A guest's window: "Code With Me: <host>" in the status bar, the host's project as the project.
registerStatusItem({ id: "codeWithMe.guest", align: "right", order: -40, component: GuestStatusItem, when: isCollabGuest });
registerCommand({ id: "CodeWithMe.LeaveSession", title: "Leave Session", category: "Code With Me", run: () => void collabApi.leave() });

if (!isDialogWindow()) {
  trackSession();
  trackPresence();
  trackFollow();
  trackEditorSharing();
  trackGuestProject();
  trackApprovals();
  trackTerminalsAndRuns();
  trackReconnects();
}
