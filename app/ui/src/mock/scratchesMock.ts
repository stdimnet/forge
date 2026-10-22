/**
 * Browser-mode scratches: files live in memory (never on disk), names, languages, gutter
 * markers, run command lines, the `.http` parser, request resolution (environments,
 * `client.global` variables, dynamic variables, body files), response handlers and the
 * response text come from the Rust `forge-scratches` crate (WebAssembly). HTTP requests
 * get canned responses.
 */
import { emitMock, registerMock } from "@/ipc";
import type { CommandLine, RunConfiguration, RunEvent } from "@/ipc/run";
import type { ScratchFile, ScratchLanguage, ScratchMarker, ScratchRunner } from "@/ipc/scratches";
import { callWasmSync, forgeWasm, type ForgeWasm } from "@/wasm";
import builtinPlugins from "./builtinPlugins.json";
import { demoProjectName } from "./demoProject";
import { mockFiles } from "./workspaceMock";
import { currentRemote } from "./remoteMock";
import { mockSettingValue } from "./settingsMock";

const DIR = "~/.config/forge/scratches";
const ROOT = `~/Projects/${demoProjectName}`;

interface MockScratch {
  content: string;
  modified: number;
  dataSource?: string;
}

const DEMO: Record<string, string> = {
  "scratch.php": `<?php

use App\\Models\\User;

$user = new User(['first_name' => 'Ada', 'last_name' => 'Lovelace', 'email' => 'ada@example.com']);

echo $user->fullName() . PHP_EOL;
dump($user->only(['first_name', 'last_name', 'email']));
`,
  "rest-api.http": `### Log in
POST http://{{host}}/api/login
Content-Type: application/json

{ "email": "ada@example.com", "password": "{{password}}" }

> {%
    client.global.set("token", response.body.token);
    client.test("Logged in", function () {
        client.assert(response.status === 200, "Expected 200 but got " + response.status);
    });
%}

### List paid orders
GET http://{{host}}/api/orders?status=paid
Accept: application/json

### Show order
GET http://{{host}}/api/orders/1042
Accept: application/json
Authorization: Bearer {{token}}

###
# @name createOrder
POST http://{{host}}/api/orders
Content-Type: application/json
Accept: application/json
Authorization: Bearer {{token}}
X-Request-Id: {{$uuid}}

{
  "customer_id": 7,
  "items": [{ "sku": "MUG-001", "quantity": 2 }]
}

> {%
    client.test("Order created", () => {
        client.assert(response.status === 201, "Expected 201 but got " + response.status);
        client.assert(response.body.data.items.length > 0, "The order has no items");
    });
    client.test("Order is paid", () => {
        client.assert(response.body.data.status === "paid", "Status is " + response.body.data.status);
    });
    client.global.set("orderId", response.body.data.id);
    client.log("Created order " + response.body.data.id);
%}
`,
  "scratch.js": `const orders = [42.5, 19.99, 7];
const total = orders.reduce((sum, amount) => sum + amount, 0);

console.log(\`Total: \${total.toFixed(2)} EUR\`);
`,
  "orders.sql": `-- Paid orders, largest first
SELECT id, number, status, total, currency, placed_at
FROM orders
WHERE status = 'paid'
ORDER BY total DESC;
`,
  "buffer1.txt": "TODO: ask Grace about the VAT rounding in InvoiceService\n",
};

const files = new Map<string, MockScratch>(
  Object.entries(DEMO).map(([name, content], i) => [name, { content, modified: Date.UTC(2026, 9, 20, 8, 30 + i) }]),
);

let wasm: ForgeWasm | undefined;
const ready = forgeWasm().then((w) => {
  callWasmSync<number>(w.scratches_load, builtinPlugins);
  wasm = w;
  return w;
});

const call = async <T>(fn: (w: ForgeWasm) => (json: string) => string, input: unknown): Promise<T> =>
  callWasmSync<T>(fn(await ready), input);

function describe(name: string): ScratchFile {
  const f = files.get(name);
  if (!f) throw new Error(`scratch '${name}' does not exist`);
  const runner = wasm ? (callWasmSync<ScratchRunner | null>(wasm.scratches_runner, { name }) ?? undefined) : undefined;
  return {
    name,
    path: `${DIR}/${name}`,
    size: new TextEncoder().encode(f.content).length,
    modified: f.modified,
    runner,
    dataSource: f.dataSource,
  };
}

const changed = () => emitMock("scratches://changed", null);
const sorted = () => [...files.keys()].sort((a, b) => a.toLowerCase().localeCompare(b.toLowerCase()));

async function validate(name: string): Promise<void> {
  const problem = await call<string | null>((w) => w.scratches_validate_name, { name });
  if (problem) throw new Error(problem);
}

registerMock("scratches_list", async () => {
  await ready;
  return sorted().map(describe);
});

registerMock("scratches_languages", ({ request }) => call<ScratchLanguage[]>((w) => w.scratches_languages, request));

