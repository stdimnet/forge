import { describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import "@/mock/settingsMock";
import "@/mock/keymapMock";
import type { EditorTab } from "@/state/editor";
import type { Cell } from "@/ipc/db";

const mocks = vi.hoisted(() => ({
  grid: undefined as undefined | { rows: Cell[][]; onSort: (column: string) => void },
  sorts: [] as { desc: boolean; resolve: (order: number[]) => void }[],
}));

vi.mock("@/features/database/DataGrid", () => ({
  DataGrid: (props: { rows: Cell[][]; onSort: (column: string) => void }) => {
    mocks.grid = props;
    return null;
  },
}));
vi.mock("@/editor/CodeEditor", () => ({ CodeEditor: () => null }));
vi.mock("@/ipc/viewers", async (orig) => {
  const real = await orig<typeof import("@/ipc/viewers")>();
  const rows = [
    ["name", "n"],
    ["b", "2"],
    ["a", "1"],
    ["c", "3"],
  ];
  return {
    ...real,
    viewersApi: {
      ...real.viewersApi,
      csvParse: () => Promise.resolve({ rows, columns: 2, delimiter: "," }),
      csvSortOrder: (_rows: string[][], _column: number, desc: boolean) =>
        new Promise<number[]>((resolve) => mocks.sorts.push({ desc, resolve })),
    },
  };
});

const { CsvEditor, setCsvMode } = await import("./CsvEditor");

const tick = () => new Promise((r) => setTimeout(r, 0));
const names = () => mocks.grid!.rows.map((r) => r[0]);

describe("CSV table", () => {
  it("keeps the newest sort order when an older reply arrives late", async () => {
    const tab = { path: "data.csv", name: "data.csv", content: "name,n\nb,2\na,1\nc,3\n" } as EditorTab;
    setCsvMode(tab.path, "table");
    const dispose = render(() => <CsvEditor tab={tab} />, document.createElement("div"));
    await tick();
    expect(names()).toEqual(["b", "a", "c"]);

    mocks.grid!.onSort("name");
    mocks.grid!.onSort("name");
    expect(mocks.sorts.map((s) => s.desc)).toEqual([false, true]);
    // The descending order (asked for last) answers first, the ascending one after it.
    mocks.sorts[1].resolve([0, 3, 1, 2]);
    await tick();
    expect(names()).toEqual(["c", "b", "a"]);
    mocks.sorts[0].resolve([0, 2, 1, 3]);
    await tick();
    expect(names()).toEqual(["c", "b", "a"]);
    dispose();
  });
});
