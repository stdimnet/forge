/**
 * Browser-mode PHP quality tools: the Laravel demo project "has" PHPStan (Larastan),
 * PHP_CodeSniffer, Laravel Pint and Rector in vendor/bin. Checks answer with demo problems
 * found in the current text (so they follow edits), fixers change the in-memory demo files'
 * text the way the real tools would for those problems. Nothing runs and no real file is
 * written; like the Rust commands, everything is refused in Safe Mode.
 */
import { invoke, registerMock } from "@/ipc";
import type {
  CheckResult,
  FixResult,
  QualityProblem,
  QualityTool,
  QualityToolInfo,
  QualityToolOutcome,
  ToolConfig,
} from "@/ipc/qualityTools";
import { TOOL_LABELS } from "@/ipc/qualityTools";
import { mockSettingValue } from "./settingsMock";
import { mockSafeMode } from "./trustMock";

const ORDER: QualityTool[] = ["phpcs", "phpCsFixer", "phpmd", "phpstan", "psalm", "pint", "rector"];

const BINARY: Record<QualityTool, string> = {
  phpstan: "phpstan",
  psalm: "psalm",
  phpCsFixer: "php-cs-fixer",
  phpcs: "phpcs",
  phpmd: "phpmd",
  pint: "pint",
  rector: "rector",
};

/** What the demo project has installed, with the version `--version` prints. */
const INSTALLED: Partial<Record<QualityTool, string>> = {
  phpstan: "PHPStan - PHP Static Analysis Tool 1.12.7",
  phpcs: "PHP_CodeSniffer version 3.10.3 (stable) by Squiz and PHPCSStandards",
  pint: "Pint 1.18.1",
  rector: "Rector 1.2.8",
};

const CONFIG_FILES: Record<QualityTool, string[]> = {
  phpstan: ["phpstan.neon", "phpstan.neon.dist", "phpstan.dist.neon"],
  psalm: ["psalm.xml", "psalm.xml.dist"],
  phpCsFixer: [".php-cs-fixer.php", ".php-cs-fixer.dist.php"],
  phpcs: [".phpcs.xml", "phpcs.xml", ".phpcs.xml.dist", "phpcs.xml.dist"],
  phpmd: ["phpmd.xml", "phpmd.xml.dist", ".phpmd.xml"],
  pint: ["pint.json"],
  rector: ["rector.php"],
};

/** Configuration files of the demo project. */
const DEMO_CONFIG: Partial<Record<QualityTool, string>> = { phpstan: "phpstan.neon", pint: "pint.json", rector: "rector.php" };

const FIXERS: Partial<Record<QualityTool, string>> = { phpCsFixer: "PHP CS Fixer", phpcs: "phpcbf", pint: "Pint", rector: "Rector" };

const str = (k: string) => {
  const v = mockSettingValue(k);
  return typeof v === "string" ? v : "";
};

function configOf(tool: QualityTool): ToolConfig {
  const k = (o: string) => `php.tools.${tool}.${o}`;
  const timeout = mockSettingValue(k("timeout"));
  return {
    tool,
    inspection: mockSettingValue(k("inspection")) === true,
    path: str(k("path")),
    fixerPath: str(k("phpcbfPath")),
    interpreter: str(k("interpreter")),
    config: str(k("config")),
    level: str(k("level")),
    options: str(k("options")),
    timeoutSecs: typeof timeout === "number" ? timeout : 30,
    severity: (str(k("severity")) || "tool") as ToolConfig["severity"],
  };
}

function interpreterNames(): string[] {
  const names: string[] = [];
  for (const key of ["php.projectInterpreters", "php.interpreters"]) {
    const list = mockSettingValue(key);
    if (Array.isArray(list)) for (const i of list) if (i && typeof i === "object" && "name" in i) names.push(String((i as { name: unknown }).name));
  }
  return names;
}

const found = (c: ToolConfig) => (c.path ? c.path : INSTALLED[c.tool] ? `vendor/bin/${BINARY[c.tool]}` : null);

function refuseInSafeMode(): void {
  if (mockSafeMode()) throw new Error("Running PHP quality tools is not allowed in Safe Mode. Trust the project to run it.");
}

registerMock("quality_tools_info", () => ({
  tools: ORDER.map((tool): QualityToolInfo => {
    const config = configOf(tool);
    return {
      tool,
      label: TOOL_LABELS[tool],
      kind: tool === "phpstan" || tool === "psalm" || tool === "phpmd" ? "analyzer" : "fixer",
      fixer: FIXERS[tool] ?? null,
      configFiles: CONFIG_FILES[tool],
      path: found(config),
      fixerPath: tool === "phpcs" ? "vendor/bin/phpcbf" : null,
      configFile: config.config || DEMO_CONFIG[tool] || null,
      config,
    };
  }),
  interpreters: interpreterNames(),
}));