registerMock("scratches_create", async ({ language, content }) => {
  const next = await call<{ name: string; template: string }>((w) => w.scratches_new_name, {
    language,
    existing: [...files.keys()],
  });
  files.set(next.name, { content: (content as string | undefined) ?? next.template, modified: Date.now() });
  changed();
  return describe(next.name);
});

registerMock("scratches_create_buffer", async () => {
  const next = await call<{ name: string }>((w) => w.scratches_next_buffer, {
    existing: [...files.entries()].map(([n, f]) => [n, f.modified]),
  });
  files.set(next.name, { content: "", modified: Date.now() });
  changed();
  return describe(next.name);
});

registerMock("scratches_read", ({ name }) => {
  const f = files.get(String(name));
  if (!f) throw new Error(`scratch '${name}' does not exist`);
  return f.content;
});

registerMock("scratches_write", async ({ name, contents }) => {
  await validate(String(name));
  const old = files.get(String(name));
  files.set(String(name), { ...old, content: String(contents), modified: Date.now() });
});

registerMock("scratches_rename", async ({ name, newName }) => {
  const from = String(name);
  const to = String(newName);
  await validate(to);
  const f = files.get(from);
  if (!f) throw new Error(`scratch '${from}' does not exist`);
  if (from !== to && files.has(to)) throw new Error(`a scratch named '${to}' already exists`);
  files.delete(from);
  files.set(to, f);
  changed();
  return describe(to);
});

registerMock("scratches_delete", ({ name }) => {
  if (!files.delete(String(name))) throw new Error(`scratch '${name}' does not exist`);
  changed();
});

registerMock("scratches_set_data_source", ({ name, dataSourceId }) => {
  const f = files.get(String(name));
  if (!f) throw new Error(`scratch '${name}' does not exist`);
  f.dataSource = (dataSourceId as string | null) || undefined;
  changed();
  return describe(String(name));
});

registerMock("scratches_markers", ({ name, content }) => call<ScratchMarker[]>((w) => w.scratches_markers, { name, content }));

// ---- runs ------------------------------------------------------------------------------

/** Command line of a `scratch` run configuration (same Rust code as the desktop backend). */
export function scratchCommandLine(cfg: RunConfiguration): CommandLine {
  const name = cfg.options?.scratch ?? "";
  if (!wasm) return { program: "php", args: [`${DIR}/${name}`], cwd: ROOT };
  return callWasmSync<CommandLine>(wasm.scratches_run_command, {
    file: `${DIR}/${name}`,
    context: {
      phpInterpreter: cfg.interpreter || "/usr/bin/php",
      projectRoot: ROOT,
      autoloader: cfg.options?.useAutoloader === "false" ? null : `${ROOT}/vendor/autoload.php`,
    },
  });
}

type Step = [delayMs: number, text: string, stream?: "stdout" | "stderr"] | [delayMs: number, exit: { code: number }];

/** Canned output of a scratch run in the Run tool window. */
export function scratchRunSteps(cfg: RunConfiguration): Step[] {
  const name = cfg.options?.scratch ?? "";
  if (name.endsWith(".php"))
    return [
      [120, "Ada Lovelace\n"],
      [
        60,
        '\x1b[38;5;208marray:3\x1b[0m [\n  \x1b[38;5;113m"first_name"\x1b[0m => \x1b[38;5;113m"Ada"\x1b[0m\n  \x1b[38;5;113m"last_name"\x1b[0m => \x1b[38;5;113m"Lovelace"\x1b[0m\n  \x1b[38;5;113m"email"\x1b[0m => \x1b[38;5;113m"ada@example.com"\x1b[0m\n] \x1b[90m// ' +
          `${DIR}/${name}:8\x1b[0m\n`,
      ],
      [40, { code: 0 }],
    ];
  if (/\.(m|c)?[jt]s$/.test(name)) return [[150, "Total: 69.49 EUR\n"], [40, { code: 0 }]];
  return [[100, "Done.\n"], [40, { code: 0 }]];
}

interface DemoResponse {
  status: number;
  reason: string;
  headers: [string, string][];
  body: string;
  ms: number;
}

