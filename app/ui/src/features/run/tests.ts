/**
 * Render state of the Run tool window: console lines and the test tree, both computed by the
 * backend (`forge-output`) and applied here as deltas / node updates.
 */
import type { ConsoleDelta, ConsoleLine, TestNodeData, TestSummary } from "@/ipc/run";

export type { TestStatus } from "@/ipc/run";

export interface TestNode extends TestNodeData {
  children: TestNode[];
  parent?: TestNode;
}

export interface TestTreeState {
  root: TestNode;
  byId: Map<number, TestNode>;
  summary: TestSummary;
}

/** Placeholder until the backend sends the tree. */
export function emptyTestTree(): TestTreeState {
  const root: TestNode = { id: 0, name: "Test Results", kind: "root", status: "running", durationText: "", console: [], children: [] };
  const counts = { total: 0, passed: 0, failed: 0, ignored: 0, running: 0 };
  return { root, byId: new Map([[0, root]]), summary: { text: "Running tests…", failed: false, done: 0, total: 0, counts } };
}

/** Insert or update nodes (parents arrive before their children). */
export function applyTestUpdates(tree: TestTreeState, nodes: TestNodeData[], summary: TestSummary): void {
  for (const data of nodes) {
    const known = tree.byId.get(data.id);
    if (known) {
      Object.assign(known, data);
      continue;
    }
    const parent = data.parentId !== undefined ? tree.byId.get(data.parentId) : undefined;
    const node: TestNode = { ...data, children: [], parent };
    tree.byId.set(data.id, node);
    parent?.children.push(node);
  }
  tree.summary = summary;
}

/** Apply a console delta in place. Lines are replaced, not mutated, so keyed renderers redraw only changed ones. */
export function applyConsoleDelta(lines: ConsoleLine[], delta: ConsoleDelta): void {
  if (delta.replaceLast && delta.lines.length && lines.length) lines.pop();
  lines.push(...delta.lines);
  if (delta.dropFront) lines.splice(0, delta.dropFront);
}

/** Text of a console line without styles. */
export const lineText = (line: ConsoleLine) => line.segments.map((s) => s.text).join("");
