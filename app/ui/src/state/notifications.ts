/**
 * Notification history (PHPStorm's Notifications tool window): every balloon is logged here
 * with its group, time and actions. How a group is shown comes from Settings | Appearance &
 * Behavior | Notifications: `notifications.balloons` (off: log only) and
 * `notifications.groups` (`<group>\t<balloon|sticky|log>` per entry).
 */
import { createSignal } from "solid-js";
import { getSetting, saveSetting } from "./settings";

export type NotificationType = "info" | "warning" | "error";
/** `balloon`: closes by itself; `sticky`: stays until closed; `log`: no balloon. */
export type DisplayMode = "balloon" | "sticky" | "log";

export const DEFAULT_GROUP = "Forge";

export interface NotificationAction {
  label: string;
  run: () => void;
}

export interface NotificationEntry {
  id: number;
  group: string;
  type: NotificationType;
  title: string;
  content?: string;
  /** `Date.now()` when it arrived. */
  time: number;
  read: boolean;
  actions?: NotificationAction[];
}

/** Keep the history bounded like PHPStorm's event log. */
export const MAX_NOTIFICATIONS = 200;

const MODES: readonly DisplayMode[] = ["balloon", "sticky", "log"];

/** `notifications.groups` entries → mode by group name (later entries win). */
export function parseGroupModes(entries: readonly string[]): Map<string, DisplayMode> {
  const out = new Map<string, DisplayMode>();
  for (const e of entries) {
    const [group, mode] = e.split("\t").map((s) => s?.trim());
    if (group && MODES.includes(mode as DisplayMode)) out.set(group, mode as DisplayMode);
  }
  return out;
}

/** The entries with `group` set to `mode` (`balloon`, the default, removes the entry). */
export function withGroupMode(entries: readonly string[], group: string, mode: DisplayMode): string[] {
  const rest = entries.filter((e) => e.split("\t")[0]?.trim() !== group);
  return mode === "balloon" ? rest : [...rest, `${group}\t${mode}`];
}

/**
 * How a notification of `group` is shown. Balloons switched off log everything; a group
 * without an entry uses its registered default, else a balloon.
 */
export function displayMode(group: string, balloons: boolean, modes: ReadonlyMap<string, DisplayMode>, defaults: ReadonlyMap<string, DisplayMode> = new Map()): DisplayMode {
  if (!balloons) return "log";
  return modes.get(group) ?? defaults.get(group) ?? "balloon";
}

const [notifications, setNotifications] = createSignal<NotificationEntry[]>([]);
export { notifications };
export const unreadCount = () => notifications().filter((n) => !n.read).length;

/** Groups contributed by features, with their default display mode. */
const groupDefaults = new Map<string, DisplayMode>();
const [registeredGroups, setRegisteredGroups] = createSignal<string[]>([DEFAULT_GROUP]);

/** Declare a notification group (shown in the settings even before it notified). */
export function registerNotificationGroup(name: string, mode: DisplayMode = "balloon"): void {
  groupDefaults.set(name, mode);
  setRegisteredGroups((g) => (g.includes(name) ? g : [...g, name].sort()));
}

/** Registered groups and the groups that notified, sorted. */
export const notificationGroups = (): string[] => [...new Set([...registeredGroups(), ...notifications().map((n) => n.group)])].sort();

const groupSettings = () => getSetting<string[]>("notifications.groups", []) ?? [];

/** The display mode of a group from the current settings. */
export function modeOf(group: string): DisplayMode {
  return displayMode(group, getSetting<boolean>("notifications.balloons", true) !== false, parseGroupModes(groupSettings()), groupDefaults);
}

/** Change how a group is shown (user settings). */
export function setGroupMode(group: string, mode: DisplayMode): Promise<void> {
  return saveSetting("notifications.groups", withGroupMode(groupSettings(), group, mode));
}

let seq = 0;

export function logNotification(n: Omit<NotificationEntry, "id" | "time" | "read"> & { time?: number }): NotificationEntry {
  const entry: NotificationEntry = { ...n, id: ++seq, time: n.time ?? Date.now(), read: false };
  setNotifications((all) => [entry, ...all].slice(0, MAX_NOTIFICATIONS));
  return entry;
}

export function markRead(id: number): void {
  setNotifications((all) => all.map((n) => (n.id === id && !n.read ? { ...n, read: true } : n)));
}

export function markAllRead(): void {
  setNotifications((all) => (all.some((n) => !n.read) ? all.map((n) => (n.read ? n : { ...n, read: true })) : all));
}

export function removeNotification(id: number): void {
  setNotifications((all) => all.filter((n) => n.id !== id));
}

export function clearNotifications(): void {
  setNotifications([]);
}
