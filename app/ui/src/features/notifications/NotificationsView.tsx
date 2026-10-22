/**
 * Notifications tool window: the balloon history, newest first, with time, group and the
 * notification's actions. Unread entries are marked; "Mark All as Read" and "Clear All" sit
 * in the header, and an entry's context menu sets how its group is displayed.
 */
import { For, Match, Show, Switch } from "solid-js";
import { Bell, BellDot, CheckCheck, CircleAlert, Info, Settings, Trash2, TriangleAlert, X } from "lucide-solid";
import { openSettings } from "@/features/settings/dialogState";
import { showResolvedMenu, type ResolvedEntry } from "@/features/contextMenu/state";
import {
  clearNotifications,
  markAllRead,
  markRead,
  modeOf,
  notifications,
  removeNotification,
  setGroupMode,
  unreadCount,
  type DisplayMode,
  type NotificationEntry,
} from "@/state/notifications";

const pad = (n: number) => String(n).padStart(2, "0");
const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/** "14:05" today, "Yesterday 14:05", else "Oct 3 14:05". */
export function timeLabel(time: number, now = Date.now()): string {
  const d = new Date(time);
  const today = new Date(now);
  const clock = `${pad(d.getHours())}:${pad(d.getMinutes())}`;
  const day = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const days = Math.round((day(today) - day(d)) / 86_400_000);
  if (days <= 0) return clock;
  if (days === 1) return `Yesterday ${clock}`;
  return `${MONTHS[d.getMonth()]} ${d.getDate()} ${clock}`;
}

const MODE_LABELS: [DisplayMode, string][] = [
  ["balloon", "Balloon"],
  ["sticky", "Sticky Balloon"],
  ["log", "No Popup (Tool Window Only)"],
];

function showEntryMenu(e: MouseEvent, n: NotificationEntry): void {
  e.preventDefault();
  const mode = modeOf(n.group);
  const entries: ResolvedEntry[] = [
    { kind: "action", label: "Mark as Read", run: () => markRead(n.id), disabled: n.read },
    { kind: "action", label: "Remove", run: () => removeNotification(n.id) },
    "separator",
    {
      kind: "submenu",
      label: `Display "${n.group}" as`,
      items: MODE_LABELS.map(([m, label]) => ({ kind: "action", label, checkable: true, checked: mode === m, run: () => void setGroupMode(n.group, m) })),
    },
    { kind: "action", label: "Notification Settings…", run: () => openSettings("notifications") },
  ];
  showResolvedMenu(entries, e.clientX, e.clientY);
}

function TypeIcon(props: { type: NotificationEntry["type"] }) {
  return (
    <Switch fallback={<Info size={15} class="notif-icon info" />}>
      <Match when={props.type === "warning"}>
        <TriangleAlert size={15} class="notif-icon warning" />
      </Match>
      <Match when={props.type === "error"}>
        <CircleAlert size={15} class="notif-icon error" />
      </Match>
    </Switch>
  );
}

/** Stripe icon: a bell with a dot while there are unread notifications. */
export function NotificationsIcon(props: { size?: number; "stroke-width"?: number }) {
  return (
    <Show when={unreadCount() > 0} fallback={<Bell size={props.size} stroke-width={props["stroke-width"]} />}>
      <BellDot size={props.size} stroke-width={props["stroke-width"]} class="notif-unread-icon" />
    </Show>
  );
}

export function NotificationsActions() {
  return (
    <>
      <button class="icon-btn" title="Mark All as Read" disabled={unreadCount() === 0} onClick={markAllRead}>
        <CheckCheck size={15} />
      </button>
      <button class="icon-btn" title="Clear All" disabled={notifications().length === 0} onClick={clearNotifications}>
        <Trash2 size={15} />
      </button>
      <button class="icon-btn" title="Notification Settings" onClick={() => openSettings("notifications")}>
        <Settings size={15} />
      </button>
    </>
  );
}

export function NotificationsView() {
  return (
    <div class="notif-view" tabIndex={-1}>
      <Show when={notifications().length} fallback={<div class="notif-empty muted">No notifications</div>}>
        <For each={notifications()}>
          {(n) => (
            <div
              class="notif-entry"
              classList={{ unread: !n.read }}
              data-notification={n.id}
              onClick={() => markRead(n.id)}
              onContextMenu={(e) => showEntryMenu(e, n)}
            >
              <TypeIcon type={n.type} />
              <div class="notif-main">
                <div class="notif-title">{n.title}</div>
                <Show when={n.content}>
                  <div class="notif-content">{n.content}</div>
                </Show>
                <Show when={n.actions?.length}>
                  <div class="notif-actions">
                    <For each={n.actions}>
                      {(a) => (
                        <button
                          class="balloon-action"
                          onClick={(e) => {
                            e.stopPropagation();
                            markRead(n.id);
                            a.run();
                          }}
                        >
                          {a.label}
                        </button>
                      )}
                    </For>
                  </div>
                </Show>
                <div class="notif-meta">
                  <span>{timeLabel(n.time)}</span>
                  <span class="notif-group">{n.group}</span>
                </div>
              </div>
              <button
                class="icon-btn notif-remove"
                title="Remove"
                onClick={(e) => {
                  e.stopPropagation();
                  removeNotification(n.id);
                }}
              >
                <X size={13} />
              </button>
            </div>
          )}
        </For>
      </Show>
    </div>
  );
}
