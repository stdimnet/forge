/**
 * TeamCity service messages (`phpunit --teamcity`, Pest, Jest reporters) turned into the
 * test tree of the Run tool window.
 */

export interface ServiceMessage {
  name: string;
  attrs: Record<string, string>;
}

const PREFIX = "##teamcity[";

/** Undo TeamCity escaping: |' |n |r |[ |] || |0xNNNN. */
export function unescapeValue(v: string): string {
  return v.replace(/\|(0x[0-9a-fA-F]{4}|.)/g, (_, c: string) => {
    if (c.length > 1) return String.fromCharCode(Number.parseInt(c.slice(2), 16));
    return ({ n: "\n", r: "\r", "'": "'", "|": "|", "[": "[", "]": "]" } as Record<string, string>)[c] ?? c;
  });
}

/** Parse one line; `null` when it is not a service message. */
export function parseServiceMessage(line: string): ServiceMessage | null {
  const start = line.indexOf(PREFIX);
  if (start < 0) return null;
  const body = line.slice(start + PREFIX.length);
  const nameMatch = /^([\w.-]+)/.exec(body);
  if (!nameMatch) return null;
  const attrs: Record<string, string> = {};
  const re = /([\w.-]+)='((?:\|.|[^|'])*)'/g;
  let m: RegExpExecArray | null;
  const rest = body.slice(nameMatch[1].length);
  while ((m = re.exec(rest))) attrs[m[1]] = unescapeValue(m[2]);
  return { name: nameMatch[1], attrs };
}

export type TestStatus = "running" | "passed" | "failed" | "ignored" | "pending";

export interface TestLocation {
  /** File path as reported (absolute or relative). */
  path: string;
  /** Class or method name after `::`, used to find the line. */
  member?: string;
  line?: number;
}

export interface TestNode {
  id: number;
  name: string;
  kind: "root" | "suite" | "test";
  status: TestStatus;
  /** Milliseconds. */
  duration?: number;
  location?: TestLocation;
  message?: string;
  details?: string;
  expected?: string;
  actual?: string;
  /** Output printed while the test ran. */
  output: string;
  children: TestNode[];
  parent?: TestNode;
}

/** `php_qn:///abs/tests/UserTest.php::\Tests\UserTest::test_x`, `file:///…:12`, ... */
export function parseLocationHint(hint: string | undefined): TestLocation | undefined {
  if (!hint) return undefined;
  const m = /^(?:php_qn|file|pest_qn|python):\/\/(.*)$/.exec(hint);
  const rest = m ? m[1] : hint;
  const [path, ...members] = rest.split("::");
  const lineMatch = /^(.*?):(\d+)$/.exec(path);
  const loc: TestLocation = lineMatch ? { path: lineMatch[1], line: Number(lineMatch[2]) } : { path };
  const member = members[members.length - 1];
  if (member) loc.member = member.replace(/^\\/, "").replace(/ with data set .*$/, "");
  return loc;
}

/** Builds and updates a test tree from service messages. */
export class TestTreeBuilder {
  readonly root: TestNode;
  /** Number of tests announced by `testCount`. */
  expectedCount?: number;
  private current: TestNode;
  private seq = 0;
  private running = new Map<string, TestNode>();

  constructor(rootName = "Test Results") {
    this.root = this.node(rootName, "root");
    this.current = this.root;
  }

  private node(name: string, kind: TestNode["kind"], parent?: TestNode): TestNode {
    const n: TestNode = { id: this.seq++, name, kind, status: "running", output: "", children: [], parent };
    parent?.children.push(n);
    return n;
  }

