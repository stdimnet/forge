/**
 * A settings page generated from the plugin settings schema: every setting whose `group`
 * is the page id, grouped by `section` under PHPStorm-style titled separators.
 */
import { createMemo, createSignal, For, Index, Match, Show, Switch, type JSX } from "solid-js";
import { ArrowDown, ArrowUp, Minus, Plus } from "lucide-solid";
import type { SettingSpec } from "@/ipc/settings";
import { settingsState } from "@/state/settings";
import { draft, draftValue, query, setDraft } from "./dialogState";
import { highlight, matchesQuery } from "./model";

/** Text with search hits highlighted. */
export function Hl(props: { text: string }) {
  return (
    <For each={highlight(props.text, query())}>
      {(s) => (s.hit ? <mark class="set-hl">{s.text}</mark> : s.text)}
    </For>
  );
}

/** Searchable text of a page's settings, used by the dialog's search. */
export function schemaText(pageId: string): string {
  return settingsState.schema
    .filter((s) => s.group === pageId)
    .map((s) => [s.title, s.description ?? "", s.section ?? "", s.key].join(" "))
    .join("\n");
}

function NumberInput(props: { spec: SettingSpec & { type: "int" | "number" } }) {
  const step = () => (props.spec.type === "number" ? (props.spec.step ?? 0.1) : 1);
  const [invalid, setInvalid] = createSignal(false);
  const commit = (raw: string) => {
    const n = props.spec.type === "int" ? Number.parseInt(raw, 10) : Number.parseFloat(raw);
    const { min, max } = props.spec;
    const ok = Number.isFinite(n) && (min === undefined || n >= min) && (max === undefined || n <= max);
    setInvalid(!ok);
    if (ok) setDraft(props.spec.key, n);
  };
  return (
    <input
      class="set-input set-number"
      classList={{ invalid: invalid() }}
      type="number"
      min={props.spec.min}
      max={props.spec.max}
      step={step()}
      value={draftValue<number>(props.spec.key)}
      onInput={(e) => commit(e.currentTarget.value)}
      title={
        props.spec.min !== undefined && props.spec.max !== undefined
          ? `${props.spec.min} – ${props.spec.max}`
          : undefined
      }
    />
  );
}

/** Editable list of strings with a PHPStorm-like +/−/↑/↓ toolbar. */
function ListEditor(props: { spec: SettingSpec }) {
  const items = () => draftValue<string[]>(props.spec.key) ?? [];
  const [sel, setSel] = createSignal<number | null>(null);
  let listEl!: HTMLDivElement;
  const update = (next: string[]) => setDraft(props.spec.key, next);
  const add = () => {
    update([...items(), ""]);
    setSel(items().length - 1);
    queueMicrotask(() => listEl.querySelector<HTMLInputElement>("input:last-of-type")?.focus());
  };
  const remove = () => {
    const i = sel();
    if (i === null) return;
    update(items().filter((_, j) => j !== i));
    setSel(items().length ? Math.min(i, items().length - 1) : null);
  };
  const move = (d: -1 | 1) => {
    const i = sel();
    if (i === null || i + d < 0 || i + d >= items().length) return;
    const next = [...items()];
    [next[i], next[i + d]] = [next[i + d], next[i]];
    update(next);
    setSel(i + d);
  };
  return (
    <div class="set-list">
      <div class="set-list-toolbar">
        <button class="icon-btn" title="Add" onClick={add}>
          <Plus size={14} />
        </button>
        <button class="icon-btn" title="Remove" disabled={sel() === null} onClick={remove}>
          <Minus size={14} />
        </button>
        <button class="icon-btn" title="Move Up" disabled={!sel()} onClick={() => move(-1)}>
          <ArrowUp size={14} />
        </button>
        <button
          class="icon-btn"
          title="Move Down"
          disabled={sel() === null || sel()! >= items().length - 1}
          onClick={() => move(1)}
        >
          <ArrowDown size={14} />
        </button>
      </div>
      <div class="set-list-items" ref={listEl}>
        <Index each={items()} fallback={<div class="set-list-empty">Nothing to show</div>}>
          {(item, i) => (
            <input
              class="set-list-item"
              classList={{ selected: sel() === i }}
              value={item()}
              spellcheck={false}
              onFocus={() => setSel(i)}
              onInput={(e) => update(items().map((v, j) => (j === i ? e.currentTarget.value : v)))}
            />
          )}
        </Index>
      </div>
    </div>
  );
}

