/**
 * Run/Debug Configurations dialog, laid out like PHPStorm's: a toolbar (+ − copy, move) and
 * the configurations grouped by type on the left, the selected configuration's form on the
 * right (Name, Store as project file, type fields, interpreter, working directory,
 * environment variables, Before launch), OK / Cancel / Apply at the bottom.
 */
import { batch, createMemo, createSignal, For, Index, Match, onMount, Show, Switch } from "solid-js";
import { createStore, produce, unwrap } from "solid-js/store";
import { Dynamic } from "solid-js/web";
import {
  ArrowDown,
  ArrowUp,
  ChevronDown,
  ChevronRight,
  CircleAlert,
  CircleHelp,
  Copy,
  Feather,
  FolderOpen,
  Globe,
  Hexagon,
  Minus,
  Package,
  Pencil,
  Play,
  Plus,
  Settings2,
  SquareTerminal,
  Wrench,
  X,
} from "lucide-solid";
import { isTauri } from "@/ipc";
import type { BeforeLaunchTask, RunConfiguration } from "@/ipc/run";
import { getSetting } from "@/state/settings";
import { decodeInterpreters, findInterpreter, GLOBAL_KEY as INTERPRETERS_GLOBAL_KEY, PROJECT_KEY as INTERPRETERS_PROJECT_KEY } from "@/features/php/interpreters";
import { workspace } from "@/state/workspace";
import { usesFramework } from "@/state/project";
import { findTool, toolGroups, toolRef } from "@/features/externalTools/model";
import { configurations, saveConfigurations, selectedName, templateFor, templates } from "./configStore";
import { notifyDialogOpener } from "@/shell/window/dialogs";
import { closeRunConfigurations, RUN_CONFIGURATIONS_DIALOG, type DialogRequest } from "./dialogState";
import {
  fieldValue,
  findType,
  fieldOptions,
  isCommonField,
  newConfiguration,
  runConfigurationTypes,
  validateConfiguration,
  type IconComponent,
  type RunConfigField,
  type RunConfigurationTypeDef,
} from "./types";
import { openMenu } from "./PopupMenu";
import { RunOnRow } from "./RunOn";

interface Item {
  uid: number;
  cfg: RunConfiguration;
}

let uidSeq = 0;
const clone = <T,>(v: T): T => JSON.parse(JSON.stringify(v)) as T;

const [state, setState] = createStore<{
  items: Item[];
  templates: Item[];
  selected: number | null;
  /** "configs" or "templates" (Edit configuration templates…). */
  mode: "configs" | "templates";
  collapsed: string[];
  dirty: boolean;
  error: string | null;
}>({ items: [], templates: [], selected: null, mode: "configs", collapsed: [], dirty: false, error: null });

function init(select?: string) {
  const items = configurations().map((cfg) => ({ uid: ++uidSeq, cfg: clone(cfg) }));
  const tpl = templates().map((cfg) => ({ uid: ++uidSeq, cfg: clone(cfg) }));
  const want = select ?? selectedName() ?? undefined;
  setState({
    items,
    templates: tpl,
    selected: items.find((i) => i.cfg.name === want)?.uid ?? items[0]?.uid ?? null,
    mode: "configs",
    collapsed: [],
    dirty: false,
    error: null,
  });
}

const list = () => (state.mode === "configs" ? state.items : state.templates);
const current = () => list().find((i) => i.uid === state.selected);

function update(fn: (cfg: RunConfiguration) => void) {
  const uid = state.selected;
  if (uid === null) return;
  const key = state.mode === "configs" ? "items" : "templates";
  setState(
    key,
    (i) => i.uid === uid,
    "cfg",
    produce((c) => fn(c)),
  );
  setState({ dirty: true, error: null });
}

function setField(key: string, value: string) {
  update((c) => {
    if (key === "arguments") c.arguments = value;
    else if (key === "interpreterOptions") c.interpreterOptions = value;
    else c.options = { ...(c.options ?? {}), [key]: value };
  });
}

function uniqueName(base: string): string {
  const names = state.items.map((i) => i.cfg.name);
  if (!names.includes(base)) return base;
  for (let n = 1; ; n++) if (!names.includes(`${base} (${n})`)) return `${base} (${n})`;
}

