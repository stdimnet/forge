/**
 * Forge-managed installs of npm language servers in the UI: progress for the status bar
 * and the settings page, and the daily update check.
 */
import { createSignal } from "solid-js";
import { langserversApi, type InstallOutcome, type ServerRow } from "@/ipc/langservers";
import { balloonsFor } from "@/keymap/dispatch";
import { onProjectDetected } from "@/state/project";
import { failureDetail, managedOption, updateSummary } from "./autoInstall";
import { applyServers } from "./store";
import { t } from "@/i18n";

const showBalloon = balloonsFor("Language Servers");

const errorText = (e: unknown) => String((e as Error)?.message ?? e);

/** A running managed install (server id, or package name for updates). */
export interface ActiveInstall {
  id: string;
  name: string;
  /** Current step or download ("Downloading Node.js v22.11.0"). */
  label: string;
  done: number;
  total: number | null;
}

const [active, setActive] = createSignal<ActiveInstall[]>([]);
export { active as activeInstalls };

export const installFor = (id: string) => active().find((a) => a.id === id);

function patch(id: string, p: Partial<ActiveInstall>) {
  setActive((list) => list.map((a) => (a.id === id ? { ...a, ...p } : a)));
}

/** The installer's error output per install, for the failure balloon. */
const stderrOf = new Map<string, string[]>();

let listening = false;
function listen() {
  if (listening) return;
  listening = true;
  void langserversApi.onInstallProgress((p) => patch(p.id, { label: p.label, done: p.done, total: p.total ?? null }));
  void langserversApi.onInstallOutput((l) => {
    if (l.stream === "stderr") stderrOf.set(l.id, [...(stderrOf.get(l.id) ?? []), l.line].slice(-20));
    // Forge's own steps name what happens next; npm's output is too noisy for the status bar.
    if (l.stream === "info" && !l.line.startsWith("$ ")) patch(l.id, { label: l.line, done: 0, total: null });
  });
}

// ---------- installs ----------

/**
 * Install a server with Forge's managed installer (Node.js and the package downloaded and
 * verified by the backend), with progress in the status bar, then register and start it.
 */
export async function installManaged(row: ServerRow): Promise<InstallOutcome | null> {
  const option = managedOption(row);
  if (!option || installFor(row.id)) return null;
  listen();
  stderrOf.delete(row.id);
  setActive((list) => [...list, { id: row.id, name: row.name, label: t("tools.langservers.installing", { name: row.name }), done: 0, total: null }]);
  let outcome: InstallOutcome | null = null;
  try {
    outcome = await langserversApi.install(row.id, option.index);
  } catch (e) {
    showBalloon(t("tools.langservers.couldNotBeInstalled", { name: row.name }), errorText(e), 0, [{ label: t("common.retry"), run: () => void installManaged(row) }]);
  } finally {
    setActive((list) => list.filter((a) => a.id !== row.id));
  }
  if (outcome === "succeeded") {
    await applyServers();
    showBalloon(t("tools.langservers.installed", { name: row.name }), t("tools.langservers.itStartsWithTheNext"), 6000);
  } else if (outcome && outcome !== "cancelled") {
    // Stays until dismissed: an install takes a while and the user may have looked away.
    const detail = failureDetail(outcome.failed.code, stderrOf.get(row.id) ?? []);
    showBalloon(t("tools.langservers.couldNotBeInstalled", { name: row.name }), detail, 0, [{ label: t("common.retry"), run: () => void installManaged(row) }]);
  }
  return outcome;
}

export const cancelManaged = (id: string) => void langserversApi.cancelInstall(id);

// ---------- updates ----------

let updatesChecked = false;

/** Updates for servers Forge installed (the backend asks the registry once a day). */
export async function checkUpdates(force = false): Promise<void> {
  const updates = await langserversApi.checkUpdates(force).catch(() => null);
  if (!updates?.length) {
    if (force) showBalloon(t("tools.langservers.languageServersAreUpTo"), undefined, 4000);
    return;
  }
  showBalloon(updates.length === 1 ? t("tools.langservers.languageServerUpdateAvailable") : t("tools.langservers.languageServerUpdatesAvailable"), updateSummary(updates), 0, [
    {
      label: t("common.update"),
      run: () => {
        for (const u of updates) void updatePackage(u.name);
      },
    },
    { label: t("tools.langservers.notNow"), run: () => {} },
  ]);
}

async function updatePackage(name: string): Promise<void> {
  listen();
  setActive((list) => [...list, { id: name, name, label: t("tools.langservers.updating", { name }), done: 0, total: null }]);
  try {
    const outcome = await langserversApi.update(name);
    if (outcome === "succeeded") {
      await applyServers();
      showBalloon(`${name} updated`, t("tools.langservers.itsLanguageServerRestartsWith"), 5000);
    }
  } catch (e) {
    showBalloon(t("tools.langservers.couldNotBeUpdated", { name }), errorText(e), 10000);
  } finally {
    setActive((list) => list.filter((a) => a.id !== name));
  }
}

/** Check for updates once, after the first project detection. */
export function startUpdateChecks(): void {
  onProjectDetected(() => {
    if (!updatesChecked) {
      updatesChecked = true;
      void checkUpdates();
    }
  });
}
