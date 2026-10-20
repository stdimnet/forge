/** State and actions of the New Project dialog (File | New Project). */
import { createEffect, createRoot, createSignal, on } from "solid-js";
import { isTauri } from "@/ipc";
import { projectApi, type CreateArgs, type Generator, type OptionValue, type Plan } from "@/ipc/project";
import { getSetting } from "@/state/settings";
import { openWorkspace } from "@/state/workspace";
import {
  applyCreateEvent,
  carryOptions,
  defaultOptions,
  initialProgress,
  nameAfterSwitch,
  type Progress,
} from "./wizard";

export const DEFAULT_GENERATOR = "empty-php";

const [isOpen, setOpen] = createSignal(false);
const [generators, setGenerators] = createSignal<Generator[]>([]);
const [selectedId, setSelectedId] = createSignal(DEFAULT_GENERATOR);
const [name, setName] = createSignal("");
const [location, setLocation] = createSignal("");
const [options, setOptions] = createSignal<Record<string, OptionValue>>({});
const [plan, setPlan] = createSignal<Plan | null>(null);
const [progress, setProgress] = createSignal<Progress | null>(null);
const [loadError, setLoadError] = createSignal<string | null>(null);

export { isOpen, generators, selectedId, name, location, options, plan, progress, loadError };
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

export function selectGenerator(id: string): void {
  const next = generators().find((g) => g.id === id);
  if (!next || progress()) return;
  setName(nameAfterSwitch(name(), selected(), next));
  setOptions(carryOptions(next, options()));
  setSelectedId(id);
}

/** Open the dialog, optionally with a generator preselected. */
export async function openNewProject(generatorId?: string): Promise<void> {
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
  setOpen(true);
}

export function closeNewProject(): void {
  if (running()) return;
  setOpen(false);
  setProgress(null);
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
    setOpen(false);
    setProgress(null);
    await openWorkspace(root);
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
