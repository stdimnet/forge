/**
 * Notifications (PHPStorm: View | Tool Windows | Notifications): the history of balloons
 * (`state/notifications.ts`, filled by `showBalloon`), Mark All as Read, Clear All and the
 * Settings | Appearance & Behavior | Notifications page (balloons on/off and the display
 * mode per group).
 */
import { registerToolWindow } from "@/features/registry";
import { registerSettingsPage } from "@/features/settingsPages";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { registerCommand } from "@/state/commands";
import { registerKeymapActions } from "@/keymap";
import { clearNotifications, markAllRead } from "@/state/notifications";
import { NotificationsActions, NotificationsIcon, NotificationsView } from "./NotificationsView";
import "./notifications.css";

registerToolWindow({
  id: "notifications",
  title: "Notifications",
  icon: NotificationsIcon,
  anchor: "right",
  order: 20,
  component: NotificationsView,
  actions: () => <NotificationsActions />,
});

registerKeymapActions([
  { id: "MarkNotificationsAsRead", title: "Mark All Notifications as Read", group: "Notifications" },
  { id: "ClearAllNotifications", title: "Clear All Notifications", group: "Notifications" },
]);
registerCommand({ id: "MarkNotificationsAsRead", title: "Mark All Notifications as Read", category: "Notifications", run: markAllRead });
registerCommand({ id: "ClearAllNotifications", title: "Clear All Notifications", category: "Notifications", run: clearNotifications });

registerSettingsPage({
  id: "notifications",
  title: "Notifications",
  group: "Appearance & Behavior",
  order: 20,
  keywords: ["balloon", "sticky", "log", "popup", "event log"],
  component: () => <SchemaPage pageId="notifications" />,
});
