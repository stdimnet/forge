/**
 * Shared terminals and runs in a Code With Me session (PHPStorm: the host's terminal and Run
 * tool window for guests). What guests may see and do is decided by the host's Rust side
 * (`forge_collab::share`); this module shows it.
 *
 * - Host: a terminal tab's context menu has **Share with Guests** (Settings | Tools | Code
 *   With Me | Share new terminal tabs shares every new tab). A terminal a guest opens on this
 *   machine appears as a tab "<guest>: Local". A guest's request to run a configuration is
 *   started here as if the host clicked Run.
 * - Guest: the host's shared terminals appear as tabs of the Terminal tool window (read-only
 *   unless the guest has Full terminal access); the host's runs appear in the Run tool
 *   window. With Full access, Run, Stop and Rerun act on the host's configurations, and new
 *   terminal tabs open on the host.
 */
import { createEffect, createRoot, on } from "solid-js";
import { collabApi, sharedRunId, type Permissions, type SharedTerminal } from "@/ipc/collab";
import { showBalloon } from "@/keymap";
import { getSetting } from "@/state/settings";
import { showToolWindow } from "@/state/layout";
import { addSourceTab, registerTerminalTabActions, setActiveTab, setNewTabGuard, tabs, updateTab, type TerminalTab } from "@/features/terminal/store";
import { closeTerminalTab } from "@/features/terminal/TerminalView";
import { endSharedSession } from "@/features/terminal/sessions";
import { findConfiguration } from "@/features/run/configStore";
import { endSharedRuns, runConfiguration, setRemoteRuns, showSharedRun } from "@/features/run/runStore";
import { applySession, isGuest, isHost, session } from "./store";
import { t } from "@/i18n";

const message = (e: unknown) => String((e as Error)?.message ?? e);

/** The tab id of one of the host's terminals in a guest's window. */
export const sharedTerminalTab = (id: number) => `cwm-terminal-${id}`;

const myPermissions = (): Permissions | undefined => (isGuest() ? session().permissions : undefined);

/** Why this guest cannot open terminals on the host, or null. */
export function terminalBlockedReason(p: Permissions | undefined): string | null {
  if (!p) return null;
  if (p.terminal === "full") return null;
  return p.terminal === "readOnly"
    ? t("tools.codeWithMe.terminalWatchOnly")
    : t("tools.codeWithMe.terminalNotShared");
}

// ---- host --------------------------------------------------------------------------------

/** Share With Guests on a terminal tab, or stop sharing it (host). */
export async function setTerminalShared(tab: TerminalTab, shared: boolean): Promise<void> {
  try {
    applySession(await collabApi.shareTerminal(tab.id, tab.title, shared));
  } catch (e) {
    showBalloon(t("tools.codeWithMe.title"), message(e), 5000);
  }
}

/** A guest asks to run a configuration: start it as if the host clicked Run. */
async function answerRunRequest(request: number, name: string, guest: string): Promise<void> {
  const cfg = findConfiguration(name);
  if (!cfg) return void collabApi.runAnswer(request, t("tools.codeWithMe.noRunConfiguration", { name }));
  try {
    await runConfiguration(cfg);
    await collabApi.runAnswer(request);
    showBalloon(t("tools.codeWithMe.title"), t("tools.codeWithMe.guestRan", { guest, name }), 4000);
  } catch (e) {
    await collabApi.runAnswer(request, message(e));
  }
}

registerTerminalTabActions((tab) =>
  isHost() && tab.source?.kind !== "existing"
    ? [{ label: t("tools.codeWithMe.shareWithGuests"), checked: !!tab.shared, run: () => void setTerminalShared(tab, !tab.shared) }]
    : [],
);

// ---- guest -------------------------------------------------------------------------------

/** What a guest's tab says when the host stopped sharing its terminal. */
export const unsharedText = () => t("tools.codeWithMe.terminalUnshared");

/**
 * Show the host's terminals as tabs and keep their size and access current. A terminal the
 * host no longer shares stays as an ended tab (its output kept) until the guest closes it;
 * shared again, it opens fresh.
 */
