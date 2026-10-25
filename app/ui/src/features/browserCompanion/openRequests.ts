/**
 * Handles files opened from outside the IDE (browser extension, forge:// links, the
 * `forge` launcher): opens them right away when they belong to the open project, and asks
 * before switching projects or opening a folder outside every known project, unless the
 * user typed the command in a terminal.
 */
import { companionApi, type OpenEvent, type PendingEvent } from "@/ipc/companion";
import { openFileAt } from "@/editor/views";
import { openWorkspace, restoreWorkspace, workspace } from "@/state/workspace";
import { confirmDialog } from "./ConfirmDialog";

export type OpenPlan =
  | { kind: "open"; path: string }
  | { kind: "switch"; root: string; path: string; ask: false }
  | { kind: "switch"; root: string; path: string; ask: true; reason: "recent" | "outside"; projectName: string };

const baseName = (p: string) => p.replace(/[\\/]+$/, "").split(/[\\/]/).pop() ?? p;

/** Decide what to do with an open request given the project currently open. */
export function planOpen(event: OpenEvent, currentRoot: string | null | undefined): OpenPlan {
  const t = event.target;
  const root = t.kind === "outside" ? t.suggestedRoot : t.root;
  if (currentRoot && currentRoot === root) return { kind: "open", path: t.path };
  // The command line is the user's own request; links and browsers may come from anywhere.
  if (event.source === "cli") return { kind: "switch", root, path: t.path, ask: false };
  return {
    kind: "switch",
    root,
    path: t.path,
    ask: true,
    reason: t.kind === "outside" ? "outside" : "recent",
    projectName: t.kind === "recent" ? t.name : baseName(root),
  };
}

function requester(event: OpenEvent): string {
  if (event.source === "browser") return event.client ? `${event.client}` : "A paired browser";
  if (event.source === "link") return "A forge:// link";
  return "The forge command";
}

async function confirmSwitch(event: OpenEvent, plan: Extract<OpenPlan, { ask: true }>): Promise<boolean> {
  const file = baseName(event.file) + (event.line ? `:${event.line}` : "");
  if (plan.reason === "recent") {
    return confirmDialog({
      title: "Open Project",
      message: `${requester(event)} wants to open ${file} from project “${plan.projectName}”, which is not open.`,
      detail: plan.root,
      question: "Open the project in this window?",
      okLabel: "Open Project",
    });
  }
  return confirmDialog({
    title: "Open File Outside Projects",
    message: `${requester(event)} wants to open a file that is not part of an open or recent project:`,
    detail: event.file,
    question: `Open its folder “${plan.projectName}” as a project?`,
    okLabel: "Open Folder",
    warning: true,
  });
}

export async function handleOpenEvent(event: OpenEvent): Promise<void> {
  // At startup the backend may already have the project open before the UI adopted it.
  if (!workspace()) await restoreWorkspace();
  const plan = planOpen(event, workspace()?.root);
  if (plan.kind === "switch") {
    if (plan.ask && !(await confirmSwitch(event, plan))) return;
    // Don't Open in "Trust and Open Project?" keeps the current project.
    if (!(await openWorkspace(plan.root))) return;
  }
  await openFileAt(plan.path, event.line, event.column);
}

async function handlePending(e: PendingEvent): Promise<void> {
  if (e.type === "openProject") await openWorkspace(e.root);
  else await handleOpenEvent(e);
}

/** Requests are handled one at a time, so dialogs never stack. */
let queue: Promise<void> = Promise.resolve();
const enqueue = (job: () => Promise<void>) => {
  queue = queue.then(job).catch((e) => console.warn("cannot open request from outside Forge:", e));
};

/** Subscribe to open requests, then fetch the ones that arrived during startup. */
export async function listenForOpenRequests(): Promise<void> {
  await companionApi.onOpen((e) => enqueue(() => handleOpenEvent(e)));
  await companionApi.onOpenProject((root) => enqueue(async () => void (await openWorkspace(root))));
  for (const e of await companionApi.uiReady()) enqueue(() => handlePending(e));
}
