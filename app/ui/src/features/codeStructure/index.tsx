/**
 * The code structure in the editor, like PHPStorm: Sticky Lines, structure breadcrumbs,
 * custom folding regions, rendered documentation comments and Reader mode, plus the
 * Structure view (Alt+7) of web files and its region groups. Scopes, regions and doc
 * comments are computed in Rust (see `model.ts`); the modules here draw them.
 */
import { createEffect, createRoot, on } from "solid-js";
import type { Extension } from "@codemirror/state";
import { ViewPlugin } from "@codemirror/view";
import { foldEffect } from "@codemirror/language";
import { BookOpen } from "lucide-solid";
import { registerEditorExtension } from "@/editor/extensions";
import { languageIdFor } from "@/editor/languages";
import { openFileAt } from "@/editor/views";
import { registerLineMarkerProvider } from "@/editor/lineMarkers";
import { registerSettingsPage } from "@/features/settingsPages";
import { registerObjectSettingEditor, SchemaPage } from "@/features/settings/SchemaPage";
import { registerMainMenuEntries } from "@/features/mainMenu";
import { registerMainMenuToggle, toggle } from "@/features/mainMenu/model";
import { activeView } from "@/features/lsp/navigation";
import { showHint } from "@/features/lsp/hint";
import { registerStructureProvider } from "@/features/php/StructureView";
import type { StructureNode } from "@/features/php/api";
import { registerKeymapActions } from "@/keymap";
import { registerCommand } from "@/state/commands";
import { getSetting, saveSetting } from "@/state/settings";
import { Text } from "@codemirror/state";
import {
  CRUMBS_DISABLED,
  STICKY_DISABLED,
  breadcrumbsOn,
  docCommentsOf,
  hasStructure,
  stickyLinesOn,
  structureOfText,
  type EditorStructure,
} from "./model";
import { stickyLines } from "./sticky";
import { structureBreadcrumbs, symbolKindOf } from "./breadcrumbs";
import { regionFolding } from "./regions";
import {
  DOC_LANGUAGES,
  allRendered,
  docCommentFolds,
  renderedDocs,
  renderedDocsField,
  setRenderAll,
  toggleDocAtLine,
} from "./renderedDocs";
import { readerMode, readerModeOn, renderDocsByDefault, toggleReaderMode } from "./readerMode";
import { LanguageToggles } from "./LanguageToggles";
import "./codeStructure.css";
import { t } from "@/i18n";

// ---- Editor ----

/** The rendered-docs default follows `editor.renderDocComments` in open editors. */
const followRenderSetting = ViewPlugin.define((view) => {
  const dispose = createRoot((d) => {
    createEffect(
      on(
        () => getSetting<boolean>("editor.renderDocComments", false),
        () =>
          queueMicrotask(() => {
            if (view.state.field(renderedDocsField, false)) {
              view.dispatch({ effects: setRenderAll.of(renderDocsByDefault(readerModeOn(view.state))) });
            }
          }),
        { defer: true },
      ),
    );
    return d;
  });
  return { destroy: dispose };
});

registerEditorExtension(({ path }) => {
  const language = languageIdFor(path);
  const ext: Extension[] = [readerMode(path)];
  if (!language) return ext;
  ext.push(stickyLines(language), structureBreadcrumbs(language), regionFolding(language));
  if (DOC_LANGUAGES.has(language)) {
    ext.push(renderedDocs(language, (state) => renderDocsByDefault(readerModeOn(state))), followRenderSetting);
  }
  return ext;
});

registerLineMarkerProvider({
  appliesTo: (path) => DOC_LANGUAGES.has(languageIdFor(path) ?? ""),
  markers: async (path, text) =>
    docCommentsOf(Text.of(text.split("\n")), languageIdFor(path)).map((c) => ({
      line: c.line,
      icon: BookOpen,
      title: t("tools.codeStructure.toggleRenderedView"),
      className: "marker-doc",
      onClick: (view) => void toggleDocAtLine(view, c.line),
    })),
});

// ---- Actions ----

const activeLanguage = () => {
  const a = activeView();
  return a ? languageIdFor(a.path) : undefined;
};

registerKeymapActions([
  { id: "ToggleRenderedDocPresentationForAll", title: "Render All Doc Comments", group: "Editor", shortcuts: {} },
  { id: "EditorToggleStickyLines", title: "Show Sticky Lines", group: "Editor", shortcuts: {} },
  { id: "ToggleReaderMode", title: "Reader Mode", group: "Editor", shortcuts: {} },
]);

registerCommand({
  id: "ToggleRenderedDocPresentation",
  title: "Toggle Rendered View",
  category: "View",
  run: () => {
    const a = activeView();
    if (!a) return;
    const { state } = a.view;
    const head = state.selection.main.head;
    if (!toggleDocAtLine(a.view, state.doc.lineAt(head).number - 1)) showHint(a.view, head, "No documentation comment here");
  },
});

