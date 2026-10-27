/**
 * Browser-mode web code quality tools. The WebForge demos "have" their tools installed:
 * `?project=react` ESLint and Prettier, `?project=angular` Stylelint (SCSS), `?project=node`
 * Biome. Checks answer with demo problems found in the current text (so they follow
 * edits); fixers and formatters change the text the way the real tools would for those
 * problems. Nothing runs and no real file is written; like the Rust commands, everything is
 * refused in Safe Mode.
 */
import { invoke, registerMock } from "@/ipc";
import type { ProblemSeverity } from "@/ipc/qualityTools";
import type {
  WebCheckResult,
  WebFixResult,
  WebProblem,
  WebTool,
  WebToolConfig,
  WebToolInfo,
  WebToolMode,
  WebToolOutcome,
  WebToolsInfo,
} from "@/ipc/webQualityTools";
import { WEB_TOOL_LABELS } from "@/ipc/webQualityTools";
import { webDemoId, type WebDemoId } from "./demoWebProjects";
import { projectFiles } from "./findMock";
import { mockSettingValue } from "./settingsMock";
import { mockSafeMode } from "./trustMock";

const ORDER: WebTool[] = ["eslint", "prettier", "stylelint", "biome"];
const PACKAGE: Record<WebTool, string> = { eslint: "eslint", prettier: "prettier", stylelint: "stylelint", biome: "@biomejs/biome" };

/** What each demo has installed (version) and its configuration file. */
const DEMO_TOOLS: Record<WebDemoId, Partial<Record<WebTool, { version: string; config: string }>>> = {
  react: {
    eslint: { version: "9.25.1", config: "eslint.config.js" },
    prettier: { version: "3.5.3", config: ".prettierrc" },
  },
  angular: { stylelint: { version: "16.19.1", config: ".stylelintrc.json" } },
  node: { biome: { version: "1.9.4", config: "biome.json" } },
  vue: {},
  webtools: {},
};

const VERSION_LINE: Record<WebTool, (v: string) => string> = {
  eslint: (v) => `v${v}`,
  prettier: (v) => v,
  stylelint: (v) => v,
  biome: (v) => v,
};

/** The files each tool handles by default ("Run for files"). */
const EXTENSIONS: Record<WebTool, RegExp> = {
  eslint: /\.(js|ts|jsx|tsx|cjs|mjs|vue|html)$/i,
  prettier: /\.(js|ts|jsx|tsx|cjs|cts|mjs|mts|vue|astro)$/i,
  stylelint: /\.(css|scss|less|vue)$/i,
  biome: /\.(js|ts|jsx|tsx|cjs|cts|mjs|mts|json|jsonc|css)$/i,
};

const str = (k: string) => {
  const v = mockSettingValue(k);
  return typeof v === "string" ? v : "";
};
const flag = (k: string, d = false) => {
  const v = mockSettingValue(k);
  return typeof v === "boolean" ? v : d;
};

const ON_SAVE: Record<WebTool, string> = {
  eslint: "actionsOnSave.eslint",
  prettier: "actionsOnSave.prettier.enabled",
  stylelint: "actionsOnSave.stylelint",
  biome: "actionsOnSave.biome",
};
const FILES: Record<WebTool, string> = {
  eslint: "web.eslint.files",
  prettier: "actionsOnSave.prettier.files",
  stylelint: "web.stylelint.files",
  biome: "web.biome.files",
};

function configOf(tool: WebTool): WebToolConfig {
  const k = (o: string) => `web.${tool}.${o}`;
  return {
    tool,
    mode: (str(k("mode")) || "automatic") as WebToolMode,
    package: str(k("path")),
    config: str(k("config")),
    options: str(k("options")),
    workingDirs: str(k("workingDirectories")),
    files: str(FILES[tool]),
    onSave: flag(ON_SAVE[tool]),
    onReformat: flag(k("onReformat")),
  };
}

const installed = (tool: WebTool) => {
  const id = webDemoId();
  return id ? DEMO_TOOLS[id][tool] : undefined;
};

