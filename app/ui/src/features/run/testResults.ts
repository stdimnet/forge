/** Test results of finished runs, for listeners such as the gutter icons. */
import type { TestNode, TestStatus } from "./teamcity";
import { statusOf } from "./TestTree";

type Listener = (entries: [string, TestStatus][]) => void;
const listeners: Listener[] = [];

export function onTestResults(l: Listener): void {
  listeners.push(l);
}

/** Publish the results of a finished tree as `Fqcn` and `Fqcn::method` keys. */
export function publishTestResults(root: TestNode): void {
  const entries: [string, TestStatus][] = [];
  const walk = (n: TestNode, suite: string | undefined) => {
    if (n.kind === "suite") {
      const fqcn = n.location?.member ?? (n.name.includes("\\") || /Test$/.test(n.name) ? n.name : undefined);
      if (fqcn) entries.push([fqcn, statusOf(n)]);
      n.children.forEach((c) => walk(c, fqcn ?? suite));
    } else if (n.kind === "test") {
      if (suite) entries.push([`${suite}::${n.location?.member ?? n.name}`, n.status]);
    } else n.children.forEach((c) => walk(c, suite));
  };
  walk(root, undefined);
  if (entries.length) listeners.forEach((l) => l(entries));
}