registerCommand({
  id: "ToggleRenderedDocPresentationForAll",
  title: "Render All Doc Comments",
  category: "Tools",
  run: () => {
    const a = activeView();
    if (!a?.view.state.field(renderedDocsField, false)) return;
    a.view.dispatch({ effects: setRenderAll.of(!allRendered(a.view.state)) });
  },
});

registerCommand({
  id: "CollapseDocComments",
  title: "Collapse Doc Comments",
  category: "Code",
  run: () => {
    const a = activeView();
    if (!a?.view.state.field(renderedDocsField, false)) return;
    const effects = docCommentFolds(a.view.state).map((r) => foldEffect.of(r));
    if (effects.length) a.view.dispatch({ effects });
  },
});

/** Switch a per-language feature for the active file's language (on globally if it was off). */
async function toggleForLanguage(enabledKey: string, disabledKey: string, on: (lang: string) => boolean) {
  const lang = activeLanguage();
  if (!lang || !hasStructure(lang)) return;
  const off = getSetting<string[]>(disabledKey, []);
  if (on(lang)) await saveSetting(disabledKey, [...off.filter((l) => l !== lang), lang]);
  else {
    if (!getSetting<boolean>(enabledKey, true)) await saveSetting(enabledKey, true);
    await saveSetting(disabledKey, off.filter((l) => l !== lang));
  }
}

registerCommand({
  id: "EditorToggleShowBreadcrumbs",
  title: "Show Breadcrumbs",
  category: "View",
  run: () => void toggleForLanguage("editor.breadcrumbs.show", CRUMBS_DISABLED, breadcrumbsOn),
});
registerMainMenuToggle("EditorToggleShowBreadcrumbs", () => breadcrumbsOn(activeLanguage()));

registerCommand({
  id: "EditorToggleStickyLines",
  title: "Show Sticky Lines",
  category: "View",
  run: () => void toggleForLanguage("editor.stickyLines.enabled", STICKY_DISABLED, stickyLinesOn),
});
registerMainMenuToggle("EditorToggleStickyLines", () => stickyLinesOn(activeLanguage()));

registerCommand({
  id: "ToggleReaderMode",
  title: "Reader Mode",
  category: "View",
  run: () => {
    const a = activeView();
    if (a) toggleReaderMode(a.view);
  },
});
registerMainMenuToggle("ToggleReaderMode", () => {
  const a = activeView();
  return !!a && readerModeOn(a.view.state);
});
registerMainMenuToggle("ToggleRenderedDocPresentationForAll", () => {
  const a = activeView();
  return !!a && allRendered(a.view.state);
});

registerMainMenuEntries("View/Active Editor", [toggle("EditorToggleStickyLines", "Show Sticky _Lines"), toggle("ToggleReaderMode", "_Reader Mode")], {
  after: "EditorToggleShowBreadcrumbs",
});
registerMainMenuEntries("Tools", [toggle("ToggleRenderedDocPresentationForAll", "Render All _Doc Comments")]);

// ---- Settings ----

registerSettingsPage({
  id: "editor.breadcrumbs",
  title: t("settings.page.editor.breadcrumbs"),
  group: "Editor",
  parent: "editor.general",
  order: 5,
  keywords: ["breadcrumbs", "structure", "placement", "top", "bottom"],
  component: () => <SchemaPage pageId="editor.breadcrumbs" />,
});
registerSettingsPage({
  id: "editor.readerMode",
  title: t("settings.page.editor.readerMode"),
  group: "Editor",
  order: 35,
  keywords: ["reader", "rendered", "doc comments", "ligatures", "line spacing", "library"],
  component: () => <SchemaPage pageId="editor.readerMode" />,
});
registerObjectSettingEditor(STICKY_DISABLED, LanguageToggles);
registerObjectSettingEditor(CRUMBS_DISABLED, LanguageToggles);

// ---- Structure view ----

function toNodes(structure: EditorStructure): StructureNode[] {
  const byIndex = new Map<number, StructureNode>();
  const roots: StructureNode[] = [];
  structure.scopes.forEach((s, index) => {
    if (!s.crumb) return;
    const node: StructureNode = {
      name: s.name,
      kind: symbolKindOf(s.kind),
      detail: "",
      modifiers: {},
      offset: 0,
      line: s.line,
      column: s.column,
      lambda: s.kind === "closure" || undefined,
      children: [],
    };
    byIndex.set(index, node);
    // The nearest enclosing scope that is shown.
    let p = s.parent;
    while (p !== undefined && !structure.scopes[p].crumb) p = structure.scopes[p].parent;
    const parent = p !== undefined ? byIndex.get(p) : undefined;
    (parent ? parent.children! : roots).push(node);
  });
  return roots;
}

/** Web files (HTML, CSS, JavaScript, TypeScript, Vue) and other languages without a pack index. */
registerStructureProvider({
  appliesTo: (path) => {
    const lang = languageIdFor(path);
    return !path.endsWith(".php") && lang !== "php" && hasStructure(lang);
  },
  load: async (path, content) => ({ path, nodes: toNodes(structureOfText(content, languageIdFor(path))) }),
  reveal: (path, node) => void openFileAt(path, node.line + 1, (node.column ?? 0) + 1),
});