/** Whether `tool` handles `path` (Biome first when it has priority). */
export function mockWebToolApplies(tool: WebTool, path: string): boolean {
  const own = (t: WebTool) => configOf(t).mode !== "disabled" && !!installed(t) && EXTENSIONS[t].test(path) && !/node_modules\//.test(path);
  if (!own(tool)) return false;
  if ((tool === "eslint" || tool === "prettier") && own("biome") && flag("web.biome.priority", true)) return false;
  return true;
}

function refuseInSafeMode(): void {
  if (mockSafeMode()) throw new Error("Running web code quality tools is not allowed in Safe Mode. Trust the project to run it.");
}

// ---- Demo problems -----------------------------------------------------------------------

interface Rule {
  tool: WebTool;
  re: RegExp;
  rule: string;
  message: (m: RegExpExecArray) => string;
  severity: ProblemSeverity;
  fixable: boolean;
  link?: string;
  /** Only in style sheets / scripts. */
  files?: RegExp;
}

const SCRIPT = /\.(m?[jt]sx?|c[jt]s|vue|html)$/i;
const STYLE = /\.(css|scss|less|vue)$/i;

const RULES: Rule[] = [
  {
    tool: "eslint",
    re: /\bvar\s+/g,
    rule: "no-var",
    message: () => "Unexpected var, use let or const instead.",
    severity: "error",
    fixable: true,
    link: "https://eslint.org/docs/latest/rules/no-var",
    files: SCRIPT,
  },
  {
    tool: "eslint",
    re: /[^=!<>]([=!]=)[^=]/g,
    rule: "eqeqeq",
    message: (m) => `Expected '${m[1]}=' and instead saw '${m[1]}'.`,
    severity: "warning",
    fixable: false,
    link: "https://eslint.org/docs/latest/rules/eqeqeq",
    files: SCRIPT,
  },
  {
    tool: "eslint",
    re: /\bconsole\.log\(/g,
    rule: "no-console",
    message: () => "Unexpected console statement. Only these console methods are allowed: warn, error.",
    severity: "warning",
    fixable: false,
    link: "https://eslint.org/docs/latest/rules/no-console",
    files: SCRIPT,
  },
  {
    tool: "stylelint",
    re: /#(?=[0-9a-fA-F]*[g-zG-Z])[0-9a-zA-Z]{3,8}\b/g,
    rule: "color-no-invalid-hex",
    message: (m) => `Unexpected invalid hex color "${m[0]}"`,
    severity: "error",
    fixable: false,
    link: "https://stylelint.io/user-guide/rules/color-no-invalid-hex",
    files: STYLE,
  },
  {
    tool: "stylelint",
    re: /#([0-9a-fA-F])\1([0-9a-fA-F])\2([0-9a-fA-F])\3\b/g,
    rule: "color-hex-length",
    message: (m) => `Expected "${m[0]}" to be "#${m[1]}${m[2]}${m[3]}".`,
    severity: "error",
    fixable: true,
    link: "https://stylelint.io/user-guide/rules/color-hex-length",
    files: STYLE,
  },
  {
    tool: "stylelint",
    re: /:\s*0(px|em|rem)\b/g,
    rule: "length-zero-no-unit",
    message: () => "Unexpected unit",
    severity: "error",
    fixable: true,
    link: "https://stylelint.io/user-guide/rules/length-zero-no-unit",
    files: STYLE,
  },
  {
    tool: "biome",
    re: /[^=!<>]([=!]=)[^=]/g,
    rule: "lint/suspicious/noDoubleEquals",
    message: (m) => `Use ${m[1]}= instead of ${m[1]}`,
    severity: "error",
    fixable: true,
    link: "https://biomejs.dev/linter/rules/no-double-equals/",
    files: SCRIPT,
  },
  {
    tool: "biome",
    re: /\blet\s+(\w+)\s*=/g,
    rule: "lint/style/useConst",
    message: () => "This let declares a variable that is only assigned once.",
    severity: "warning",
    fixable: true,
    link: "https://biomejs.dev/linter/rules/use-const/",
    files: SCRIPT,
  },
  {
    tool: "biome",
    re: /(\w|\))!\./g,
    rule: "lint/style/noNonNullAssertion",
    message: () => "Forbidden non-null assertion.",
    severity: "warning",
    fixable: false,
    link: "https://biomejs.dev/linter/rules/no-non-null-assertion/",
    files: SCRIPT,
  },
];