  /** Apply one message; returns whether the tree changed. */
  apply(msg: ServiceMessage): boolean {
    const a = msg.attrs;
    switch (msg.name) {
      case "testCount":
        this.expectedCount = Number(a.count) || undefined;
        return true;
      case "testSuiteStarted": {
        const suite = this.node(a.name ?? "", "suite", this.current);
        suite.location = parseLocationHint(a.locationHint);
        this.current = suite;
        return true;
      }
      case "testSuiteFinished": {
        if (this.current !== this.root) {
          this.current.status = suiteStatus(this.current);
          this.current.duration = sumDurations(this.current);
          this.current = this.current.parent ?? this.root;
        }
        return true;
      }
      case "testStarted": {
        const t = this.node(a.name ?? "", "test", this.current);
        t.location = parseLocationHint(a.locationHint);
        this.running.set(a.name ?? "", t);
        return true;
      }
      case "testFailed": {
        const t = this.find(a.name);
        if (!t) return false;
        t.status = "failed";
        t.message = a.message;
        t.details = a.details;
        if (a.expected !== undefined || a.actual !== undefined) {
          t.expected = a.expected;
          t.actual = a.actual;
        }
        return true;
      }
      case "testIgnored": {
        const t = this.find(a.name) ?? this.node(a.name ?? "", "test", this.current);
        t.status = "ignored";
        t.message = a.message;
        return true;
      }
      case "testStdOut":
      case "testStdErr": {
        const t = this.find(a.name);
        if (!t) return false;
        t.output += a.out ?? "";
        return true;
      }
      case "testFinished": {
        const t = this.find(a.name);
        if (!t) return false;
        if (t.status === "running") t.status = "passed";
        const d = Number(a.duration);
        if (!Number.isNaN(d)) t.duration = d;
        this.running.delete(a.name ?? "");
        return true;
      }
      default:
        return false;
    }
  }

  private find(name: string | undefined): TestNode | undefined {
    return this.running.get(name ?? "") ?? findLast(this.current, (n) => n.kind === "test" && n.name === name);
  }

  /** Close the tree when the process ends: unfinished tests become terminated (failed). */
  finish(exitCode: number | null): void {
    const close = (n: TestNode) => {
      n.children.forEach(close);
      if (n.kind === "test" && n.status === "running") n.status = exitCode === 0 ? "passed" : "failed";
      if (n.kind !== "test") {
        n.status = n.children.length ? suiteStatus(n) : exitCode === 0 ? "passed" : "failed";
        n.duration = sumDurations(n);
      }
    };
    close(this.root);
  }
}

function findLast(n: TestNode, pred: (n: TestNode) => boolean): TestNode | undefined {
  for (let i = n.children.length - 1; i >= 0; i--) {
    if (pred(n.children[i])) return n.children[i];
  }
  return undefined;
}

export function suiteStatus(n: TestNode): TestStatus {
  const kids = n.children.map((c) => (c.kind === "test" ? c.status : suiteStatus(c)));
  if (kids.includes("failed")) return "failed";
  if (kids.includes("running")) return "running";
  if (kids.length && kids.every((s) => s === "ignored")) return "ignored";
  return "passed";
}

function sumDurations(n: TestNode): number | undefined {
  if (n.kind === "test") return n.duration;
  let sum = 0;
  let any = false;
  for (const c of n.children) {
    const d = sumDurations(c);
    if (d !== undefined) {
      sum += d;
      any = true;
    }
  }
  return any ? sum : undefined;
}

export interface TestCounts {
  total: number;
  passed: number;
  failed: number;
  ignored: number;
  running: number;
}

export function countTests(n: TestNode): TestCounts {
  const c: TestCounts = { total: 0, passed: 0, failed: 0, ignored: 0, running: 0 };
  const walk = (x: TestNode) => {
    if (x.kind === "test") {
      c.total++;
      if (x.status === "pending") c.running++;
      else c[x.status]++;
    }
    x.children.forEach(walk);
  };
  walk(n);
  return c;
}

/** "1 s 245 ms", "87 ms" like PHPStorm's test durations. */
export function formatDuration(ms: number | undefined): string {
  if (ms === undefined) return "";
  if (ms < 1000) return `${Math.round(ms)} ms`;
  const s = Math.floor(ms / 1000);
  const rest = Math.round(ms % 1000);
  if (s < 60) return rest ? `${s} s ${rest} ms` : `${s} s`;
  return `${Math.floor(s / 60)} m ${s % 60} s`;
}
