/**
 * File | Repair IDE: PHPStorm's step-by-step repair. Each step runs, then a balloon reports
 * it and offers the next one; "Everything Works Now" ends the repair and "More…" picks any of
 * the remaining steps. The last step is Invalidate Caches.
 */
import { balloonsFor } from "@/keymap";
import { askChoice } from "@/features/contextMenu/dialogs";
import { runCommand } from "@/state/commands";
import { isPluginActive } from "@/state/project";
import { isRemoteProject, closeWorkspace, openWorkspace, restoreWorkspace, workspace } from "@/state/workspace";
import { maintenanceApi } from "@/ipc/maintenance";
import { synchronize } from "@/features/system";
import { refreshAllDirs } from "@/features/project/ProjectView";
import { t } from "@/i18n";

export const REPAIR_GROUP = "Repair IDE";
const showBalloon = balloonsFor(REPAIR_GROUP);

export interface RepairStep {
  id: string;
  title: string;
  /** Balloon text after the step ran. */
  done: string;
  run: () => Promise<void>;
}

/** Repair IDE's "Refresh project files": re-read open files, the Project view and Git status. */
export async function refreshProjectFiles(): Promise<void> {
  synchronize();
  refreshAllDirs();
}

async function rescanIndexes(): Promise<void> {
  const jobs: Promise<unknown>[] = [maintenanceApi.rescanIndexes()];
  // Language packs rescan their own indexes when they are active.
  if (isPluginActive("forge.php")) jobs.push(runCommand("php.reindex"));
  if (isPluginActive("forge.lua")) jobs.push(runCommand("lua.reindex"));
  await Promise.all(jobs);
}

async function reopenProject(): Promise<void> {
  const ws = workspace();
  if (!ws) return;
  if (isRemoteProject()) {
    await restoreWorkspace();
    return;
  }
  closeWorkspace();
  await openWorkspace(ws.root, { askTrust: false });
}

export const REPAIR_STEPS: RepairStep[] = [
  { id: "refresh", title: "Refresh project files", done: "Project files were refreshed.", run: refreshProjectFiles },
  { id: "rescan", title: "Rescan project indexes", done: "Project indexes were rescanned.", run: rescanIndexes },
  { id: "reopen", title: "Reopen project", done: "The project was reopened.", run: reopenProject },
  {
    id: "invalidate",
    title: "Invalidate caches and restart",
    done: "",
    run: () => runCommand("InvalidateCaches"),
  },
];

/** Run step `index`, then offer the next one in a balloon. */
export async function runRepairStep(index: number): Promise<void> {
  const step = REPAIR_STEPS[index];
  if (!step) return;
  try {
    await step.run();
  } catch (e) {
    showBalloon(`${step.title} failed`, String(e instanceof Error ? e.message : e), 6000, undefined, { type: "error" });
    return;
  }
  const next = REPAIR_STEPS[index + 1];
  if (!next) return;
  showBalloon(
    t("tools.maintenance.repairIdeStepOf", { value: index + 1, count: REPAIR_STEPS.length }),
    t("tools.maintenance.ifTheProblemPersistsTry", { done: step.done }),
    0,
    [
      { label: next.title, run: () => void runRepairStep(index + 1) },
      { label: "Everything Works Now", run: () => undefined },
      { label: "More…", run: () => void chooseStep(index + 1) },
    ],
  );
}

/** "More…": pick any of the remaining steps. */
async function chooseStep(from: number): Promise<void> {
  const rest = REPAIR_STEPS.slice(from);
  const id = await askChoice({
    title: "Repair IDE",
    message: "Which step should Forge run next?",
    choices: rest.map((s) => ({ id: s.id, label: s.title })).reverse(),
  });
  const index = REPAIR_STEPS.findIndex((s) => s.id === id);
  if (index >= 0) await runRepairStep(index);
}

/** File | Repair IDE: start with the first step. */
export const startRepair = () => runRepairStep(0);
