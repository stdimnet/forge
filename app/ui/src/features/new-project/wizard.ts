/**
 * Pure state logic of the New Project wizard: grouping generators, option defaults,
 * name/location handling, the command preview and the creation progress reducer.
 */
import type { CreateEvent, Generator, GeneratorOption, OptionValue, Plan } from "@/ipc/project";

export const GROUP_ORDER = ["PHP", "Web", "Java", "Lua", "Ruby", ".NET", "Other"];

export interface GeneratorGroup {
  group: string;
  items: Generator[];
}

/** Generators grouped like the left list of the dialog (PHP, Web, Lua, Other). */
export function groupGenerators(gens: Generator[]): GeneratorGroup[] {
  const rank = (g: string) => {
    const i = GROUP_ORDER.indexOf(g);
    return i < 0 ? GROUP_ORDER.length : i;
  };
  const groups = [...new Set(gens.map((g) => g.group))].sort((a, b) => rank(a) - rank(b));
  return groups.map((group) => ({ group, items: gens.filter((g) => g.group === group) }));
}

export function defaultOptions(gen: Generator): Record<string, OptionValue> {
  return Object.fromEntries(gen.options.map((o) => [o.id, o.default]));
}

/**
 * Options after switching generators: shared options (git, PHP version, ...) keep what the
 * user chose, everything else starts from the new generator's defaults.
 */
export function carryOptions(next: Generator, previous: Record<string, OptionValue>): Record<string, OptionValue> {
  const out = defaultOptions(next);
  for (const o of next.options) {
    const v = previous[o.id];
    if (v === undefined || typeof v !== typeof o.default) continue;
    if (o.kind === "enum" && !o.choices?.some((c) => c.value === v)) continue;
    out[o.id] = v;
  }
  return out;
}

/** The option is shown: every `visibleWhen` condition holds for the current values. */
export function isOptionVisible(option: GeneratorOption, values: Record<string, OptionValue>, gen: Generator): boolean {
  return (option.visibleWhen ?? []).every((c) => {
    const v = values[c.option] ?? gen.options.find((o) => o.id === c.option)?.default;
    return typeof v === "string" && c.values.includes(v);
  });
}

/** Replace the choices of one option (after "Refresh"), keeping everything else. */
export function withChoices(gens: Generator[], genId: string, optionId: string, choices: GeneratorOption["choices"]): Generator[] {
  return gens.map((g) =>
    g.id !== genId ? g : { ...g, options: g.options.map((o) => (o.id === optionId ? { ...o, choices } : o)) },
  );
}

/** Keep a name the user typed; replace an untouched default with the next generator's. */
export function nameAfterSwitch(current: string, previous: Generator | undefined, next: Generator): string {
  const untouched = !current.trim() || current === previous?.defaultName;
  return untouched ? next.defaultName : current;
}

/** Same rules as `forge_project::validate_name`, for instant feedback while typing. */
export function validateName(name: string): string | null {
  const n = name.trim();
  if (!n) return "Enter a project name";
  if (n === "." || n === "..") return "Invalid project name";
  // eslint-disable-next-line no-control-regex
  const bad = n.match(/[/\\:*?"<>|\u0000-\u001f]/);
  if (bad) return `The name must not contain '${bad[0]}'`;
  return null;
}

/** Join location and name with the separator the location already uses. */
export function joinPath(location: string, name: string): string {
  const loc = location.trim();
  const sep = loc.includes("\\") && !loc.includes("/") ? "\\" : "/";
  if (!loc) return name.trim();
  return loc.endsWith(sep) ? `${loc}${name.trim()}` : `${loc}${sep}${name.trim()}`;
}

/** Shorten the home directory to `~` for display. */
export function tildify(path: string, home: string | null): string {
  if (!home) return path;
  const h = home.replace(/[\\/]+$/, "");
  return path === h || path.startsWith(`${h}/`) || path.startsWith(`${h}\\`) ? `~${path.slice(h.length)}` : path;
}

export interface PreviewLine {
  kind: "cd" | "command" | "template" | "file";
  text: string;
}

/**
 * Shell-like preview of a plan: `cd` into the location, the commands, and for built-in
 * templates the files they write.
 */
export function previewLines(plan: Plan | null, location: string): PreviewLine[] {
  if (!plan) return [];
  const out: PreviewLine[] = [{ kind: "cd", text: `cd ${location.trim() || "."}` }];
  for (const line of plan.preview) {
    const template = line.endsWith("from the built-in template");
    out.push({ kind: template ? "template" : "command", text: line });
    if (template) for (const f of plan.files ?? []) out.push({ kind: "file", text: f });
  }
  return out;
}

const TOOL_NAMES: Record<string, string> = {
  composer: "Composer",
  npm: "npm (Node.js)",
  npx: "npx (Node.js)",
  git: "Git",
  symfony: "Symfony CLI",
};

export const toolName = (tool: string) => TOOL_NAMES[tool] ?? tool;

// ---- Creation progress -------------------------------------------------------------

export type Phase = "form" | "running" | "done" | "failed";

export interface Progress {
  phase: Phase;
  step: number;
  total: number;
  label: string;
  lines: { text: string; stderr: boolean }[];
  root?: string;
  error?: string;
}

export const MAX_LINES = 400;

export const initialProgress = (): Progress => ({
  phase: "running",
  step: 0,
  total: 0,
  label: "Preparing…",
  lines: [],
});

export function applyCreateEvent(p: Progress, e: CreateEvent): Progress {
  switch (e.type) {
    case "step":
      return {
        ...p,
        step: e.index,
        total: e.total,
        label: e.label,
        lines: [...p.lines, { text: `$ ${e.label}`, stderr: false }].slice(-MAX_LINES),
      };
    case "output":
      if (!e.line.trim()) return p;
      return { ...p, lines: [...p.lines, { text: e.line, stderr: e.stderr }].slice(-MAX_LINES) };
    case "done":
      return { ...p, phase: "done", step: p.total, root: e.root, label: "Project created" };
    case "failed":
      return { ...p, phase: "failed", error: e.error };
  }
}

/** 0..100; a step counts as half done while it runs. */
export function progressPercent(p: Progress): number {
  if (p.phase === "done") return 100;
  if (!p.total) return 4;
  return Math.min(98, Math.round(((p.step + 0.5) / p.total) * 100));
}
