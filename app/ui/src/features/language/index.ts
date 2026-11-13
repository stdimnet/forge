/**
 * Settings | Appearance & Behavior | System Settings | Language: the IDE language
 * (`ide.language`). A change applies at once to everything rendered through `t()`; text
 * computed at startup (tool window and settings page titles, the main menu's action list)
 * follows after a restart, which the main window offers like PHPStorm does.
 */
import { createEffect, createRoot, on } from "solid-js";
import { isTauri } from "@/ipc";
import { maintenanceApi } from "@/ipc/maintenance";
import { LANGUAGE_SETTING, locale, setLocale, t } from "@/i18n";
import { getSetting, settingsState } from "@/state/settings";
import { askChoice } from "@/features/contextMenu/dialogs";
import { dialogBoot } from "@/shell/window/mode";

/** Ask to restart so the whole UI shows the new language. */
export async function offerRestart(): Promise<void> {
  const choice = await askChoice({
    title: t("language.restart.title"),
    message: t("language.restart.message"),
    choices: [
      { id: "later", label: t("language.restart.later") },
      { id: "restart", label: t("language.restart.now") },
    ],
  });
  if (choice !== "restart") return;
  await maintenanceApi.restart();
  if (!isTauri()) location.reload();
}

createRoot(() =>
  createEffect(
    on(
      () => (settingsState.loaded ? getSetting<string>(LANGUAGE_SETTING, "en") : undefined),
      (language) => {
        if (!language || language === locale()) return;
        setLocale(language);
        if (!dialogBoot()) void offerRestart();
      },
    ),
  ),
);
