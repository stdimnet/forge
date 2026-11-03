import { describe, expect, it } from "vitest";
import type { Participant } from "@/ipc/collab";
import { PRESETS } from "@/ipc/collab";
import { bannerText, followersOf, followTarget, locationLabel, nextFollow, NOT_FOLLOWING, presenceFile, type FollowState } from "./followState";

const host: Participant = { id: 0, name: "Robert", role: "host", color: 1, permissions: PRESETS.fullAccess };
const alex: Participant = { id: 1, name: "Alex", role: "guest", color: 2, permissions: PRESETS.readOnly };

describe("follow state machine", () => {
  it("follows someone else, never oneself", () => {
    expect(nextFollow(NOT_FOLLOWING, { type: "follow", leader: 1, me: 0 })).toEqual({ leader: 1, forced: false });
    expect(nextFollow(NOT_FOLLOWING, { type: "follow", leader: 0, me: 0 })).toBe(NOT_FOLLOWING);
  });

  it("stops on Stop, on the user's own typing, clicking or file switch, and when the session ends", () => {
    const s: FollowState = { leader: 1, forced: false };
    for (const type of ["stop", "local", "ended"] as const) expect(nextFollow(s, { type })).toEqual(NOT_FOLLOWING);
    // Nothing changes when not following.
    expect(nextFollow(NOT_FOLLOWING, { type: "local" })).toBe(NOT_FOLLOWING);
  });

  it("Make Everyone Follow You forces following the host, and guests can still stop", () => {
    const forced = nextFollow({ leader: 2, forced: false }, { type: "forced", on: true, host: 0 });
    expect(forced).toEqual({ leader: 0, forced: true });
    expect(nextFollow(forced, { type: "local" })).toEqual(NOT_FOLLOWING);
    // Following the host again keeps the request; turning it off releases forced followers only.
    expect(nextFollow(forced, { type: "follow", leader: 0, me: 1 })).toBe(forced);
    expect(nextFollow(forced, { type: "forced", on: false, host: 0 })).toEqual(NOT_FOLLOWING);
    const own = { leader: 0, forced: false };
    expect(nextFollow(own, { type: "forced", on: false, host: 0 })).toBe(own);
  });

  it("ends following when the leader leaves", () => {
    const s = { leader: 2, forced: false };
    expect(nextFollow(s, { type: "participants", ids: [0, 1, 2] })).toBe(s);
    expect(nextFollow(s, { type: "participants", ids: [0, 1] })).toEqual(NOT_FOLLOWING);
  });
});

describe("where following leads", () => {
  it("opens the host's files as shared tabs for guests and as files for the host", () => {
    const p = { file: "app/Order.php", caret: { line: 12, column: 3 }, viewport: { topLine: 4, bottomLine: 40 } };
    expect(followTarget(p, "guest")).toEqual({ tab: "cwm://app/Order.php", topLine: 4, caret: { line: 12, column: 3 } });
    expect(followTarget(p, "host")?.tab).toBe("app/Order.php");
    // A leader in a file this window may not see (or in none): stay where you are.
    expect(followTarget({}, "guest")).toBeNull();
    expect(followTarget(undefined, "guest")).toBeNull();
  });

  it("names only project files in the presence it sends", () => {
    expect(presenceFile("cwm://app/Order.php")).toBe("app/Order.php");
    expect(presenceFile("app/Order.php")).toBe("app/Order.php");
    expect(presenceFile("db-console://shop/1")).toBeUndefined();
    expect(presenceFile("forge:welcome")).toBeUndefined();
    expect(presenceFile(null)).toBeUndefined();
  });
});

describe("texts", () => {
  it("says whom you follow and whether the host asked", () => {
    expect(bannerText({ leader: 1, forced: false }, [host, alex])).toBe("Following Alex");
    expect(bannerText({ leader: 0, forced: true }, [host, alex])).toBe("Robert asked everyone to follow");
    expect(bannerText(NOT_FOLLOWING, [host, alex])).toBeUndefined();
    expect(bannerText({ leader: 7, forced: false }, [host, alex])).toBeUndefined();
  });

  it("shows where participants are and who follows whom", () => {
    expect(locationLabel({ file: "app/Http/OrderController.php", caret: { line: 42, column: 1 } })).toBe("OrderController.php:42");
    expect(locationLabel({ file: "README.md" })).toBe("README.md");
    expect(locationLabel({})).toBeUndefined();
    const presence = new Map([
      [1, { following: 0 }],
      [2, { following: 1 }],
      [3, { following: 0 }],
    ]);
    expect(followersOf(0, presence)).toEqual([1, 3]);
  });
});
