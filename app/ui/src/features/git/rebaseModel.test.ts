import { describe, expect, it } from "vitest";
import type { RebasePlan } from "./logApi";
import { changed, initialRows, messageRow, moveRows, moveTo, problem, setAction, setMessage, toEntries, unite } from "./rebaseModel";

const sha = (n: number) => String(n).repeat(40).slice(0, 40);
const plan: RebasePlan = {
  base: sha(9),
  branch: "main",
  // Oldest first, like Rust sends them.
  commits: [1, 2, 3, 4].map((n) => ({ sha: sha(n), subject: `Commit ${n}`, message: `Commit ${n}\n\nBody ${n}`, author: "Marlon", time: n })),
  entries: [1, 2, 3, 4].map((n) => ({ sha: sha(n), action: "pick" as const })),
};
const order = (rows: { sha: string }[]) => rows.map((r) => r.sha[0]).join("");
const actions = (rows: { action: string }[]) => rows.map((r) => r.action).join(",");

describe("rebase dialog model", () => {
  it("shows the newest commit first and sends the oldest first", () => {
    const rows = initialRows(plan);
    expect(order(rows)).toBe("4321");
    expect(toEntries(rows).map((e) => e.sha[0])).toEqual(["1", "2", "3", "4"]);
    expect(problem(rows)).toBeNull();
    expect(changed(rows, initialRows(plan))).toBe(false);
  });

  it("moves rows up and down and by drag and drop", () => {
    const rows = initialRows(plan);
    expect(order(moveRows(rows, new Set([sha(2)]), -1))).toBe("4231");
    expect(order(moveRows(rows, new Set([sha(4), sha(3)]), 1))).toBe("2431");
    expect(order(moveRows(rows, new Set([sha(4)]), -1))).toBe("4321");
    expect(order(moveTo(rows, new Set([sha(1)]), 0))).toBe("1432");
    expect(order(moveTo(rows, new Set([sha(4), sha(3)]), 4))).toBe("2143");
  });

  it("rewords with an editable message; unchanged rewords stay picks", () => {
    let rows = setAction(initialRows(plan), new Set([sha(3)]), "reword");
    expect(rows[1].newMessage).toBe("Commit 3\n\nBody 3");
    expect(toEntries(rows)[2]).toEqual({ sha: sha(3), action: "pick" });
    rows = setMessage(rows, 1, "Better");
    expect(toEntries(rows)[2]).toEqual({ sha: sha(3), action: "reword", message: "Better" });
  });

  it("squashes combine the messages of the group on its newest squash", () => {
    let rows = setAction(initialRows(plan), new Set([sha(3)]), "squash");
    expect(rows[1].newMessage).toBe("Commit 2\n\nBody 2\n\nCommit 3\n\nBody 3");
    rows = setMessage(rows, 2, "United");
    expect(rows[1].newMessage).toBe("United");
    rows = setAction(rows, new Set([sha(4)]), "squash");
    expect(rows[0].newMessage).toBe("United\n\nCommit 4\n\nBody 4");
    expect(rows[1].newMessage).toBeNull();
    expect(messageRow(rows, 2)).toBe(0);
    const entries = toEntries(rows);
    expect(entries.map((e) => e.action)).toEqual(["pick", "pick", "squash", "squash"]);
    expect(entries[3].message).toContain("United");
  });

  it("unites selected commits above the oldest of them", () => {
    const rows = unite(initialRows(plan), new Set([sha(4), sha(2)]));
    expect(order(rows)).toBe("3421");
    expect(actions(rows)).toBe("pick,squash,pick,pick");
    expect(rows[1].newMessage).toBe("Commit 2\n\nBody 2\n\nCommit 4\n\nBody 4");
  });

  it("reports squashes without an older commit", () => {
    const rows = setAction(initialRows(plan), new Set([sha(1)]), "fixup");
    expect(problem(rows)).toContain("Commit 1");
    const dropped = setAction(setAction(initialRows(plan), new Set([sha(1)]), "drop"), new Set([sha(2)]), "fixup");
    expect(problem(dropped)).toContain("Commit 2");
  });
});
