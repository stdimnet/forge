/**
 * Browser-mode Agent Tasks: a scripted fake agent. A new task replays a recorded run (explore,
 * edit, failing tests, a command approval, the fix, green tests) and leaves a diff against the
 * demo project. Nothing runs and no file is written; Apply only reports what it would do.
 */
import { emitMock, registerMock } from "@/ipc";
import type { AgentTask, ApplyAnswer, ApplyMode, FileChange, StepKind, TaskRequest, TestResult } from "@/ipc/agentTasks";
import { mockSettingValue } from "./settingsMock";
import { mockFiles } from "./workspaceMock";

/** Delays of the replay are multiplied by `scale` (tests set it to 0). */
export const agentTaskMockTiming = { scale: 1 };
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms * agentTaskMockTiming.scale));
const CHANGED = "agent-tasks://changed";

const CHECKOUT = "app/Services/Checkout.php";
const TEST = "tests/Unit/CheckoutTest.php";

const checkoutBefore = () => mockFiles[CHECKOUT] ?? "<?php\n\nclass Checkout\n{\n}\n";

function checkoutAfter(): string {
  return checkoutBefore().replace(
    /\n}\n$/,
    `

    /**
     * The gross total of a net amount, rounded to cents.
     */
    public function total(float $net, ?float $taxRate = null): float
    {
        return round($net * (1 + ($taxRate ?? $this->taxRate)), 2);
    }
}
`,
  );
}

const testBefore = () => mockFiles[TEST] ?? "";
const testAfter = () =>
  testBefore().replace(
    /\n}\n$/,
    `

    public function test_total_uses_the_default_tax_rate(): void
    {
        $this->assertSame(10.7, (new Checkout('EUR', 0.07))->total(10.0));
    }
}
`,
  );

const failing: TestResult = {
  green: false,
  exitCode: 1,
  passed: 0,
  failed: 1,
  failures: ["test_total_includes_tax: Failed asserting that 138.0 is identical to 119.0."],
  outputTail: "PHPUnit 11.4.3\n\nF.S\n\nThere was 1 failure:\n\n1) Tests\\Unit\\CheckoutTest::test_total_includes_tax\nFailed asserting that 138.0 is identical to 119.0.\n\nFAILURES!\nTests: 3, Assertions: 2, Failures: 1, Skipped: 1.",
};
const green: TestResult = {
  green: true,
  exitCode: 0,
  passed: 2,
  failed: 0,
  failures: [],
  outputTail: "PHPUnit 11.4.3\n\n..S\n\nOK, but some tests were skipped!\nTests: 3, Assertions: 2, Skipped: 1.",
};

const tool = (t: string, summary: string, output: string, isError = false): StepKind => ({
  kind: "tool",
  tool: t,
  summary,
  input: {},
  output,
  isError,
});

/** The recorded run: (delay, step); "approval" waits for the answer to the step before. */
function script(command: string): Array<[number, StepKind | "approval"]> {
  return [
    [300, { kind: "info", text: "Creating the sandbox (a git worktree in Forge's cache folder)" }],
    [400, { kind: "info", text: "Sandbox ready at ~/.cache/forge/agent-tasks/acme-shop/task-demo" }],
    [300, { kind: "info", text: "Forge agent with Anthropic (claude-sonnet-4-5)" }],
    [700, { kind: "thought", text: "CheckoutTest calls Checkout::total(), which does not exist yet. Let me look at the class and the test." }],
    [500, tool("read_file", CHECKOUT, checkoutBefore())],
    [500, tool("read_file", TEST, testBefore())],
    [800, { kind: "thought", text: "I'll add total() using the configured tax rate unless one is passed." }],
    [600, tool("edit_file", CHECKOUT, `Edited ${CHECKOUT}`)],
    [400, { kind: "info", text: `Running Tests: ${command}` }],
    [900, { kind: "tests", result: failing }],
    [700, { kind: "thought", text: "The tax is applied twice. Let me check the syntax of the file before the next run." }],
    [400, { kind: "approval", command: "php -l app/Services/Checkout.php", state: "pending" }],
    [0, "approval"],
    [500, tool("run_command", "php -l app/Services/Checkout.php", "exit code 0\nNo syntax errors detected in app/Services/Checkout.php")],
    [600, tool("edit_file", CHECKOUT, `Edited ${CHECKOUT}`)],
    [500, tool("edit_file", TEST, `Edited ${TEST}`)],
    [400, { kind: "info", text: `Running Tests: ${command}` }],
    [900, { kind: "tests", result: green }],
    [
      300,
      {
        kind: "finished",
        outcome: {
          kind: "completed",
          summary: "Added Checkout::total(), which applies the configured tax rate (or the one passed) and rounds to cents, plus a test for the default rate. All tests pass.",
          tests: green,
        },
      },
    ],
  ];
}

