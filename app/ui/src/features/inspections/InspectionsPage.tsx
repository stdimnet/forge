/**
 * Settings | Editor | Inspections, like PHPStorm: the profile box with its ⚙ menu (Duplicate,
 * Rename, Copy to Project / IDE, Delete, Restore Defaults, Export, Import, custom
 * severities), the inspections by language and group with search and checkboxes, and the
 * selected inspection's description, severity, scope-specific settings and options. The
 * tree and every edit are computed in Rust (`forge-inspections`).
 */
import { createMemo, createSignal, For, onMount, Show } from "solid-js";
import { ChevronDown, ChevronRight, Plus, Search, Settings, Trash2 } from "lucide-solid";
import { isTauri } from "@/ipc";
import {
  inspectionsApi,
  type InspectionOption,
  type Profile,
  type ScopeSettings,
  type ToolOp,
  type ViewGroup,
  type ViewLeaf,
} from "@/ipc/inspections";
import { showBalloon } from "@/keymap";
import { showContextMenu, item, separator, submenu, type MenuEntry } from "@/features/contextMenu/state";
import { askChoice, askConfirm, askName } from "@/features/contextMenu/dialogs";
import { takePageArgs } from "@/features/settings/dialogState";
import { scopeEntries } from "@/features/scopes/model";
import { beginDraft, change, draft, INSPECTIONS_PAGE } from "./draft";
import { profileText, profileView, selectedProfile, severities, severityName, specOf } from "./model";

const [query, setQuery] = createSignal("");
const [selected, setSelected] = createSignal<string | null>(null);
const [collapsed, setCollapsed] = createSignal<Set<string>>(new Set());

const current = (): Profile | undefined => selectedProfile(draft());

const levelLabel = (p: Profile) => (p.level === "project" ? "Project" : "IDE");

function freeName(base: string): string {
  const names = new Set(draft()?.profiles.map((p) => p.name) ?? []);
  if (!names.has(base)) return base;
  for (let i = 2; ; i++) if (!names.has(`${base} (${i})`)) return `${base} (${i})`;
}

function nameProblem(name: string, except?: string): string | null {
  const n = name.trim();
  if (!n) return "The profile name is empty";
  if (n !== except && draft()?.profiles.some((p) => p.name === n)) return `A profile named '${n}' already exists`;
  return null;
}

const askProfileName = (title: string, initial: string, confirm: string, except?: string) =>
  askName({ title, label: "Name:", initial, confirm, validate: (n) => nameProblem(n, except) });

async function exportProfile(format: "xml" | "json"): Promise<void> {
  const p = current();
  if (!p) return;
  const file = `${p.name.replace(/[^\w.-]+/g, "_")}.${format}`;
  try {
    if (isTauri()) {
      const { save } = await import("@tauri-apps/plugin-dialog");
      const path = await save({ defaultPath: file, filters: [{ name: format === "xml" ? "Inspection profile XML" : "Forge inspection profile JSON", extensions: [format] }] });
      if (!path) return;
      await inspectionsApi.exportProfile(p, format, path);
      showBalloon(`Profile "${p.name}" exported`, path);
      return;
    }
    const url = URL.createObjectURL(new Blob([profileText(p, format)], { type: format === "xml" ? "application/xml" : "application/json" }));
    const a = document.createElement("a");
    a.href = url;
    a.download = file;
    a.click();
    URL.revokeObjectURL(url);
    showBalloon(`Profile "${p.name}" exported`, file);
  } catch (e) {
    showBalloon("Export failed", e instanceof Error ? e.message : String(e));
  }
}

function pickFileText(accept: string): Promise<string | null> {
  return new Promise((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = accept;
    input.onchange = () => {
      const f = input.files?.[0];
      if (!f) return resolve(null);
      void f.text().then(resolve, () => resolve(null));
    };
    input.click();
  });
}

