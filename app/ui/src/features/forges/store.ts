/**
 * Shared state of the code host feature: the repository context (host, account,
 * capabilities), the merge request list filter, CI status of the current branch, and a
 * refresh counter that views watch.
 */
import { createRoot, createSignal, createEffect, on } from "solid-js";
import { forgesApi, type Capabilities, type Pipeline, type RepoContext } from "@/ipc/forges";
import { currentBranch } from "@/state/vcs";
import { getSetting } from "@/state/settings";
import { workspace } from "@/state/workspace";

const [context, setContext] = createSignal<RepoContext | null>(null);
const [contextLoaded, setContextLoaded] = createSignal(false);
const [contextError, setContextError] = createSignal<string | null>(null);
/** Bumped to make lists and open tabs reload. */
const [revision, setRevision] = createSignal(0);
const [branchPipeline, setBranchPipeline] = createSignal<Pipeline | null>(null);

export { context, contextLoaded, contextError, revision, branchPipeline };

const fallbackCaps: Capabilities = {
  resolveThreads: true,
  approvals: true,
  draft: true,
  mergeMethods: ["merge", "squash"],
  requestName: "Pull Request",
  numberPrefix: "#",
};

export const caps = (): Capabilities => context()?.capabilities ?? fallbackCaps;
/** "Merge Request" on GitLab, otherwise "Pull Request". */
export const requestName = () => caps().requestName;
export const ref = (n: number) => `${caps().numberPrefix}${n}`;
/** The logged-in user on the project's host. */
export const me = () => context()?.account?.username ?? null;

export const errorText = (e: unknown) => String((e as Error)?.message ?? e);

export async function loadContext(): Promise<void> {
  try {
    setContext(await forgesApi.repo());
    setContextError(null);
  } catch (e) {
    setContext(null);
    setContextError(errorText(e));
  } finally {
    setContextLoaded(true);
  }
}

export async function refreshAll(): Promise<void> {
  await loadContext();
  setRevision((r) => r + 1);
  void refreshBranchPipeline();
}

export async function refreshBranchPipeline(): Promise<void> {
  if (!context() || !getSetting<boolean>("vcs.hosts.statusBarPipeline", true)) {
    setBranchPipeline(null);
    return;
  }
  try {
    setBranchPipeline(await forgesApi.ciStatus(currentBranch()));
  } catch {
    setBranchPipeline(null);
  }
}

let started = false;

export function startForgesStore(): void {
  if (started) return;
  started = true;
  createRoot(() => {
    createEffect(
      on(
        () => workspace()?.root,
        () => void refreshAll(),
      ),
    );
    createEffect(on(currentBranch, () => void refreshBranchPipeline(), { defer: true }));
    createEffect(
      on(
        () => getSetting<boolean>("vcs.hosts.statusBarPipeline", true),
        () => void refreshBranchPipeline(),
        { defer: true },
      ),
    );
  });
  if (typeof window !== "undefined") {
    let last = Date.now();
    setInterval(() => {
      const every = getSetting<number>("vcs.hosts.refreshSeconds", 120);
      if (!every || document.visibilityState !== "visible" || Date.now() - last < every * 1000) return;
      last = Date.now();
      setRevision((r) => r + 1);
      void refreshBranchPipeline();
    }, 5000);
  }
}
