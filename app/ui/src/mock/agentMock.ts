/**
 * Browser-mode agent backend: replays a scripted, realistic Claude Code session with
 * timers so the AI chat can be explored and screenshotted without the CLI.
 */
import { emitMock, invoke, registerMock } from "@/ipc";
import type { AgentEvent, MessageContext } from "@/ipc/agent";

const CWD = "/home/dev/Projects/acme-shop";
const abs = (p: string) => `${CWD}/${p}`;

interface MockSession {
  id: string;
  started: boolean;
  /** Bumped on interrupt so a running script stops at its next step. */
  generation: number;
  running: boolean;
}

const sessions = new Map<string, MockSession>();
let counter = 0;

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

class Cancelled extends Error {}

function runner(s: MockSession) {
  const gen = s.generation;
  const emit = (e: AgentEvent) => emitMock(`agent://${s.id}`, e);
  const wait = async (ms: number) => {
    await sleep(ms);
    if (s.generation !== gen) throw new Cancelled();
  };
  const stream = async (type: "assistantText" | "thinking", text: string, speed = 18) => {
    const parts = text.match(/[\s\S]{1,6}/g) ?? [];
    for (const p of parts) {
      emit({ type, text: p, delta: true } as AgentEvent);
      await wait(speed);
    }
  };
  let toolN = 0;
  const tool = async (name: string, input: Record<string, unknown>, run: () => Promise<string> | string, ms = 350, isError = false) => {
    const id = `toolu_mock_${++toolN}`;
    emit({ type: "toolUse", id, name, input });
    await wait(ms);
    emit({ type: "toolResult", toolUseId: id, content: await run(), isError });
    await wait(120);
  };
  return { emit, wait, stream, tool };
}

const numbered = (src: string) =>
  src
    .split("\n")
    .map((l, i) => `${String(i + 1).padStart(6)}→${l}`)
    .join("\n");

const ORDER_OLD = `use Illuminate\\Database\\Eloquent\\Model;
use Illuminate\\Database\\Eloquent\\Relations\\BelongsTo;

class Order extends Model
{
    protected $casts = ['status' => OrderStatus::class];
`;

const ORDER_NEW = `use Illuminate\\Database\\Eloquent\\Builder;
use Illuminate\\Database\\Eloquent\\Model;
use Illuminate\\Database\\Eloquent\\Relations\\BelongsTo;

class Order extends Model
{
    protected $casts = ['status' => OrderStatus::class];

    /**
     * Only orders that have been paid.
     */
    public function scopePaid(Builder $query): Builder
    {
        return $query->where('status', OrderStatus::Paid);
    }
`;

const TEST_EDITS = [
  {
    old_string: "use Tests\\TestCase;",
    new_string: "use App\\Models\\Order;\nuse App\\Models\\OrderStatus;\nuse Illuminate\\Foundation\\Testing\\RefreshDatabase;\nuse Tests\\TestCase;",
  },
  {
    old_string: "        $this->assertTrue(true);\n    }\n",
    new_string:
      "        $this->assertTrue(true);\n    }\n\n    public function test_paid_scope_only_returns_paid_orders(): void\n    {\n        Order::factory()->create(['status' => OrderStatus::Paid]);\n        Order::factory()->create(['status' => OrderStatus::Pending]);\n\n        $this->assertSame(1, Order::paid()->count());\n    }\n",
  },
];

async function applyEdit(path: string, edits: { old_string: string; new_string: string }[]): Promise<string> {
  let src = await invoke<string>("fs_read_file", { path });
  for (const e of edits) src = src.replace(e.old_string, e.new_string);
  await invoke("fs_write_file", { path, contents: src });
  return `The file ${abs(path)} has been updated.`;
}

const todos = (states: ("pending" | "in_progress" | "completed")[]) => ({
  todos: [
    { content: "Add paid scope to Order model", activeForm: "Adding paid scope to Order model", status: states[0] },
    { content: "Cover the scope with a feature test", activeForm: "Writing a feature test for the scope", status: states[1] },
    { content: "Run the test suite", activeForm: "Running the test suite", status: states[2] },
  ],
});