async function importProfile(): Promise<void> {
  try {
    let source: { text?: string; path?: string };
    if (isTauri()) {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const path = await open({ multiple: false, filters: [{ name: "Inspection profile", extensions: ["xml", "json"] }] });
      if (typeof path !== "string") return;
      source = { path };
    } else {
      const text = await pickFileText(".xml,.json");
      if (text === null) return;
      source = { text };
    }
    const profile = await inspectionsApi.importProfile(source);
    if (change({ op: "add", profile })) showBalloon(`Profile "${profile.name}" imported`, "Apply the settings to keep it.");
  } catch (e) {
    showBalloon("Import failed", e instanceof Error ? e.message : String(e));
  }
}

async function addSeverity(): Promise<void> {
  const name = await askName({ title: "New Highlighting Severity", label: "Name:", initial: "", confirm: "OK", validate: (n) => (n.trim() ? null : "The name is empty") });
  if (!name) return;
  const look = await askChoice({
    title: "New Highlighting Severity",
    message: `Highlight "${name}" problems in the editor like:`,
    choices: [
      { id: "info", label: "Info" },
      { id: "weak", label: "Weak Warning" },
      { id: "warning", label: "Warning" },
      { id: "error", label: "Error" },
    ],
  });
  if (!look) return;
  const custom = draft()?.severities ?? [];
  change({ op: "setSeverities", severities: [...custom, { id: name, name, editor: look as "error", custom: true }] });
}

function profileMenu(): MenuEntry[] {
  const p = current();
  const d = draft();
  if (!p || !d) return [];
  const builtin = p.name === "Default" || p.name === "Project Default";
  const entries: MenuEntry[] = [
    item("Duplicate…", () => void askProfileName("Duplicate Profile", freeName(`${p.name} copy`), "OK").then((n) => n && change({ op: "copy", from: p.name, name: n.trim(), level: p.level }))),
  ];
  if (p.level === "ide" && d.hasProject) {
    entries.push(item("Copy to Project…", () => void askProfileName("Copy to Project", freeName(p.name === "Default" ? "Project Copy" : p.name), "OK").then((n) => n && change({ op: "copy", from: p.name, name: n.trim(), level: "project" }))));
  }
  if (p.level === "project") {
    entries.push(item("Copy to IDE…", () => void askProfileName("Copy to IDE", freeName(`${p.name} (IDE)`), "OK").then((n) => n && change({ op: "copy", from: p.name, name: n.trim(), level: "ide" }))));
  }
  entries.push(
    item("Rename…", () => void askProfileName("Rename Profile", p.name, "Rename", p.name).then((n) => n && change({ op: "rename", from: p.name, name: n.trim() })), undefined, { disabled: builtin }),
    item(
      "Delete",
      () =>
        void askConfirm({ title: "Delete Profile", message: `Delete the profile "${p.name}"?`, confirm: "Delete" }).then(
          (ok) => ok && change({ op: "delete", name: p.name }),
        ),
      undefined,
      { disabled: builtin },
    ),
    separator,
    item("Restore Defaults", () => change({ op: "reset", name: p.name })),
    separator,
    submenu("Export", [item("Inspection Profile XML…", () => void exportProfile("xml")), item("Forge Profile JSON…", () => void exportProfile("json"))]),
    item("Import Profile…", () => void importProfile()),
    separator,
    item("Add Custom Severity…", () => void addSeverity()),
  );
  return entries;
}

function GroupCheck(props: { group: ViewGroup }) {
  return (
    <input
      type="checkbox"
      class="insp-check"
      checked={props.group.check === "all"}
      ref={(el) => queueMicrotask(() => (el.indeterminate = props.group.check === "some"))}
      onClick={(e) => e.stopPropagation()}
      onChange={(e) => {
        const p = current();
        if (p) change({ op: "edit", name: p.name, edit: { op: "setEnabled", ids: props.group.ids, enabled: e.currentTarget.checked } });
      }}
      aria-label={`Enable ${props.group.name}`}
    />
  );
}

