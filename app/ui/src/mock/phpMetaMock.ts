/**
 * Browser-mode PHP metadata and attributes: approximations of the Rust PHP index over the
 * demo project (`.phpstorm.meta.php`, JetBrains attributes in its classes), the demo include
 * path for "External Libraries", and the source roots computed by the same Rust code as
 * the desktop app (`forge-file-templates` as WebAssembly). Nothing is written anywhere.
 */
import { registerMock } from "@/ipc";
import type { LibraryNode, LibraryRoot, PhpCompletion, PhpCompletionItem, PhpProblem, SourceRootEntry, SourceRootSetting } from "@/ipc/phpMeta";
import { callWasm, forgeWasm } from "@/wasm";
import { demoLibraryExists, demoLibraryFiles } from "./demoLibraries";
import { mockSettingValue } from "./settingsMock";
import { mockFiles } from "./workspaceMock";

const isPhp = (p: string) => /\.(php|phtml|inc)$/i.test(p) && !p.endsWith(".blade.php");

interface MethodInfo {
  deprecated?: { reason: string; replacement?: string };
  pure?: boolean;
  /** Expected values by argument index. */
  expected: Record<number, string[]>;
}

const unquote = (v: string) => v.trim().replace(/^['"]|['"]$/g, "");
const split = (list: string) =>
  list
    .split(",")
    .map((x) => x.trim())
    .filter(Boolean);

/** Methods with JetBrains attributes in the demo's PHP files, and the metadata's values. */
function demoMethods(): Map<string, MethodInfo> {
  const out = new Map<string, MethodInfo>();
  const get = (name: string) => {
    let m = out.get(name.toLowerCase());
    if (!m) out.set(name.toLowerCase(), (m = { expected: {} }));
    return m;
  };
  for (const [path, text] of Object.entries(mockFiles)) {
    if (!isPhp(path)) continue;
    for (const m of text.matchAll(/#\[Deprecated\(([^\n]*)\)\]\s*(?:(?:public|protected|private|static)\s+)*function\s+(\w+)/g)) {
      const reason = /reason:\s*'([^']*)'/.exec(m[1])?.[1] ?? "";
      const replacement = /replacement:\s*'((?:[^'\\]|\\.)*)'/.exec(m[1])?.[1]?.replace(/\\'/g, "'");
      get(m[2]).deprecated = { reason, replacement };
    }
    for (const m of text.matchAll(/#\[Pure\]\s*(?:(?:public|protected|private|static)\s+)*function\s+(\w+)/g)) get(m[1]).pure = true;
    for (const m of text.matchAll(/function\s+(\w+)\(((?:[^()]|\([^()]*\))*)\)/g)) {
      split(m[2].replace(/\[[^\]]*\]/g, (x) => x.replace(/,/g, "\u0000"))).forEach((param, i) => {
        const ev = /#\[ExpectedValues\(\[([^\]]*)\]\)\]/.exec(param.replace(/\u0000/g, ","));
        if (ev) get(m[1]).expected[i] = split(ev[1]);
      });
    }
  }
  const meta = mockFiles[".phpstorm.meta.php"] ?? "";
  const sets = new Map<string, string[]>();
  for (const m of meta.matchAll(/registerArgumentsSet\('(\w+)',\s*([^;]*)\);/g)) sets.set(m[1], split(m[2].replace(/\)$/, "")));
  for (const m of meta.matchAll(/expectedArguments\(\\[\w\\]+::(\w+)\(\),\s*(\d+),\s*([^;]*)\);/g)) {
    const raw = m[3];
    const set = /argumentsSet\('(\w+)'\)/.exec(raw);
    get(m[1]).expected[Number(m[2])] = set ? (sets.get(set[1]) ?? []) : split(raw);
  }
  return out;
}

async function expectedNamespace(path: string): Promise<string | null> {
  const dir = path.split("/").slice(0, -1).join("/");
  const composerPath = String(mockSettingValue("php.composer.json") || "composer.json");
  return callWasm<string | null>((await forgeWasm()).templates_php_namespace, {
    composerJson: mockFiles[composerPath] ?? null,
    composerPath,
    dir,
    sourceRoots: mockSettingValue("php.sourceRoots") ?? [],
  });
}

/**
 * What Rust's PHP data-flow analysis (`phpforge_lang_php::dataflow`, tree-sitter, not built
 * to WebAssembly) reports for the demo files, found again by their text so edits keep them.
 */
