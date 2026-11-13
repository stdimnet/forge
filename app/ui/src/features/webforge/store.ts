/** WebForge state: which plugins apply to the open project and its npm packages. */
import { createEffect, createRoot, createSignal, on } from "solid-js";
import { npmApi, type NpmPackage, type WebFramework } from "@/ipc/web";
import { isPluginActive, usesFramework, usesProjectType } from "@/state/project";
import { onFsChanged } from "@/state/fsWatch";
import { workspace } from "@/state/workspace";

/** HTML, CSS & JavaScript (forge.web) applies: the project has web languages. */
export const webActive = (): boolean => usesProjectType("webforge") && isPluginActive("forge.web");

/** The project has a package.json (npm tool window, run icons). */
export const npmActive = (): boolean => webActive() && (usesProjectType("npm") || usesProjectType("pnpm") || usesProjectType("yarn") || usesProjectType("bun"));

const FRAMEWORK_PLUGIN: Record<WebFramework, { plugin: string; types: string[] }> = {
  react: { plugin: "forge.react", types: ["react", "next"] },
  vue: { plugin: "forge.vue", types: ["vue", "nuxt"] },
  angular: { plugin: "forge.angular", types: ["angular"] },
};

/** A framework plugin is enabled and the project uses the framework. */
export const frameworkActive = (fw: WebFramework): boolean =>
  webActive() && isPluginActive(FRAMEWORK_PLUGIN[fw].plugin) && FRAMEWORK_PLUGIN[fw].types.some((t) => usesFramework(t));

const [packages, setPackages] = createSignal<NpmPackage[]>([]);
const [loading, setLoading] = createSignal(false);
const [loaded, setLoaded] = createSignal(false);

export { packages, loading, loaded };

let pending: Promise<void> | null = null;

/** Read the project's package.json files (again with `refresh`). */
export function loadPackages(refresh = false): Promise<void> {
  if (pending && !refresh) return pending;
  setLoading(true);
  pending = (async () => {
    try {
      setPackages(await npmApi.packages(refresh));
    } catch (e) {
      console.warn("npm scripts could not be read", e);
      setPackages([]);
    } finally {
      setLoading(false);
      setLoaded(true);
      pending = null;
    }
  })();
  return pending;
}

const isManifest = (p: string) => p === "package.json" || p.endsWith("/package.json") || /(^|\/)(pnpm-lock\.yaml|yarn\.lock|bun\.lockb?|package-lock\.json)$/.test(p);

if (typeof window !== "undefined") {
  createRoot(() =>
    createEffect(
      on(
        () => [workspace()?.root, npmActive()] as const,
        ([root, active]) => {
          if (root && active) void loadPackages();
          else setPackages([]);
        },
      ),
    ),
  );
  // Scripts follow edits of package.json and lock files.
  onFsChanged((e) => {
    if (npmActive() && [...e.changed, ...e.removed].some(isManifest)) void loadPackages(true);
  });
}