/** `let x = …` is only a problem when `x` is never assigned again. */
function reassigned(text: string, name: string): boolean {
  return new RegExp(`\\b${name}\\s*([-+*/]?=[^=]|\\+\\+|--)`, "g").test(text.replace(new RegExp(`\\blet\\s+${name}\\s*=`), ""));
}

/** The demo problems of `tool` in `text`. */
export function mockWebProblems(tool: WebTool, file: string, text: string): WebProblem[] {
  const out: WebProblem[] = [];
  const lines = text.split("\n");
  lines.forEach((line, i) => {
    const code = line.replace(/\/\/.*$/, "");
    if (/^\s*(\*|\/\*)/.test(line)) return;
    for (const r of RULES) {
      if (r.tool !== tool || (r.files && !r.files.test(file))) continue;
      r.re.lastIndex = 0;
      let m: RegExpExecArray | null;
      while ((m = r.re.exec(code))) {
        if (r.rule === "lint/style/useConst" && reassigned(text, m[1])) continue;
        // The operator itself, not the characters around it.
        const at = m[1] && /[=!]=/.test(m[1]) ? m.index + m[0].indexOf(m[1]) : m.index + (r.rule === "lint/style/noNonNullAssertion" ? 1 : 0);
        const len = m[1] && /[=!]=/.test(m[1]) ? 2 : r.rule === "lint/style/noNonNullAssertion" ? 1 : m[0].trimEnd().length;
        out.push({
          tool,
          file,
          line: i,
          column: at,
          endLine: i,
          endColumn: at + len,
          severity: r.severity,
          message: r.message(m),
          rule: r.rule,
          fixable: r.fixable,
          link: r.link,
        });
      }
    }
  });
  return out;
}

/** What Prettier (and Biome's formatter) would change in the demo files. */
export function mockFormat(text: string): string {
  return text
    .split("\n")
    .map((line) => {
      const indent = line.match(/^\s*/)![0].replace(/\t/g, "  ");
      const body = line
        .slice(line.match(/^\s*/)![0].length)
        .replace(/\s+$/, "")
        .replace(/ {2,}/g, " ")
        .replace(/\( +/g, "(")
        .replace(/ +\)/g, ")")
        .replace(/ +;/g, ";");
      return body ? indent + body : "";
    })
    .join("\n")
    .replace(/\n{3,}/g, "\n\n");
}

