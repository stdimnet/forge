/**
 * Settings | Editor | Code Style: schemes (Default, Project and more IDE schemes), the General
 * page and one page per language with presets and a live preview, and the code style pass
 * after Reformat Code. Options, schemes and formatting live in Rust (`forge-codestyle`).
 */
import { registerSettingsPage } from "@/features/settingsPages";
import { registerDraftSource } from "@/features/settings/dialogState";
import { registerFormatPostProcessor } from "@/features/lsp/format";
import { codeStyleApi, type CodeStyleLanguage } from "@/ipc/codeStyle";
import { fileTypeOf } from "@/editor/fileTypes";
import { GeneralPage, LanguagePage } from "./CodeStylePages";
import { applyDraft, discardDraft, GENERAL_PAGE, hasChanges, loadModel, pageModified, pageOf, resetPage } from "./store";
import "./codeStyle.css";
import { t } from "@/i18n";

const keywordsOf = (lang: CodeStyleLanguage) => [
  ...new Set([...lang.options.map((o) => o.title), ...lang.tabs.map((t) => t.title), ...lang.presets.map((p) => p.name)]),
];

registerSettingsPage({
  id: GENERAL_PAGE,
  title: t("settings.page.code-style"),
  group: "Editor",
  order: 20,
  keywords: ["formatting", "scheme", "EditorConfig", "line separator", "hard wrap", "formatter markers"],
  component: GeneralPage,
});

// One page per language, from the model.
void loadModel()
  .then((model) => {
    registerSettingsPage({
      id: GENERAL_PAGE,
      title: t("settings.page.code-style"),
      group: "Editor",
      order: 20,
      keywords: ["formatting", "scheme", "EditorConfig", "import scheme", "export scheme", ...keywordsOf(model.general)],
      component: GeneralPage,
    });
    model.languages.forEach((lang, i) =>
      registerSettingsPage({
        id: pageOf(lang.id),
        title: lang.name,
        group: "Editor",
        parent: GENERAL_PAGE,
        order: i,
        keywords: keywordsOf(lang),
        component: () => <LanguagePage language={lang.id} />,
      }),
    );
  })
  .catch((e) => console.warn("code style model could not be loaded", e));

registerDraftSource({ modified: pageModified, hasChanges, apply: applyDraft, discard: discardDraft, resetPage });

// Reformat Code: after the formatter (and PHP's blank lines), the options it does not know,
// blank lines, indents on empty lines and the formatter markers (Rust, on the buffer).
registerFormatPostProcessor(async (path, text, original) => {
  const fileType = fileTypeOf(path, text).id;
  if (!fileType) return text;
  return codeStyleApi.postFormat({ fileType, original, text });
}, 100);
