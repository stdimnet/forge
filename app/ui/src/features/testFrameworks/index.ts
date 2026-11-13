/**
 * PHP test frameworks: Settings | PHP | Test Frameworks (PHPUnit, Pest, Codeception, Behat,
 * PHPSpec per interpreter, plus the auto-test delay) and Behat's Go to Declaration from a
 * step of a `.feature` file to its step definition (Ctrl+B / Ctrl+Click), computed in Rust.
 * The run configuration types, gutter icons, Test History and auto-test live in the run
 * feature.
 */
import { createComponent } from "solid-js";
import { registerSettingsPage } from "@/features/settingsPages";
import { registerObjectSettingEditor, SchemaPage } from "@/features/settings/SchemaPage";
import { registerDeclarationProvider } from "@/features/lsp/goto";
import { chooseDeclarationAt } from "@/features/navigation/declarationChooser";
import { testFrameworksApi } from "@/ipc/testFrameworks";
import { TestFrameworksEditor } from "./TestFrameworksEditor";
import "./testFrameworks.css";
import { t } from "@/i18n";

export const SETTINGS_PAGE = "lang.php.test-frameworks";

registerSettingsPage({
  id: SETTINGS_PAGE,
  title: t("settings.page.test-frameworks"),
  group: "Languages & Frameworks",
  parent: "lang.php",
  order: 2,
  keywords: ["phpunit", "pest", "codeception", "behat", "phpspec", "test", "autoloader", "phar", "bootstrap", "auto-test"],
  component: () => createComponent(SchemaPage, { pageId: SETTINGS_PAGE }),
});
registerObjectSettingEditor("php.testFrameworks", TestFrameworksEditor);

/** A step line of a feature file (`Given …`, `And …`, `* …`). */
const STEP = /^\s*(?:Given|When|Then|And|But|\*)\s/;

registerDeclarationProvider(async (path, view, pos) => {
  if (!/\.feature$/i.test(path)) return false;
  const line = view.state.doc.lineAt(pos);
  if (!STEP.test(line.text)) return false;
  const targets = await testFrameworksApi.stepDefinitions(view.state.doc.toString(), line.number - 1);
  if (!targets.length) return false;
  chooseDeclarationAt(
    view,
    pos,
    targets.map((t) => ({ path: t.path, line: t.line, column: t.column })),
    { title: "Choose Step Definition" },
  );
  return true;
});
