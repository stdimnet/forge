/**
 * Forge-managed installs of npm language servers in the UI: progress for the status bar
 * and the settings page, the one-time Intelephense licence consent, the "Intelephense is
 * not installed" offer when a PHP project opens and the daily update check.
 */
import { createSignal } from "solid-js";
import { langserversApi, type InstallOutcome, type ServerRow } from "@/ipc/langservers";
import { showBalloon } from "@/keymap/dispatch";
import { getSetting, saveSetting } from "@/state/settings";
import { isPluginActive, onProjectDetected } from "@/state/project";
import { workspace } from "@/state/workspace";
import { INTELEPHENSE_ID, LICENCE_ACCEPTED_KEY, AUTO_INSTALL_KEY, managedOption, offerIntelephense, updateSummary } from "./autoInstall";
import { applyServers, refreshRows, rows } from "./store";

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

let listening = false;
function listen() {
  if (listening) return;
  listening = true;
  void langserversApi.onInstallProgress((p) => patch(p.id, { label: p.label, done: p.done, total: p.total ?? null }));
  void langserversApi.onInstallOutput((l) => {
    // Forge's own steps name what happens next; npm's output is too noisy for the status bar.
    if (l.stream === "info" && !l.line.startsWith("$ ")) patch(l.id, { label: l.line, done: 0, total: null });
  });
}

// ---------- licence consent ----------

const [consentRequest, setConsentRequest] = createSignal<((ok: boolean) => void) | null>(null);
export { consentRequest };

/** Answer of the licence dialog. */
export function answerConsent(ok: boolean): void {
  const resolve = consentRequest();
  setConsentRequest(null);
  resolve?.(ok);
}

export const licenceAccepted = () => getSetting<boolean>(LICENCE_ACCEPTED_KEY, false);

/** Ask once for the Intelephense licence; true when accepted (now or before). */
export async function ensureLicence(): Promise<boolean> {
  if (licenceAccepted()) return true;
  const ok = await new Promise<boolean>((resolve) => setConsentRequest(() => resolve));
  if (ok) await saveSetting(LICENCE_ACCEPTED_KEY, true).catch(() => undefined);
  return ok;
}

// ---------- installs ----------

/**
 * Install a server with Forge's managed installer (Node.js and the package downloaded and
 * verified by the backend), with progress in the status bar, then register and start it.
 */
export async function installManaged(row: ServerRow): Promise<InstallOutcome | null> {
  const option = managedOption(row);
  if (!option || installFor(row.id)) return null;
  if (row.id === INTELEPHENSE_ID && !(await ensureLicence())) return null;
  listen();
  setActive((list) => [...list, { id: row.id, name: row.name, label: `Installing ${row.name}…`, done: 0, total: null }]);
  let outcome: InstallOutcome | null = null;
  try {
    outcome = await langserversApi.install(row.id, option.index);
  } catch (e) {
    showBalloon(`${row.name} could not be installed`, errorText(e), 0, [{ label: "Retry", run: () => void installManaged(row) }]);
  } finally {
    setActive((list) => list.filter((a) => a.id !== row.id));
  }
  if (outcome === "succeeded") {
    await applyServers();
    showBalloon(
      `${row.name} installed`,
      row.id === INTELEPHENSE_ID ? "The PHP language server is starting for this project." : "It starts with the next matching file.",
      6000,
    );
  } else if (outcome && outcome !== "cancelled") {
    showBalloon(`${row.name} could not be installed`, "See Settings | Languages & Frameworks | Language Servers.", 10000);
  }
  return outcome;
}

export const cancelManaged = (id: string) => void langserversApi.cancelInstall(id);

// ---------- project open ----------

let offered = false;
let updatesChecked = false;

/** Offer Intelephense for a PHP project that has none (see `autoInstall.ts`). */
export async function checkIntelephense(): Promise<void> {
  const php = isPluginActive("forge.php");
  if (!php) return;
  await refreshRows();
  const action = offerIntelephense(
    {
      php,
      host: workspace()?.remote?.label,
      row: rows().find((r) => r.id === INTELEPHENSE_ID),
      autoInstall: getSetting<string>(AUTO_INSTALL_KEY, "ask"),
      server: getSetting<string>("php.lsp.server", "auto"),
      offered,
    },
    { showBalloon, install: installManaged, saveSetting: (k, v) => saveSetting(k, v) },
  );
  if (action !== "none") offered = true;
}

/** "Install Intelephense" from Find Action: install (or reinstall) it now. */
export async function installIntelephense(): Promise<void> {
  await refreshRows();
  const row = rows().find((r) => r.id === INTELEPHENSE_ID);
  if (row) await installManaged(row);
}

/** Updates for servers Forge installed (the backend asks the registry once a day). */
export async function checkUpdates(force = false): Promise<void> {
  const updates = await langserversApi.checkUpdates(force).catch(() => null);
  if (!updates?.length) {
    if (force) showBalloon("Language servers are up to date", undefined, 4000);
    return;
  }
  showBalloon(updates.length === 1 ? "Language server update available" : "Language server updates available", updateSummary(updates), 0, [
    {
      label: "Update",
      run: () => {
        for (const u of updates) void updatePackage(u.name);
      },
    },
    { label: "Not now", run: () => {} },
  ]);
}

async function updatePackage(name: string): Promise<void> {
  listen();
  setActive((list) => [...list, { id: name, name, label: `Updating ${name}…`, done: 0, total: null }]);
  try {
    const outcome = await langserversApi.update(name);
    if (outcome === "succeeded") {
      await applyServers();
      showBalloon(`${name} updated`, "Its language server restarts with the new version.", 5000);
    }
  } catch (e) {
    showBalloon(`${name} could not be updated`, errorText(e), 10000);
  } finally {
    setActive((list) => list.filter((a) => a.id !== name));
  }
}

/** Run the checks after each project detection. */
export function startAutoInstall(): void {
  onProjectDetected(() => {
    void checkIntelephense();
    if (!updatesChecked) {
      updatesChecked = true;
      void checkUpdates();
    }
  });
}
