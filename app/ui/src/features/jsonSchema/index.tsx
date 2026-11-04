/**
 * JSON Schema for JSON, JSON5/JSONC and YAML files (`forge-jsonschema`), like PHPStorm's:
 *
 * - problems, completion and hover documentation in the editor (`editor.ts`),
 * - Go to Declaration (Ctrl+B / Ctrl+Click) from a property into its schema, which opens
 *   bundled and downloaded schemas read-only (`json-schema://…`),
 * - the status bar widget with the file's schema, and a popup to pick another one,
 * - Settings | Languages & Frameworks | Schemas and DTDs: JSON Schema Mappings and Remote
 *   JSON Schemas.
 */
import { EditorState, type Extension } from "@codemirror/state";
import { showPanel, type Panel } from "@codemirror/view";
import { FileJson } from "lucide-solid";
import { jsonSchemaApi, isSchemaLocation, SCHEMA_PREFIX } from "@/ipc/jsonSchema";
import { registerEditorExtension } from "@/editor/extensions";
import { openFileAt } from "@/editor/views";
import { registerDeclarationProvider } from "@/features/lsp/goto";
import { h, iconNode } from "@/features/lsp/util";
import { chooseDeclarationAt } from "@/features/navigation/declarationChooser";
import { registerSettingsPage } from "@/features/settingsPages";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { openSettings } from "@/features/settings/dialogState";
import { registerStatusItem } from "@/features/statusItems";
import { registerFileSystem } from "@/state/editor";
import { jsonSchemaEditor } from "./editor";
import { JsonSchemaMappingsPage } from "./MappingsPage";
import { JsonSchemaStatus } from "./SchemaStatus";
import { active, schemaLanguage } from "./state";
import "./jsonSchema.css";
import { t, t as tr } from "@/i18n";

registerEditorExtension(({ path }) => {
  const language = schemaLanguage(path);
  return language ? jsonSchemaEditor(path, language) : null;
});

// ---- Go to Declaration into the schema ------------------------------------------------

registerDeclarationProvider(async (path, view, pos) => {
  const language = schemaLanguage(path);
  if (!language || !active()) return false;
  const targets = await jsonSchemaApi.goto(path, language, view.state.doc.toString(), pos);
  if (!targets.length) return false;
  chooseDeclarationAt(view, pos, targets, {
    open: (t) => void openFileAt(t.path, t.line + 1, (t.column ?? 0) + 1, { flash: true }),
  });
  return true;
});

// ---- Read-only schema files ------------------------------------------------------------

const details = new Map<string, string>();

registerFileSystem({
  prefix: SCHEMA_PREFIX,
  async read(path) {
    const file = await jsonSchemaApi.read(path);
    details.set(path, file.detail);
    return file.text;
  },
  write: async () => {
    throw new Error("JSON schemas of Forge are read-only");
  },
  breadcrumbs: (path) => ["JSON Schemas", path.split("/").pop() ?? path],
});

function schemaFileEditor(path: string): Extension {
  const banner = showPanel.of((): Panel => {
    const dom = h(
      "div",
      "library-banner",
      iconNode(FileJson, 13, "library-banner-icon"),
      h("strong", undefined, "Read-only JSON schema"),
      h("span", "library-banner-text", details.get(path) ?? ""),
    );
    return { dom, top: true };
  });
  return [EditorState.readOnly.of(true), banner];
}

registerEditorExtension(({ path }) => (isSchemaLocation(path) ? schemaFileEditor(path) : null));

// ---- Status bar and settings -------------------------------------------------------------

registerStatusItem({ id: "json-schema", align: "right", order: 60, component: JsonSchemaStatus });

registerSettingsPage({
  id: "lang.schemas",
  title: t("settings.page.lang.schemas"),
  group: "Languages & Frameworks",
  order: 70,
  keywords: ["json schema", "dtd", "xsd", "schemastore"],
  component: () => (
    <div class="set-page set-group-page">
      <button class="set-link" onClick={() => openSettings("lang.json-schema")}>
        {tr("tools.jsonSchema.jsonSchemaMappings")}
      </button>
      <button class="set-link" onClick={() => openSettings("lang.json-schema.remote")}>
        {tr("tools.jsonSchema.remoteJsonSchemas")}
      </button>
    </div>
  ),
});

registerSettingsPage({
  id: "lang.json-schema",
  title: t("settings.page.lang.json-schema"),
  group: "Languages & Frameworks",
  parent: "lang.schemas",
  order: 10,
  keywords: ["json", "yaml", "schema", "$schema", "mapping", "composer.json", "package.json", "compose", "github actions", "gitlab ci"],
  component: JsonSchemaMappingsPage,
});

registerSettingsPage({
  id: "lang.json-schema.remote",
  title: t("settings.page.lang.json-schema.remote"),
  group: "Languages & Frameworks",
  parent: "lang.schemas",
  order: 20,
  keywords: ["download", "schemastore", "catalog", "remote", "url"],
  component: () => <SchemaPage pageId="lang.json-schema.remote" />,
});
