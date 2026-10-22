/**
 * Editing state of the "Rebasing Commits" dialog: rows in log order (newest first) with their
 * action and new message, moving rows, Unite and the plan sent to Rust (oldest first). The
 * todo file itself is generated in Rust (`forge_vcs::rebase::plan_todo`).
 *
 * A squash or fixup row melds into the closest older row that is neither squash, fixup nor
 * drop (its "group head"). The message of a group with squashes is kept on its newest squash.
 */
import type { RebaseAction, RebaseEntry, RebasePlan } from "./logApi";

export interface RebaseRow {
  sha: string;
  subject: string;
  /** The original full message. */
  message: string;
  author: string;
  time: number;
  action: RebaseAction;
  /** Reword: the new message; squash: the message of the group (on its newest squash). */
  newMessage: string | null;
}

const melds = (a: RebaseAction) => a === "squash" || a === "fixup";
const passes = (a: RebaseAction) => melds(a) || a === "drop";

/** Rows of a plan, newest first. */
export function initialRows(plan: RebasePlan): RebaseRow[] {
  const bySha = new Map(plan.commits.map((c) => [c.sha, c]));
  const rows = plan.entries.map((e): RebaseRow => {
    const c = bySha.get(e.sha);
    return {
      sha: e.sha,
      subject: c?.subject ?? e.sha.slice(0, 8),
      message: c?.message ?? "",
      author: c?.author ?? "",
      time: c?.time ?? 0,
      action: e.action,
      newMessage: e.message ?? null,
    };
  });
  return normalize(rows.reverse());
}

/** Index of the row that row `i` melds into, or null when there is none. */
export function groupHead(rows: RebaseRow[], i: number): number | null {
  for (let j = i + 1; j < rows.length; j++) if (!passes(rows[j].action)) return j;
  return null;
}

/** Indices of the squash/fixup/drop rows that belong to head `h` (newest last). */
function members(rows: RebaseRow[], h: number): number[] {
  const out: number[] = [];
  for (let j = h - 1; j >= 0 && passes(rows[j].action); j--) out.push(j);
  return out;
}

/** The row whose `newMessage` holds the message of row `i`'s group, if the group has one. */
export function messageRow(rows: RebaseRow[], i: number): number | null {
  const row = rows[i];
  if (!row) return null;
  if (row.action === "reword") {
    const squashes = members(rows, i).filter((j) => rows[j].action === "squash");
    return squashes.length ? squashes[squashes.length - 1] : i;
  }
  const h = melds(row.action) ? groupHead(rows, i) : passes(row.action) ? null : i;
  if (h === null) return row.action === "squash" ? i : null;
  const squashes = members(rows, h).filter((j) => rows[j].action === "squash");
  return squashes.length ? squashes[squashes.length - 1] : rows[h].action === "reword" ? h : null;
}

/** Keep each group's message on its newest squash; fill in combined messages. */
function normalize(rows: RebaseRow[]): RebaseRow[] {
  const out = rows.map((r) => ({ ...r }));
  for (let h = out.length - 1; h >= 0; h--) {
    if (passes(out[h].action)) continue;
    const squashes = members(out, h).filter((j) => out[j].action === "squash");
    if (!squashes.length) continue;
    const keep = squashes[squashes.length - 1];
    const existing = [keep, ...squashes].map((j) => out[j].newMessage).find((m) => m !== null) ?? null;
    const combined = [out[h].action === "reword" && out[h].newMessage !== null ? out[h].newMessage! : out[h].message, ...squashes.map((j) => out[j].message)]
      .map((m) => m.trim())
      .join("\n\n");
    for (const j of squashes) out[j].newMessage = null;
    out[keep].newMessage = existing ?? combined;
  }
  for (const r of out) if (r.action !== "reword" && r.action !== "squash") r.newMessage = null;
  return out;
}

