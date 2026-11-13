/**
 * Browser-mode agent backend: replays scripted, realistic Claude Code and Gemini CLI
 * sessions with timers so the AI chat can be explored and screenshotted without the CLIs.
 * The `chat` updates come from the Rust conversation model (WebAssembly), like in the
 * desktop app. The Gemini session is recorded `stream-json` output that goes through the
 * Rust Gemini parser first, so tool calls are mapped exactly as in the desktop app.
 */
import { emitMock, invoke, registerMock } from "@/ipc";
import type { AgentEvent, AgentProfile, ChatUpdate, MessageContext, PermissionDecision, PermissionMode } from "@/ipc/agent";
import { forgeWasm, type ForgeWasm } from "@/wasm";
import { mockSettingValue } from "./settingsMock";

const CWD = "/home/dev/Projects/acme-shop";
const abs = (p: string) => `${CWD}/${p}`;

interface MockSession {
  id: string;
  agentId: string;
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

/** Demo machine: both CLIs are installed. Profiles (models, modes) come from Rust. */
const DEMO_AGENTS: Record<string, { exe: string; found: string }> = {
  "claude-code": { exe: "claude", found: "/usr/local/bin/claude" },
  "gemini-cli": { exe: "gemini", found: "/usr/local/bin/gemini" },
};

/** Where a configured path counts as installed: an absolute path naming the CLI's executable. */
function detect(agentId: string, configured: string | undefined): string | null {
  const demo = DEMO_AGENTS[agentId];
  const custom = configured?.trim();
  if (!demo) return null;
  if (!custom) return demo.found;
  return /^(\/|[A-Za-z]:\\)/.test(custom) && custom.replace(/\\/g, "/").split("/").pop() === demo.exe ? custom : null;
}

registerMock("agent_list", async ({ executables }) => {
  wasm ??= await forgeWasm();
  const explicit = executables as Record<string, string> | undefined;
  const profiles = JSON.parse(wasm.agent_profiles()) as AgentProfile[];
  return profiles.map((p) => {
    // Like the backend: explicit paths (unsaved Settings edits), else Settings > Tools > AI Agents.
    const configured = explicit ? explicit[p.id] : (mockSettingValue(`agents.${p.id}.path`) as string | null);
    const path = detect(p.id, configured ?? undefined);
    return { ...p, supported: true, installed: !!path, path };
  });
});

registerMock("agent_start", async ({ agentId, options }) => {
  const agent = String(agentId);
  const configured = (options as { executable?: string } | undefined)?.executable ?? (mockSettingValue(`agents.${agent}.path`) as string | null);
  if (!(agent in DEMO_AGENTS)) throw new Error(`unknown agent ${agent}`);
  if (!detect(agent, configured ?? undefined)) {
    const name = agent === "gemini-cli" ? "Gemini CLI" : "Claude Code";
    throw new Error(`${name} was not found. Install it or set its path in Settings.`);
  }
  const id = `mock-${++counter}`;
  const mode = ((options as { permissionMode?: PermissionMode } | undefined)?.permissionMode ?? "default") as PermissionMode;
  wasm ??= await forgeWasm();
  const chat = Number(wasm.chat_new());
  sessions.set(id, { id, agentId: agent, started: false, generation: 0, running: false, mode, pending: new Map(), allowed: new Set(), chat });
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
  const msg = String(text);
  if (s.agentId === "gemini-cli") {
    // One Gemini CLI process per turn: every turn starts with its own `init`.
    s.running = true;
    const script = /paid/i.test(msg) ? geminiPaidScopeScript(s) : geminiGenericScript(s, msg, context as MessageContext | undefined);
    script
      .catch((e) => {
        if (!(e instanceof Cancelled)) emit({ type: "error", message: String(e) });
      })
      .finally(() => (s.running = false));
    return;
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

// ---- Gemini CLI ------------------------------------------------------------------------

const GEMINI_SESSION = "c2f1e0d9-6b5a-4c3b-8a2f-1e0d9c8b7a6f";

/**
 * A Gemini CLI turn as recorded `stream-json` lines: each line goes through the Rust parser
 * (`forge_agent_cli::gemini`) and the resulting events through the conversation model.
 */
function geminiRunner(s: MockSession) {
  const { wait } = runner(s);
  const started = Date.now();
  let toolN = 0;
  const line = (obj: Record<string, unknown>) => {
    const json = JSON.stringify({ ...obj, timestamp: new Date().toISOString() });
    const events = wasm ? (JSON.parse(wasm.agent_parse_lines(JSON.stringify({ agent: "gemini-cli", lines: [json] }))) as AgentEvent[]) : [];
    for (const e of events) {
      // Gemini does not report its working directory; the session fills in the project root.
      emitAgent(s, e.type === "sessionStarted" ? { ...e, cwd: e.cwd ?? CWD } : e);
    }
  };
  const say = async (text: string, speed = 16) => {
    for (const chunk of text.match(/[\s\S]{1,12}/g) ?? []) {
      line({ type: "message", role: "assistant", content: chunk, delta: true });
      await wait(speed);
    }
  };
  const tool = async (name: string, parameters: Record<string, unknown>, run: () => Promise<string> | string, ms = 300) => {
    const id = `${name}-${Date.now()}-${++toolN}`;
    line({ type: "tool_use", tool_name: name, tool_id: id, parameters });
    await wait(ms);
    try {
      line({ type: "tool_result", tool_id: id, status: "success", output: await run() });
    } catch (e) {
      if (e instanceof Cancelled) throw e;
      line({ type: "tool_result", tool_id: id, status: "error", output: "", error: { type: "tool_error", message: String(e) } });
    }
    await wait(120);
  };
  const begin = async (prompt: string) => {
    line({ type: "init", session_id: GEMINI_SESSION, model: "gemini-2.5-pro" });
    line({ type: "message", role: "user", content: prompt });
    await wait(500);
  };
  const finish = (input: number, output: number, cached: number) =>
    line({
      type: "result",
      status: "success",
      stats: { total_tokens: input + output, input_tokens: input, output_tokens: output, cached, input: input - cached, duration_ms: Date.now() - started, tool_calls: toolN },
    });
  return { wait, say, tool, begin, finish, line };
}

const SCOPE_SNIPPET = `/**
 * Only orders that have been paid.
 */
public function scopePaid(Builder $query): Builder
{
    return $query->where('status', OrderStatus::Paid);
}`;

async function geminiPaidScopeScript(s: MockSession) {
  const { say, tool, begin, finish } = geminiRunner(s);
  const canEdit = s.mode === "acceptEdits" || s.mode === "bypassPermissions";
  const canRun = s.mode === "bypassPermissions";
  await begin("Add a scope for paid orders to Order model");
  await say("I'll add a `paid` query scope to the `Order` model. First I'll check the model and the status enum.");
  await tool("read_file", { absolute_path: abs("app/Models/Order.php") }, () => invoke<string>("fs_read_file", { path: "app/Models/Order.php" }), 300);
  await tool("read_file", { absolute_path: abs("app/Models/OrderStatus.php") }, () => invoke<string>("fs_read_file", { path: "app/Models/OrderStatus.php" }), 250);
  await tool(
    "search_file_content",
    { pattern: "OrderStatus::Paid", dir_path: CWD },
    () => 'Found 1 match for pattern "OrderStatus::Paid" in path ".":\n---\nFile: app/Models/User.php\nL36: ->where(\'status\', OrderStatus::Paid)\n---',
    300,
  );
  if (!canEdit) {
    await say(
      [
        "`OrderStatus` is a backed enum with a `Paid` case, so a local scope can compare against it directly. Add this to `App\\Models\\Order` (and import `Illuminate\\Database\\Eloquent\\Builder`):",
        "",
        "```php",
        SCOPE_SNIPPET,
        "```",
        "",
        "I'm in read-only mode, so I didn't change any files. Switch the approval mode to **Auto-accept edits** and I'll apply it.",
      ].join("\n"),
      10,
    );
    finish(14_820, 312, 9_600);
    return;
  }
  await say("`OrderStatus` is a backed enum with a `Paid` case, so the scope can compare against it directly.");
  await tool(
    "replace",
    { file_path: abs("app/Models/Order.php"), old_string: ORDER_OLD, new_string: ORDER_NEW, expected_replacements: 1 },
    async () => {
      await applyEdit("app/Models/Order.php", [{ old_string: ORDER_OLD, new_string: ORDER_NEW }]);
      return `Successfully modified file: ${abs("app/Models/Order.php")} (1 replacements).`;
    },
    600,
  );
  if (canRun) {
    await tool(
      "run_shell_command",
      { command: "php artisan test --filter=OrderTest", description: "Run the order tests." },
      () =>
        [
          "Command: php artisan test --filter=OrderTest",
          "Directory: (root)",
          "Output: ",
          "   PASS  Tests\\Feature\\OrderTest",
          "  ✓ user can view order                                   0.08s",
          "",
          "  Tests:    1 passed (2 assertions)",
          "  Duration: 0.38s",
          "Exit Code: 0",
        ].join("\n"),
      2400,
    );
  }
  await say(
    [
      "Added a `paid` local scope to `App\\Models\\Order`:",
      "",
      "```php",
      "$orders = Order::paid()->latest()->get();",
      "```",
      "",
      canRun
        ? "The order tests still pass."
        : "Shell commands are off in **Auto-accept edits** mode, so I didn't run the tests. Run `php artisan test --filter=OrderTest` to check.",
    ].join("\n"),
    10,
  );
  finish(31_240, 586, 22_400);
}

async function geminiGenericScript(s: MockSession, text: string, ctx: MessageContext | undefined) {
  const { say, tool, begin, finish } = geminiRunner(s);
  await begin(text);
  for (const f of ctx?.files ?? []) {
    await tool("read_file", { absolute_path: abs(f) }, () => invoke<string>("fs_read_file", { path: f }).catch(() => ""), 300);
  }
  await say(
    `This is Forge's browser demo, so no real agent is running. In the desktop app your message would go to the Gemini CLI:\n\n> ${text.split("\n")[0]}\n\nTry **"Add a scope for paid orders to Order model"** to see a scripted session with file reads and an edit.`,
  );
  finish(2_140, 88, 0);
}