registerMock("quality_tools_validate", ({ config }) => {
  refuseInSafeMode();
  const c = config as ToolConfig;
  const version = INSTALLED[c.tool];
  const program = c.path || `vendor/bin/${BINARY[c.tool]}`;
  const commandLine = c.interpreter ? `php ${program} --version (${c.interpreter})` : `${program} --version`;
  return version && !c.path
    ? { ok: true, message: version, commandLine }
    : { ok: false, message: `${TOOL_LABELS[c.tool]} is not installed in the project (${program}).`, commandLine };
});

// ---- Demo problems ------------------------------------------------------------------------

interface DemoProblem {
  tool: QualityTool;
  file: string;
  /** Where in the text: the problem sits on the first match (none: not reported). */
  match: RegExp;
  severity: QualityProblem["severity"];
  message: string;
  rule?: string;
  fixable?: boolean;
  /** Underline the match only (else the line). */
  exact?: boolean;
}

const CONTROLLER = "app/Http/Controllers/PaymentController.php";
const GATEWAY = "app/Services/PaymentGateway.php";
const USER = "app/Models/User.php";

const DEMO: DemoProblem[] = [
  {
    tool: "phpstan",
    file: CONTROLLER,
    match: /'later'/,
    severity: "error",
    message: "Parameter #1 $mode of method App\\Services\\PaymentGateway::capture() expects 'auto'|'manual', 'later' given.",
    rule: "argument.type",
    exact: true,
  },
  {
    tool: "phpstan",
    file: CONTROLLER,
    match: /app\('payments'\)->charge/,
    severity: "error",
    message: "Cannot call method charge() on mixed.",
    rule: "method.nonObject",
    exact: true,
  },
  {
    tool: "phpstan",
    file: USER,
    match: /OrderStatus::Paid/,
    severity: "error",
    message: "Access to constant Paid on an unknown class App\\Models\\OrderStatus.",
    rule: "class.notFound",
    exact: true,
  },
  {
    tool: "phpstan",
    file: USER,
    match: /protected function casts\(\): array/,
    severity: "error",
    message: "Method App\\Models\\User::casts() return type has no value type specified in iterable type array.",
    rule: "missingType.iterableValue",
  },
  {
    tool: "psalm",
    file: GATEWAY,
    match: /string \$mode/,
    severity: "weak",
    message: "Param $mode is never referenced in this method",
    rule: "UnusedParam",
    exact: true,
  },
  {
    tool: "phpmd",
    file: GATEWAY,
    match: /\$mode\)/,
    severity: "weak",
    message: "Avoid unused parameters such as '$mode'.",
    rule: "UnusedFormalParameter",
  },
  {
    tool: "phpcs",
    file: GATEWAY,
    match: /#\[Deprecated\(/,
    severity: "warning",
    message: "Line exceeds 120 characters; contains 126 characters",
    rule: "Generic.Files.LineLength.TooLong",
  },
  {
    tool: "phpCsFixer",
    file: USER,
    match: /use HasFactory, Notifiable;/,
    severity: "weak",
    message: "PHP CS Fixer: single_trait_insert_per_statement",
    rule: "single_trait_insert_per_statement",
    fixable: true,
  },
  {
    tool: "pint",
    file: USER,
    match: /use HasFactory, Notifiable;/,
    severity: "weak",
    message: "Laravel Pint: single_trait_insert_per_statement",
    rule: "single_trait_insert_per_statement",
    fixable: true,
  },
  {
    tool: "rector",
    file: GATEWAY,
    match: /^class PaymentGateway$/m,
    severity: "weak",
    message: "Rector: FinalizeClassesWithoutChildrenRector",
    rule: "Rector\\Privatization\\Rector\\Class_\\FinalizeClassesWithoutChildrenRector",
    fixable: true,
  },
];

function position(text: string, index: number): { line: number; column: number } {
  const before = text.slice(0, index);
  const line = before.split("\n").length - 1;
  return { line, column: index - (before.lastIndexOf("\n") + 1) };
}

/** The problems `tool` reports for `text` of `file`. */
function problemsIn(tool: QualityTool, file: string, text: string): QualityProblem[] {
  const out: QualityProblem[] = [];
  for (const d of DEMO) {
    if (d.tool !== tool || d.file !== file) continue;
    const m = d.match.exec(text);
    if (!m) continue;
    const at = position(text, m.index);
    out.push({
      tool,
      file,
      line: at.line,
      column: d.exact ? at.column : 0,
      ...(d.exact ? { endLine: at.line, endColumn: at.column + m[0].length } : {}),
      severity: d.severity,
      message: d.message,
      rule: d.rule,
      fixable: !!d.fixable,
    });
  }
  // phpcs: trailing whitespace anywhere (shows on the fly while typing).
  if (tool === "phpcs" && /\.php$/.test(file)) {
    text.split("\n").forEach((l, i) => {
      const m = /[ \t]+$/.exec(l);
      if (m && l.trim()) {
        out.push({
          tool,
          file,
          line: i,
          column: m.index,
          endLine: i,
          endColumn: l.length,
          severity: "error",
          message: "Whitespace found at end of line",
          rule: "Squiz.WhiteSpace.SuperfluousWhitespace.EndLine",
          fixable: true,
        });
      }
    });
  }
  return out;
}

