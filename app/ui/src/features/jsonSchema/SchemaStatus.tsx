/**
 * Status bar widget of JSON and YAML files: the JSON schema the file is validated with
 * ("JSON schema: composer.json", "No JSON schema"), like PHPStorm's. A click opens a popup
 * to pick another schema for the file (stored as a project mapping) or edit the mappings.
 */
import { createResource, Show } from "solid-js";
import { TriangleAlert } from "lucide-solid";
import { jsonSchemaApi, type BuiltinSchema } from "@/ipc/jsonSchema";
import { showResolvedMenu, type ResolvedEntry } from "@/features/contextMenu/state";
import { openSettings } from "@/features/settings/dialogState";
import { showBalloon } from "@/keymap";
import { activeTab } from "@/state/editor";
import { saveSetting } from "@/state/settings";
import { assignSchema } from "./mappings";
import { active, MAPPINGS_KEY, mappings, schemaLanguage, schemas } from "./state";

function choose(path: string, name: string, schema: string): void {
  void saveSetting(MAPPINGS_KEY, assignSchema(mappings(), path, { name, schema }), "project").catch((e) =>
    showBalloon("Cannot change the JSON schema", String(e instanceof Error ? e.message : e), 6000),
  );
}

export function JsonSchemaStatus() {
  const [builtins] = createResource(() => jsonSchemaApi.builtins().catch(() => [] as BuiltinSchema[]), { initialValue: [] });
  const path = () => {
    const t = activeTab();
    return t && !t.view && active() && schemaLanguage(t.path) ? t.path : null;
  };
  const info = () => {
    const p = path();
    return p ? schemas[p] : undefined;
  };
  const label = () => {
    const i = info();
    return i && i.uri ? `JSON schema: ${i.name}` : "No JSON schema";
  };
  const tooltip = () => {
    const i = info();
    if (!i || !i.uri) return "No JSON schema is used for this file. Click to choose one";
    const from = { user: "a schema mapping", inline: "$schema in the file", builtin: "a built-in mapping", catalog: "the SchemaStore catalog" }[i.source];
    return `JSON schema: ${i.name} (from ${from})${i.error ? `\n${i.error}` : ""}\nClick to choose another schema`;
  };
  const open = (el: HTMLElement) => {
    const p = path();
    if (!p) return;
    const i = info();
    const entries: ResolvedEntry[] = [];
    const known = (uri: string) => builtins().some((b) => b.uri === uri) || mappings().some((m) => m.schema === uri && m.schema);
    if (i?.uri && !known(i.uri)) {
      entries.push({ kind: "action", label: i.name, checkable: true, checked: true, run: () => undefined }, "separator");
    }
    entries.push(
      { kind: "action", label: "No JSON schema", checkable: true, checked: !!i && !i.uri, run: () => choose(p, "No JSON schema", "") },
      "separator",
    );
    const seen = new Set<string>();
    for (const m of mappings()) {
      if (!m.schema || seen.has(m.name)) continue;
      seen.add(m.name);
      entries.push({
        kind: "action",
        label: m.name,
        checkable: true,
        checked: i?.source === "user" && i.name === m.name,
        run: () => choose(p, m.name, m.schema),
      });
    }
    if (seen.size) entries.push("separator");
    for (const b of builtins()) {
      entries.push({
        kind: "action",
        label: b.name,
        checkable: true,
        checked: !!i && i.uri === b.uri && i.source !== "user",
        run: () => choose(p, b.name, b.uri),
      });
    }
    entries.push("separator", { kind: "action", label: "Edit Schema Mappings…", run: () => openSettings("lang.json-schema") });
    const r = el.getBoundingClientRect();
    showResolvedMenu(entries, r.left, r.top - 2, { above: true, class: "status-popup" });
  };
  return (
    <Show when={path()}>
      <button class="status-item" data-status="json-schema" title={tooltip()} onClick={(e) => open(e.currentTarget)}>
        <Show when={info()?.error}>
          <TriangleAlert size={12} class="json-schema-status-warning" />
        </Show>
        {label()}
      </button>
    </Show>
  );
}