function Control(props: { spec: SettingSpec }) {
  const spec = props.spec;
  return (
    <Switch>
      <Match when={spec.type === "enum" && spec}>
        {(s) => (
          <select
            class="set-input set-select"
            value={draftValue<string>(spec.key)}
            onChange={(e) => setDraft(spec.key, e.currentTarget.value)}
          >
            <For each={s().options}>{(o) => <option value={o.value}>{o.label}</option>}</For>
          </select>
        )}
      </Match>
      <Match when={(spec.type === "int" || spec.type === "number") && spec}>
        {(s) => <NumberInput spec={s() as SettingSpec & { type: "int" | "number" }} />}
      </Match>
      <Match when={spec.type === "string" || spec.type === "path"}>
        <input
          class="set-input set-text"
          classList={{ "set-path": spec.type === "path" }}
          spellcheck={false}
          value={draftValue<string>(spec.key)}
          placeholder={spec.type === "path" ? "Default" : undefined}
          onInput={(e) => setDraft(spec.key, e.currentTarget.value)}
        />
      </Match>
    </Switch>
  );
}

export function SettingRow(props: { spec: SettingSpec }) {
  const spec = props.spec;
  const hit = () =>
    query().trim() !== "" && matchesQuery(`${spec.title} ${spec.description ?? ""} ${spec.section ?? ""}`, query());
  const modified = () => spec.key in draft;
  const description = () =>
    spec.description ? (
      <div class="set-desc">
        <Hl text={spec.description} />
      </div>
    ) : null;
  return (
    <div class="set-row" classList={{ hit: hit(), modified: modified() }} data-setting={spec.key}>
      <Switch>
        <Match when={spec.type === "bool"}>
          <label class="set-check">
            <input
              type="checkbox"
              checked={draftValue<boolean>(spec.key)}
              onChange={(e) => setDraft(spec.key, e.currentTarget.checked)}
            />
            <span>
              <Hl text={spec.title} />
            </span>
          </label>
          <div class="set-indent">{description()}</div>
        </Match>
        <Match when={spec.type === "list"}>
          <div class="set-label set-label-block">
            <Hl text={spec.title} />:
          </div>
          {description()}
          <ListEditor spec={spec} />
        </Match>
        <Match when={true}>
          <div class="set-line">
            <label class="set-label">
              <Hl text={spec.title} />:
            </label>
            <Control spec={spec} />
          </div>
          {description()}
        </Match>
      </Switch>
    </div>
  );
}

export function SchemaPage(props: {
  pageId: string;
  /** Extra content at the top of a section (by section title), e.g. a detection status. */
  sectionHeader?: (section: string) => JSX.Element;
}) {
  const specs = createMemo(() =>
    settingsState.schema.filter((s) => s.group === props.pageId).sort((a, b) => a.order - b.order),
  );
  /** Consecutive runs of settings with the same section, in schema order. */
  const sections = createMemo(() => {
    const out: { title?: string; specs: SettingSpec[] }[] = [];
    for (const s of specs()) {
      const last = out[out.length - 1];
      if (last && last.title === s.section) last.specs.push(s);
      else out.push({ title: s.section, specs: [s] });
    }
    return out;
  });
  return (
    <div class="set-page">
      <Show
        when={specs().length}
        fallback={
          <div class="set-empty">
            {settingsState.loaded
              ? "No settings to show. The plugin that provides this page may be disabled."
              : "Loading…"}
          </div>
        }
      >
        <For each={sections()}>
          {(section) => (
            <section class="set-section">
              <Show when={section.title}>
                <div class="set-section-title">
                  <span>
                    <Hl text={section.title!} />
                  </span>
                </div>
              </Show>
              <div class="set-section-body" classList={{ indented: !!section.title }}>
                <Show when={section.title ? props.sectionHeader : undefined}>{(header) => header()(section.title!)}</Show>
                <For each={section.specs}>{(spec) => <SettingRow spec={spec} />}</For>
              </div>
            </section>
          )}
        </For>
      </Show>
    </div>
  );
}
