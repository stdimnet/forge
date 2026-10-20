/**
 * Browser-mode agent backend: replays a scripted, realistic Claude Code session with
 * timers so the AI chat can be explored and screenshotted without the CLI. The `chat`
 * updates come from the Rust conversation model (WebAssembly), like in the desktop app.
 */
import { emitMock, invoke, registerMock } from "@/ipc";
import type { AgentEvent, ChatUpdate, MessageContext, PermissionDecision, PermissionMode } from "@/ipc/agent";
import { forgeWasm, type ForgeWasm } from "@/wasm";

const CWD = "/home/dev/Projects/acme-shop";
const abs = (p: string) => `${CWD}/${p}`;

interface MockSession {
  id: string;
  started: boolean;
  /** Bumped on interrupt so a running script stops at its next step. */
  generation: number;
  running: boolean;
  mode: PermissionMode;
  /** Open permission requests by id. */
  pending: Map<string, { resolve: (d: PermissionDecision) => void; reject: (e: Error) => void }>;
  /** "Allow for this session" rules (tool name, or Bash with its exact command). */
  allowed: Set<string>;
  /** Handle of the session's conversation model in the WebAssembly module. */
  chat: number;
}

const sessions = new Map<string, MockSession>();
let counter = 0;
let permN = 0;
let wasm: ForgeWasm | undefined;

/** Emit an agent event and the conversation update the Rust model computes from it. */
function emitAgent(s: MockSession, e: AgentEvent): void {
  emitMock(`agent://${s.id}`, e);
  if (!wasm) return;
  const update = JSON.parse(wasm.chat_apply(JSON.stringify({ handle: s.chat, event: e, now: Date.now() }))) as ChatUpdate;
  emitMock(`agent://${s.id}`, { type: "chat", ...update });
}

const DENIED = "The user doesn't want to proceed with this tool use. The tool use was rejected.";

/** Tools Claude Code asks about in each permission mode (read-only tools never ask). */
export function needsPermission(mode: PermissionMode, tool: string): boolean {
  const edit = ["Edit", "MultiEdit", "Write", "NotebookEdit"].includes(tool);
  if (mode === "bypassPermissions" || mode === "plan") return false;
  if (mode === "acceptEdits") return tool === "Bash";
  return edit || tool === "Bash";
}

const ruleOf = (tool: string, input: Record<string, unknown>) =>
  tool === "Bash" ? `Bash(${String(input.command ?? "").trim()})` : tool;

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

class Cancelled extends Error {}

