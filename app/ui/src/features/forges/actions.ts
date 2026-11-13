/** Code host actions shared by the tool window, detail tabs and commands. */
import { createSignal } from "solid-js";
import { GitPullRequest, CircleDot } from "lucide-solid";
import { forgesApi, type ChangedFile, type HostKind, type MergeRequestDetails } from "@/ipc/forges";
import { openCustomTab } from "@/state/editor";
import { notify, reloadOpenFiles, repositoryChanged } from "@/features/git/store";
import { setDiffRequest } from "@/features/git/overlays";
import { caps, errorText, ref, refreshAll, requestName } from "./store";
import { MergeRequestTab } from "./MergeRequestTab";
import { IssueTab } from "./IssueTab";
import { t as tr } from "@/i18n";

export const settingsPageFor = (kind: HostKind) => `vcs.${kind}`;

export const mrTabId = (n: number) => `forge:mr/${n}`;

export function openMergeRequest(number: number, title?: string): void {
  openCustomTab({
    id: mrTabId(number),
    title: `${ref(number)} ${title ?? requestName()}`.slice(0, 48),
    tooltip: title ? `${requestName()} ${ref(number)}: ${title}` : undefined,
    icon: GitPullRequest,
    component: () => MergeRequestTab({ number }),
  });
}

export function openIssue(number: number, title?: string): void {
  openCustomTab({
    id: `forge:issue/${number}`,
    title: `#${number} ${title ?? "Issue"}`.slice(0, 48),
    tooltip: title ? `Issue #${number}: ${title}` : undefined,
    icon: CircleDot,
    component: () => IssueTab({ number }),
  });
}

// ---- dialogs ------------------------------------------------------------------------------

export type Dialog =
  | { kind: "create" }
  | { kind: "merge"; mr: MergeRequestDetails; onDone: () => void }
  | { kind: "confirm"; title: string; text: string; ok: string; danger?: boolean; run: () => Promise<void> };

const [dialog, setDialog] = createSignal<Dialog | null>(null);
export { dialog, setDialog };

export function openCreateDialog(): void {
  setDialog({ kind: "create" });
}

/** Run a host operation, reporting failures as an error balloon. Returns success. */
export async function run(title: string, op: () => Promise<unknown>, success?: string): Promise<boolean> {
  try {
    await op();
    if (success) notify("success", success);
    return true;
  } catch (e) {
    notify("error", title, errorText(e), 10000);
    return false;
  }
}

export async function approve(mr: MergeRequestDetails, onDone: () => void): Promise<void> {
  if (await run("Approve failed", () => forgesApi.approve(mr.number), `Approved ${ref(mr.number)}`)) onDone();
}

export function confirmClose(mr: MergeRequestDetails, onDone: () => void): void {
  setDialog({
    kind: "confirm",
    title: `Close ${requestName()}`,
    text: `Close ${requestName().toLowerCase()} ${ref(mr.number)} “${mr.title}” without merging?`,
    ok: "Close",
    danger: true,
    run: async () => {
      if (await run("Close failed", () => forgesApi.close(mr.number), `Closed ${ref(mr.number)}`)) {
        onDone();
        void refreshAll();
      }
    },
  });
}

export async function checkoutRequest(mr: { number: number; sourceBranch: string }): Promise<void> {
  try {
    const branch = await forgesApi.checkout(mr.number, mr.sourceBranch);
    await reloadOpenFiles();
    notify("info", tr("tools.forges.checkedOut", { branch }), `${requestName()} ${ref(mr.number)}`);
  } catch (e) {
    notify("error", tr("tools.forges.checkoutFailed"), errorText(e), 10000);
  } finally {
    await repositoryChanged();
  }
}

/** Side-by-side diff of one file of a merge request (base vs head) in the Git diff viewer. */
export async function showRequestDiff(mr: MergeRequestDetails, file: ChangedFile): Promise<void> {
  const { path, oldPath, status } = file;
  const base = mr.baseSha ?? mr.targetBranch;
  const head = mr.headSha ?? mr.sourceBranch;
  try {
    const [left, right] = await Promise.all([
      status === "added" ? Promise.resolve("") : forgesApi.file(oldPath ?? path, base).then((t) => t ?? ""),
      status === "deleted" ? Promise.resolve("") : forgesApi.file(path, head).then((t) => t ?? ""),
    ]);
    setDiffRequest({
      path,
      left: { title: status === "added" ? "Empty (new file)" : `${mr.targetBranch} (${base.slice(0, 8)})`, content: left },
      right: { title: status === "deleted" ? "Deleted" : `${mr.sourceBranch} (${head.slice(0, 8)})`, content: right },
      editable: false,
    });
  } catch (e) {
    notify("error", tr("tools.forges.cannotLoadTheDiff"), errorText(e), 10000);
  }
}

export const canResolve = () => caps().resolveThreads;