/** Template of a type, created on first use in templates mode. */
function templateItem(type: RunConfigurationTypeDef): Item {
  let t = state.templates.find((i) => i.cfg.type === type.id);
  if (!t) {
    t = { uid: ++uidSeq, cfg: newConfiguration(type, type.id) };
    setState("templates", (l) => [...l, t!]);
  }
  return t;
}

function addConfiguration(type: RunConfigurationTypeDef) {
  const tpl = state.templates.find((i) => i.cfg.type === type.id)?.cfg ?? templateFor(type.id);
  const base = newConfiguration(type, "Unnamed");
  const cfg: RunConfiguration = tpl ? { ...clone(unwrap(tpl)), beforeLaunch: [...(tpl.beforeLaunch ?? [])] } : base;
  cfg.name = uniqueName("Unnamed");
  cfg.type = type.id;
  const item = { uid: ++uidSeq, cfg };
  batch(() => {
    setState("items", (l) => [...l, item]);
    setState({ selected: item.uid, mode: "configs", dirty: true, collapsed: state.collapsed.filter((c) => c !== type.id) });
  });
  queueMicrotask(() => document.querySelector<HTMLInputElement>(".rc-name-input")?.select());
}

function openAddMenu(e: MouseEvent) {
  const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
  openMenu({
    x: r.left,
    y: r.bottom + 4,
    items: runConfigurationTypes().map((t) => ({
      label: t.name,
      icon: t.icon,
      iconColor: t.color,
      run: () => (state.mode === "templates" ? setState("selected", templateItem(t).uid) : addConfiguration(t)),
    })),
  });
}

function removeSelected() {
  if (state.mode !== "configs" || state.selected === null) return;
  const idx = state.items.findIndex((i) => i.uid === state.selected);
  const rest = state.items.filter((i) => i.uid !== state.selected);
  setState({ items: rest, selected: rest[Math.min(idx, rest.length - 1)]?.uid ?? null, dirty: true, error: null });
}

function copySelected() {
  const it = current();
  if (!it || state.mode !== "configs") return;
  const item = { uid: ++uidSeq, cfg: { ...clone(unwrap(it.cfg)), name: uniqueName(it.cfg.name) } };
  setState({ items: [...state.items, item], selected: item.uid, dirty: true });
}

function moveSelected(delta: number) {
  const idx = state.items.findIndex((i) => i.uid === state.selected);
  const to = idx + delta;
  if (idx < 0 || to < 0 || to >= state.items.length) return;
  const next = [...state.items];
  [next[idx], next[to]] = [next[to], next[idx]];
  setState({ items: next, dirty: true });
}

function validateAll(): string | null {
  const seen = new Set<string>();
  for (const { cfg } of state.items) {
    if (!cfg.name.trim()) return "Configuration name is empty";
    if (seen.has(cfg.name)) return `Configuration "${cfg.name}" already exists`;
    seen.add(cfg.name);
  }
  return null;
}

async function apply(): Promise<boolean> {
  const problem = validateAll();
  if (problem) {
    setState("error", problem);
    return false;
  }
  const cfgs = state.items.map((i) => clone(unwrap(i.cfg)));
  const sel = current()?.cfg.name;
  await saveConfigurations(cfgs, state.mode === "configs" && sel ? sel : undefined, state.templates.map((i) => clone(unwrap(i.cfg))));
  setState("dirty", false);
  notifyDialogOpener(RUN_CONFIGURATIONS_DIALOG, "saved");
  return true;
}

// ---- Form building blocks ------------------------------------------------------------

async function browse(directory: boolean): Promise<string | null> {
  if (!isTauri()) return null;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const root = workspace()?.root;
  const picked = await open({ directory, multiple: false, defaultPath: root });
  if (typeof picked !== "string") return null;
  return root && picked.startsWith(`${root}/`) ? picked.slice(root.length + 1) : picked;
}