export function syncGuestTerminals(list: SharedTerminal[], me: number | undefined, readOnly: boolean): void {
  const shown = new Set<string>();
  for (const term of list) {
    if (term.owner === me) continue;
    const id = sharedTerminalTab(term.id);
    shown.add(id);
    const stale = tabs().find((x) => x.id === id);
    if (stale?.source?.kind === "collab" && stale.source.ended) closeTerminalTab(id);
    const tab = tabs().find((x) => x.id === id);
    const source = { kind: "collab" as const, terminal: term.id, readOnly, cols: term.cols, rows: term.rows };
    if (!tab) {
      addSourceTab(id, term.title, source);
      const host = session().participants.find((p) => p.role === "host");
      if (term.owner === host?.id && term.running) {
        showBalloon(t("tools.codeWithMe.title"), t("tools.codeWithMe.hostSharedTerminal", { host: host.name, title: term.title }), 6000, [
          {
            label: t("tools.codeWithMe.show"),
            run: () => {
              showToolWindow("terminal");
              setActiveTab(id);
            },
          },
        ]);
      }
    } else if (tab.source?.kind === "collab" && (tab.source.cols !== term.cols || tab.source.rows !== term.rows || tab.source.readOnly !== readOnly)) {
      updateTab(id, { source });
    }
  }
  for (const tab of tabs()) {
    if (tab.source?.kind !== "collab" || shown.has(tab.id) || tab.source.ended) continue;
    updateTab(tab.id, { source: { ...tab.source, ended: unsharedText() } });
    endSharedSession(tab.id, unsharedText());
  }
}

let guestWindow = false;

/** This window became a guest: new terminals open on the host, Run acts on the host. */
function enterGuest(): void {
  if (guestWindow) return;
  guestWindow = true;
  setNewTabGuard(() => terminalBlockedReason(myPermissions()));
  setRemoteRuns({
    start: async (cfg) => {
      if (myPermissions()?.run !== "full") throw new Error(t("tools.codeWithMe.runNotAllowed"));
      await collabApi.runStart(cfg.name);
    },
    stop: (run) => collabApi.runStop(run),
    input: (run, text) => collabApi.runInput(run, text),
    attach: (run) => collabApi.runAttach(run),
    detach: (run) => void collabApi.runDetach(run).catch(() => undefined),
    unavailable: t("tools.codeWithMe.debugUnavailable"),
  });
}

/** The session ended for this guest: the host's terminals close, its runs stay as they ended. */
function leaveGuest(): void {
  if (!guestWindow) return;
  guestWindow = false;
  setNewTabGuard(null);
  setRemoteRuns(null);
  endSharedRuns();
  for (const tab of tabs()) if (tab.source?.kind === "collab") closeTerminalTab(tab.id);
}

let tracking = false;

export function trackTerminalsAndRuns(): void {
  if (tracking) return;
  tracking = true;
  void collabApi.onRunRequest((r) => {
    if (isHost()) void answerRunRequest(r.request, r.configuration, r.guest);
  });
  void collabApi.onTerminalOpened(({ id, title }) => {
    if (!isHost()) return;
    addSourceTab(id, title, { kind: "existing" });
    showBalloon(t("tools.codeWithMe.title"), t("tools.codeWithMe.guestOpenedTerminal", { guest: title.replace(/: Local$/, "") }), 6000, [
      {
        label: t("tools.codeWithMe.show"),
        run: () => {
          showToolWindow("terminal");
          setActiveTab(id);
        },
      },
    ]);
  });
  createRoot(() => {
    // Host: which tabs are shared, and sharing new tabs when the setting says so.
    let known: Set<string> | null = null;
    createEffect(() => {
      const shared = new Set(isHost() ? (session().terminals ?? []).map((t) => t.key) : []);
      for (const tab of tabs()) if (!!tab.shared !== shared.has(tab.id)) updateTab(tab.id, { shared: shared.has(tab.id) });
    });
    createEffect(
      on(
        () => (isHost() ? tabs().map((tab) => tab.id) : null),
        (ids) => {
          if (!ids) return void (known = null);
          if (known && getSetting<boolean>("codeWithMe.shareNewTerminals", false)) {
            for (const tab of tabs()) if (!known.has(tab.id) && !tab.source && !tab.ssh) void setTerminalShared(tab, true);
          }
          known = new Set(ids);
        },
      ),
    );

    // Guest: the host's terminals and runs, and what this guest may do with them. Every
    // effect makes sure the guest's window is set up, whichever runs first.
    createEffect(on(isGuest, (guest) => (guest ? enterGuest() : leaveGuest())));
    createEffect(() => {
      if (!isGuest()) return;
      enterGuest();
      syncGuestTerminals(session().terminals ?? [], session().me, myPermissions()?.terminal !== "full");
    });
    createEffect(() => {
      if (!isGuest()) return;
      enterGuest();
      for (const run of session().runs ?? []) void showSharedRun(run, sharedRunId(run.id), run.startedBy === session().me);
    });
  });
}
