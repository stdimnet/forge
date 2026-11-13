/**
 * Laravel plugin state: whether it applies to the open project, the project summary and
 * the route list (reloaded when route files are saved).
 */
import { createEffect, createRoot, createSignal, on } from "solid-js";
import { laravelApi, type LaravelInfo, type LaravelLocation, type LaravelRoute } from "@/ipc/laravel";
import type { SettingValue } from "@/ipc/settings";
import { getSetting } from "@/state/settings";
import { isPluginActive, usesFramework } from "@/state/project";
import { onFileSaved } from "@/state/editor";
import { workspace } from "@/state/workspace";
import { revealLocation } from "@/features/lsp/navigation";

/** Enabled in Settings | Plugins and the open project is a Laravel application. */
export const laravelActive = (): boolean => isPluginActive("forge.laravel") && usesFramework("laravel");

export const laravelSetting = <T extends SettingValue>(key: string, fallback: T): T => getSetting<T>(key, fallback) ?? fallback;

const [info, setInfo] = createSignal<LaravelInfo | null>(null);
const [routes, setRoutes] = createSignal<LaravelRoute[]>([]);
const [routesSource, setRoutesSource] = createSignal<"static" | "artisan" | null>(null);
const [routesLoading, setRoutesLoading] = createSignal(false);
const [routesError, setRoutesError] = createSignal<string | null>(null);

export { info as laravelInfo, routes, routesSource, routesLoading, routesError };

let routesRequested = false;

export async function loadRoutes(): Promise<void> {
  routesRequested = true;
  setRoutesLoading(true);
  try {
    const r = await laravelApi.routes();
    setRoutes(r.routes);
    setRoutesSource(r.source);
    setRoutesError(null);
  } catch (e) {
    setRoutesError(String(e));
  } finally {
    setRoutesLoading(false);
  }
}

export async function refreshInfo(): Promise<void> {
  try {
    setInfo(await laravelApi.detect());
  } catch (e) {
    console.warn("Laravel detection failed", e);
  }
}

/** Open a location in the editor. */
export const reveal = (l: LaravelLocation) => revealLocation(l.path, { line: l.line, character: l.character });

/** `App\Http\Controllers\OrderController@show` → `OrderController@show`. */
export function shortAction(action: string): string {
  const [cls, method] = action.split("@");
  const short = cls.split("\\").pop() ?? cls;
  return method ? `${short}@${method}` : short;
}

if (typeof window !== "undefined") {
  createRoot(() => {
    createEffect(
      on(
        () => !!workspace() && laravelActive(),
        (active) => {
          setRoutes([]);
          setRoutesSource(null);
          routesRequested = false;
          if (active) void refreshInfo();
          else setInfo(null);
        },
      ),
    );
  });
  onFileSaved((path) => {
    if (!laravelActive()) return;
    if (routesRequested && (path.startsWith("routes/") || path.startsWith("app/Http/"))) void loadRoutes();
    if (/^(routes|config|app\/Models|database\/migrations|resources\/views|lang)\//.test(path)) void refreshInfo();
  });
}