function PathInput(props: { value: string; placeholder?: string; directory?: boolean; onChange: (v: string) => void; name: string }) {
  return (
    <div class="rc-path">
      <input
        class="rc-input"
        name={props.name}
        value={props.value}
        placeholder={props.placeholder}
        onInput={(e) => props.onChange(e.currentTarget.value)}
      />
      <button
        class="icon-btn rc-browse"
        title={props.directory ? "Browse for a directory" : "Browse for a file"}
        onClick={async () => {
          const v = await browse(!!props.directory);
          if (v !== null) props.onChange(v);
        }}
      >
        <FolderOpen size={14} />
      </button>
    </div>
  );
}

function Field(props: { field: RunConfigField; cfg: RunConfiguration }) {
  const value = () => fieldValue(props.cfg, props.field.key) || (isCommonField(props.field.key) ? "" : (props.field.default ?? ""));
  const set = (v: string) => setField(props.field.key, v);
  return (
    <Show when={!props.field.visible || props.field.visible(props.cfg)}>
      <Switch
        fallback={
          <label class="rc-row">
            <span class="rc-label">{props.field.label}:</span>
            <input
              class="rc-input"
              name={props.field.key}
              value={value()}
              placeholder={props.field.placeholder}
              onInput={(e) => set(e.currentTarget.value)}
            />
          </label>
        }
      >
        <Match when={props.field.kind === "segmented"}>
          <div class="rc-row">
            <span class="rc-label">{props.field.label}:</span>
            <div class="rc-radios" role="radiogroup">
              <For each={fieldOptions(props.field)}>
                {(o) => (
                  <label class="rc-radio">
                    <input type="radio" name={`${props.field.key}`} checked={value() === o.value} onChange={() => set(o.value)} />
                    <span>{o.label}</span>
                  </label>
                )}
              </For>
            </div>
          </div>
        </Match>
        <Match when={props.field.kind === "enum"}>
          <label class="rc-row">
            <span class="rc-label">{props.field.label}:</span>
            <select class="rc-select" value={value()} onChange={(e) => set(e.currentTarget.value)}>
              <For each={fieldOptions(props.field)}>{(o) => <option value={o.value}>{o.label}</option>}</For>
            </select>
          </label>
        </Match>
        <Match when={props.field.kind === "bool"}>
          <label class="rc-check rc-indent">
            <input type="checkbox" checked={value() === "true"} onChange={(e) => set(String(e.currentTarget.checked))} />
            <span>{props.field.label}</span>
          </label>
        </Match>
        <Match when={props.field.kind === "path"}>
          <div class="rc-row">
            <span class="rc-label">{props.field.label}:</span>
            <PathInput name={props.field.key} value={value()} placeholder={props.field.placeholder} directory={props.field.directory} onChange={set} />
          </div>
        </Match>
        <Match when={props.field.kind === "text"}>
          <label class="rc-row top">
            <span class="rc-label">{props.field.label}:</span>
            <textarea
              class="rc-input rc-textarea"
              value={value()}
              placeholder={props.field.placeholder}
              rows={4}
              onInput={(e) => set(e.currentTarget.value)}
            />
          </label>
        </Match>
      </Switch>
    </Show>
  );
}

