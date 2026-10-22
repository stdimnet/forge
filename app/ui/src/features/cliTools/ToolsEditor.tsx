/**
 * Settings | Tools | Command Line Tool Support: the table of tools (`commandLineTools.tools`)
 * with Add (Choose Tool to Add: Composer, Laravel Artisan, Symfony Console, WP-CLI, Drush,
 * Tool based on Symfony Console, Tool based on PHP script), Remove and Edit, and the form
 * with alias, path, PHP interpreter and "Show console in". Tools the project uses but that
 * are not configured are listed as auto-detected; editing one adds it to the list.
 */
import { createMemo, createResource, createSignal, For, Index, Show } from "solid-js";
import { Minus, Pencil, Plus } from "lucide-solid";
import { cliToolsApi, TOOL_KINDS, toolKindLabel, type CliTool, type ToolKind } from "@/ipc/cliTools";
import type { SettingObject, SettingSpec } from "@/ipc/settings";
import { decodeInterpreters, GLOBAL_KEY, PROJECT_KEY } from "@/features/php/interpreters";
import { draftValue, setDraft } from "@/features/settings/dialogState";

/** A row of the table: a configured entry (by index) or a detected tool. */
type Row = { entry: SettingObject; index: number | null };

const toEntry = (t: CliTool): SettingObject => ({
  kind: t.kind,
  name: t.name,
  alias: t.alias,
  path: t.path,
  showConsoleIn: t.showConsoleIn,
  ...(t.interpreter ? { interpreter: t.interpreter } : {}),
});

/** Why the entry cannot be saved, `null` when it is fine. */
export function validateEntry(entry: SettingObject, others: SettingObject[]): string | null {
  const alias = (entry.alias ?? "").trim();
  if (!alias) return "Specify an alias.";
  if (/\s/.test(alias)) return "The alias must not contain spaces.";
  if (others.some((o) => o.alias === alias)) return `The alias '${alias}' is already used.`;
  if (!(entry.path ?? "").trim() && (entry.kind === "symfony-based" || entry.kind === "php-script")) return "Specify the path to the script.";
  return null;
}

