/** State and actions of the New Project dialog (File | New Project). */
import { createEffect, createRoot, createSignal, on } from "solid-js";
import { isTauri } from "@/ipc";
import { projectApi, type CreateArgs, type Generator, type OptionValue, type Plan } from "@/ipc/project";
import { getSetting } from "@/state/settings";
import { closeDialog, openDialogWindow } from "@/shell/window/dialogs";
import {
  applyCreateEvent,
  carryOptions,
  defaultOptions,
  initialProgress,
  nameAfterSwitch,
  withChoices,
  type Progress,
} from "./wizard";

export const DEFAULT_GENERATOR = "empty-php";

export const NEW_PROJECT_DIALOG = "new-project";

export interface NewProjectProps {
  /** Generator to preselect. */
  generator?: string;
}

/** Result of the dialog: the created project, opened by the window that opened the dialog. */
export interface NewProjectResult {
  root: string;
}

const [isOpen, setOpen] = createSignal(false);
const [generators, setGenerators] = createSignal<Generator[]>([]);
const [selectedId, setSelectedId] = createSignal(DEFAULT_GENERATOR);
const [name, setName] = createSignal("");
const [location, setLocation] = createSignal("");
const [options, setOptions] = createSignal<Record<string, OptionValue>>({});
const [plan, setPlan] = createSignal<Plan | null>(null);
const [progress, setProgress] = createSignal<Progress | null>(null);
const [loadError, setLoadError] = createSignal<string | null>(null);
/** Option being refreshed, and the last refresh error per option. */
const [refreshing, setRefreshing] = createSignal<string | null>(null);
const [refreshError, setRefreshError] = createSignal<Record<string, string>>({});

export { isOpen, generators, selectedId, name, location, options, plan, progress, loadError, refreshing, refreshError };
export { setName, setLocation };

export const selected = (): Generator | undefined => generators().find((g) => g.id === selectedId());

export const args = (): CreateArgs => ({
  generator: selectedId(),
  location: location(),
  name: name(),
  options: options(),
});

export const running = () => progress()?.phase === "running";

/** The form can be submitted: valid name and location, all tools installed. */
export const canCreate = () => {
  const p = plan();
  return !!p && !p.error && p.missingTools.length === 0 && !progress();
};

export function setOption(id: string, value: OptionValue): void {
  setOptions((o) => ({ ...o, [id]: value }));
}

/**
 * "Refresh" next to an option with a `refresh` command (the Minecraft versions): fetch fresh
 * choices, which may use the network, so only on the user's click.
 */
export async function refreshOption(optionId: string): Promise<void> {
  const gen = selected();
  const command = gen?.options.find((o) => o.id === optionId)?.refresh;
  if (!gen || !command || refreshing()) return;
  setRefreshing(optionId);
  setRefreshError((e) => ({ ...e, [optionId]: "" }));
  try {
    const choices = await projectApi.refreshChoices(command);
    setGenerators((gens) => withChoices(gens, gen.id, optionId, choices));
  } catch (e) {
    setRefreshError((errs) => ({ ...errs, [optionId]: String(e).replace(/^Error: /, "") }));
  } finally {
    setRefreshing(null);
  }
}

export function selectGenerator(id: string): void {
  const next = generators().find((g) => g.id === id);
  if (!next || progress()) return;
  setName(nameAfterSwitch(name(), selected(), next));
  setOptions(carryOptions(next, options()));
  setSelectedId(id);
}

/** Open the dialog (its own window in the desktop app), optionally with a generator preselected. */
export function openNewProject(generatorId?: string): Promise<NewProjectResult | null> {
  return openDialogWindow<NewProjectResult>(NEW_PROJECT_DIALOG, { generator: generatorId } satisfies NewProjectProps);
}

/** Load the generators and reset the form, before the dialog shows. */
export async function prepareNewProject(generatorId?: string): Promise<void> {
  setProgress(null);
  setLoadError(null);
  if (!generators().length) {
    try {
      setGenerators(await projectApi.generators());
    } catch (e) {
      setLoadError(String(e));
    }
  }
  const gen = generators().find((g) => g.id === (generatorId ?? selectedId())) ?? generators()[0];
  if (gen) {
    setSelectedId(gen.id);
    setOptions(defaultOptions(gen));
    setName(gen.defaultName);
  }
  setLocation(getSetting<string>("system.defaultProjectDirectory", "~/Projects") || "~/Projects");
}

/** The dialog content mounted / unmounted. */
export function setDialogShown(shown: boolean): void {
  setOpen(shown);
  if (!shown) setProgress(null);
}

export function closeNewProject(): void {
  if (running()) return;
  closeDialog(NEW_PROJECT_DIALOG);
}

/** Native folder chooser for the location (desktop only). */
export async function browseLocation(): Promise<void> {
  if (!isTauri()) return;
  const [{ open }, { homeDir }] = await Promise.all([
    import("@tauri-apps/plugin-dialog"),
    import("@tauri-apps/api/path"),
  ]);
  let start = location().trim();
  if (start === "~" || start.startsWith("~/") || start.startsWith("~\\")) {
    start = (await homeDir()).replace(/[\\/]+$/, "") + start.slice(1);
  }
  const dir = await open({
    directory: true,
    multiple: false,
    defaultPath: start || undefined,
    title: "Select Base Directory",
  });
  if (typeof dir === "string") setLocation(dir);
}

/** Switch to the offline generator the plan suggests (missing CLI). */
export function useFallback(): void {
  const fb = plan()?.fallback;
  if (fb) selectGenerator(fb);
}

export async function createProject(): Promise<void> {
  if (!canCreate()) return;
  const request = args();
  setProgress(initialProgress());
  const unlisten = await projectApi.onCreate((e) => setProgress((p) => (p ? applyCreateEvent(p, e) : p)));
  try {
    const root = await projectApi.create(request);
    setProgress((p) => (p ? { ...p, phase: "done", root } : p));
    unlisten();
    closeDialog(NEW_PROJECT_DIALOG, { root } satisfies NewProjectResult);
  } catch (e) {
    unlisten();
    const error = String(e).replace(/^Error: /, "");
    setProgress((p) => (p ? { ...p, phase: "failed", error: p.error ?? error } : p));
  }
}

/** Cancel a running creation, or leave the progress view after a failure. */
export function cancelCreation(): void {
  if (running()) void projectApi.cancel();
  else setProgress(null);
}

// Recompute the plan (command preview, missing tools, errors) while the form changes.
let planSeq = 0;
createRoot(() => {
  createEffect(
    on([isOpen, selectedId, name, location, options], () => {
      if (!isOpen() || !generators().length) return;
      const seq = ++planSeq;
      const request = args();
      setTimeout(() => {
        if (seq !== planSeq) return;
        projectApi
          .plan(request)
          .then((p) => seq === planSeq && setPlan(p))
          .catch((e) => seq === planSeq && setPlan({ target: "", preview: [], missingTools: [], error: String(e) }));
      }, 80);
    }),
  );
});