const readFile = (path: string) => invoke<string>("fs_read_file", { path }).catch(() => "");

/** Tools that read unsaved text (phpcs and PHP CS Fixer on stdin, PHPStan's temporary copy). */
const READS_UNSAVED: QualityTool[] = ["phpcs", "phpCsFixer", "phpstan"];

function outcome(tool: QualityTool): QualityToolOutcome {
  return INSTALLED[tool]
    ? { tool, label: TOOL_LABELS[tool], status: "ok", message: "" }
    : {
        tool,
        label: TOOL_LABELS[tool],
        status: "missing",
        message: `${TOOL_LABELS[tool]} is not installed in the project (vendor/bin/${BINARY[tool]}). Install it with Composer or set its path in Settings | Languages & Frameworks | PHP | Quality Tools.`,
      };
}

function withSeverity(tool: QualityTool, list: QualityProblem[]): QualityProblem[] {
  const choice = configOf(tool).severity;
  return choice === "tool" ? list : list.map((p) => ({ ...p, severity: choice }));
}

registerMock("quality_tools_check", async ({ path, text }): Promise<CheckResult> => {
  refuseInSafeMode();
  const file = String(path);
  const saved = await readFile(file);
  const unsaved = typeof text === "string" && text !== saved;
  const content = unsaved ? String(text) : saved;
  const tools = ORDER.filter((t) => configOf(t).inspection && (!unsaved || READS_UNSAVED.includes(t)));
  const outcomes = tools.map(outcome);
  const problems = tools.filter((t) => INSTALLED[t]).flatMap((t) => withSeverity(t, problemsIn(t, file, content)));
  return { files: [file], tools, problems, outcomes };
});

registerMock("quality_tools_inspect", async ({ paths }): Promise<CheckResult> => {
  refuseInSafeMode();
  const tools = ORDER.filter((t) => configOf(t).inspection);
  const scope = (paths as string[] | null) ?? null;
  const files = [...new Set(DEMO.map((d) => d.file))].filter((f) => !scope || scope.some((s) => f === s || f.startsWith(`${s}/`)));
  const problems: QualityProblem[] = [];
  for (const f of files) {
    const text = await readFile(f);
    for (const t of tools) if (INSTALLED[t]) problems.push(...withSeverity(t, problemsIn(t, f, text)));
  }
  return { files: scope ?? [], tools, problems, outcomes: tools.map(outcome) };
});

// ---- Fixers -------------------------------------------------------------------------------

/** The text after `tool` fixed the demo problems in it. */
export function mockFixedText(tool: QualityTool, text: string): string {
  switch (tool) {
    case "phpcs":
      return text.replace(/[ \t]+$/gm, "");
    case "phpCsFixer":
    case "pint":
      return text.replace(/^(\s*)use HasFactory, Notifiable;$/m, "$1use HasFactory;\n$1use Notifiable;").replace(/[ \t]+$/gm, "");
    case "rector":
      return text.replace(/^class PaymentGateway$/m, "final class PaymentGateway");
    default:
      return text;
  }
}

/** A unified diff of one hunk around the changed lines (3 lines of context). */
export function mockDiff(before: string, after: string): string {
  if (before === after) return "";
  const a = before.replace(/\n$/, "").split("\n");
  const b = after.replace(/\n$/, "").split("\n");
  let start = 0;
  while (start < a.length && start < b.length && a[start] === b[start]) start++;
  let end = 0;
  while (end < a.length - start && end < b.length - start && a[a.length - 1 - end] === b[b.length - 1 - end]) end++;
  const from = Math.max(0, start - 3);
  const oldTo = Math.min(a.length, a.length - end + 3);
  const newTo = Math.min(b.length, b.length - end + 3);
  const lines = [
    "--- Original",
    "+++ New",
    `@@ -${from + 1},${oldTo - from} +${from + 1},${newTo - from} @@`,
    ...a.slice(from, start).map((l) => ` ${l}`),
    ...a.slice(start, a.length - end).map((l) => `-${l}`),
    ...b.slice(start, b.length - end).map((l) => `+${l}`),
    ...a.slice(a.length - end, oldTo).map((l) => ` ${l}`),
  ];
  return `${lines.join("\n")}\n`;
}

registerMock("quality_tools_fix", ({ path, tool, text }): FixResult => {
  refuseInSafeMode();
  const t = tool as QualityTool;
  if (!FIXERS[t]) throw new Error(`${TOOL_LABELS[t]} does not fix files`);
  if (!INSTALLED[t]) throw new Error(`${TOOL_LABELS[t]} is not installed in the project (vendor/bin/${BINARY[t]}).`);
  const before = String(text);
  const fixed = mockFixedText(t, before);
  const diff = t === "rector" || t === "phpCsFixer" ? mockDiff(before, fixed) : "";
  const rules = t === "rector" && fixed !== before ? ["Rector\\Privatization\\Rector\\Class_\\FinalizeClassesWithoutChildrenRector"] : [];
  void path;
  return { tool: t, text: fixed, changed: fixed !== before, diff, rules };
});
