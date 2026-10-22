/**
 * The rebase, merge, cherry-pick or revert in progress, like PHPStorm: the branch widgets show
 * "Rebasing main", and the Git menu offers Continue Rebase, Skip Commit and Abort Rebase (Abort
 * Merge while merging). Continuing may stop at the next conflict, which opens the resolver.
 */
import { createEffect, createRoot, createSignal, on } from "solid-js";
import { setBranchOperation } from "@/state/vcs";
import { opsApi, type OngoingOperation, type OperationAction, type OperationState } from "./opsApi";
import { notify, reloadOpenFiles, repoAvailable, revision } from "./store";
import { runMerging } from "./conflicts";

const [operation, setOperation] = createSignal<OperationState>({ operation: null, branch: null, onto: null });
export { operation };

const VERB: Record<OngoingOperation, string> = {
  rebase: "Rebasing",
  merge: "Merging",
  cherryPick: "Cherry-picking in",
  revert: "Reverting in",
};

const NOUN: Record<OngoingOperation, string> = {
  rebase: "Rebase",
  merge: "Merge",
  cherryPick: "Cherry-Pick",
  revert: "Revert",
};

/** "Rebasing main", or null without an operation in progress. */
export function operationLabel(s: OperationState): string | null {
  if (!s.operation) return null;
  return `${VERB[s.operation]} ${s.branch ?? "HEAD"}`;
}

export const rebasing = () => operation().operation === "rebase";
export const merging = () => operation().operation === "merge";

export async function refreshOperation(): Promise<void> {
  const s = repoAvailable()
    ? await opsApi.operationState().catch((): OperationState => ({ operation: null, branch: null, onto: null }))
    : { operation: null, branch: null, onto: null };
  setOperation(s);
  setBranchOperation(operationLabel(s));
}

const DONE: Record<OperationAction, string> = { continue: "continued", abort: "aborted", skip: "skipped a commit" };

/** Continue / Abort / Skip the operation in progress. */
export async function operationAction(action: OperationAction): Promise<void> {
  const op = operation().operation;
  if (!op) return notify("info", "No operation is in progress");
  const out = await runMerging(`${NOUN[op]} ${action} failed`, () => opsApi.operationAction(action));
  await refreshOperation();
  if (out === undefined) return;
  await reloadOpenFiles();
  notify("success", `${NOUN[op]} ${DONE[action]}`, out.split("\n").filter(Boolean).pop());
}

let started = false;

/** Follow repository changes (called once by the feature). */
export function watchOperation(): void {
  if (started) return;
  started = true;
  createRoot(() => createEffect(on(revision, () => void refreshOperation())));
}