const tasks: AgentTask[] = [];
const approvals = new Map<string, (allow: boolean) => void>();
const cancelled = new Set<string>();

function emit(t: AgentTask) {
  t.updated = Date.now();
  emitMock(CHANGED, structuredClone(t));
}

function push(t: AgentTask, kind: StepKind): number {
  const index = t.steps.length;
  t.steps.push({ ...kind, index, at: Date.now() } as AgentTask["steps"][number]);
  emit(t);
  return index;
}

function title(prompt: string): string {
  const line = prompt.split("\n").map((l) => l.trim()).find(Boolean) ?? "Untitled task";
  return line.length > 80 ? `${line.slice(0, 79).trimEnd()}…` : line;
}

function newTask(request: TaskRequest, created = Date.now()): AgentTask {
  return {
    id: `task-${created.toString(16)}`,
    title: title(request.prompt),
    request,
    status: "queued",
    created,
    updated: created,
    sandbox: null,
    baseCommit: null,
    outcome: null,
    error: null,
    applied: null,
    steps: [],
  };
}

async function play(t: AgentTask) {
  t.status = "running";
  t.sandbox = `~/.cache/forge/agent-tasks/acme-shop/${t.id}`;
  t.baseCommit = "4f2c9e1";
  emit(t);
  const command = t.request.tests.kind === "command" ? t.request.tests.command : t.request.tests.kind === "configuration" ? t.request.tests.name : "vendor/bin/phpunit";
  for (const [delay, step] of script(command)) {
    await sleep(delay);
    if (cancelled.has(t.id)) {
      t.status = "cancelled";
      push(t, { kind: "info", text: "Cancelled" });
      return;
    }
    if (step === "approval") {
      const index = t.steps.length - 1;
      const allowed = await new Promise<boolean>((resolve) => approvals.set(`${t.id}:${index}`, resolve));
      t.steps[index] = { ...t.steps[index], kind: "approval", command: "php -l app/Services/Checkout.php", state: allowed ? "allowed" : "denied" } as AgentTask["steps"][number];
      emit(t);
      continue;
    }
    if (step.kind === "finished") {
      t.status = "needsReview";
      t.outcome = step.outcome;
    }
    push(t, step);
  }
}

/** A finished demo task so the window is not empty in screenshots. */
function seed() {
  if (tasks.length) return;
  const t = newTask(
    {
      prompt: "Add Checkout::total() so the CheckoutTest passes. Keep the tax rate configurable.",
      scope: ["app/Services", "tests/Unit"],
      backend: { kind: "builtin" },
      tests: { kind: "command", command: "vendor/bin/phpunit tests/Unit/CheckoutTest.php" },
    },
    Date.now() - 6 * 60_000,
  );
  t.status = "needsReview";
  t.sandbox = `~/.cache/forge/agent-tasks/acme-shop/${t.id}`;
  t.baseCommit = "4f2c9e1";
  for (const [, step] of script("vendor/bin/phpunit tests/Unit/CheckoutTest.php")) {
    if (step === "approval") {
      const s = t.steps[t.steps.length - 1];
      if (s.kind === "approval") s.state = "allowed";
      continue;
    }
    if (step.kind === "finished") t.outcome = step.outcome;
    t.steps.push({ ...step, index: t.steps.length, at: t.created + t.steps.length * 4000 } as AgentTask["steps"][number]);
  }
  const done = newTask(
    {
      prompt: "Rename OrderStatus::Canceled to Cancelled everywhere",
      scope: [],
      backend: { kind: "cli", agentId: "claude-code" },
      tests: { kind: "none" },
    },
    Date.now() - 50 * 60_000,
  );
  done.status = "applied";
  done.applied = "Created branch agent/rename-cancelled (9b1d2e7)";
  done.outcome = { kind: "completed", summary: "Renamed the enum case and its 4 usages.", tests: null };
  done.steps.push({ kind: "info", text: done.applied, index: 0, at: done.created });
  tasks.push(done, t);
}