export function ToolsEditor(props: { spec: SettingSpec }) {
  const [tools] = createResource(() => cliToolsApi.list().catch(() => [] as CliTool[]), { initialValue: [] });
  const entries = () => draftValue<SettingObject[]>(props.spec.key) ?? [];
  const detected = createMemo(() =>
    tools()
      .filter((t) => t.detected)
      .filter((t) => !entries().some((e) => e.kind === t.kind || e.alias === t.alias))
      .map(toEntry),
  );
  const rows = createMemo<Row[]>(() => [
    ...entries().map((entry, index) => ({ entry, index })),
    ...detected().map((entry) => ({ entry, index: null })),
  ]);
  const interpreters = createMemo(() => decodeInterpreters(draftValue(GLOBAL_KEY), draftValue(PROJECT_KEY)).map((i) => i.name));

  const [sel, setSel] = createSignal<number | null>(null);
  const [choosing, setChoosing] = createSignal(false);
  /** The entry in the form and the configured row it edits (`null`: a new entry). */
  const [form, setForm] = createSignal<Row | null>(null);

  const update = (next: SettingObject[]) => setDraft(props.spec.key, next);
  const selected = () => (sel() === null ? undefined : rows()[sel()!]);

  const add = (kind: ToolKind) => {
    setChoosing(false);
    const k = TOOL_KINDS.find((x) => x.kind === kind)!;
    const used = new Set(entries().map((e) => e.alias));
    let alias = k.alias || "t";
    for (let n = 2; used.has(alias); n++) alias = `${k.alias || "t"}${n}`;
    setForm({ index: null, entry: { kind, name: k.label, alias, path: k.path, showConsoleIn: "toolWindow" } });
  };
  const edit = () => {
    const r = selected();
    if (r) setForm({ index: r.index, entry: { ...r.entry } });
  };
  const remove = () => {
    const r = selected();
    if (!r || r.index === null) return;
    update(entries().filter((_, j) => j !== r.index));
    setSel(null);
    setForm(null);
  };
  const toggle = (index: number, on: boolean) =>
    update(entries().map((e, j) => (j === index ? (on ? (({ enabled: _, ...rest }) => rest)(e) : { ...e, enabled: "false" }) : e)));

  const current = () => form()?.entry;
  const set = (key: string, value: string) => setForm((f) => f && { ...f, entry: { ...f.entry, [key]: value } });
  const problem = () => {
    const f = form();
    return f ? validateEntry(f.entry, entries().filter((_, j) => j !== f.index)) : null;
  };
  const save = () => {
    const f = form();
    if (!f || problem()) return;
    const entry: SettingObject = {};
    for (const [k, v] of Object.entries(f.entry)) if (v.trim() || k === "path") entry[k] = v.trim();
    const next = [...entries()];
    if (f.index === null) next.push(entry);
    else next[f.index] = entry;
    update(next);
    setSel(f.index ?? next.length - 1);
    setForm(null);
  };

  return (
    <div class="set-delims cli-tools-editor">
      <div class="set-list set-table-box cli-tools-table">
        <div class="set-list-toolbar">
          <button class="icon-btn" title="Add" data-testid="cli-tools-add" onClick={() => setChoosing((v) => !v)}>
            <Plus size={14} />
          </button>
          <button class="icon-btn" title="Remove" disabled={selected()?.index === null || sel() === null} onClick={remove}>
            <Minus size={14} />
          </button>
          <button class="icon-btn" title="Edit" disabled={sel() === null} onClick={edit}>
            <Pencil size={13} />
          </button>
        </div>
        <Show when={choosing()}>
          <div class="cli-tools-choose" role="listbox" aria-label="Choose Tool to Add">
            <div class="cli-tools-choose-title">Choose Tool to Add</div>
            <For each={TOOL_KINDS}>
              {(k) => (
                <div class="cli-tools-choose-item" role="option" data-tool-kind={k.kind} onClick={() => add(k.kind)}>
                  {k.label}
                </div>
              )}
            </For>
          </div>
        </Show>
        <table class="set-table">
          <thead>
            <tr>
              <th />
              <th>Name</th>
              <th>Alias</th>
              <th>Path</th>
              <th>Show console in</th>
            </tr>
          </thead>
          <tbody>
            <Index each={rows()}>
              {(row, i) => (
                <tr
                  classList={{ selected: sel() === i, "cli-tools-detected": row().index === null }}
                  data-alias={row().entry.alias}
                  title={row().index === null ? "Detected in the project. Edit it to change its settings." : undefined}
                  onClick={() => setSel(i)}
                  onDblClick={() => {
                    setSel(i);
                    edit();
                  }}
                >
                  <td>
                    <input
                      type="checkbox"
                      checked={row().entry.enabled !== "false"}
                      disabled={row().index === null}
                      title="Enabled"
                      onClick={(e) => e.stopPropagation()}
                      onChange={(e) => toggle(row().index!, e.currentTarget.checked)}
                    />
                  </td>
                  <td>
                    {row().entry.name?.trim() || toolKindLabel(row().entry.kind)}
                    <Show when={row().index === null}>
                      <span class="muted"> (auto-detected)</span>
                    </Show>
                  </td>
                  <td class="set-table-mono">{row().entry.alias}</td>
                  <td class="set-table-mono cli-tools-path">{row().entry.path}</td>
                  <td>{row().entry.showConsoleIn === "popup" ? "Pop-up window" : "Tool window"}</td>
                </tr>
              )}
            </Index>
          </tbody>
        </table>
        <Show when={!rows().length}>
          <div class="set-list-empty">No tools. Click + to add Composer, Artisan, Symfony Console, WP-CLI or another tool.</div>
        </Show>
      </div>

      <Show when={form()}>
        <div class="set-delims-form" data-testid="cli-tools-form">
          <div class="cli-tools-form-title">{toolKindLabel(current()!.kind)}</div>
          <div class="set-delims-grid">
            <label class="set-label">Name:</label>
            <input class="set-input" value={current()!.name ?? ""} onInput={(e) => set("name", e.currentTarget.value)} />
            <label class="set-label">Alias:</label>
            <input
              class="set-input cli-tools-alias"
              spellcheck={false}
              value={current()!.alias ?? ""}
              onInput={(e) => set("alias", e.currentTarget.value)}
            />
            <label class="set-label">{current()!.kind === "composer" ? "Path to composer:" : current()!.kind === "wp-cli" ? "Path to wp:" : "Path to script:"}</label>
            <input
              class="set-input set-table-mono"
              spellcheck={false}
              placeholder={TOOL_KINDS.find((k) => k.kind === current()!.kind)?.path || "bin/tool"}
              value={current()!.path ?? ""}
              onInput={(e) => set("path", e.currentTarget.value)}
            />
            <label class="set-label">PHP interpreter:</label>
            <select class="set-input set-select" value={current()!.interpreter ?? ""} onChange={(e) => set("interpreter", e.currentTarget.value)}>
              <option value="">Project default</option>
              <For each={interpreters()}>{(name) => <option value={name}>{name}</option>}</For>
            </select>
            <label class="set-label">Show console in:</label>
            <div class="cli-tools-radios">
              <label>
                <input
                  type="radio"
                  name="cli-console"
                  checked={current()!.showConsoleIn !== "popup"}
                  onChange={() => set("showConsoleIn", "toolWindow")}
                />{" "}
                Tool window
              </label>
              <label>
                <input type="radio" name="cli-console" checked={current()!.showConsoleIn === "popup"} onChange={() => set("showConsoleIn", "popup")} /> Pop-up
                window
              </label>
            </div>
          </div>
          <div class="set-delims-actions">
            <Show when={problem()}>
              <span class="set-error">{problem()}</span>
            </Show>
            <button class="btn-primary set-btn" disabled={!!problem()} onClick={save}>
              OK
            </button>
            <button class="btn-secondary set-btn" onClick={() => setForm(null)}>
              Cancel
            </button>
          </div>
        </div>
      </Show>
    </div>
  );
}
