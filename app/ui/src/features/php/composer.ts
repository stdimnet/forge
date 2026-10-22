/**
 * Composer settings at runtime (Settings | Languages & Frameworks | PHP | Composer): the
 * language level in effect (from composer.json while synchronized, computed in Rust) and
 * the update check when a project opens.
 */
import { createEffect, createRoot, createSignal, on } from "solid-js";
import { showBalloon } from "@/keymap";
import { onFsChanged } from "@/state/fsWatch";
import { isPluginActive } from "@/state/project";
import { getSetting, settingsState } from "@/state/settings";
import { workspace } from "@/state/workspace";
import { isDialogWindow } from "@/shell/window/mode";
import { phpApi, type LanguageLevel, type OutdatedPackage } from "./api";

const [languageLevel, setLanguageLevel] = createSignal<LanguageLevel | null>(null);

export { languageLevel };

async function refreshLanguageLevel(): Promise<void> {
  if (!workspace()) {
    setLanguageLevel(null);
    return;
  }
  setLanguageLevel(await phpApi.languageLevel().catch(() => null));
}

/** The update check's balloon text, e.g. "laravel/framework 12.1.0 → 12.3.0, …". */
export function outdatedSummary(packages: OutdatedPackage[]): string {
  const shown = packages.slice(0, 4).map((p) => `${p.name} ${p.version} → ${p.latest}`);
  const more = packages.length - shown.length;
  return shown.join(", ") + (more > 0 ? ` and ${more} more` : "");
}

async function checkUpdates(): Promise<void> {
  const outdated = await phpApi.composerOutdated().catch(() => [] as OutdatedPackage[]);
  if (!outdated.length) return;
  const n = outdated.length;
  showBalloon(`${n} Composer package${n === 1 ? " has" : "s have"} updates`, outdatedSummary(outdated), 12000);
}

createRoot(() => {
  // Again when the project, a PHP or Composer setting or composer.json changes.
  createEffect(
    on(
      () => [
        workspace()?.root,
        isPluginActive("forge.php"),
        JSON.stringify(Object.entries(settingsState.values).filter(([k]) => k === "php.languageLevel" || k.startsWith("php.composer."))),
      ],
      () => void refreshLanguageLevel(),
    ),
  );
  createEffect(
    on(
      () => workspace()?.root,
      (root) => {
        if (!root) return;
        // Once the project's plugins are known.
        // Update checks belong to the main window, not to dialog windows (Settings).
        if (isDialogWindow()) return;
        setTimeout(() => {
          if (workspace()?.root === root && isPluginActive("forge.composer") && getSetting<boolean>("php.composer.checkUpdates", false)) {
            void checkUpdates();
          }
        }, 3000);
      },
    ),
  );
});

onFsChanged((c) => {
  const json = getSetting<string>("php.composer.json", "composer.json");
  if ([...c.changed, ...c.removed].includes(json)) void refreshLanguageLevel();
});
