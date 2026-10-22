/** Test results of finished runs, for listeners such as the gutter icons. */
import type { TestNode, TestStatus } from "./tests";

type Listener = (entries: [string, TestStatus][]) => void;
const listeners: Listener[] = [];

export function onTestResults(l: Listener): void {
  listeners.push(l);
}

/** The project file of a node: its own location or the nearest suite's. */
export const fileOf = (n: TestNode | undefined): string | undefined => {
  for (let x = n; x; x = x.parent) if (x.location?.workspacePath) return x.location.workspacePath;
  return undefined;
};

/**
 * Result keys of a finished tree, as `test_discover` names its targets: `<path>` for suites
 * with a file and `<path>::<test>` for tests (by method / member and by name; data sets and
 * example rows count for their test, a failed one fails it).
 */
export function resultKeys(root: TestNode): Map<string, TestStatus> {
  const out = new Map<string, TestStatus>();
  const put = (key: string, status: TestStatus) => {
    const known = out.get(key);
    if (!known || status === "failed" || (known === "ignored" && status === "passed")) out.set(key, status);
  };
  const walk = (n: TestNode) => {
    const path = fileOf(n);
    if (n.kind === "suite" && n.location?.workspacePath) put(n.location.workspacePath, n.status);
    if (n.kind === "test" && path) {
      const names = new Set([n.location?.member, n.name].filter((x): x is string => !!x));
      for (const name of [...names]) {
        names.add(name.replace(/ with (data set .*|\(.*\))$/, ""));
        names.add(name.replace(/ #\d+$/, ""));
      }
      for (const name of names) put(`${path}::${name}`, n.status);
    }
    n.children.forEach(walk);
  };
  walk(root);
  return out;
}

/** Publish the results of a finished tree. */
export function publishTestResults(root: TestNode): void {
  const entries = [...resultKeys(root)];
  if (entries.length) listeners.forEach((l) => l(entries));
}
