/**
 * Settings | Tools | Actions on Save, like PHPStorm: Reformat code, Optimize imports,
 * Rearrange code, Run code cleanup, Run biome check --write, Run eslint --fix, Run stylelint --fix, Run Prettier, Run PHP CS Fixer and Run
 * Laravel Pint (and Rector, phpcbf) on explicit save (Ctrl+S), optionally on autosave. The logic lives in
 * `crates/forge-save-actions` and `packs/phpforge/lang-php`; `run.ts` connects it to
 * `saveFile`.
 */
import { setSaveActionsRunner } from "@/state/editor";
import { registerSettingsPage } from "@/features/settingsPages";
import { ActionsOnSavePage } from "./ActionsOnSavePage";
import { saveActionsRunner } from "./run";
import "./actionsOnSave.css";
import { t } from "@/i18n";

setSaveActionsRunner(saveActionsRunner);

registerSettingsPage({
  id: "tools.actions-on-save",
  title: t("settings.page.tools.actions-on-save"),
  group: "Tools",
  order: 5,
  keywords: [
    "save",
    "reformat",
    "optimize imports",
    "rearrange",
    "code cleanup",
    "eslint",
    "prettier",
    "stylelint",
    "biome",
    "php cs fixer",
    "pint",
    "phpcbf",
    "rector",
    "autosave",
  ],
  component: ActionsOnSavePage,
});
