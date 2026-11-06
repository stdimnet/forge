import { describe, expect, it } from "vitest";
import type { LightEditSnapshot } from "@/ipc/lightEdit";
import { closedTabs, isLightEditTab, planSync } from "./sync";

const file = (path: string) => ({ id: `light-edit:${path}`, path, name: path.split("/").pop()! });

describe("LightEdit tab sync", () => {
  it("opens only the files that have no tab and shows the requested one", () => {
    const snapshot: LightEditSnapshot = {
      files: [file("/home/me/notes.md"), file("/etc/hosts")],
      active: "light-edit:/etc/hosts",
      reveal: { id: "light-edit:/etc/hosts", line: 3, column: null },
    };
    const plan = planSync(snapshot, ["light-edit:/home/me/notes.md"]);
    expect(plan.toOpen).toEqual(["light-edit:/etc/hosts"]);
    expect(plan.show).toEqual({ id: "light-edit:/etc/hosts", line: 3, column: null });
  });

  it("reuses the tab of a file opened again", () => {
    // A second "Open With" of notes.md: Rust keeps one tab and makes it active.
    const snapshot: LightEditSnapshot = {
      files: [file("/home/me/notes.md"), file("/etc/hosts")],
      active: "light-edit:/home/me/notes.md",
      reveal: { id: "light-edit:/home/me/notes.md", line: null, column: null },
    };
    const plan = planSync(snapshot, ["light-edit:/home/me/notes.md", "light-edit:/etc/hosts"]);
    expect(plan.toOpen).toEqual([]);
    expect(plan.show?.id).toBe("light-edit:/home/me/notes.md");
  });

  it("falls back to the active tab and reports closed tabs", () => {
    const plan = planSync({ files: [file("/a.txt")], active: "light-edit:/a.txt", reveal: null }, []);
    expect(plan.show).toEqual({ id: "light-edit:/a.txt", line: null, column: null });
    expect(closedTabs(["light-edit:/a.txt", "light-edit:/b.txt"], ["light-edit:/b.txt"])).toEqual(["light-edit:/a.txt"]);
    expect(isLightEditTab("light-edit:/a.txt")).toBe(true);
    expect(isLightEditTab("app/Models/User.php")).toBe(false);
  });
});
