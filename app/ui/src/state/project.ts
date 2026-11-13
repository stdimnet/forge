/**
 * What the open project is made of (languages, frameworks, packs), detected by the backend
 * when a project opens. Features use it to activate only where they apply:
 *
 *   if (usesFramework("laravel") && isPluginActive("forge.laravel")) { ... }
 */
import { createRoot, createSignal, on, createEffect } from "solid-js";
import { projectApi, type Framework, type ProjectInfo } from "@/ipc/project";
import { settingsState } from "./settings";
import { workspace } from "./workspace";

const [projectInfo, setProjectInfo] = createSignal<ProjectInfo | null>(null);
const listeners = new Set<(info: ProjectInfo, root: string) => void>();

export { projectInfo };

export const usesFramework = (id: string): boolean => !!projectInfo()?.frameworks.some((f) => f.id === id);
export const usesLanguage = (id: string): boolean => !!projectInfo()?.languages.includes(id);

/** Language, framework, pack or tool id, like `ProjectInfo::uses` in Rust. */
export function usesProjectType(id: string, info: ProjectInfo | null = projectInfo()): boolean {
  if (!info) return false;
  return (
    info.languages.includes(id) ||
    info.packs.includes(id) ||
    info.tools.includes(id) ||
    info.frameworks.some((f) => f.id === id)
  );
}

/** Plugins without project types are always active; others need a matching project. */
export function pluginAppliesTo(projectTypes: string[] | undefined, info: ProjectInfo | null): boolean {
  if (!projectTypes?.length) return true;
  return projectTypes.some((t) => usesProjectType(t, info));
}

/** Enabled in Settings | Plugins and used by the open project. */
export function isPluginActive(pluginId: string): boolean {
  const p = settingsState.plugins.find((x) => x.id === pluginId);
  return !!p?.enabled && pluginAppliesTo(p.projectTypes, projectInfo());
}

/** `Laravel 12`, `PHP 8.3`, `LÖVE 11`, `Neovim`: the short label the status bar shows. */
export function frameworkLabel(f: Framework | undefined | null): string {
  if (!f) return "";
  if (!f.version) return f.name;
  const parts = f.version.split(".");
  const isLanguage = f.id === f.language;
  const short = isLanguage || parts[0] === "0" ? parts.slice(0, 2).join(".") : parts[0];
  return `${f.name} ${short}`;
}

/** Called after each detection (e.g. to show the "support enabled" notification). */
export function onProjectDetected(cb: (info: ProjectInfo, root: string) => void): () => void {
  listeners.add(cb);
  return () => listeners.delete(cb);
}

function adopt(root: string, info: ProjectInfo): void {
  if (workspace()?.root !== root) return;
  setProjectInfo(info);
  for (const l of listeners) l(info, root);
}

/** Detect again, e.g. after composer.json changed. */
export async function refreshProjectInfo(): Promise<void> {
  const root = workspace()?.root;
  if (!root) return;
  try {
    adopt(root, await projectApi.detect());
  } catch (e) {
    console.warn("project detection failed", e);
  }
}

let started = false;

/** Start following the open project. Safe to call more than once. */
export function trackProjectInfo(): void {
  if (started) return;
  started = true;
  void projectApi.onDetected((d) => adopt(d.root, d.info));
  createRoot(() =>
    createEffect(
      on(
        () => workspace()?.root,
        (root) => {
          setProjectInfo(null);
          if (root) void refreshProjectInfo();
        },
      ),
    ),
  );
}