const enabled = () => mockSettingValue("ai.enabled") !== false;

registerMock("agent_tasks_list", () => {
  seed();
  return structuredClone([...tasks].sort((a, b) => b.created - a.created));
});

registerMock("agent_task_create", ({ request }) => {
  if (!enabled()) throw new Error("AI features are turned off for this project");
  const r = request as TaskRequest;
  if (!r.prompt?.trim()) throw new Error("Describe the task first");
  const t = newTask(r);
  tasks.push(t);
  emit(t);
  void play(t);
  return structuredClone(t);
});

registerMock("agent_task_cancel", ({ id }) => {
  cancelled.add(String(id));
  for (const [key, resolve] of approvals) if (key.startsWith(`${id}:`)) resolve(false);
});

registerMock("agent_task_approve", ({ id, step, allow }) => {
  const key = `${id}:${step}`;
  const resolve = approvals.get(key);
  if (!resolve) throw new Error("This approval is no longer pending");
  approvals.delete(key);
  resolve(Boolean(allow));
});

function find(id: unknown): AgentTask {
  const t = tasks.find((x) => x.id === id);
  if (!t) throw new Error(`no agent task ${String(id)}`);
  return t;
}

function lines(text: string) {
  return text.split("\n").length;
}

registerMock("agent_task_changes", ({ id }): FileChange[] => {
  const t = find(id);
  if (!t.sandbox || t.status === "applied" || t.status === "discarded") throw new Error("This task has no sandbox");
  if (!t.steps.some((s) => s.kind === "tool" && s.tool === "edit_file")) return [];
  return [
    { path: CHECKOUT, kind: "modified", additions: lines(checkoutAfter()) - lines(checkoutBefore()), deletions: 0, binary: false },
    { path: TEST, kind: "modified", additions: lines(testAfter()) - lines(testBefore()), deletions: 0, binary: false },
  ];
});

registerMock("agent_task_file_diff", ({ path }) => {
  if (path === CHECKOUT) return { before: checkoutBefore(), after: checkoutAfter() };
  if (path === TEST) return { before: testBefore(), after: testAfter() };
  return { before: null, after: null };
});

registerMock("agent_task_apply", ({ id, mode, paths }): ApplyAnswer => {
  const t = find(id);
  const m = mode as ApplyMode;
  const files = (paths as string[] | undefined)?.length ? (paths as string[]) : [CHECKOUT, TEST];
  const created = m.mode === "commit" ? "a3e81f0" : m.mode === "branch" ? `${m.branch} (a3e81f0)` : null;
  const text = m.mode === "apply" ? "Applied to the working tree" : m.mode === "commit" ? `Committed as ${created}` : `Created branch ${created}`;
  if (!(paths as string[] | undefined)?.length) {
    t.status = "applied";
    t.applied = text;
    push(t, { kind: "info", text });
  }
  return { applied: true, files, conflicts: [], message: null, created };
});

registerMock("agent_task_discard", ({ id }) => {
  const t = find(id);
  if (t.status !== "applied") t.status = "discarded";
  emit(t);
});

registerMock("agent_task_remove", ({ id }) => {
  const i = tasks.findIndex((x) => x.id === id);
  if (i >= 0) tasks.splice(i, 1);
});