function InterpreterRow(props: { cfg: RunConfiguration; type: RunConfigurationTypeDef }) {
  const projectDefault = () =>
    props.type.defaultInterpreter?.() ||
    (props.type.interpreter === "php"
      ? getSetting<string>("php.interpreter", "php") || "php"
      : props.type.interpreter === "shell"
        ? "/bin/sh"
        : props.type.interpreter === "node"
          ? "node"
          : (newConfiguration(props.type, "").interpreter ?? ""));
  // PHP: the interpreters of Settings | PHP | CLI Interpreters, chosen by name.
  const phpInterpreters = () =>
    props.type.interpreter === "php" ? decodeInterpreters(getSetting(INTERPRETERS_GLOBAL_KEY, []), getSetting(INTERPRETERS_PROJECT_KEY, [])) : [];
  const named = () => findInterpreter(phpInterpreters(), props.cfg.interpreter ?? "");
  const custom = () => !!props.cfg.interpreter && !named();
  const label = () => (props.type.interpreter === "php" ? "Interpreter" : props.type.interpreter === "shell" ? "Interpreter path" : "Interpreter");
  const defaultLabel = () => {
    if (props.type.interpreter !== "php") return `Default (${projectDefault()})`;
    const i = findInterpreter(phpInterpreters(), projectDefault());
    return i ? `${i.name}${i.version ? ` (${i.version})` : ""} — project default` : `${projectDefault()} — project default`;
  };
  return (
    <>
      <div class="rc-row">
        <span class="rc-label">{label()}:</span>
        <div class="rc-interp">
          <select
            class="rc-select"
            data-testid="rc-interpreter"
            value={custom() ? "custom" : named() ? `name:${named()!.name}` : ""}
            onChange={(e) => {
              const v = e.currentTarget.value;
              update((c) => (c.interpreter = v === "custom" ? (findInterpreter(phpInterpreters(), projectDefault()) ? "php" : projectDefault() || "php") : v.startsWith("name:") ? v.slice(5) : ""));
            }}
          >
            <option value="">{defaultLabel()}</option>
            <For each={phpInterpreters()}>
              {(i) => (
                <option value={`name:${i.name}`}>
                  {i.name}
                  {i.version ? ` (${i.version})` : ""}
                </option>
              )}
            </For>
            <option value="custom">Custom…</option>
          </select>
          <Show when={custom()}>
            <PathInput name="interpreter" value={props.cfg.interpreter ?? ""} onChange={(v) => update((c) => (c.interpreter = v))} />
          </Show>
        </div>
      </div>
      <label class="rc-row">
        <span class="rc-label">Interpreter options:</span>
        <input
          class="rc-input"
          value={props.cfg.interpreterOptions ?? ""}
          placeholder={props.type.interpreter === "php" ? "-d memory_limit=-1" : ""}
          onInput={(e) => setField("interpreterOptions", e.currentTarget.value)}
        />
      </label>
    </>
  );
}

const envText = (env: Record<string, string> | undefined) =>
  Object.entries(env ?? {})
    .map(([k, v]) => `${k}=${v}`)
    .join(";");

function EnvEditor(props: { cfg: RunConfiguration }) {
  const [open, setOpen] = createSignal(false);
  const rows = () => Object.entries(props.cfg.env ?? {});
  const setRows = (next: [string, string][]) =>
    update((c) => {
      c.env = Object.fromEntries(next.filter(([k]) => k.trim() !== "" || true));
    });
  return (
    <div class="rc-row top">
      <span class="rc-label">Environment variables:</span>
      <div class="rc-env">
        <div class="rc-path">
          <input
            class="rc-input"
            value={envText(props.cfg.env)}
            placeholder="NAME=value;OTHER=value"
            onChange={(e) =>
              update((c) => {
                c.env = Object.fromEntries(
                  e.currentTarget.value
                    .split(";")
                    .map((p) => p.trim())
                    .filter(Boolean)
                    .map((p) => {
                      const i = p.indexOf("=");
                      return i < 0 ? [p, ""] : [p.slice(0, i).trim(), p.slice(i + 1)];
                    }),
                );
              })
            }
          />
          <button class="icon-btn rc-browse" classList={{ active: open() }} title="Edit environment variables" onClick={() => setOpen((o) => !o)}>
            <Pencil size={13} />
          </button>
        </div>
        <Show when={open()}>
          <div class="rc-env-table">
            <div class="rc-env-head">
              <span>Name</span>
              <span>Value</span>
              <span />
            </div>
            <For each={rows()}>
              {([k, v], i) => (
                <div class="rc-env-row">
                  <input
                    class="rc-input"
                    value={k}
                    onChange={(e) => {
                      const next = rows().map((r) => [...r] as [string, string]);
                      next[i()][0] = e.currentTarget.value;
                      setRows(next);
                    }}
                  />
                  <input
                    class="rc-input"
                    value={v}
                    onChange={(e) => {
                      const next = rows().map((r) => [...r] as [string, string]);
                      next[i()][1] = e.currentTarget.value;
                      setRows(next);
                    }}
                  />
                  <button class="icon-btn" title="Remove" onClick={() => setRows(rows().filter((_, j) => j !== i()))}>
                    <Minus size={13} />
                  </button>
                </div>
              )}
            </For>
            <button
              class="rc-env-add"
              onClick={() => {
                let n = 1;
                while ((props.cfg.env ?? {})[`NAME${n}`] !== undefined) n++;
                setRows([...rows(), [`NAME${n}`, ""]]);
              }}
            >
              <Plus size={13} /> Add variable
            </button>
          </div>
        </Show>
      </div>
    </div>
  );
}