const DEMO_DATA_FLOW: { path: string; needle: string; nth: number; code: string; message: string; fix?: "keepExpression" | "removeLine" }[] = [
  {
    path: "app/Services/InvoiceService.php",
    needle: "$lines = array_map(fn ($line) => $line->price * $line->quantity, $order->lines)",
    nth: 0,
    code: "PhpUnusedAssignment",
    message: "The value assigned to '$lines' is never used",
    fix: "keepExpression",
  },
  { path: "app/Services/LegacyPricing.php", needle: "$prefix = 'ORD'", nth: 0, code: "PhpUnusedAssignment", message: "The value assigned to '$prefix' is never used", fix: "removeLine" },
  { path: "app/Services/LegacyPricing.php", needle: "$verbose", nth: 1, code: "PhpConstantCondition", message: "Condition is always false" },
  { path: "app/Services/LegacyPricing.php", needle: "$customer", nth: 1, code: "PhpVariableIsAlwaysNull", message: "Variable '$customer' is always null here" },
  { path: "app/Services/LegacyPricing.php", needle: "$taxRate", nth: 0, code: "PhpUndefinedVariable", message: "Variable '$taxRate' is undefined here" },
];

function demoDataFlow(path: string, text: string): PhpProblem[] {
  const out: PhpProblem[] = [];
  for (const d of DEMO_DATA_FLOW.filter((x) => x.path === path)) {
    let at = -1;
    for (let i = 0; i <= d.nth; i++) {
      at = text.indexOf(d.needle, at + 1);
      if (at < 0) break;
    }
    if (at < 0) continue;
    const end = at + d.needle.length;
    const lineStart = text.lastIndexOf("\n", at) + 1;
    const lineEnd = text.indexOf("\n", end);
    const fix =
      d.fix === "keepExpression"
        ? { title: "Remove assignment, keep the expression", edits: [{ start: at, end: text.indexOf("=", at) + 2, newText: "" }] }
        : d.fix === "removeLine"
          ? { title: "Remove unused assignment", edits: [{ start: lineStart, end: lineEnd < 0 ? text.length : lineEnd + 1, newText: "" }] }
          : undefined;
    out.push({ start: at, end, severity: "warning", message: d.message, code: d.code, unnecessary: d.code === "PhpUnusedAssignment", fix });
  }
  return out;
}

registerMock("php_inspect", async ({ path, content }): Promise<PhpProblem[]> => {
  const p = String(path);
  const text = (content as string | undefined) ?? mockFiles[p];
  if (!isPhp(p) || text === undefined) return [];
  const methods = demoMethods();
  const out: PhpProblem[] = demoDataFlow(p, text);
  for (const m of text.matchAll(/(\$\w+(?:\(\))?|\w+\([^()]*\))->(\w+)\(([^()]*)\)/g)) {
    const info = methods.get(m[2].toLowerCase());
    if (!info) continue;
    const nameAt = m.index! + m[1].length + 2;
    if (info.deprecated) {
      const args = split(m[3]);
      const replacement = info.deprecated.replacement
        ?.replace("%parametersList%", args.join(", "))
        .replace(/%parameter(\d+)%/g, (_x, n) => args[Number(n)] ?? "")
        .replace("%class%", m[1]);
      out.push({
        start: nameAt,
        end: nameAt + m[2].length,
        severity: "weak",
        message: `Method '${m[2]}' is deprecated${info.deprecated.reason ? `: ${info.deprecated.reason}` : ""}`,
        code: "PhpDeprecation",
        deprecated: true,
        fix: replacement ? { title: `Replace with '${replacement}'`, edits: [{ start: m.index!, end: m.index! + m[0].length, newText: replacement }] } : undefined,
      });
    }
    const args = split(m[3]);
    args.forEach((arg, i) => {
      const expected = info.expected[i];
      if (!expected || !/^'[^']*'$/.test(arg) || expected.map(unquote).includes(unquote(arg))) return;
      const at = m.index! + m[0].indexOf(arg, m[1].length + m[2].length + 3);
      out.push({ start: at, end: at + arg.length, severity: "warning", message: `Value should be one of: ${expected.join(", ")}`, code: "PhpExpectedValuesShouldBeUsed" });
    });
    // A pure call as a statement of its own.
    const lineStart = text.lastIndexOf("\n", m.index!) + 1;
    if (info.pure && /^\s*$/.test(text.slice(lineStart, m.index!)) && text.slice(m.index! + m[0].length).startsWith(";")) {
      out.push({ start: m.index!, end: m.index! + m[0].length, severity: "weak", message: "The result of the pure method call is not used", code: "PhpExpressionResultUnused" });
    }
  }
  const ns = /^namespace\s+([\w\\]+)\s*;/m.exec(text);
  if (ns && !p.startsWith("vendor/")) {
    const expected = await expectedNamespace(p);
    if (expected && expected !== ns[1]) {
      const start = ns.index + ns[0].indexOf(ns[1]);
      out.push({
        start,
        end: start + ns[1].length,
        severity: "warning",
        message: "Namespace name doesn't match PSR-0/PSR-4 project structure",
        code: "PhpIllegalPsrClassPath",
        fix: { title: `Set namespace to '${expected}'`, edits: [{ start, end: start + ns[1].length, newText: expected }] },
      });
    }
  }
  return out.sort((a, b) => a.start - b.start);
});