function cannedResponse(method: string, url: string): DemoResponse {
  const headers = (len: number, extra: [string, string][] = []): [string, string][] => [
    ["Server", "nginx/1.27.2"],
    ["Content-Type", "application/json"],
    ["Content-Length", String(len)],
    ["Cache-Control", "no-cache, private"],
    ["Date", "Tue, 20 Oct 2026 09:15:02 GMT"],
    ["X-RateLimit-Limit", "60"],
    ["X-RateLimit-Remaining", "58"],
    ...extra,
  ];
  const json = (status: number, reason: string, value: unknown, ms: number, extra: [string, string][] = []): DemoResponse => {
    const body = JSON.stringify(value);
    return { status, reason, headers: headers(body.length, extra), body, ms };
  };
  const path = url.replace(/^\w+:\/\/[^/]+/, "");
  if (method === "POST" && path.startsWith("/api/login"))
    return json(200, "OK", { token: "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOjd9.demo", expires_in: 3600 }, 71);
  if (method === "POST" && path.startsWith("/api/orders"))
    return json(201, "Created", { data: { id: 1043, status: "pending", total: "25.80", items: [{ sku: "MUG-001", quantity: 2 }] } }, 87, [
      ["Location", "http://acme-shop.test/api/orders/1043"],
    ]);
  if (/^\/api\/orders\/\d+/.test(path))
    return json(
      200,
      "OK",
      {
        data: {
          id: 1042,
          status: "paid",
          total: "42.50",
          currency: "EUR",
          customer: { id: 7, name: "Ada Lovelace", email: "ada@example.com" },
          items: [
            { sku: "MUG-001", name: "Forge mug", quantity: 2, price: "12.90" },
            { sku: "TEE-RED-M", name: "Ember T-shirt", quantity: 1, price: "16.70" },
          ],
          paid_at: "2026-10-19T17:42:11+00:00",
        },
      },
      64,
    );
  if (path.startsWith("/api/orders"))
    return json(200, "OK", { data: [{ id: 1042, status: "paid", total: "42.50" }, { id: 1038, status: "paid", total: "19.99" }], meta: { current_page: 1, total: 2 } }, 112);
  return { status: 404, reason: "Not Found", headers: headers(31), body: '{"message":"Route not found."}', ms: 23 };
}

/** `client.global` variables of this browser session. */
let httpGlobals: Record<string, string> = {};

const envFiles = () => ({ public: mockFiles["http-client.env.json"] ?? null, private: mockFiles["http-client.private.env.json"] ?? null });

registerMock("scratches_http_environments", async () => {
  const envs = await call<{ names: string[]; default?: string }>((w) => w.scratches_http_environments, envFiles());
  return { names: envs.names, default: envs.default };
});

interface Resolved {
  request: { method: string; url: string };
  problems: string[];
  handler?: string | null;
}

registerMock("scratches_http_run", async ({ runId, content, line, env, source, debug }) => {
  const w = await ready;
  const { variables } = callWasmSync<{ variables: Record<string, string> }>(w.scratches_http_environments, {
    ...envFiles(),
    name: (env as string | undefined) || null,
  });
  const src = String(source ?? "");
  const scratch = src.startsWith("scratch://");
  const resolved = callWasmSync<Resolved | null>(w.scratches_http_request, {
    content,
    line,
    env: variables,
    globals: httpGlobals,
    timestamp: Math.floor(Date.now() / 1000),
    seed: Math.floor(Math.random() * 2 ** 32),
    // Body and handler files: next to a scratch, or relative to the project file.
    base: scratch ? "" : src.split("/").slice(0, -1).join("/"),
    files: scratch ? Object.fromEntries([...files].map(([n, f]) => [n, f.content])) : mockFiles,
    // Debug: the Xdebug session cookie.
    debug: !!debug,
  });
  if (!resolved) throw new Error("no request at the caret");
  const { request } = resolved;
  const id = String(runId);
  const event = `run://${id}`;
  const commandLine = `${request.method} ${request.url}`;
  const resp = cannedResponse(request.method, request.url);
  const output = Number(w.run_output_new(JSON.stringify({ testRunner: false, root: ROOT, cwd: ROOT })));
  const response = {
    version: "HTTP/1.1",
    status: resp.status,
    reason: resp.reason,
    headers: resp.headers.map(([name, value]) => ({ name, value })),
    body: resp.body,
    size: resp.body.length,
    elapsedMs: resp.ms,
  };
  let text: string;
  if (resolved.problems.length) {
    text = callWasmSync<string>(w.scratches_http_render, { request, error: resolved.problems.join("\n") });
  } else {
    // A remote project sends from its host through an SSH tunnel (`httpClient.sendFrom`).
    const remote = currentRemote();
    const viaHost = remote && mockSettingValue("httpClient.sendFrom") !== "local";
    text =
      callWasmSync<string>(w.scratches_http_render, { request }) +
      (viaHost ? `\x1b[90mSent from ${remote.label} through an SSH tunnel\x1b[0m\n\n` : "") +
      callWasmSync<string>(w.scratches_http_render, { response });
    if (resolved.handler) {
      const out = callWasmSync<{ globals: Record<string, string>; text: string }>(w.scratches_http_handler, {
        script: resolved.handler,
        response,
        globals: httpGlobals,
      });
      httpGlobals = out.globals;
      text += out.text;
    }
  }
  setTimeout(() => emitMock(event, { type: "started", pid: 0, commandLine } satisfies RunEvent), 0);
  setTimeout(() => {
    const rendered: RunEvent[] = JSON.parse(w.run_output_handle(JSON.stringify({ handle: output, input: { type: "output", stream: "stdout", text } })));
    rendered.forEach((e) => emitMock(event, e));
    emitMock(event, { type: "exited", code: 0, signal: null } satisfies RunEvent);
    w.run_output_drop(JSON.stringify({ handle: output }));
  }, resp.ms * 3);
  return { runId: id, commandLine };
});
