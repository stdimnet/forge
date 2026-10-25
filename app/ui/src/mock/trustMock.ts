/**
 * Browser-mode project trust. The demo projects live in `~/Projects`, a trusted location, so
 * they open without the "Trust and Open Project?" dialog. Any other folder is untrusted:
 * `openWorkspace("~/Downloads/legacy-crm")` asks, and "Preview in Safe Mode" opens the demo
 * files in Safe Mode (the run mocks then refuse like the Rust commands do). Nothing is stored.
 */
import { emitMock, mockHandler, registerMock } from "@/ipc";
import type { TrustCheck, TrustedEntry, TrustStatus } from "@/ipc/trust";
import { TRUST_CHANGED_EVENT } from "@/ipc/trust";
import { isRemoteDemo, remoteWorkspace } from "./remoteMock";
import { mockProjectName } from "./workspaceMock";

const demoRoot = `~/Projects/${mockProjectName}`;

const locations = new Set<string>(["~/Projects"]);
const projects = new Set<string>(["~/.config/nvim"]);
/** The folder the UI last opened (`null`: the demo project itself). */
let opened: string | null = null;

const clean = (p: string) => p.trim().replace(/\\/g, "/").replace(/\/+$/, "") || "/";
const inside = (location: string, project: string) => project === location || project.startsWith(`${location}/`);
const parentOf = (p: string) => {
  const i = p.lastIndexOf("/");
  return i > 1 ? p.slice(0, i) : undefined;
};

function check(path: string): TrustCheck {
  const project = clean(path);
  const location = [...locations].find((l) => inside(l, project));
  const reason = projects.has(project) ? { kind: "project" as const } : location ? { kind: "location" as const, location } : undefined;
  const parent = parentOf(project);
  return { project, display: project, trusted: !!reason, reason, parent, parentDisplay: parent };
}

/** The open project's folder as the trust store sees it. */
function openRoot(): string {
  if (isRemoteDemo()) return remoteWorkspace().root;
  return opened ?? demoRoot;
}

function status(): TrustStatus {
  const c = check(openRoot());
  return { ...c, safeMode: !c.trusted, remote: isRemoteDemo() };
}

/** The open demo project is in Safe Mode (read by the run mocks). */
export const mockSafeMode = (): boolean => !isRemoteDemo() && !check(openRoot()).trusted;

const changed = () => emitMock(TRUST_CHANGED_EVENT, status());

function entries(): TrustedEntry[] {
  return [
    ...[...locations].map((key) => ({ key, display: key, kind: "location" as const })),
    ...[...projects].map((key) => ({ key, display: key, kind: "project" as const })),
  ].sort((a, b) => a.display.localeCompare(b.display));
}

// "demo" is what the browser build opens by default; it is the trusted demo project.
registerMock("trust_check", ({ path }) => (String(path) === "demo" ? check(demoRoot) : check(String(path))));
registerMock("trust_status", () => status());
registerMock("trust_project", ({ path, location }) => {
  const target = path ? clean(String(path)) : openRoot();
  if (location) locations.add(clean(String(location)));
  else projects.add(target);
  changed();
  return check(target);
});
registerMock("trust_locations", () => entries());
registerMock("trust_location_add", ({ path }) => {
  const p = clean(String(path));
  if (!p.startsWith("/") && !p.startsWith("~/") && !/^[A-Za-z]:\//.test(p)) throw new Error(`not an absolute path: ${path}`);
  if (p === "/" || p === "~") throw new Error(`${p} is a file system root; choose a folder`);
  locations.add(p);
  changed();
  return entries();
});
registerMock("trust_location_remove", ({ key }) => {
  const k = String(key);
  if (!locations.delete(k) && !projects.delete(k)) throw new Error(`${k} is not trusted`);
  changed();
  return entries();
});

// Remember which folder was opened (the files are the demo project's either way).
const open = mockHandler("workspace_open");
registerMock("workspace_open", async (args) => {
  const path = String(args.path ?? "");
  opened = path === "demo" || !path ? null : clean(path);
  const info = await open?.(args);
  changed();
  return info;
});

// Like `start_built` in Rust: nothing of an untrusted project runs.
const refusal = (what: string) =>
  `${what} is not available in Safe Mode: the project is not trusted. Trust the project (Trust Project… in the Safe Mode banner) to run its code.`;
for (const [cmd, what] of [
  ["run_start", "Running programs of the project"],
  ["debug_listen_start", "The debugger"],
  ["agent_start", "Starting AI agents"],
  ["symfony_console_run", "Running Symfony console commands"],
  ["cli_tools_run", "Running command line tools"],
  ["save_actions_run_tools", "Running external tools on save"],
] as const) {
  const inner = mockHandler(cmd);
  if (!inner) continue;
  registerMock(cmd, (args) => {
    if (mockSafeMode()) throw new Error(refusal(what));
    return inner(args);
  });
}