registerMock("php_complete", ({ path, offset, content }): PhpCompletion | null => {
  const p = String(path);
  const text = (content as string | undefined) ?? mockFiles[p] ?? "";
  const at = Number(offset);
  const before = text.slice(0, at);
  const inString = /'([\w-]*)$/.exec(before);
  const items = (values: string[], detail: string): PhpCompletionItem[] =>
    values.map((v) => (inString ? { label: unquote(v), insert: unquote(v), kind: "value", detail } : { label: v, insert: v, kind: "value", detail }));
  const from = inString ? at - inString[1].length : at - (/[\w\\:]*$/.exec(before)?.[0].length ?? 0);
  const rest = inString ? (/^[\w-]*/.exec(text.slice(at))?.[0].length ?? 0) : 0;
  // `app('…')`: the keys of the metadata map.
  if (/\bapp\('[\w-]*$/.test(before)) {
    const keys = [...(mockFiles[".phpstorm.meta.php"] ?? "").matchAll(/'(\w+)' => \\/g)].map((m) => `'${m[1]}'`);
    return { from, to: at + rest, items: items(keys, "expected value") };
  }
  // `$x->status() === '…'`: the expected return values.
  const cmp = /->(\w+)\(\)\s*[!=]==?\s*'[\w-]*$/.exec(before);
  if (cmp) {
    const meta = mockFiles[".phpstorm.meta.php"] ?? "";
    const m = new RegExp(`expectedReturnValues\\(\\\\[\\w\\\\]+::${cmp[1]}\\(\\),\\s*([^;]*)\\);`).exec(meta);
    return m ? { from, to: at + rest, items: items(split(m[1].replace(/\)$/, "")), "expected return value") } : null;
  }
  // `$payment['…']`: array shape keys.
  if (/\$payment\['[\w-]*$/.test(before)) {
    const shape = /#\[ArrayShape\(\[([^\]]*)\]\)\]/.exec(mockFiles["app/Services/PaymentGateway.php"] ?? "");
    const keys = shape ? [...shape[1].matchAll(/'(\w+)' => '(\w+)'/g)] : [];
    return { from, to: at + rest, items: keys.map((k) => ({ label: k[1], insert: k[1], kind: "key", detail: k[2] })) };
  }
  // An argument of a method with expected values.
  const call = /->(\w+)\(([^()]*)$/.exec(before);
  if (call) {
    const info = demoMethods().get(call[1].toLowerCase());
    const index = (call[2].match(/,/g) ?? []).length;
    const values = info?.expected[index];
    if (values?.length) return { from, to: at + rest, items: items(values, "expected value") };
  }
  return null;
});

registerMock("php_language_places", () => []);

const includePaths = () => ((mockSettingValue("php.includePaths") as string[] | null) ?? []).filter((p) => p.trim());

registerMock("php_libraries", (): LibraryRoot[] =>
  includePaths().map((p) => ({ name: p.replace(/\/+$/, "").split("/").pop() || p, path: p.replace(/\/+$/, ""), exists: demoLibraryExists(p) })),
);

registerMock("php_library_list", ({ path }): LibraryNode[] => {
  const dir = String(path).replace(/\/+$/, "");
  if (!includePaths().some((p) => `${dir}/`.startsWith(`${p.replace(/\/+$/, "")}/`))) throw new Error(`not inside an include path: ${dir}`);
  const names = new Map<string, boolean>();
  for (const f of Object.keys(demoLibraryFiles)) {
    if (!f.startsWith(`${dir}/`)) continue;
    const rest = f.slice(dir.length + 1).split("/");
    names.set(rest[0], rest.length > 1 || names.get(rest[0]) === true);
  }
  return [...names.entries()]
    .map(([name, isDir]) => ({ name, path: `${dir}/${name}`, dir: isDir, open: isDir ? undefined : `library://file/${dir}/${name}` }))
    .sort((a, b) => Number(b.dir) - Number(a.dir) || a.name.localeCompare(b.name));
});

registerMock("php_source_roots", async ({ overrides }): Promise<SourceRootEntry[]> => {
  const composerPath = String(mockSettingValue("php.composer.json") || "composer.json");
  return callWasm((await forgeWasm()).php_source_roots, {
    composerJson: mockFiles[composerPath] ?? null,
    composerPath,
    sourceRoots: (overrides as SourceRootSetting[] | undefined) ?? mockSettingValue("php.sourceRoots") ?? [],
  });
});
