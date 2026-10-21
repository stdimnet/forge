import { beforeEach, describe, expect, it } from "vitest";
import { balloons, showBalloon } from "@/keymap/dispatch";
import { timeLabel } from "@/features/notifications/NotificationsView";
import {
  MAX_NOTIFICATIONS,
  clearNotifications,
  displayMode,
  logNotification,
  markAllRead,
  notifications,
  parseGroupModes,
  unreadCount,
  withGroupMode,
} from "./notifications";
import { setLocalSetting } from "./settings";

describe("notifications", () => {
  beforeEach(() => {
    clearNotifications();
    setLocalSetting("notifications.balloons", true);
    setLocalSetting("notifications.groups", []);
  });

  it("reads and writes the display mode per group", () => {
    const modes = parseGroupModes(["Git\tsticky", "Run\tlog", "Bad\tpopup", "\tlog"]);
    expect([...modes]).toEqual([
      ["Git", "sticky"],
      ["Run", "log"],
    ]);
    expect(withGroupMode(["Git\tsticky", "Run\tlog"], "Git", "log")).toEqual(["Run\tlog", "Git\tlog"]);
    expect(withGroupMode(["Git\tsticky"], "Git", "balloon")).toEqual([]);
    expect(displayMode("Git", true, modes)).toBe("sticky");
    expect(displayMode("Other", true, modes, new Map([["Other", "log"]]))).toBe("log");
    expect(displayMode("Git", false, modes)).toBe("log");
  });

  it("keeps a bounded history with read state", () => {
    for (let i = 0; i < MAX_NOTIFICATIONS + 5; i++) logNotification({ group: "Forge", type: "info", title: `n${i}` });
    expect(notifications()).toHaveLength(MAX_NOTIFICATIONS);
    expect(notifications()[0].title).toBe(`n${MAX_NOTIFICATIONS + 4}`);
    expect(unreadCount()).toBe(MAX_NOTIFICATIONS);
    markAllRead();
    expect(unreadCount()).toBe(0);
  });

  it("logs balloons and honours log-only groups", () => {
    setLocalSetting("notifications.groups", ["Quiet\tlog"]);
    const shown = balloons().length;
    showBalloon("Indexed", "12 files", 2600, undefined, { group: "Quiet" });
    expect(balloons()).toHaveLength(shown);
    showBalloon("Copied", undefined, 2600, undefined, { log: false });
    expect(notifications().map((n) => n.title)).toEqual(["Indexed"]);
    expect(notifications()[0].group).toBe("Quiet");
  });

  it("formats times like the tool window", () => {
    const now = new Date(2026, 9, 21, 15, 0).getTime();
    expect(timeLabel(new Date(2026, 9, 21, 9, 5).getTime(), now)).toBe("09:05");
    expect(timeLabel(new Date(2026, 9, 20, 23, 59).getTime(), now)).toBe("Yesterday 23:59");
    expect(timeLabel(new Date(2026, 8, 3, 8, 0).getTime(), now)).toBe("Sep 3 08:00");
  });
});