function Leaf(props: { leaf: ViewLeaf; depth: number }) {
  return (
    <div
      class="insp-row"
      classList={{ selected: selected() === props.leaf.id }}
      style={{ "padding-left": `${8 + props.depth * 16 + 16}px` }}
      onClick={() => setSelected(props.leaf.id)}
      data-inspection={props.leaf.id}
    >
      <input
        type="checkbox"
        class="insp-check"
        checked={props.leaf.enabled}
        onClick={(e) => e.stopPropagation()}
        onChange={(e) => {
          const p = current();
          if (p) change({ op: "edit", name: p.name, edit: { op: "setEnabled", ids: [props.leaf.id], enabled: e.currentTarget.checked } });
        }}
        aria-label={`Enable ${props.leaf.name}`}
      />
      <span class="insp-name" classList={{ modified: props.leaf.modified }}>
        {props.leaf.name}
      </span>
    </div>
  );
}

function Group(props: { group: ViewGroup; depth: number }) {
  const open = () => query().trim() !== "" || !collapsed().has(props.group.path);
  const toggle = () =>
    setCollapsed((prev) => {
      const next = new Set(prev);
      next.has(props.group.path) ? next.delete(props.group.path) : next.add(props.group.path);
      return next;
    });
  return (
    <>
      <div class="insp-row insp-group" style={{ "padding-left": `${8 + props.depth * 16}px` }} onClick={toggle} data-group={props.group.path}>
        <span class="insp-chevron">{open() ? <ChevronDown size={14} /> : <ChevronRight size={14} />}</span>
        <GroupCheck group={props.group} />
        <span class="insp-name" classList={{ modified: props.group.modified }}>
          {props.group.name}
        </span>
      </div>
      <Show when={open()}>
        <For each={props.group.groups}>{(g) => <Group group={g} depth={props.depth + 1} />}</For>
        <For each={props.group.inspections}>{(l) => <Leaf leaf={l} depth={props.depth} />}</For>
      </Show>
    </>
  );
}

function SeveritySelect(props: { value: string | null | undefined; allowReported: boolean; onChange: (v: string) => void; label: string }) {
  return (
    <select class="set-input set-select insp-severity" value={props.value ?? ""} aria-label={props.label} onChange={(e) => props.onChange(e.currentTarget.value)}>
      <Show when={props.allowReported}>
        <option value="">As reported by the tool</option>
      </Show>
      <For each={severities(draft()?.severities ?? [])}>{(s) => <option value={s.id}>{s.name}</option>}</For>
    </select>
  );
}

function OptionEditor(props: { id: string; option: InspectionOption; value: unknown }) {
  const set = (value: unknown) => {
    const p = current();
    if (p) change({ op: "edit", name: p.name, edit: { op: "setOption", id: props.id, option: props.option.id, value } });
  };
  return (
    <label class="insp-option">
      <Show
        when={props.option.type === "bool"}
        fallback={
          <>
            <span>{props.option.label}:</span>
            <Show
              when={props.option.type === "enum"}
              fallback={
                <input
                  class="set-input insp-option-input"
                  type={props.option.type === "int" ? "number" : "text"}
                  min={props.option.min}
                  max={props.option.max}
                  value={String(props.value ?? "")}
                  onChange={(e) => set(props.option.type === "int" ? Number(e.currentTarget.value) : e.currentTarget.value)}
                />
              }
            >
              <select class="set-input set-select" value={String(props.value ?? "")} onChange={(e) => set(e.currentTarget.value)}>
                <For each={props.option.choices ?? []}>{(c) => <option value={c.value}>{c.label}</option>}</For>
              </select>
            </Show>
          </>
        }
      >
        <input type="checkbox" checked={props.value === true} onChange={(e) => set(e.currentTarget.checked)} />
        <span>{props.option.label}</span>
      </Show>
    </label>
  );
}

