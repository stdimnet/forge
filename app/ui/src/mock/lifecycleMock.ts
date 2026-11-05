/**
 * Browser-mode project lifecycle (`project://lifecycle`, `project_before_close`,
 * `workspace_close`): like Rust, every `workspace_open` sends `closing` for the project that
 * was open and `opened` for the new one (before the command returns), and `ready` a moment
 * later. Imported last, so it wraps the final `workspace_open`.
 *
 * A few folders of the recent projects list open as projects of their own so switching
 * projects can be tried in the browser (their files are the demo project's): `symfony-api`
 * and `shop-api` are on other branches, `~/.config/nvim` has no git repository.
 */
import { emitMock, mockHandler, registerMock } from "@/ipc";
import type { LifecycleEvent, LifecycleProject, Objection, ProjectPhase } from "@/ipc/projectLifecycle";
import type { WorkspaceInfo } from "@/ipc/workspace";
import { LIFECYCLE_EVENT } from "@/ipc/projectLifecycle";

interface MockProject {
  name: string;
  /** Branch of its repository; null: not a git repository. */
  branch: string | null;
  /** Changed files shown in the project tree. */
  changes?: Record<string, "modified" | "untracked">;
}

/** Folders that open as projects of their own (see the file comment). */
export const MOCK_SWITCH_PROJECTS: Record<string, MockProject> = {
  "~/Projects/symfony-api": { name: "symfony-api", branch: "feature/api-v2", changes: { "routes/api.php": "modified", "app/Models/Order.php": "untracked" } },
  "~/Projects/shop-api": { name: "shop-api", branch: "release/3.2" },
  "~/.config/nvim": { name: "dotfiles.nvim", branch: null },
};

let current: LifecycleProject | null = null;
let generation = 0;

/** The open project's mock entry, when it is one of {@link MOCK_SWITCH_PROJECTS}. */
export const mockSwitchProject = (): MockProject | undefined => (current ? MOCK_SWITCH_PROJECTS[current.root] : undefined);

/** The project the mock lifecycle has open. */
export const mockLifecycleProject = (): LifecycleProject | null => current;

const send = (phase: ProjectPhase, project: LifecycleProject) =>
  emitMock(LIFECYCLE_EVENT, { phase, generation, project } satisfies LifecycleEvent);

/** Objections of the mocked Rust subscribers (the fake terminals add theirs). */
const objectionSources: (() => Objection[])[] = [];

export function registerMockObjections(source: () => Objection[]): void {
  objectionSources.push(source);
}

/** Runs before the closing event, like the Rust subscribers (the fake terminals end their shells). */
const closingHooks: ((project: LifecycleProject) => void)[] = [];

export function onMockProjectClosing(hook: (project: LifecycleProject) => void): void {
  closingHooks.push(hook);
}

function close(): void {
  const closing = current;
  if (!closing) return;
  current = null;
  for (const hook of closingHooks) {
    try {
      hook(closing);
    } catch (e) {
      console.warn("mock closing hook failed", e);
    }
  }
  send("closing", closing);
}

function open(info: WorkspaceInfo): void {
  close();
  generation++;
  const project: LifecycleProject = { ...info, trusted: true, ...(MOCK_SWITCH_PROJECTS[info.root]?.branch !== null ? { vcsRoot: info.root } : {}) };
  current = project;
  send("opened", project);
  const opened = generation;
  setTimeout(() => {
    if (generation === opened && current) send("ready", current);
  }, 300);
}

const inner = mockHandler("workspace_open");
registerMock("workspace_open", async (args) => {
  const path = String(args.path ?? "");
  let info = (await inner?.(args)) as WorkspaceInfo;
  const other = MOCK_SWITCH_PROJECTS[path];
  if (other) info = { name: other.name, root: path };
  open(info);
  return info;
});

registerMock("workspace_close", () => close());
registerMock("project_before_close", (): Objection[] => (current ? objectionSources.flatMap((s) => s()) : []));

// Projects opened another way (remote, dev containers, Code With Me) go through these.
for (const cmd of ["remote_open_project", "devcontainer_open", "wsl_open_project"]) {
  const wrapped = mockHandler(cmd);
  if (!wrapped) continue;
  registerMock(cmd, async (args) => {
    const info = (await wrapped(args)) as WorkspaceInfo | { workspace?: WorkspaceInfo };
    const ws = "root" in info ? (info as WorkspaceInfo) : info.workspace;
    if (ws?.root) open(ws);
    return info;
  });
}

// `git status` of the switch projects: their own branch and changes, or no repository.
const gitStatus = mockHandler("git_status");
registerMock("git_status", async (args) => {
  const p = mockSwitchProject();
  if (!p) return gitStatus?.(args);
  if (p.branch === null) throw new Error(`not a git repository: ${current?.root}`);
  return {
    branch: p.branch,
    head: "4be1c0d2a9f37e18c5d6b0a4e2f9c7d1a3b5e6f8",
    upstream: `origin/${p.branch}`,
    ahead: 0,
    behind: 0,
    entries: Object.entries(p.changes ?? {}).map(([path, state]) => ({
      path,
      origPath: null,
      index: state === "untracked" ? "untracked" : "unmodified",
      worktree: state,
      conflict: false,
    })),
  };
});
