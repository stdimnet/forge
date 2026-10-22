/**
 * Settings | Languages & Frameworks | Schemas and DTDs | JSON Schema Mappings, laid out like
 * PHPStorm's: the user's mappings on the left; on the right the selected mapping's name,
 * schema file or URL, schema version and the files, directories and patterns it applies to.
 * Values go through the dialog's draft (`jsonSchema.mappings`), so Apply / Cancel work; the
 * built-in mappings are listed read-only below.
 */
import { createResource, createSignal, For, Index, Show } from "solid-js";
import { Asterisk, FileJson, FileText, Folder, Minus, Plus } from "lucide-solid";
import { jsonSchemaApi, type BuiltinSchema, type SchemaMapping } from "@/ipc/jsonSchema";
import { draftValue, setDraft } from "@/features/settings/dialogState";
import { entryKind, fileLines, newMappingName, SCHEMA_VERSIONS, type MappingKind } from "./mappings";
import { MAPPINGS_KEY } from "./state";

const KIND_ICON = { file: FileText, directory: Folder, pattern: Asterisk } as const;
const KIND_LABEL: Record<MappingKind, string> = { file: "File", directory: "Directory", pattern: "File path pattern" };
const PLACEHOLDER: Record<MappingKind, string> = { file: "config/app.json", directory: "config/", pattern: "*.app.json" };

