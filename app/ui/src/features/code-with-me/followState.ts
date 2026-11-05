/**
 * Follow, as a state machine without the editor: whom this window follows, whether the host
 * asked for it (Make Everyone Follow You), where following leads, and what the banner says.
 * `follow.ts` feeds it with presence and local actions and drives the editor.
 */
import type { Participant, Presence } from "@/ipc/collab";
import { GUEST_PREFIX } from "./docs";
import { t } from "@/i18n";

export interface FollowState {
  /** The participant this window follows. */
  leader: number | null;
  /** The host asked everyone to follow it. */
  forced: boolean;
}

export const NOT_FOLLOWING: FollowState = { leader: null, forced: false };

export type FollowEvent =
  /** Follow (participant list, toolbar avatar, Follow Participant…). */
  | { type: "follow"; leader: number; me: number | undefined }
  /** Stop in the banner, Stop Following. */
  | { type: "stop" }
  /** Typing, clicking into the editor or opening another file, as in PHPStorm. */
  | { type: "local" }
  /** Make Everyone Follow You turned on or off by the host. */
  | { type: "forced"; on: boolean; host: number }
  /** The participants changed: following someone who left ends. */
  | { type: "participants"; ids: number[] }
  /** The session ended. */
  | { type: "ended" };

export function nextFollow(s: FollowState, e: FollowEvent): FollowState {
  switch (e.type) {
    case "follow":
      return e.leader === e.me || e.leader === s.leader ? s : { leader: e.leader, forced: false };
    case "stop":
    case "local":
    case "ended":
      return s.leader === null ? s : NOT_FOLLOWING;
    case "forced":
      if (e.on) return { leader: e.host, forced: true };
      return s.forced ? NOT_FOLLOWING : s;
    case "participants":
      return s.leader !== null && !e.ids.includes(s.leader) ? NOT_FOLLOWING : s;
  }
}

/** Project files only (not scratches, consoles or other pseudo paths). */
const isProjectFile = (path: string) => !path.includes("://") && !path.startsWith("forge:");

/** The workspace-relative file of a tab, as presence names it (`undefined` for other tabs). */
export function presenceFile(tabPath: string | null | undefined): string | undefined {
  if (!tabPath) return undefined;
  if (tabPath.startsWith(GUEST_PREFIX)) return tabPath.slice(GUEST_PREFIX.length) || undefined;
  return isProjectFile(tabPath) ? tabPath : undefined;
}

/** The tab a participant's file opens in: guests see the host's files as `cwm://` tabs. */
export function tabFor(file: string, role: "host" | "guest" | "none"): string {
  return role === "guest" ? GUEST_PREFIX + file : file;
}

export interface FollowTarget {
  tab: string;
  /** 1-based line to put at the top of the editor. */
  topLine?: number;
  /** The leader's caret, when its viewport is unknown. */
  caret?: { line: number; column: number };
}

/** Where following `presence` leads; `null` while the leader is in no file this window may see. */
export function followTarget(presence: Presence | undefined, role: "host" | "guest" | "none"): FollowTarget | null {
  if (!presence?.file) return null;
  return {
    tab: tabFor(presence.file, role),
    topLine: presence.viewport?.topLine,
    caret: presence.caret,
  };
}

/** The banner text: "Following Alex", or "Robert asked everyone to follow" (forced). */
export function bannerText(s: FollowState, participants: Participant[]): string | undefined {
  if (s.leader === null) return undefined;
  const leader = participants.find((p) => p.id === s.leader);
  if (!leader) return undefined;
  return s.forced
    ? t("tools.codeWithMe.askedEveryoneToFollow", { name: leader.name })
    : t("tools.codeWithMe.followingLeader", { name: leader.name });
}

/** "OrderController.php:42" for the participant list. */
export function locationLabel(p: Presence | undefined): string | undefined {
  if (!p?.file) return undefined;
  const name = p.file.split("/").pop() ?? p.file;
  return p.caret ? `${name}:${p.caret.line}` : name;
}

/** The participants following `id`. */
export const followersOf = (id: number, presence: Map<number, Presence>): number[] =>
  [...presence.entries()].filter(([, p]) => p.following === id).map(([who]) => who);
