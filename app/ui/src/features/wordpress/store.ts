/**
 * WordPress project data for the tool window, the editor features and the WP-CLI action:
 * the detection (site, plugin or theme, WP-CLI, stubs) and the hook index summary. Loaded
 * when a WordPress project opens; the backend rescans its files at most every 10 s.
 */
import { createEffect, createRoot, createSignal, on } from "solid-js";
import { wordpressApi, WORDPRESS_PLUGIN_ID, type HookSummary, type WordPressInfo } from "@/ipc/wordpress";
import { balloonsFor } from "@/keymap";
import { isPluginActive, usesFramework } from "@/state/project";
import { getSetting, saveSetting } from "@/state/settings";
import { workspace } from "@/state/workspace";

export const showWpBalloon = balloonsFor("WordPress");

/** The WordPress plugin applies to the open project. */
export const wordpressActive = (): boolean => usesFramework("wordpress") && isPluginActive(WORDPRESS_PLUGIN_ID);

const [info, setInfo] = createSignal<WordPressInfo | null>(null);
const [hooks, setHooks] = createSignal<HookSummary[]>([]);
const [loading, setLoading] = createSignal(false);
export { info as wordpressInfo, hooks, loading };

let pending: Promise<void> | null = null;

/** Load (or with `force`, rescan) the detection and the hook list. */
export function loadWordPress(force = false): Promise<void> {
  if (pending && !force) return pending;
  setLoading(true);
  pending = (async () => {
    try {
      const i = force ? await wordpressApi.refresh() : await wordpressApi.detect();
      setInfo(i);
      setHooks(i ? await wordpressApi.hooks() : []);
      if (i) offerStubs(i);
    } catch (e) {
      console.warn("WordPress index failed", e);
    } finally {
      setLoading(false);
      pending = null;
    }
  })();
  return pending;
}

/** `WordPress 6.6 site`, `Plugin "Acme Events" 1.2.0`, `Theme "Anvil"`. */
export function describe(i: WordPressInfo): string {
  const v = i.version ? ` ${i.version.split(".").slice(0, 2).join(".")}` : "";
  switch (i.kind) {
    case "plugin":
      return `Plugin "${i.header?.name ?? "?"}"${i.header?.version ? ` ${i.header.version}` : ""}`;
    case "theme":
      return `Theme "${i.header?.name ?? "?"}"${i.header?.version ? ` ${i.header.version}` : ""}`;
    case "composer":
      return "WordPress (Composer)";
    case "content":
      return "wp-content";
    default:
      return `WordPress${v}`;
  }
}

// ---- Stubs ----------------------------------------------------------------------------

const STUBS_COMMAND = "require --dev php-stubs/wordpress-stubs";
let offered = "";

/** Install php-stubs/wordpress-stubs with Composer (the user chose to). */
export async function installStubs(): Promise<void> {
  const { runComposer } = await import("@/features/composer/state");
  const code = await runComposer(STUBS_COMMAND);
  if (code === 0) void loadWordPress(true);
}

/** Once per project: offer the stubs when they are missing (never downloaded silently). */
function offerStubs(i: WordPressInfo): void {
  const root = workspace()?.root ?? "";
  if (i.stubs || offered === root || !getSetting<boolean>("wordpress.offerStubs", true)) return;
  offered = root;
  showWpBalloon(
    "WordPress stubs are not installed",
    "Go to Declaration on WordPress functions uses php-stubs/wordpress-stubs from vendor/. Forge does not ship them; Composer installs them from Packagist.",
    0,
    [
      { label: "Install with Composer", run: () => void installStubs() },
      { label: "Don't ask again", run: () => void saveSetting("wordpress.offerStubs", false) },
    ],
  );
}

if (typeof window !== "undefined") {
  createRoot(() =>
    createEffect(
      on(
        () => [workspace()?.root, wordpressActive()] as const,
        ([root, active]) => {
          if (root && active) void loadWordPress();
          else {
            setInfo(null);
            setHooks([]);
          }
        },
      ),
    ),
  );
}
