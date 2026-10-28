/**
 * Browser-mode AI in the editor: a fake provider with canned answers. The answers go through
 * the same Rust code as the desktop app (WebAssembly: `forge_ai_editor` post-processing,
 * prompts, proposals and the `.aiignore` policy); only the model is fake. Nothing is sent
 * anywhere and no file is written.
 */
import { invoke, registerMock } from "@/ipc";
import type { ProvidersState } from "@/ipc/ai";
import type { ActionRequest, AiEditorStatus, ExplainStarted, InlineAnswer, InlineRequest, Proposal, ProviderSummary, Verdict } from "@/ipc/aiEditor";
import { callWasm, forgeWasm } from "@/wasm";
import { streamMockText } from "./aiMock";
import { mockSettingValue } from "./settingsMock";
import { mockFiles } from "./workspaceMock";

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const LOCAL = new Set(["ollama", "lmstudio", "llamacpp"]);
const FIM = new Set(["ollama", "lmstudio", "llamacpp", "custom"]);

function setting<T>(key: string, fallback: T): T {
  const v = mockSettingValue(key);
  return v == null ? fallback : (v as T);
}

async function verdict(path: string, language: string | undefined, inline: boolean): Promise<Verdict> {
  const wasm = await forgeWasm();
  return callWasm<Verdict>(wasm.ai_policy_check, {
    path,
    language,
    inline,
    projectEnabled: setting("ai.enabled", true),
    disabledLanguages: setting<string[]>("ai.completion.disabledLanguages", []),
    ignore: [mockFiles[".aiignore"], mockFiles[".aiexclude"]].filter((t): t is string => t != null),
  });
}

/** The language id Rust uses for `path` (via the inline context). */
async function languageOf(path: string, language?: string): Promise<string> {
  const wasm = await forgeWasm();
  return (await callWasm<{ language: string }>(wasm.ai_inline_finish, { request: { path, language, text: "", offset: 0 }, raw: "" })).language;
}

async function summary(feature: "completion" | "chat"): Promise<ProviderSummary> {
  const state = await invoke<ProvidersState>("ai_providers_list");
  const sel = state.features[feature];
  const id = sel?.provider ?? state.defaultProvider;
  const p = state.providers.find((x) => x.id === id);
  if (!p) throw new Error("no AI provider configured");
  if (!p.enabled) throw new Error(`AI provider ${p.name} is disabled (enable it in Settings | Tools | AI Providers)`);
  const mode = setting<string>("ai.completion.mode", "auto");
  return {
    id: p.id,
    name: p.name,
    model: (sel?.provider === p.id ? sel.model : null) ?? p.defaultModel ?? null,
    local: LOCAL.has(p.kind),
    fim: mode === "fim" || (mode === "auto" && FIM.has(p.kind)),
  };
}

registerMock("ai_editor_status", async ({ path, language }): Promise<AiEditorStatus> => {
  const settle = (f: "completion" | "chat") =>
    summary(f).then(
      (s) => [s, null] as const,
      (e: Error) => [null, e.message] as const,
    );
  const [[completion, completionProblem], [actions, actionsProblem]] = await Promise.all([settle("completion"), settle("chat")]);
  const p = path ? String(path) : null;
  const lang = language ? String(language) : undefined;
  return {
    projectEnabled: setting("ai.enabled", true),
    completionEnabled: setting("ai.completion.enabled", false),
    completion,
    completionProblem,
    actions,
    actionsProblem,
    language: p ? (await languageOf(p, lang)) : null,
    file: p ? await verdict(p, lang, true) : null,
    fileActions: p ? await verdict(p, lang, false) : null,
  };
});

// ---- inline completion

const lineAt = (text: string, offset: number) => {
  const start = text.lastIndexOf("\n", offset - 1) + 1;
  const end = text.indexOf("\n", offset);
  return { before: text.slice(start, offset), after: text.slice(offset, end < 0 ? text.length : end) };
};

/**
 * What the fake model answers, like a real one sometimes would: chat-style answers echo the
 * line and wrap code in fences, which the Rust post-processing removes.
 */