export function JsonSchemaMappingsPage() {
  const [builtins] = createResource(() => jsonSchemaApi.builtins().catch(() => [] as BuiltinSchema[]), { initialValue: [] });
  const list = () => draftValue<SchemaMapping[]>(MAPPINGS_KEY) ?? [];
  const [sel, setSel] = createSignal<number | null>(list().length ? 0 : null);
  const [entrySel, setEntrySel] = createSignal<number | null>(null);
  const [adding, setAdding] = createSignal(false);
  let entriesEl!: HTMLDivElement;

  const current = () => {
    const i = sel();
    return i === null ? undefined : list()[i];
  };
  const update = (patch: Partial<SchemaMapping>) => {
    const i = sel();
    if (i === null) return;
    setDraft(
      MAPPINGS_KEY,
      list().map((m, j) => (j === i ? { ...m, ...patch } : m)),
    );
  };
  const add = () => {
    setDraft(MAPPINGS_KEY, [...list(), { name: newMappingName(list()), schema: "", version: "", files: "" }]);
    setSel(list().length - 1);
    setEntrySel(null);
  };
  const remove = () => {
    const i = sel();
    if (i === null) return;
    setDraft(MAPPINGS_KEY, list().filter((_, j) => j !== i));
    setSel(list().length ? Math.min(i, list().length - 1) : null);
  };

  const entries = () => fileLines(current()?.files ?? "");
  const setEntries = (next: string[]) => update({ files: next.join("\n") });
  const addEntry = (kind: MappingKind) => {
    setAdding(false);
    setEntries([...entries(), PLACEHOLDER[kind]]);
    setEntrySel(entries().length - 1);
    queueMicrotask(() => {
      const inputs = entriesEl.querySelectorAll<HTMLInputElement>("input");
      const last = inputs[inputs.length - 1];
      last?.focus();
      last?.select();
    });
  };
  const removeEntry = () => {
    const i = entrySel();
    if (i === null) return;
    setEntries(entries().filter((_, j) => j !== i));
    setEntrySel(entries().length ? Math.min(i, entries().length - 1) : null);
  };

  return (
    <div class="set-page json-schema-page">
      <div class="json-schema-split">
        <div class="set-list json-schema-list">
          <div class="set-list-toolbar">
            <button class="icon-btn" title="Add" onClick={add}>
              <Plus size={14} />
            </button>
            <button class="icon-btn" title="Remove" disabled={sel() === null} onClick={remove}>
              <Minus size={14} />
            </button>
          </div>
          <div class="set-list-items json-schema-names">
            <Index each={list()} fallback={<div class="set-list-empty">No user mappings</div>}>
              {(m, i) => (
                <button
                  class="json-schema-name"
                  classList={{ selected: sel() === i }}
                  data-mapping={m().name}
                  onClick={() => {
                    setSel(i);
                    setEntrySel(null);
                  }}
                >
                  <FileJson size={13} />
                  <span>{m().name || "Unnamed"}</span>
                </button>
              )}
            </Index>
          </div>
        </div>
        <Show
          when={current()}
          fallback={<div class="set-desc json-schema-hint">Add a mapping to validate files with a JSON schema of your own.</div>}
        >
          {(m) => (
            <div class="json-schema-form">
              <div class="set-line">
                <label class="set-label json-schema-label">Name:</label>
                <input class="set-input" value={m().name} spellcheck={false} onInput={(e) => update({ name: e.currentTarget.value })} />
              </div>
              <div class="set-line">
                <label class="set-label json-schema-label">Schema file or URL:</label>
                <input
                  class="set-input set-path"
                  value={m().schema}
                  placeholder="schemas/app.schema.json or https://…"
                  spellcheck={false}
                  onInput={(e) => update({ schema: e.currentTarget.value })}
                />
              </div>
              <div class="set-line">
                <label class="set-label json-schema-label">Schema version:</label>
                <select class="set-select" value={m().version ?? ""} onChange={(e) => update({ version: e.currentTarget.value })}>
                  <For each={SCHEMA_VERSIONS}>{(v) => <option value={v.value}>{v.label}</option>}</For>
                </select>
              </div>
              <Show when={!m().schema.trim()}>
                <div class="set-desc">Without a schema, the files below are not validated (“No JSON schema”).</div>
              </Show>
              <div class="set-list json-schema-entries">
                <div class="set-list-toolbar">
                  <div class="json-schema-add">
                    <button class="icon-btn" title="Add" onClick={() => setAdding((a) => !a)}>
                      <Plus size={14} />
                    </button>
                    <Show when={adding()}>
                      <div class="context-menu popup json-schema-add-menu" role="menu">
                        <For each={["file", "directory", "pattern"] as MappingKind[]}>
                          {(kind) => {
                            const Icon = KIND_ICON[kind];
                            return (
                              <button class="context-menu-item" role="menuitem" onClick={() => addEntry(kind)}>
                                <Icon size={13} />
                                <span>Add {KIND_LABEL[kind]}</span>
                              </button>
                            );
                          }}
                        </For>
                      </div>
                    </Show>
                  </div>
                  <button class="icon-btn" title="Remove" disabled={entrySel() === null} onClick={removeEntry}>
                    <Minus size={14} />
                  </button>
                </div>
                <div class="set-list-items" ref={entriesEl}>
                  <Index each={entries()} fallback={<div class="set-list-empty">No files are mapped yet</div>}>
                    {(entry, i) => {
                      const kind = () => entryKind(entry());
                      return (
                        <div class="json-schema-entry" classList={{ selected: entrySel() === i }} title={KIND_LABEL[kind()]}>
                          {(() => {
                            const Icon = KIND_ICON[kind()];
                            return <Icon size={13} class="json-schema-entry-icon" />;
                          })()}
                          <input
                            class="set-list-item"
                            value={entry()}
                            spellcheck={false}
                            onFocus={() => setEntrySel(i)}
                            onInput={(e) => setEntries(entries().map((v, j) => (j === i ? e.currentTarget.value : v)))}
                          />
                        </div>
                      );
                    }}
                  </Index>
                </div>
              </div>
              <div class="set-desc">
                Paths are relative to the project root. A directory ends with <code>/</code>; patterns use <code>*</code>,{" "}
                <code>**</code>, <code>?</code> and <code>{"{a,b}"}</code> and match file names when they contain no <code>/</code>.
              </div>
            </div>
          )}
        </Show>
      </div>
      <section class="set-section">
        <div class="set-section-title">
          <span>Built-in Mappings</span>
        </div>
        <div class="set-section-body indented">
          <table class="set-table json-schema-builtins">
            <thead>
              <tr>
                <th>Schema</th>
                <th>Files</th>
              </tr>
            </thead>
            <tbody>
              <For each={builtins()}>
                {(b) => (
                  <tr title={b.uri}>
                    <td>{b.name}</td>
                    <td class="set-table-mono">{b.files.join(", ")}</td>
                  </tr>
                )}
              </For>
            </tbody>
          </table>
          <div class="set-desc">
            Bundled with Forge. A <code>"$schema"</code> in the file, or <code># yaml-language-server: $schema=…</code> in YAML, picks
            another schema; your mappings win over both.
          </div>
        </div>
      </section>
    </div>
  );
}