/** Set the action of the selected rows (Pick, Edit, Reword, Squash, Fixup, Drop). */
export function setAction(rows: RebaseRow[], shas: Set<string>, action: RebaseAction): RebaseRow[] {
  const out = rows.map((r) => ({ ...r }));
  for (let i = out.length - 1; i >= 0; i--) {
    const r = out[i];
    if (!shas.has(r.sha) || r.action === action) continue;
    if (action === "reword") r.newMessage = r.message;
    if (action === "squash") {
      // Add this commit's message to the group's message, if the group already has one.
      const h = groupHead(out, i);
      const at = h === null ? null : messageRow(out, h);
      r.action = action;
      if (at !== null && out[at].newMessage !== null) {
        r.newMessage = `${out[at].newMessage!.trim()}\n\n${r.message.trim()}`;
        out[at].newMessage = null;
      } else r.newMessage = null;
      continue;
    }
    r.action = action;
  }
  return normalize(out);
}

/** Change the message of row `i`'s group (or of the reworded row). */
export function setMessage(rows: RebaseRow[], i: number, message: string): RebaseRow[] {
  const at = messageRow(rows, i);
  if (at === null) return rows;
  return rows.map((r, j) => (j === at ? { ...r, newMessage: message } : r));
}

/** Move the selected rows one step up (newer, `-1`) or down (older, `1`). */
export function moveRows(rows: RebaseRow[], shas: Set<string>, dir: -1 | 1): RebaseRow[] {
  const out = rows.slice();
  const order = dir === -1 ? out.map((_, i) => i) : out.map((_, i) => out.length - 1 - i);
  for (const i of order) {
    const j = i + dir;
    if (!shas.has(out[i].sha) || j < 0 || j >= out.length || shas.has(out[j].sha)) continue;
    [out[i], out[j]] = [out[j], out[i]];
  }
  return normalize(out);
}

/** Drag and drop: put the dragged rows (in their order) before row `target` (`rows.length` = at the end). */
export function moveTo(rows: RebaseRow[], shas: Set<string>, target: number): RebaseRow[] {
  const moved = rows.filter((r) => shas.has(r.sha));
  const before = rows.slice(0, target).filter((r) => !shas.has(r.sha)).length;
  const rest = rows.filter((r) => !shas.has(r.sha));
  rest.splice(before, 0, ...moved);
  return normalize(rest);
}

/** Unite: the selected commits become squashes right above (newer than) the oldest selected one. */
export function unite(rows: RebaseRow[], shas: Set<string>): RebaseRow[] {
  const selected = rows.filter((r) => shas.has(r.sha));
  if (selected.length < 2) return setAction(rows, shas, "squash");
  const oldest = selected[selected.length - 1];
  const others = selected.slice(0, -1);
  const moving = new Set(others.map((r) => r.sha));
  const rest = rows.filter((r) => !moving.has(r.sha));
  const at = rest.findIndex((r) => r.sha === oldest.sha);
  rest.splice(at, 0, ...others.map((r): RebaseRow => ({ ...r, action: "squash", newMessage: null })));
  return normalize(rest);
}

/** The plan for Rust, oldest first; unchanged rewords become picks. */
export function toEntries(rows: RebaseRow[]): RebaseEntry[] {
  return rows
    .slice()
    .reverse()
    .map((r): RebaseEntry => {
      if (r.action === "reword") return r.newMessage !== null && r.newMessage !== r.message ? { sha: r.sha, action: "reword", message: r.newMessage } : { sha: r.sha, action: "pick" };
      if (r.action === "squash") return { sha: r.sha, action: "squash", message: r.newMessage };
      return { sha: r.sha, action: r.action };
    });
}

/** Why the plan cannot start, or null. */
export function problem(rows: RebaseRow[]): string | null {
  for (let i = 0; i < rows.length; i++) {
    const r = rows[i];
    if (melds(r.action) && groupHead(rows, i) === null) return `"${r.subject}" has no older commit to ${r.action === "squash" ? "squash" : "fix up"} into`;
    if ((r.action === "reword" || r.action === "squash") && r.newMessage !== null && !r.newMessage.trim()) return "The commit message is empty";
  }
  return null;
}

/** Whether the rows still match the initial plan (Reset is enabled otherwise). */
export function changed(rows: RebaseRow[], initial: RebaseRow[]): boolean {
  return rows.length !== initial.length || rows.some((r, i) => r.sha !== initial[i].sha || r.action !== initial[i].action || r.newMessage !== initial[i].newMessage);
}