function cannedInline(req: InlineRequest, alternative: number): string {
  const { before } = lineAt(req.text, req.offset);
  const php = req.path.endsWith(".php");
  const indent = /^\s*/.exec(before)?.[0] ?? "";
  const trimmed = before.trim();
  if (/return\s*$/.test(before)) {
    const options = php
      ? ["$this->items->sum(fn (Item $item) => $item->price * $item->quantity);", "array_sum(array_map(fn ($i) => $i->total(), $this->items));"]
      : ["items.reduce((sum, item) => sum + item.price * item.quantity, 0);", "items.map((i) => i.total()).reduce((a, b) => a + b, 0);"];
    return `\`\`\`\n${before}${options[alternative % options.length]}\n\`\`\``;
  }
  if (!trimmed) {
    const blocks = php
      ? [
          `$total = 0;\n${indent}foreach ($this->items as $item) {\n${indent}    $total += $item->price * $item->quantity;\n${indent}}\n${indent}return $total;\n}`,
          `return collect($this->items)\n${indent}    ->sum(fn ($item) => $item->price * $item->quantity);`,
        ]
      : [
          `let total = 0;\n${indent}for (const item of items) {\n${indent}  total += item.price * item.quantity;\n${indent}}\n${indent}return total;\n}`,
          `return items.reduce((sum, item) => sum + item.price * item.quantity, 0);`,
        ];
    return blocks[alternative % blocks.length];
  }
  if (/\($/.test(trimmed)) return php ? "$request->validated());" : "options);";
  if (/=\s*$/.test(before)) return php ? "null;" : "undefined;";
  if (/^(public |private |protected )?function \w*$/.test(trimmed)) return php ? "total(): int" : "total() {";
  return "";
}

const pendingInline = new Map<string, () => void>();

registerMock("ai_inline_complete", async ({ request, requestId, temperature }): Promise<InlineAnswer> => {
  if (!setting("ai.completion.enabled", false)) return { text: null, reason: "disabled" };
  const req = request as InlineRequest;
  const v = await verdict(req.path, req.language, true);
  if (v.status !== "allowed") return { text: null, reason: v.message };
  const provider = await summary("completion");
  const id = String(requestId);
  pendingInline.get(id)?.();
  let cancelled = false;
  pendingInline.set(id, () => (cancelled = true));
  await sleep(provider.local ? 180 : 450);
  pendingInline.delete(id);
  if (cancelled) return { text: null, reason: "cancelled" };
  const alternative = temperature ? Math.round(Number(temperature) * 10) : 0;
  const raw = cannedInline(req, alternative);
  const wasm = await forgeWasm();
  const out = await callWasm<{ text: string | null }>(wasm.ai_inline_finish, {
    request: { ...req, multiline: setting("ai.completion.multiline", "auto") },
    raw,
  });
  return { text: out.text };
});

registerMock("ai_inline_cancel", ({ requestId }) => {
  pendingInline.get(String(requestId))?.();
  pendingInline.delete(String(requestId));
});

// ---- actions

interface PromptInfo {
  title: string;
  startLine: number;
  endLine: number;
  testPath?: string;
}

async function check(req: ActionRequest): Promise<PromptInfo> {
  const v = await verdict(req.path, req.language, false);
  if (v.status !== "allowed") throw new Error(v.message);
  await summary("chat");
  const wasm = await forgeWasm();
  return callWasm<PromptInfo>(wasm.ai_action_prompt, req);
}

const humanize = (name: string) =>
  name
    .replace(/([a-z])([A-Z])/g, "$1 $2")
    .replace(/_/g, " ")
    .toLowerCase()
    .replace(/^./, (c) => c.toUpperCase());

function declaration(req: ActionRequest, p: PromptInfo): { name: string; params: string[]; line: string } {
  const line = req.text.split("\n")[p.startLine] ?? "";
  const m = /(?:function|def|fn|func)?\s*([A-Za-z_$][\w$]*)\s*\(([^)]*)\)/.exec(line);
  const params = (m?.[2] ?? "")
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean);
  return { name: m?.[1] ?? "value", params, line };
}

function cannedDocs(req: ActionRequest, p: PromptInfo): string {
  const { name, params, line } = declaration(req, p);
  const php = req.path.endsWith(".php");
  const summaryLine = `${humanize(name)}.`;
  if (req.path.endsWith(".py")) return `"""${summaryLine}\n\nArgs:\n${params.map((x) => `    ${x.split(":")[0]}: The ${x.split(":")[0]}.`).join("\n")}\n"""`;
  const tags = params.map((x) => {
    if (php) {
      const [type, variable] = x.includes("$") ? [x.slice(0, x.indexOf("$")).trim() || "mixed", x.slice(x.indexOf("$")).split(/\s|=/)[0]] : ["mixed", x];
      return `@param ${type} ${variable} The ${variable.replace("$", "")}.`;
    }
    const n = x.split(/[:=\s]/)[0];
    return `@param ${n} The ${n}.`;
  });
  const ret = php ? (/\):\s*([\w\\?|]+)/.exec(line)?.[1] ?? "mixed") : null;
  const lines = [summaryLine, "", ...tags, ...(ret && ret !== "void" ? [`@return ${ret}`] : []), ...(php && /Exception|throw/.test(req.text) ? ["@throws \\InvalidArgumentException"] : [])];
  return "```\n/**\n" + lines.map((l) => (l ? ` * ${l}` : " *")).join("\n") + "\n */\n```";
}