async function paidScopeScript(s: MockSession) {
  const { emit, wait, stream, tool } = runner(s);
  await wait(500);
  emit({ type: "thinking", text: "", delta: true });
  await stream(
    "thinking",
    "The user wants a query scope for paid orders. Order casts `status` to the OrderStatus enum, so a local scope comparing against OrderStatus::Paid fits. Check the model and the enum first, then add a test.",
    14,
  );
  await wait(200);
  await stream("assistantText", "I'll add a `paid` query scope to the `Order` model. Let me look at the model and the status enum first.");
  await wait(250);
  await tool("TodoWrite", todos(["in_progress", "pending", "pending"]), () => "Todos have been modified successfully.", 150);
  await tool("Read", { file_path: abs("app/Models/Order.php") }, async () => numbered(await invoke<string>("fs_read_file", { path: "app/Models/Order.php" })), 300);
  await tool("Read", { file_path: abs("app/Models/OrderStatus.php") }, async () => numbered(await invoke<string>("fs_read_file", { path: "app/Models/OrderStatus.php" })), 250);
  await tool("Grep", { pattern: "OrderStatus::Paid", output_mode: "content", "-n": true }, () => "app/Models/User.php:36:            ->where('status', OrderStatus::Paid)\n", 300);
  await stream("assistantText", "`OrderStatus` is a backed enum with a `Paid` case, so the scope can compare against it directly. Adding the scope:");
  await wait(250);
  await tool(
    "Edit",
    { file_path: abs("app/Models/Order.php"), old_string: ORDER_OLD, new_string: ORDER_NEW },
    () => applyEdit("app/Models/Order.php", [{ old_string: ORDER_OLD, new_string: ORDER_NEW }]),
    600,
  );
  await tool("TodoWrite", todos(["completed", "in_progress", "pending"]), () => "Todos have been modified successfully.", 150);
  await tool("MultiEdit", { file_path: abs("tests/Feature/OrderTest.php"), edits: TEST_EDITS }, () => applyEdit("tests/Feature/OrderTest.php", TEST_EDITS), 500);
  await tool("TodoWrite", todos(["completed", "completed", "in_progress"]), () => "Todos have been modified successfully.", 150);
  await tool(
    "Bash",
    { command: "php artisan test --filter=OrderTest", description: "Run the order tests" },
    () =>
      [
        "",
        "   PASS  Tests\\Feature\\OrderTest",
        "  ✓ user can view order                                   0.08s",
        "  ✓ paid scope only returns paid orders                   0.05s",
        "",
        "  Tests:    2 passed (3 assertions)",
        "  Duration: 0.41s",
      ].join("\n"),
    3200,
  );
  await tool("TodoWrite", todos(["completed", "completed", "completed"]), () => "Todos have been modified successfully.", 120);
  await stream(
    "assistantText",
    [
      "Added a `paid` local scope to `App\\Models\\Order`:",
      "",
      "```php",
      "$orders = Order::paid()->latest()->get();",
      "```",
      "",
      "**Changes**",
      "- `app/Models/Order.php` — new `scopePaid(Builder $query)` filtering on `OrderStatus::Paid`",
      "- `tests/Feature/OrderTest.php` — test that only paid orders are returned",
      "",
      "Both tests pass. `User::totalSpent()` still filters in memory; it could use `$this->orders()->paid()->sum('total')` to do the work in SQL.",
    ].join("\n"),
    10,
  );
  await wait(150);
  emit({
    type: "turnFinished",
    result: "Added a paid scope.",
    isError: false,
    durationMs: 24610,
    numTurns: 11,
    costUsd: 0.0612,
    usage: { inputTokens: 38, outputTokens: 1240, cacheReadTokens: 41210, cacheWriteTokens: 5320 },
  });
}

async function genericScript(s: MockSession, text: string, ctx: MessageContext | undefined) {
  const { emit, wait, stream, tool } = runner(s);
  await wait(400);
  for (const f of ctx?.files ?? []) {
    await tool("Read", { file_path: abs(f) }, async () => numbered(await invoke<string>("fs_read_file", { path: f }).catch(() => "")), 300);
  }
  await stream(
    "assistantText",
    `This is Forge's browser demo, so no real agent is running. In the desktop app your message would go to the Claude Code CLI:\n\n> ${text.split("\n")[0]}\n\nTry **"Add a scope for paid orders to Order model"** to see a scripted session with file reads, an edit and a test run.`,
  );
  emit({ type: "turnFinished", result: null, isError: false, durationMs: 1800, numTurns: 1, costUsd: 0.0021, usage: { inputTokens: 12, outputTokens: 96 } });
}

registerMock("agent_list", () => [
  { id: "claude-code", name: "Claude Code", supported: true, installed: true, path: "/usr/local/bin/claude", models: ["default", "sonnet", "opus", "haiku"] },
  { id: "gemini-cli", name: "Gemini CLI", supported: false, installed: false, path: null, models: ["gemini-2.5-pro", "gemini-2.5-flash"] },
]);

registerMock("agent_start", ({ agentId }) => {
  if (agentId !== "claude-code") throw new Error(`${String(agentId)} is not yet supported in Forge`);
  const id = `mock-${++counter}`;
  sessions.set(id, { id, started: false, generation: 0, running: false });
  return id;
});

registerMock("agent_send", ({ sessionId, text, context }) => {
  const s = sessions.get(String(sessionId));
  if (!s) throw new Error(`no agent session ${sessionId}`);
  const emit = (e: AgentEvent) => emitMock(`agent://${s.id}`, e);
  if (!s.started) {
    s.started = true;
    setTimeout(() =>
      emit({
        type: "sessionStarted",
        sessionId: "5f0c1d2e-8a7b-4c3d-9e1f-2a3b4c5d6e7f",
        model: "claude-sonnet-4-5",
        tools: ["Task", "Bash", "Glob", "Grep", "Read", "Edit", "MultiEdit", "Write", "TodoWrite", "WebFetch", "WebSearch"],
        cwd: CWD,
        permissionMode: "default",
      }),
    );
  }
  const msg = String(text);
  const script = /paid/i.test(msg) ? paidScopeScript(s) : genericScript(s, msg, context as MessageContext | undefined);
  s.running = true;
  script
    .catch((e) => {
      if (!(e instanceof Cancelled)) emit({ type: "error", message: String(e) });
    })
    .finally(() => (s.running = false));
});

function kill(s: MockSession, interrupted: boolean) {
  s.generation++;
  const emit = (e: AgentEvent) => emitMock(`agent://${s.id}`, e);
  if (interrupted && s.running) emit({ type: "interrupted" });
  emit({ type: "exited", code: null });
  s.started = false;
}

registerMock("agent_interrupt", ({ sessionId }) => {
  const s = sessions.get(String(sessionId));
  if (s) kill(s, true);
});

registerMock("agent_stop", ({ sessionId }) => {
  const s = sessions.get(String(sessionId));
  if (s) {
    kill(s, false);
    sessions.delete(s.id);
  }
});