/** Kinds of Before launch tasks with their label and the placeholder of their value. */
const TASK_KINDS: Record<string, { label: string; icon: IconComponent; placeholder?: string }> = {
  runConfiguration: { label: "Run", icon: Settings2 },
  externalTool: { label: "External tool", icon: Wrench },
  browser: { label: "Launch Web Browser", icon: Globe, placeholder: "http://localhost:8000" },
  npm: { label: "Run npm script", icon: Hexagon, placeholder: "build" },
  composer: { label: "Run Composer script", icon: Package, placeholder: "dump-autoload" },
  artisan: { label: "Run Artisan command", icon: Feather, placeholder: "migrate" },
  shell: { label: "Run", icon: SquareTerminal, placeholder: "composer dump-autoload" },
};

function BeforeLaunch(props: { cfg: RunConfiguration }) {
  const [open, setOpen] = createSignal(true);
  const [sel, setSel] = createSignal<number | null>(null);
  const tasks = () => props.cfg.beforeLaunch ?? [];
  const setTasks = (next: BeforeLaunchTask[]) => update((c) => (c.beforeLaunch = next));
  const add = (task: BeforeLaunchTask) => {
    setTasks([...tasks(), task]);
    setSel(tasks().length - 1);
    if (task.value === "") queueMicrotask(() => document.querySelector<HTMLInputElement>(".rc-bl-item.selected input")?.focus());
  };
  const move = (delta: number) => {
    const i = sel();
    const to = i === null ? -1 : i + delta;
    if (i === null || to < 0 || to >= tasks().length) return;
    const next = [...tasks()];
    [next[i], next[to]] = [next[to], next[i]];
    setTasks(next);
    setSel(to);
  };
  const addMenu = (e: MouseEvent) => {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const at = { x: r.left, y: r.bottom + 4 };
    const others = state.items.filter((i) => i.cfg.name !== props.cfg.name);
    const tools = toolGroups();
    openMenu({
      ...at,
      items: [
        {
          label: "Run External tool",
          icon: Wrench,
          run: () =>
            openMenu({
              ...at,
              items: tools.length
                ? tools.flatMap((g, gi) => [
                    ...(gi ? [{ separator: true }] : []),
                    ...g.tools.map((t) => ({ label: `${g.name} › ${t.name}`, icon: Wrench, run: () => add({ type: "externalTool", value: toolRef(t) }) })),
                  ])
                : [
                    {
                      label: "Configure External Tools…",
                      icon: Settings2,
                      run: () => void import("@/features/settings/dialogState").then((m) => m.openSettings("tools.external-tools")),
                    },
                  ],
            }),
        },
        {
          label: "Run Another Configuration",
          icon: Play,
          disabled: !others.length,
          run: () =>
            openMenu({
              ...at,
              items: others.map((o) => ({
                label: o.cfg.name,
                icon: findType(o.cfg.type)?.icon,
                iconColor: findType(o.cfg.type)?.color,
                run: () => add({ type: "runConfiguration", value: o.cfg.name }),
              })),
            }),
        },
        { label: "Launch Web Browser", icon: Globe, run: () => add({ type: "browser", value: "" }) },
        { label: "Run npm script", icon: Hexagon, run: () => add({ type: "npm", value: "" }) },
        { label: "Run Composer script", icon: Package, run: () => add({ type: "composer", value: "" }) },
        ...(usesFramework("laravel") || tasks().some((t) => t.type === "artisan")
          ? [{ label: "Run Artisan command", icon: Feather, run: () => add({ type: "artisan", value: "" }) }]
          : []),
        { separator: true },
        { label: "Run Shell Command…", icon: SquareTerminal, run: () => add({ type: "shell", value: "" }) },
      ],
    });
  };
  const setValue = (i: number, value: string) => {
    const next = tasks().map((x) => ({ ...x }));
    next[i].value = value;
    setTasks(next);
  };
  return (
    <div class="rc-section">
      <button class="rc-section-title" onClick={() => setOpen((o) => !o)}>
        {open() ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
        <span>Before launch</span>
        <Show when={tasks().length}>
          <span class="rc-count">{tasks().length}</span>
        </Show>
      </button>
      <Show when={open()}>
        <div class="rc-bl" data-testid="rc-before-launch">
          <div class="rc-bl-tools">
            <button class="icon-btn" title="Add" onClick={addMenu}>
              <Plus size={14} />
            </button>
            <button class="icon-btn" title="Remove" disabled={sel() === null} onClick={() => (setTasks(tasks().filter((_, i) => i !== sel())), setSel(null))}>
              <Minus size={14} />
            </button>
            <button class="icon-btn" title="Move Up" disabled={!sel()} onClick={() => move(-1)}>
              <ArrowUp size={14} />
            </button>
            <button class="icon-btn" title="Move Down" disabled={sel() === null || sel() === tasks().length - 1} onClick={() => move(1)}>
              <ArrowDown size={14} />
            </button>
          </div>
          <div class="rc-bl-list">
            {/* Index keeps rows (and the focused input) while values change. */}
            <Index each={tasks()} fallback={<div class="rc-bl-empty muted">There are no tasks to run before launch</div>}>
              {(t, i) => {
                const kind = () => TASK_KINDS[t().type];
                return (
                  <div class="rc-bl-item" classList={{ selected: sel() === i }} onClick={() => setSel(i)}>
                    <Dynamic
                      component={t().type === "runConfiguration" ? (findType(findConfigType(t().value))?.icon ?? Settings2) : (kind()?.icon ?? Wrench)}
                      size={13}
                    />
                    <Switch
                      fallback={
                        <>
                          <span class="rc-bl-kind">{kind()?.label ?? t().type}</span>
                          <input
                            class="rc-input rc-bl-input"
                            value={t().value}
                            placeholder={kind()?.placeholder}
                            onInput={(e) => setValue(i, e.currentTarget.value)}
                          />
                        </>
                      }
                    >
                      <Match when={t().type === "runConfiguration"}>
                        <span>Run '{t().value}'</span>
                      </Match>
                      <Match when={t().type === "externalTool"}>
                        <span class="rc-bl-kind">{kind()!.label}</span>
                        <select class="rc-select rc-bl-input" value={t().value} onChange={(e) => setValue(i, e.currentTarget.value)}>
                          <Show when={!findTool(t().value)}>
                            <option value={t().value}>{t().value || "Select a tool"} (not found)</option>
                          </Show>
                          <For each={toolGroups()}>
                            {(g) => (
                              <optgroup label={g.name}>
                                <For each={g.tools}>{(tool) => <option value={toolRef(tool)}>{tool.name}</option>}</For>
                              </optgroup>
                            )}
                          </For>
                        </select>
                      </Match>
                    </Switch>
                  </div>
                );
              }}
            </Index>
          </div>
        </div>
      </Show>
    </div>
  );
}

const findConfigType = (name: string) => state.items.find((i) => i.cfg.name === name)?.cfg.type ?? "";

function ConfigForm(props: { item: Item }) {
  const cfg = () => props.item.cfg;
  const type = () => findType(cfg().type);
  const problem = createMemo(() => (state.mode === "configs" ? validateConfiguration(cfg()) : null));
  return (
    <div class="rc-form">
      <Show when={state.mode === "configs"}>
        <div class="rc-name-row">
          <label class="rc-row grow">
            <span class="rc-label">Name:</span>
            <input class="rc-input rc-name-input" value={cfg().name} onInput={(e) => update((c) => (c.name = e.currentTarget.value))} />
          </label>
          <label class="rc-check">
            <input
              type="checkbox"
              checked={!!cfg().storeAsProjectFile}
              onChange={(e) => update((c) => (c.storeAsProjectFile = e.currentTarget.checked))}
            />
            <span>Store as project file</span>
          </label>
        </div>
      </Show>
      <Show when={state.mode === "templates"}>
        <div class="rc-template-note muted">
          Template for new <b>{type()?.name}</b> configurations.
        </div>
      </Show>
      <Show when={type()} fallback={<div class="rc-problem">Unknown configuration type "{cfg().type}". Is its plugin disabled?</div>}>
        {(t) => (
          <Show
            when={!t().form}
            fallback={<Dynamic component={t().form!} cfg={cfg()} update={update} configurations={state.items.map((i) => i.cfg)} />}
          >
            <Show when={!t().start && !t().noRunTarget}>
              <div class="rc-group rc-run-on">
                <RunOnRow cfg={cfg()} update={update} php={t().interpreter === "php"} />
              </div>
            </Show>
            <Show when={t().fields.length}>
              <div class="rc-group-title">{t().testRunner ? "Test Runner" : "Configuration"}</div>
              <div class="rc-group">
                <For each={t().fields}>{(f) => <Field field={f} cfg={cfg()} />}</For>
                <Show when={t().argumentsLabel !== false && t().argumentsLabel}>
                  <label class="rc-row">
                    <span class="rc-label">{t().argumentsLabel as string}:</span>
                    <input class="rc-input" value={cfg().arguments ?? ""} onInput={(e) => setField("arguments", e.currentTarget.value)} />
                  </label>
                </Show>
              </div>
            </Show>
            <div class="rc-group-title">Command Line</div>
            <div class="rc-group">
              <Show when={t().interpreter}>
                <InterpreterRow cfg={cfg()} type={t()} />
              </Show>
              <div class="rc-row">
                <span class="rc-label">Working directory:</span>
                <PathInput
                  name="workingDirectory"
                  directory
                  value={cfg().workingDirectory ?? ""}
                  placeholder={workspace()?.root ?? "Project root"}
                  onChange={(v) => update((c) => (c.workingDirectory = v))}
                />
              </div>
              <EnvEditor cfg={cfg()} />
            </div>
            <BeforeLaunch cfg={cfg()} />
          </Show>
        )}
      </Show>
      <Show when={problem()}>
        <div class="rc-problem">
          <CircleAlert size={14} />
          <span>Error: {problem()}</span>
        </div>
      </Show>
    </div>
  );
}

function EmptyForm() {
  return (
    <div class="rc-empty">
      <p class="muted">Click the + button to create a new configuration based on templates.</p>
      <div class="rc-quick">
        <For each={runConfigurationTypes().slice(0, 8)}>
          {(t) => (
            <button class="rc-quick-btn" onClick={() => addConfiguration(t)}>
              <span style={{ color: t.color }}>
                <Dynamic component={t.icon} size={15} />
              </span>
              {t.name}
            </button>
          )}
        </For>
      </div>
    </div>
  );
}

function Tree() {
  const groups = createMemo(() => {
    const types = runConfigurationTypes();
    const byType = new Map<string, Item[]>();
    for (const it of list()) byType.set(it.cfg.type, [...(byType.get(it.cfg.type) ?? []), it]);
    const ordered = types.filter((t) => byType.has(t.id)).map((t) => ({ id: t.id, type: t as RunConfigurationTypeDef | undefined, items: byType.get(t.id)! }));
    for (const [id, items] of byType) if (!types.some((t) => t.id === id)) ordered.push({ id, type: undefined, items });
    return ordered;
  });
  return (
    <div class="rc-tree" role="tree">
      <Show when={state.mode === "templates"}>
        <div class="rc-tree-root">
          <Settings2 size={14} /> Templates
        </div>
        <For each={runConfigurationTypes()}>
          {(t) => {
            const it = () => state.templates.find((i) => i.cfg.type === t.id);
            return (
              <div
                class="tree-row rc-tree-item"
                classList={{ selected: !!it() && state.selected === it()!.uid }}
                style={{ "padding-left": "26px" }}
                onClick={() => setState("selected", templateItem(t).uid)}
              >
                <span style={{ color: t.color }}>
                  <Dynamic component={t.icon} size={14} />
                </span>
                <span>{t.name}</span>
              </div>
            );
          }}
        </For>
      </Show>
      <Show when={state.mode === "configs"}>
        <For each={groups()} fallback={<div class="rc-tree-empty muted">No configurations added.</div>}>
          {(g) => {
            const collapsed = () => state.collapsed.includes(g.id);
            return (
              <>
                <div
                  class="tree-row rc-tree-group"
                  onClick={() =>
                    setState("collapsed", (c) => (c.includes(g.id) ? c.filter((x) => x !== g.id) : [...c, g.id]))
                  }
                >
                  {collapsed() ? <ChevronRight size={13} /> : <ChevronDown size={13} />}
                  <span style={{ color: g.type?.color }}>
                    <Dynamic component={g.type?.icon ?? Settings2} size={14} />
                  </span>
                  <span>{g.type?.name ?? g.id}</span>
                </div>
                <Show when={!collapsed()}>
                  <For each={g.items}>
                    {(it) => (
                      <div
                        class="tree-row rc-tree-item"
                        classList={{ selected: state.selected === it.uid, invalid: !!validateConfiguration(it.cfg) }}
                        data-config={it.cfg.name}
                        style={{ "padding-left": "40px" }}
                        onClick={() => setState("selected", it.uid)}
                      >
                        <span style={{ color: g.type?.color }}>
                          <Dynamic component={g.type?.icon ?? Settings2} size={14} />
                        </span>
                        <span class="rc-tree-name">{it.cfg.name || "Unnamed"}</span>
                        <Show when={validateConfiguration(it.cfg)}>
                          <X size={11} class="rc-tree-bad" />
                        </Show>
                      </div>
                    )}
                  </For>
                </Show>
              </>
            );
          }}
        </For>
      </Show>
    </div>
  );
}

/** Content of the Run/Debug Configurations dialog (the frame comes from `WindowFrame`). */
export function RunConfigurationsDialog(props: { props: DialogRequest }) {
  let el!: HTMLDivElement;
  init(props.props?.select);
  onMount(() => el.focus());
  const close = () => closeRunConfigurations();
  return (
    <div
      ref={el}
      class="rc-dialog"
      tabIndex={-1}
      onKeyDown={(e) => {
        if (e.key === "Escape") {
          e.stopPropagation();
          close();
        } else if (e.key === "Enter" && !(e.target as Element).matches("textarea, button")) {
          e.preventDefault();
          void apply().then((ok) => ok && close());
        }
      }}
    >
      <div class="rc-body">
        <div class="rc-side">
          <div class="rc-toolbar">
            <button class="icon-btn" title="Add New Configuration (Alt+Insert)" onClick={openAddMenu}>
              <Plus size={15} />
            </button>
            <button class="icon-btn" title="Remove Configuration" disabled={state.mode !== "configs" || state.selected === null} onClick={removeSelected}>
              <Minus size={15} />
            </button>
            <button class="icon-btn" title="Copy Configuration" disabled={state.mode !== "configs" || state.selected === null} onClick={copySelected}>
              <Copy size={14} />
            </button>
            <button class="icon-btn" title="Move Up" disabled={state.mode !== "configs"} onClick={() => moveSelected(-1)}>
              <ArrowUp size={14} />
            </button>
            <button class="icon-btn" title="Move Down" disabled={state.mode !== "configs"} onClick={() => moveSelected(1)}>
              <ArrowDown size={14} />
            </button>
          </div>
          <Tree />
          <button
            class="rc-templates-link link-btn"
            onClick={() =>
              setState({
                mode: state.mode === "configs" ? "templates" : "configs",
                selected: state.mode === "configs" ? null : (state.items[0]?.uid ?? null),
              })
            }
          >
            {state.mode === "configs" ? "Edit configuration templates…" : "Back to configurations"}
          </button>
        </div>
        <div class="rc-main">
          <Show when={current()} keyed fallback={state.mode === "configs" ? <EmptyForm /> : <div class="rc-empty muted">Select a template on the left.</div>}>
            {(it) => <ConfigForm item={it} />}
          </Show>
        </div>
      </div>
      <div class="dialog-buttons rc-buttons">
        <button class="icon-btn rc-help" title="Help">
          <CircleHelp size={16} />
        </button>
        <Show when={state.error}>
          <span class="rc-save-error">{state.error}</span>
        </Show>
        <span class="rc-spacer" />
        <button class="btn-primary" onClick={() => void apply().then((ok) => ok && close())}>
          OK
        </button>
        <button class="btn-secondary" onClick={close}>
          Cancel
        </button>
        <button class="btn-secondary" disabled={!state.dirty} onClick={() => void apply()}>
          Apply
        </button>
      </div>
    </div>
  );
}
