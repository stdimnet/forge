/**
 * OpenAPI (forge.openapi): validation comes from the JSON Schema support with the bundled
 * OpenAPI 3.0 / 3.1 schemas (chosen by the file's `openapi:` key in Rust); this module adds
 * the "OpenAPI" view of the Endpoints tool window, the editor with a native preview and
 * Generate HTTP Request.
 */
import { createEffect, createRoot, on } from "solid-js";
import { registerEndpointsView } from "@/features/endpoints/registry";
import { registerFileEditor } from "@/shell/editorViews";
import { TextEditorWithPreview } from "@/editor/TextEditorWithPreview";
import { registerSettingsPage } from "@/features/settingsPages";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { registerCommand } from "@/state/commands";
import { getSetting } from "@/state/settings";
import { isPluginActive } from "@/state/project";
import { workspace } from "@/state/workspace";
import { onFileSaved } from "@/state/editor";
import { activeView } from "@/features/lsp/navigation";
import { isDialogWindow } from "@/shell/window/mode";
import { balloonsFor } from "@/keymap";
import { OpenApiEndpointsActions, OpenApiEndpointsView } from "./EndpointsView";
import { OpenApiPreview } from "./OpenApiPreview";
import { generateHttpRequest, isOpenApiPath, loadOpenApi, openApiFiles } from "./store";
import { t } from "@/i18n";

const showBalloon = balloonsFor("OpenAPI");
const active = () => isPluginActive("forge.openapi");

registerEndpointsView({
  id: "openapi",
  title: t("langs.webTooling.openapi"),
  order: 30,
  when: () => active() && openApiFiles().length > 0,
  component: OpenApiEndpointsView,
  actions: OpenApiEndpointsActions,
});

registerFileEditor({
  id: "openapi",
  accepts: (path) => active() && getSetting<boolean>("openapi.preview", true) && isOpenApiPath(path),
  component: (props) => (
    <TextEditorWithPreview tab={props.tab} group={props.group} defaultLayout={() => "split"} preview={() => <OpenApiPreview tab={props.tab} />} />
  ),
});

registerSettingsPage({
  id: "lang.openapi",
  title: t("settings.page.lang.openapi"),
  group: "Languages & Frameworks",
  order: 60,
  keywords: ["openapi", "swagger", "endpoints", "preview", "http request"],
  component: () => <SchemaPage pageId="lang.openapi" />,
});

registerCommand({
  id: "OpenAPI.GenerateHttpRequest",
  title: "Generate HTTP Request",
  category: "Tools",
  run: () => {
    const a = activeView();
    const file = a && openApiFiles().find((f) => f.path === a.path);
    if (!a || !file) return void showBalloon(t("common.generateHttpRequest"), t("langs.webTooling.putTheCaretOnAn"));
    const line = a.view.state.doc.lineAt(a.view.state.selection.main.head).number - 1;
    // The operation whose method key is on or above the caret.
    const ep = [...file.endpoints].reverse().find((e) => e.line <= line) ?? file.endpoints[0];
    if (ep) void generateHttpRequest(file.path, ep.method, ep.path);
  },
});

if (typeof document !== "undefined" && !isDialogWindow()) {
  createRoot(() => {
    createEffect(
      on(
        () => [workspace()?.root, active()] as const,
        ([root, on]) => {
          if (root && on) void loadOpenApi();
        },
      ),
    );
  });
  onFileSaved((path) => {
    if (active() && /\.(ya?ml|json)$/i.test(path)) void loadOpenApi();
  });
}