function cannedTests(req: ActionRequest, p: PromptInfo): string {
  const { name } = declaration(req, p);
  const base = req.path.split("/").pop()!.replace(/\.\w+$/, "");
  if (req.path.endsWith(".php")) {
    const ns = /namespace\s+([^;]+);/.exec(req.text)?.[1] ?? "App";
    const cls = /class\s+(\w+)/.exec(req.text)?.[1] ?? base;
    const testNs = `Tests\\Unit${ns.replace(/^App/, "")}`;
    return [
      "```php",
      "<?php",
      "",
      `namespace ${testNs};`,
      "",
      `use ${ns}\\${cls};`,
      "use PHPUnit\\Framework\\TestCase;",
      "",
      `class ${cls}Test extends TestCase`,
      "{",
      `    public function test_${name}_returns_expected_value(): void`,
      "    {",
      `        $subject = new ${cls}();`,
      "",
      `        $this->assertNotNull($subject->${name}());`,
      "    }",
      "",
      `    public function test_${name}_handles_empty_input(): void`,
      "    {",
      `        $subject = new ${cls}();`,
      "",
      `        $this->assertEmpty($subject->${name}([]) ?: []);`,
      "    }",
      "}",
      "```",
    ].join("\n");
  }
  return [
    "```ts",
    `import { describe, expect, it } from "vitest";`,
    `import { ${name} } from "./${base}";`,
    "",
    `describe("${name}", () => {`,
    `  it("returns the expected value", () => {`,
    `    expect(${name}()).toBeDefined();`,
    "  });",
    "",
    `  it("handles empty input", () => {`,
    `    expect(() => ${name}()).not.toThrow();`,
    "  });",
    "});",
    "```",
  ].join("\n");
}

function cannedFix(req: ActionRequest, p: PromptInfo): string {
  const lines = req.text.split("\n").slice(p.startLine, p.endLine + 1);
  for (const problem of req.problems ?? []) {
    const i = problem.line - p.startLine;
    const l = lines[i];
    if (l == null) continue;
    const undefinedVar = /Undefined variable '?\$?(\w+)'?/i.exec(problem.message);
    if (undefinedVar) {
      const indent = /^\s*/.exec(l)?.[0] ?? "";
      lines.splice(i, 0, `${indent}$${undefinedVar[1]} = [];`);
    } else if (!/[;{}:,]\s*$/.test(l)) lines[i] = `${l.trimEnd()};`;
    else lines[i] = l.replace(/\s+$/, "");
  }
  return "Here is the corrected code:\n\n```\n" + lines.join("\n") + "\n```\n\nThe statement was incomplete.";
}

registerMock("ai_action", async ({ request }): Promise<Proposal> => {
  const req = request as ActionRequest;
  const p = await check(req);
  await sleep(700);
  const reply = req.kind === "docs" ? cannedDocs(req, p) : req.kind === "tests" ? cannedTests(req, p) : cannedFix(req, p);
  const existing = p.testPath && p.testPath !== req.path ? mockFiles[p.testPath] : undefined;
  const wasm = await forgeWasm();
  return callWasm<Proposal>(wasm.ai_action_proposal, { request: { ...req, existingTest: existing ?? null }, reply });
});

function cannedExplanation(req: ActionRequest, p: PromptInfo): string {
  const { name } = declaration(req, p);
  const lines = p.endLine - p.startLine + 1;
  return [
    `**\`${name}\`** (${lines} lines) computes a value from the object's state and returns it.`,
    "",
    "How it works:",
    "",
    `1. It reads the inputs it needs (the parameters and the relevant properties).`,
    "2. It loops over the items and accumulates the result.",
    "3. It returns the accumulated value to the caller.",
    "",
    "Things to watch:",
    "",
    "- An empty input returns the initial value, so callers never get `null`.",
    "- Prices are summed as integers (cents), which avoids floating point rounding errors.",
    `- The method has no side effects, so it is safe to call repeatedly.`,
  ].join("\n");
}

registerMock("ai_explain", async ({ request, streamId }): Promise<ExplainStarted> => {
  const req = request as ActionRequest;
  const p = await check({ ...req, kind: "explain" });
  const provider = await summary("chat");
  streamMockText(String(streamId), cannedExplanation(req, p), 25);
  return { title: p.title, prompt: p.title, provider: provider.name, model: provider.model, startLine: p.startLine, endLine: p.endLine };
});