function Details(props: { id: string }) {
  const spec = () => specOf(props.id);
  const tool = () => current()?.tools[props.id];
  const enabled = () => tool()?.enabled ?? spec()?.enabled ?? true;
  const level = () => tool()?.severity ?? (spec()?.external ? null : spec()?.severity);
  const scopes = (): ScopeSettings[] => tool()?.scopes ?? [];
  const options = () => {
    const set = tool()?.options ?? {};
    return (spec()?.options ?? []).map((o) => ({ option: o, value: o.id in set ? set[o.id] : o.default }));
  };
  const edit = (edit: ToolOp) => {
    const p = current();
    if (p) change({ op: "edit", name: p.name, edit });
  };
  const setScopes = (list: ScopeSettings[]) => edit({ op: "setScopes", id: props.id, scopes: list });
  const scopeNames = () => scopeEntries().entries.map((e) => e.name);
  const addScope = (e: MouseEvent) => {
    const used = new Set(scopes().map((s) => s.scope));
    const entries = scopeNames()
      .filter((n) => !used.has(n))
      .map((n) => item(n, () => setScopes([...scopes(), { scope: n, enabled: true, severity: level() ?? undefined }])));
    if (!entries.length) return;
    showContextMenu(entries, e.clientX, e.clientY);
  };
  return (
    <Show when={spec()}>
      {(s) => (
        <div class="insp-details">
          <div class="insp-details-title">{s().name}</div>
          <div class="insp-details-meta">
            <code>{s().id}</code>
            <span>{s().group.replace(/\//g, " | ")}</span>
            <Show when={s().external}>
              <span class="insp-tag">External tool</span>
            </Show>
            <Show when={s().cleanup}>
              <span class="insp-tag">Code Cleanup</span>
            </Show>
            <Show when={s().batch === false}>
              <span class="insp-tag">Editor only</span>
            </Show>
          </div>
          <div class="insp-row-form">
            <label class="insp-enabled">
              <input type="checkbox" checked={enabled()} onChange={(e) => edit({ op: "setEnabled", ids: [s().id], enabled: e.currentTarget.checked })} />
              Enabled
            </label>
            <span class="insp-form-label">Severity:</span>
            <SeveritySelect
              value={level()}
              allowReported={!!s().external}
              label="Severity"
              onChange={(v) => (v ? edit({ op: "setSeverity", ids: [s().id], severity: v }) : edit({ op: "reset", ids: [s().id] }))}
            />
            <Show when={current()?.tools[s().id]}>
              <button class="link-btn insp-reset" onClick={() => edit({ op: "reset", ids: [s().id] })}>
                Reset to default
              </button>
            </Show>
          </div>
          <div class="insp-section">
            <div class="insp-section-head">
              <span>Scopes</span>
              <button class="icon-btn" title="Add Scope" aria-label="Add Scope" onClick={addScope}>
                <Plus size={14} />
              </button>
            </div>
            <table class="insp-scopes">
              <tbody>
                <For each={scopes()}>
                  {(sc, i) => (
                    <tr>
                      <td>
                        <input
                          type="checkbox"
                          checked={sc.enabled}
                          aria-label={`Enabled in ${sc.scope}`}
                          onChange={(e) => setScopes(scopes().map((x, j) => (j === i() ? { ...x, enabled: e.currentTarget.checked } : x)))}
                        />
                      </td>
                      <td class="insp-scope-name">{sc.scope}</td>
                      <td>
                        <SeveritySelect
                          value={sc.severity ?? level()}
                          allowReported={!!s().external}
                          label={`Severity in ${sc.scope}`}
                          onChange={(v) => setScopes(scopes().map((x, j) => (j === i() ? { ...x, severity: v || undefined } : x)))}
                        />
                      </td>
                      <td>
                        <button class="icon-btn" title="Remove Scope" aria-label={`Remove ${sc.scope}`} onClick={() => setScopes(scopes().filter((_, j) => j !== i()))}>
                          <Trash2 size={13} />
                        </button>
                      </td>
                    </tr>
                  )}
                </For>
                <tr class="insp-everywhere">
                  <td>
                    <input type="checkbox" checked={enabled()} aria-label="Enabled everywhere else" onChange={(e) => edit({ op: "setEnabled", ids: [s().id], enabled: e.currentTarget.checked })} />
                  </td>
                  <td class="insp-scope-name">{scopes().length ? "Everywhere else" : "In All Scopes"}</td>
                  <td>{severityName(level(), draft()?.severities)}</td>
                  <td />
                </tr>
              </tbody>
            </table>
          </div>
          <Show when={options().length}>
            <div class="insp-section">
              <div class="insp-section-head">
                <span>Options</span>
              </div>
              <For each={options()}>{(o) => <OptionEditor id={s().id} option={o.option} value={o.value} />}</For>
            </div>
          </Show>
          <Show when={s().description}>
            <div class="insp-section">
              <div class="insp-section-head">
                <span>Description</span>
              </div>
              <p class="insp-description">{s().description}</p>
            </div>
          </Show>
        </div>
      )}
    </Show>
  );
}

export function InspectionsPage() {
  onMount(() => {
    void beginDraft().then(() => {
      const arg = takePageArgs<{ inspection?: string }>(INSPECTIONS_PAGE);
      if (arg?.inspection) {
        setSelected(arg.inspection);
        setQuery("");
      }
    });
  });
  const tree = createMemo(() => {
    const p = current();
    return p ? profileView(p, query()) : null;
  });
  const ide = () => draft()?.profiles.filter((p) => p.level === "ide") ?? [];
  const project = () => draft()?.profiles.filter((p) => p.level === "project") ?? [];
  return (
    <div class="settings-page insp-page" data-page={INSPECTIONS_PAGE}>
      <div class="insp-profile-bar">
        <label class="insp-form-label" for="insp-profile">
          Profile:
        </label>
        <select
          id="insp-profile"
          class="set-input set-select insp-profile-select"
          value={draft()?.selected ?? ""}
          onChange={(e) => change({ op: "select", name: e.currentTarget.value })}
        >
          <Show when={project().length}>
            <optgroup label="Stored in Project">
              <For each={project()}>{(p) => <option value={p.name}>{p.name}</option>}</For>
            </optgroup>
          </Show>
          <optgroup label="Stored in IDE">
            <For each={ide()}>{(p) => <option value={p.name}>{p.name}</option>}</For>
          </optgroup>
        </select>
        <Show when={current()}>{(p) => <span class="insp-level">{levelLabel(p())}</span>}</Show>
        <button class="icon-btn insp-gear" title="Show Profile Actions" aria-label="Show Profile Actions" onClick={(e) => showContextMenu(profileMenu(), e.clientX, e.clientY)}>
          <Settings size={15} />
        </button>
      </div>
      <div class="insp-split">
        <div class="insp-left">
          <div class="insp-search">
            <Search size={13} />
            <input class="insp-search-input" placeholder="Search inspections" value={query()} onInput={(e) => setQuery(e.currentTarget.value)} spellcheck={false} />
          </div>
          <div class="insp-tree" role="tree">
            <Show when={tree()} fallback={<div class="insp-empty">Loading…</div>}>
              {(t) => (
                <>
                  <For each={t().groups}>{(g) => <Group group={g} depth={0} />}</For>
                  <For each={t().inspections}>{(l) => <Leaf leaf={l} depth={-1} />}</For>
                  <Show when={!t().groups.length && !t().inspections.length}>
                    <div class="insp-empty">Nothing found</div>
                  </Show>
                </>
              )}
            </Show>
          </div>
        </div>
        <div class="insp-right">
          <Show when={selected()} fallback={<div class="insp-empty">Select an inspection to see its description and settings.</div>}>
            {(id) => <Details id={id()} />}
          </Show>
        </div>
      </div>
    </div>
  );
}
