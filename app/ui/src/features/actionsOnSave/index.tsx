/**
 * Settings | Tools | Actions on Save, like PHPStorm: Reformat code, Optimize imports,
 * Rearrange code, Run code cleanup, Run eslint --fix, Run Prettier, Run PHP CS Fixer and Run
 * Laravel Pint on explicit save (Ctrl+S), optionally on autosave. The logic lives in
 * `crates/forge-save-actions` and `packs/phpforge/lang-php`; `run.ts` connects it to
 * `saveFile`.
 */
import { setSaveActionsRunner } from "@/state/editor";
import { registerSettingsPage } from "@/features/settingsPages";
import { ActionsOnSavePage } from "./ActionsOnSavePage";
import { saveActionsRunner } from "./run";
import "./actionsOnSave.css";

setSaveActionsRunner(saveActionsRunner);

registerSettingsPage({
  id: "tools.actions-on-save",
  title: "Actions on Save",
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
    "php cs fixer",
    "pint",
    "autosave",
  ],
  component: ActionsOnSavePage,
});