function runner(s: MockSession) {
  const gen = s.generation;
  const emit = (e: AgentEvent) => emitAgent(s, e);
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
  /** Ask like Claude Code's `can_use_tool` control request; resolves with the answer. */
  const ask = async (toolName: string, input: Record<string, unknown>, toolUseId: string): Promise<boolean> => {
    if (!needsPermission(s.mode, toolName) || s.allowed.has(ruleOf(toolName, input))) return true;
    const requestId = `perm_mock_${++permN}`;
    const answer = new Promise<PermissionDecision>((resolve, reject) => s.pending.set(requestId, { resolve, reject }));
    emit({ type: "permissionRequest", requestId, toolName, input, toolUseId });
    const decision = await answer;
    if (s.generation !== gen) throw new Cancelled();
    if (decision.behavior === "allow" && decision.scope === "session") s.allowed.add(ruleOf(toolName, input));
    return decision.behavior === "allow";
  };
  /** Run a tool; returns false when the user denied it. */
  const tool = async (name: string, input: Record<string, unknown>, run: () => Promise<string> | string, ms = 350, isError = false) => {
    const id = `toolu_mock_${++toolN}`;
    emit({ type: "toolUse", id, name, input });
    if (!(await ask(name, input, id))) {
      emit({ type: "toolResult", toolUseId: id, content: DENIED, isError: true });
      await wait(200);
      return false;
    }
    await wait(ms);
    emit({ type: "toolResult", toolUseId: id, content: await run(), isError });
    await wait(120);
    return true;
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
  const stopped = async (text: string) => {
    await stream("assistantText", text);
    emit({ type: "turnFinished", result: text, isError: false, durationMs: 9120, numTurns: 6, costUsd: 0.0214, usage: { inputTokens: 24, outputTokens: 410 } });
  };
  const edited = await tool(
    "Edit",
    { file_path: abs("app/Models/Order.php"), old_string: ORDER_OLD, new_string: ORDER_NEW },
    () => applyEdit("app/Models/Order.php", [{ old_string: ORDER_OLD, new_string: ORDER_NEW }]),
    600,
  );
  if (!edited) return stopped("Understood, I left `Order.php` unchanged. Tell me how you'd like the scope to look instead.");
  await tool("TodoWrite", todos(["completed", "in_progress", "pending"]), () => "Todos have been modified successfully.", 150);
  const tested = await tool(
    "MultiEdit",
    { file_path: abs("tests/Feature/OrderTest.php"), edits: TEST_EDITS },
    () => applyEdit("tests/Feature/OrderTest.php", TEST_EDITS),
    500,
  );
  if (!tested) return stopped("OK, no test added. The `paid` scope is in place in `app/Models/Order.php`.");
  await tool("TodoWrite", todos(["completed", "completed", "in_progress"]), () => "Todos have been modified successfully.", 150);
  const ran = await tool(
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
  if (!ran) return stopped("I didn't run the tests. Run `php artisan test --filter=OrderTest` when you're ready; the scope and its test are in place.");
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

/** Demo machine: Claude Code is installed, Gemini CLI is not. */
const DEMO_AGENTS = [
  { id: "claude-code", name: "Claude Code", supported: true, exe: "claude", found: "/usr/local/bin/claude", models: ["default", "sonnet", "opus", "haiku"] },
  { id: "gemini-cli", name: "Gemini CLI", supported: false, exe: "gemini", found: null, models: ["gemini-2.5-pro", "gemini-2.5-flash"] },
];

registerMock("agent_list", ({ executables }) => {
  const explicit = (executables ?? {}) as Record<string, string>;
  return DEMO_AGENTS.map(({ exe, found, ...a }) => {
    const custom = explicit[a.id]?.trim();
    // A configured path counts as installed when it names the CLI's executable.
    const path = custom ? (/^(\/|[A-Za-z]:\\)/.test(custom) && custom.replace(/\\/g, "/").split("/").pop() === exe ? custom : null) : found;
    return { ...a, installed: !!path, path };
  });
});

registerMock("agent_start", async ({ agentId, options }) => {
  if (agentId !== "claude-code") throw new Error(`${String(agentId)} is not yet supported in Forge`);
  const id = `mock-${++counter}`;
  const mode = ((options as { permissionMode?: PermissionMode } | undefined)?.permissionMode ?? "default") as PermissionMode;
  wasm ??= await forgeWasm();
  const chat = Number(wasm.chat_new());
  sessions.set(id, { id, started: false, generation: 0, running: false, mode, pending: new Map(), allowed: new Set(), chat });
  return id;
});

registerMock("agent_respond_permission", ({ sessionId, requestId, decision }) => {
  const s = sessions.get(String(sessionId));
  const pending = s?.pending.get(String(requestId));
  if (!s || !pending) throw new Error(`no pending permission request ${String(requestId)}`);
  s.pending.delete(String(requestId));
  const d = decision as PermissionDecision;
  const status = d.behavior === "deny" ? "denied" : d.scope === "session" ? "allowedForSession" : "allowed";
  const update = wasm && (JSON.parse(wasm.chat_set_permission(JSON.stringify({ handle: s.chat, requestId, status }))) as ChatUpdate | null);
  if (update) emitMock(`agent://${s.id}`, { type: "chat", ...update });
  pending.resolve(d);
});

registerMock("agent_send", ({ sessionId, text, context, turnId }) => {
  const s = sessions.get(String(sessionId));
  if (!s) throw new Error(`no agent session ${sessionId}`);
  const emit = (e: AgentEvent) => emitAgent(s, e);
  if (wasm) {
    const begin = { handle: s.chat, turnId: Number(turnId ?? 0), prompt: String(text), context: context ?? { files: [] } };
    emitMock(`agent://${s.id}`, { type: "chat", ...(JSON.parse(wasm.chat_begin_turn(JSON.stringify(begin))) as ChatUpdate) });
  }
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
  for (const p of s.pending.values()) p.reject(new Cancelled());
  s.pending.clear();
  const emit = (e: AgentEvent) => emitAgent(s, e);
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
    wasm?.chat_drop(JSON.stringify({ handle: s.chat }));
    sessions.delete(s.id);
  }
});