/** The text after `tool`'s fixes. */
export function mockWebFixed(tool: WebTool, file: string, text: string): string {
  switch (tool) {
    case "eslint":
      // no-var: `let` (prefer-const turns it into `const` where it is never reassigned).
      return text.replace(/\bvar(\s+)/g, "let$1");
    case "stylelint":
      return text.replace(/#([0-9a-fA-F])\1([0-9a-fA-F])\2([0-9a-fA-F])\3\b/g, "#$1$2$3").replace(/(:\s*)0(px|em|rem)\b/g, "$10");
    case "biome": {
      let out = text.replace(/([^=!<>])([=!])=([^=])/g, "$1$2==$3");
      for (const p of mockWebProblems("biome", file, out)) {
        if (p.rule !== "lint/style/useConst") continue;
        const lines = out.split("\n");
        lines[p.line] = lines[p.line].slice(0, p.column) + lines[p.line].slice(p.column).replace(/^let/, "const");
        out = lines.join("\n");
      }
      return out;
    }
    case "prettier":
      return mockFormat(text);
  }
}

/** Actions on Save: the text after `tool` fixed the saved file, null when it does not apply. */
export function mockWebSaveFix(tool: WebTool, file: string, text: string): string | null {
  if (!mockWebToolApplies(tool, file)) return null;
  return tool === "biome" ? mockFormat(mockWebFixed("biome", file, text)) : mockWebFixed(tool, file, text);
}

export const mockWebToolInstalled = (tool: WebTool): boolean => !!installed(tool);

// ---- Commands ----------------------------------------------------------------------------

function info(tool: WebTool): WebToolInfo {
  const found = installed(tool);
  return {
    tool,
    label: WEB_TOOL_LABELS[tool],
    package: PACKAGE[tool],
    configFiles: [],
    path: found ? `node_modules/${PACKAGE[tool]}` : null,
    version: found?.version ?? null,
    configFile: found?.config ?? null,
    config: configOf(tool),
  };
}

registerMock("web_quality_tools_info", (): WebToolsInfo => ({
  tools: ORDER.map(info),
  node: str("web.node.interpreter") || "/usr/bin/node",
}));

registerMock("web_quality_tools_validate", ({ config }) => {
  refuseInSafeMode();
  const c = config as WebToolConfig;
  const found = installed(c.tool);
  const script = `node_modules/${PACKAGE[c.tool]}/bin/${c.tool}`;
  if (!found) {
    return { ok: false, message: `${WEB_TOOL_LABELS[c.tool]} is configured but not installed (node_modules/${PACKAGE[c.tool]}).`, commandLine: "" };
  }
  return { ok: true, message: VERSION_LINE[c.tool](found.version), commandLine: `/usr/bin/node ${script} --version` };
});

const LINTERS: WebTool[] = ["eslint", "stylelint", "biome"];

function checkText(path: string, text: string): { problems: WebProblem[]; outcomes: WebToolOutcome[] } {
  const problems: WebProblem[] = [];
  const outcomes: WebToolOutcome[] = [];
  for (const tool of LINTERS) {
    if (!mockWebToolApplies(tool, path)) continue;
    problems.push(...mockWebProblems(tool, path, text));
    outcomes.push({ tool, label: WEB_TOOL_LABELS[tool], status: "ok", message: "" });
  }
  return { problems, outcomes };
}

registerMock("web_quality_tools_check", async ({ path, text }): Promise<WebCheckResult> => {
  refuseInSafeMode();
  const file = String(path);
  const body = typeof text === "string" ? text : await invoke<string>("fs_read_file", { path: file }).catch(() => "");
  return { files: [file], tools: LINTERS, ...checkText(file, body) };
});

registerMock("web_quality_tools_inspect", async ({ paths }): Promise<WebCheckResult> => {
  refuseInSafeMode();
  const scope = (paths as string[] | null) ?? [];
  const inScope = projectFiles().filter(([f]) => !scope.length || scope.some((s) => f === s || f.startsWith(`${s.replace(/\/$/, "")}/`)));
  const problems: WebProblem[] = [];
  const outcomes = new Map<WebTool, WebToolOutcome>();
  for (const [f, text] of inScope) {
    const r = checkText(f, text);
    problems.push(...r.problems);
    for (const o of r.outcomes) outcomes.set(o.tool, o);
  }
  return { files: scope, tools: [...outcomes.keys()], problems, outcomes: [...outcomes.values()] };
});

registerMock("web_quality_tools_fix", ({ path, tool, text }): WebFixResult => {
  refuseInSafeMode();
  const t = tool as WebTool;
  const file = String(path);
  const before = String(text);
  if (!mockWebToolApplies(t, file)) {
    throw new Error(installed(t) ? `${WEB_TOOL_LABELS[t]} does not run for ${file}` : `No ${WEB_TOOL_LABELS[t]} configuration file was found for ${file}`);
  }
  const after = mockWebFixed(t, file, before);
  return { tool: t, text: after, changed: after !== before };
});

registerMock("web_quality_tools_format", ({ path, text }): WebFixResult | null => {
  if (mockSafeMode()) return null;
  const file = String(path);
  const before = String(text);
  const tool = (["biome", "prettier"] as WebTool[]).find((t) => configOf(t).onReformat && mockWebToolApplies(t, file));
  if (!tool) return null;
  const after = mockFormat(before);
  return { tool, text: after, changed: after !== before };
});
