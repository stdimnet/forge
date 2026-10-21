/**
 * Code With Me (`forge.codewithme`): the toolbar icon with the Start Session popup,
 * Tools | Code With Me (manifest `code-with-me.toml`) and Settings | Tools | Code With Me.
 * The backend is `crates/forge-collab`; the plan is `docs/plans/code-with-me.md`.
 */
import { registerToolbarItem } from "@/features/toolbarItems";
import { registerSettingsPage } from "@/features/settingsPages";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { registerCommand } from "@/state/commands";
import { registerKeymapActions } from "@/keymap/store";
import { showBalloon } from "@/keymap";
import { isDialogWindow } from "@/shell/window/mode";
import { registerStatusItem } from "@/features/statusItems";
import { isCollabGuest } from "@/state/workspace";
import { collabApi, type PermissionLevel } from "@/ipc/collab";
import { setPopupOpen, trackSession } from "./store";
import { ToolbarButton } from "./ToolbarButton";
import { GuestStatusItem } from "./GuestStatusItem";
import { trackGuestProject } from "./guest";
import "./codeWithMe.css";
import "./guest.css";

function startSession(_level: PermissionLevel): void {
  setPopupOpen(false);
  showBalloon("Code With Me", "Sharing sessions is not available yet.");
}

registerToolbarItem({ id: "codeWithMe", order: 30, component: () => <ToolbarButton onStart={startSession} /> });

registerSettingsPage({
  id: "tools.code-with-me",
  title: "Code With Me",
  group: "Tools",
  order: 40,
  keywords: ["collaboration", "pair programming", "share", "session", "guest", "host", "invite"],
  component: () => <SchemaPage pageId="tools.code-with-me" />,
});

// PHPStorm binds no shortcuts to Code With Me; the actions are listed under its plugin.
registerKeymapActions(
  [
    ["CodeWithMe.StartSession", "Start Session…"],
    ["CodeWithMe.CopySessionLink", "Copy Session Link"],
    ["CodeWithMe.EndSession", "End Session"],
    ["CodeWithMe.JoinSession", "Join Session…"],
    ["CodeWithMe.OpenSharedFile", "Open Shared File…"],
    ["CodeWithMe.LeaveSession", "Leave Session"],
  ].map(([id, title]) => ({ id, title, group: "Plugins/Code With Me" })),
);

registerCommand({ id: "CodeWithMe.StartSession", title: "Start Session…", category: "Code With Me", run: () => void setPopupOpen(true) });

// A guest's window: "Code With Me: <host>" in the status bar, the host's project as the project.
registerStatusItem({ id: "codeWithMe.guest", align: "right", order: -40, component: GuestStatusItem, when: isCollabGuest });
registerCommand({ id: "CodeWithMe.LeaveSession", title: "Leave Session", category: "Code With Me", run: () => void collabApi.leave() });

if (!isDialogWindow()) {
  trackSession();
  trackGuestProject();
}
