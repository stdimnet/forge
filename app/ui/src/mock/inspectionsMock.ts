/**
 * Browser-mode inspections: the registry of the enabled demo plugins, profiles in memory,
 * and Inspect Code / Code Cleanup over the demo files with the same Rust code as the desktop
 * app (`forge-inspections` through WebAssembly: profile, scopes, suppressions, duplicated
 * code, the report). The PHP problems come from the `php_inspect` mock. Fixes change the
 * in-memory demo files only.
 */
import { emitMock, invoke, registerMock } from "@/ipc";
import type { CleanupPlan, InspectRequest, Profile, ProfilesState, Registry, Report } from "@/ipc/inspections";
import type { PhpProblem } from "@/ipc/phpMeta";
import type { ScopeFilter } from "@/ipc/scopes";
import { callWasm, forgeWasm } from "@/wasm";
import { projectFiles } from "./findMock";
import { mockInspectionContributions, mockSettingValue } from "./settingsMock";
import { mockFiles } from "./workspaceMock";

const encoder = new TextEncoder();
/** UTF-16 offset in `text` → UTF-8 byte offset (Rust's offsets). */
const bytes = (text: string, at: number) => encoder.encode(text.slice(0, at)).length;

async function registry(): Promise<Registry> {
  return callWasm<Registry>((await forgeWasm()).inspections_registry, mockInspectionContributions());
}

let state: ProfilesState = {
  profiles: [
    { name: "Default", level: "ide", tools: {} },
    { name: "Project Default", level: "project", tools: {} },
  ],
  selected: "Project Default",
  hasProject: true,
  severities: [],
};

registerMock("inspections_registry", async () => {
  const w = await forgeWasm();
  return { registry: await registry(), severities: await callWasm(w.inspections_severities, { custom: [] }) };
});

registerMock("inspections_profiles", () => structuredClone(state));

registerMock("inspections_save", ({ profiles }) => {
  state = structuredClone(profiles as ProfilesState);
  emitMock("inspections://changed", structuredClone(state));
  return structuredClone(state);
});

registerMock("inspections_export_profile", async ({ profile, format }) =>
  callWasm<string>((await forgeWasm()).inspections_export_profile, { registry: await registry(), profile, format }),
);

registerMock("inspections_import_profile", async ({ text }) => {
  if (typeof text !== "string") throw new Error("Browser mode imports from a picked file only");
  return callWasm<Profile>((await forgeWasm()).inspections_import_profile, { registry: await registry(), text });
});

const SOURCE = /\.(php|phtml|inc|js|jsx|ts|tsx|mjs|vue|py|rs|go|java|kt|lua|rb|css|scss|twig|sql)$/i;

async function scopeFiles(scope: InspectRequest["scope"]): Promise<[string, string][]> {
  const all = projectFiles().filter(([p]) => SOURCE.test(p) && !p.startsWith("vendor/") && !p.startsWith("node_modules/"));
  switch (scope.kind) {
    case "file":
      return all.filter(([p]) => p === scope.path);
    case "files":
      return all.filter(([p]) => scope.paths?.includes(p));
    case "directory": {
      const d = (scope.path ?? "").replace(/^\/+|\/+$/g, "");
      return all.filter(([p]) => !d || p.startsWith(`${d}/`));
    }
    case "scope": {
      const inside = await callWasm<boolean[]>((await forgeWasm()).scopes_contains, { filter: scope.filter as ScopeFilter, paths: all.map(([p]) => p) });
      return all.filter((_, i) => inside[i]);
    }
    default:
      return scope.includeTests === false ? all.filter(([p]) => !/(^|\/)tests?\//.test(p)) : all;
  }
}

/** The PHP index and data-flow problems of the demo files, as the backend reports them. */
async function phpProblems(files: [string, string][]) {
  const out = [];
  for (const [path, text] of files) {
    if (!/\.(php|phtml|inc)$/.test(path) || path.endsWith(".blade.php")) continue;
    const found = await invoke<PhpProblem[]>("php_inspect", { path, content: text }).catch(() => [] as PhpProblem[]);
    for (const p of found) {
      out.push({
        inspection: p.code,
        path,
        start: bytes(text, p.start),
        end: bytes(text, p.end),
        message: p.message,
        unnecessary: p.unnecessary,
        deprecated: p.deprecated,
        fixes: p.fix
          ? [{ title: p.fix.title, safe: true, edits: p.fix.edits.map((e) => ({ path, start: bytes(text, e.start), end: bytes(text, e.end), text: e.newText })) }]
          : [],
      });
    }
  }
  return out;
}

async function run(request: InspectRequest, cleanup: boolean): Promise<{ report: Report; texts: Record<string, string> }> {
  const files = await scopeFiles(request.scope);
  emitMock("inspections://progress", { runId: request.runId, done: 0, total: files.length + 1, text: "Inspecting" });
  const w = await forgeWasm();
  const report = await callWasm<Report>(w.inspections_run, {
    registry: await registry(),
    profiles: state,
    profile: request.profile,
    files: files.map(([path, text]) => ({ path, text })),
    problems: await phpProblems(files),
    scopes: { shared: mockSettingValue("scopes.shared") ?? [], local: mockSettingValue("scopes.local") ?? [] },
    context: { testRoots: ["tests"] },
    only: request.only,
    scope: request.scope.label,
    cleanup,
  });
  emitMock("inspections://progress", { runId: request.runId, done: files.length + 1, total: files.length + 1, text: "" });
  return { report, texts: Object.fromEntries(files) };
}

registerMock("inspections_run", async ({ request }) => (await run(request as InspectRequest, false)).report);

registerMock("inspections_cancel", () => undefined);

registerMock("inspections_cleanup_plan", async ({ request }) => {
  const { report, texts } = await run(request as InspectRequest, true);
  return callWasm<CleanupPlan>((await forgeWasm()).inspections_cleanup_plan, { registry: await registry(), problems: report.problems, texts });
});

registerMock("inspections_cleanup_apply", ({ files }) => {
  const written: string[] = [];
  for (const f of files as { path: string; after: string }[]) {
    mockFiles[f.path] = f.after;
    written.push(f.path);
  }
  return { written, skipped: [] };
});

registerMock("inspections_apply_fix", async ({ fix }) => {
  const w = await forgeWasm();
  const edits = (fix as { edits: { path: string; start: number; end: number; text: string }[] }).edits;
  const written: string[] = [];
  for (const path of [...new Set(edits.map((e) => e.path))]) {
    const text = mockFiles[path];
    if (text === undefined) continue;
    mockFiles[path] = await callWasm<string>(w.inspections_apply_edits, { text, edits: edits.filter((e) => e.path === path) });
    written.push(path);
  }
  return { written, skipped: [] };
});

registerMock("inspections_export_report", async ({ report, format }) =>
  callWasm<string>((await forgeWasm()).inspections_export_report, { report, format }),
);
