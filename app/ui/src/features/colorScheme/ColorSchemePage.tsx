/**
 * Settings | Editor | Color Scheme — PHPStorm's page: scheme selector (follow the theme,
 * Ember, Anvil and user schemes) with Duplicate / Rename / Delete / Import / Export, the
 * attribute list, the editor of one attribute (foreground, background, bold, italic, effect)
 * and a live preview. Every value and style comes from Rust (`forge-colorscheme`); changes
 * are stored as they are made.
 */
import { For, Show, createMemo, createSignal, onMount, type JSX } from "solid-js";
import { ChevronDown, Copy, RotateCcw, Settings } from "lucide-solid";
import { isTauri } from "@/ipc";
import { colorSchemeApi, type AttributeView, type CssStyle, type Effect, type TextAttributes } from "@/ipc/colorScheme";
import { showBalloon } from "@/keymap";
import { askConfirm, askName } from "@/features/contextMenu/dialogs";
import { item, separator, showContextMenu, type MenuEntry } from "@/features/contextMenu/state";
import {
  changeColorScheme,
  colorSchemes,
  freeSchemeName,
  importColorScheme,
  refreshColorSchemes,
  schemeNameProblem,
} from "./store";
import { t } from "@/i18n";

const ask = (title: string, initial: string, confirm: string, except?: string) =>
  askName({ title, label: t("common.nameLabel"), initial, confirm, validate: (n) => schemeNameProblem(n, except) });

const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

/** Browser mode: read a file the user picks. */
function pickFileText(accept: string): Promise<string | null> {
  return new Promise((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = accept;
    input.onchange = () => {
      const file = input.files?.[0];
      if (!file) return resolve(null);
      void file.text().then(resolve, () => resolve(null));
    };
    input.click();
  });
}

const ICLS = { name: "IntelliJ color scheme", extensions: ["icls", "xml"] };

async function importScheme(): Promise<void> {
  try {
    if (isTauri()) {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const path = await open({ multiple: false, filters: [ICLS] });
      if (typeof path !== "string") return;
      await importColorScheme({ path });
    } else {
      const text = await pickFileText(".icls,.xml");
      if (text === null) return;
      await importColorScheme({ text });
    }
    showBalloon(t("tools.colorScheme.schemeImported", { value: colorSchemes()?.active ?? "" }));
  } catch (e) {
    showBalloon(t("common.importFailed"), message(e), 6000, undefined, { type: "error" });
  }
}

async function exportScheme(name: string): Promise<void> {
  const file = `${name}.icls`;
  try {
    if (isTauri()) {
      const { save } = await import("@tauri-apps/plugin-dialog");
      const path = await save({ defaultPath: file, filters: [ICLS] });
      if (!path) return;
      await colorSchemeApi.export(name, path);
      showBalloon(t("tools.colorScheme.schemeExported", { name }), path);
      return;
    }
    const text = await colorSchemeApi.export(name);
    const url = URL.createObjectURL(new Blob([text], { type: "application/xml" }));
    const a = document.createElement("a");
    a.href = url;
    a.download = file;
    a.click();
    URL.revokeObjectURL(url);
    showBalloon(t("tools.colorScheme.schemeExported", { name }), file);
  } catch (e) {
    showBalloon(t("common.exportFailed"), message(e), 6000, undefined, { type: "error" });
  }
}

/** Ask for a name, then run `then` with it (nothing when cancelled). */
function withName(title: string, initial: string, confirm: string, then: (name: string) => unknown, except?: string): void {
  void ask(title, initial, confirm, except).then((n) => {
    if (n) void then(n);
  });
}

const duplicate = (name: string) =>
  withName("Duplicate Scheme", freeSchemeName(`${name} copy`), "OK", (n) => changeColorScheme({ op: "duplicate", name: n }));

function schemeMenu(): MenuEntry[] {
  const s = colorSchemes();
  if (!s) return [];
  const name = s.active;
  const entries: MenuEntry[] = [item("Duplicate…", () => duplicate(name))];
  if (s.editable) {
    entries.push(
      item("Rename…", () => withName("Rename Scheme", name, "Rename", (n) => changeColorScheme({ op: "rename", scheme: name, name: n }), name)),
      item("Restore Defaults", () => void changeColorScheme({ op: "resetAll" }), undefined, { disabled: !s.modifiedCount }),
      item("Delete", () => {
        void askConfirm({ title: t("tools.colorScheme.deleteScheme"), message: t("tools.colorScheme.deleteTheColorScheme", { name }), confirm: t("common.delete") }).then((ok) => {
          if (ok) void changeColorScheme({ op: "delete", scheme: name });
        });
      }),
    );
  }
  entries.push(separator, item("Import Scheme…", () => void importScheme()), item("Export…", () => void exportScheme(name)));
  return entries;
}

/** Solid style object of a Rust `CssStyle`. */
const css = (s: CssStyle | undefined): JSX.CSSProperties => ({
  color: s?.color,
  "background-color": s?.backgroundColor,
  "font-weight": s?.fontWeight,
  "font-style": s?.fontStyle,
  "text-decoration": s?.textDecoration,
  "box-shadow": s?.boxShadow,
});

/** A checkbox that turns a color on, plus the color well and its hex value. */
function ColorRow(props: {
  label: string;
  value?: string;
  fallback: string;
  disabled: boolean;
  onChange: (color: string | undefined) => void;
}) {
  return (
    <div class="csch-color-row">
      <label class="set-check">
        <input
          type="checkbox"
          checked={!!props.value}
          disabled={props.disabled}
          onChange={(e) => props.onChange(e.currentTarget.checked ? props.fallback : undefined)}
        />
        <span>{props.label}</span>
      </label>
      <input
        type="color"
        class="csch-well"
        aria-label={`${props.label} color`}
        value={props.value ?? props.fallback}
        disabled={props.disabled || !props.value}
        onInput={(e) => props.onChange(e.currentTarget.value)}
      />
      <span class="csch-hex muted">{props.value ?? ""}</span>
    </div>
  );
}

function AttributeEditor(props: { attr: AttributeView }) {
  const s = () => colorSchemes();
  const editable = () => !!s()?.editable;
  const disabled = () => !editable();
  const value = () => props.attr.value;
  const text = () => s()?.attributes.find((a) => a.key === "TEXT")?.value;
  const set = (patch: Partial<TextAttributes>) => {
    const next: TextAttributes = { ...value(), ...patch };
    if (!next.effect) delete next.effectColor;
    void changeColorScheme({ op: "setAttribute", key: props.attr.key, value: next });
  };
  const f = () => props.attr.fields;
  return (
    <div class="csch-editor" data-attribute={props.attr.key}>
      <div class="csch-editor-title">{props.attr.title}</div>
      <Show when={f().font}>
        <div class="csch-font">
          <label class="set-check">
            <input type="checkbox" checked={!!value().bold} disabled={disabled()} onChange={(e) => set({ bold: e.currentTarget.checked })} />
            <span>{t("tools.colorScheme.bold")}</span>
          </label>
          <label class="set-check">
            <input type="checkbox" checked={!!value().italic} disabled={disabled()} onChange={(e) => set({ italic: e.currentTarget.checked })} />
            <span>{t("tools.colorScheme.italic")}</span>
          </label>
        </div>
      </Show>
      <Show when={f().foreground}>
        <ColorRow
          label={t("tools.colorScheme.foreground")}
          value={value().foreground}
          fallback={props.attr.parentValue.foreground ?? text()?.foreground ?? "#808080"}
          disabled={disabled()}
          onChange={(c) => set({ foreground: c })}
        />
      </Show>
      <Show when={f().background}>
        <ColorRow
          label={t("tools.colorScheme.background")}
          value={value().background}
          fallback={props.attr.parentValue.background ?? text()?.background ?? "#808080"}
          disabled={disabled()}
          onChange={(c) => set({ background: c })}
        />
      </Show>
      <Show when={f().effect}>
        <ColorRow
          label={t("tools.colorScheme.effects")}
          value={value().effect ? (value().effectColor ?? value().foreground ?? text()?.foreground) : undefined}
          fallback={value().foreground ?? text()?.foreground ?? "#808080"}
          disabled={disabled()}
          onChange={(c) => set(c ? { effect: value().effect ?? "underline", effectColor: c } : { effect: undefined })}
        />
        <div class="select-wrap csch-effect">
          <select
            aria-label={t("tools.colorScheme.effect")}
            value={value().effect ?? "underline"}
            disabled={disabled() || !value().effect}
            onChange={(e) => set({ effect: e.currentTarget.value as Effect })}
          >
            <For each={s()?.effects ?? []}>{(eff) => <option value={eff.id}>{eff.title}</option>}</For>
          </select>
          <ChevronDown size={14} class="select-chevron" />
        </div>
      </Show>
      <div class="csch-inherit muted">
        <Show
          when={editable()}
          fallback={
            <>
              <span>{t("tools.colorScheme.bundledSchemesCannotBeChanged")}</span>
              <button class="link-btn" onClick={() => duplicate(s()?.active ?? "")}>
                <Copy size={12} /> {t("tools.colorScheme.duplicateEllipsis")}
              </button>
            </>
          }
        >
          <Show when={!props.attr.inherited} fallback={<span>{t("tools.colorScheme.inheritedFrom", { base: s()?.base })}</span>}>
            <button class="link-btn" onClick={() => void changeColorScheme({ op: "setAttribute", key: props.attr.key })}>
              <RotateCcw size={12} /> {t("tools.colorScheme.resetTo", { base: s()?.base })}
            </button>
          </Show>
        </Show>
      </div>
    </div>
  );
}

function Preview(props: { onPick: (key: string) => void }) {
  const styles = createMemo(() => new Map((colorSchemes()?.attributes ?? []).map((a) => [a.key, a.style])));
  const bg = (key: string) => styles().get(key)?.backgroundColor;
  return (
    <div class="csch-preview" style={{ ...css(styles().get("TEXT")) }} onClick={() => props.onPick("TEXT")}>
      <For each={colorSchemes()?.preview ?? []}>
        {(line, i) => (
          <div class="csch-line" style={{ "background-color": line.caretRow ? bg("CARET_ROW") : undefined }}>
            <span
              class="csch-ln"
              style={{ color: styles().get("LINE_NUMBERS")?.color }}
              onClick={(e) => (e.stopPropagation(), props.onPick("LINE_NUMBERS"))}
            >
              {i() + 1}
            </span>
            <span class="csch-code">
              <For each={line.segments}>
                {(seg, j) => (
                  <>
                    <Show when={line.caretRow && j() === 1}>
                      <span
                        class="csch-caret"
                        style={{ "border-color": styles().get("CARET")?.color }}
                        onClick={(e) => (e.stopPropagation(), props.onPick("CARET"))}
                      />
                    </Show>
                    <span
                      style={{
                        ...css(seg.attribute ? styles().get(seg.attribute) : undefined),
                        ...(seg.overlay ? { "background-color": bg(seg.overlay) } : {}),
                      }}
                      onClick={(e) => {
                        e.stopPropagation();
                        props.onPick(seg.overlay ?? seg.attribute ?? (line.caretRow ? "CARET_ROW" : "TEXT"));
                      }}
                    >
                      {seg.text}
                    </span>
                  </>
                )}
              </For>
            </span>
          </div>
        )}
      </For>
    </div>
  );
}

export function ColorSchemePage() {
  const [selected, setSelected] = createSignal("KEYWORD");
  onMount(() => void refreshColorSchemes());
  const s = () => colorSchemes();
  const groups = createMemo(() => {
    const out: { name: string; attrs: AttributeView[] }[] = [];
    for (const a of s()?.attributes ?? []) {
      let g = out.find((x) => x.name === a.group);
      if (!g) out.push((g = { name: a.group, attrs: [] }));
      g.attrs.push(a);
    }
    return out;
  });
  const current = () => s()?.attributes.find((a) => a.key === selected());
  const followLabel = () => `Follow the theme (${s()?.followsTheme ? s()?.active : "Ember / Anvil"})`;
  return (
    <div class="color-scheme-page">
      <div class="csch-top">
        <label class="form-label" for="csch-scheme">
          {t("tools.colorScheme.schemeLabel")}
        </label>
        <div class="select-wrap">
          <select
            id="csch-scheme"
            value={s()?.followsTheme ? "" : (s()?.active ?? "")}
            onChange={(e) => void changeColorScheme({ op: "select", scheme: e.currentTarget.value })}
          >
            <option value="">{followLabel()}</option>
            <For each={(s()?.schemes ?? []).filter((x) => x.bundled)}>{(x) => <option value={x.name}>{x.name}</option>}</For>
            <Show when={(s()?.schemes ?? []).some((x) => !x.bundled)}>
              <optgroup label={t("tools.colorScheme.custom")}>
                <For each={(s()?.schemes ?? []).filter((x) => !x.bundled)}>{(x) => <option value={x.name}>{x.name}</option>}</For>
              </optgroup>
            </Show>
          </select>
          <ChevronDown size={14} class="select-chevron" />
        </div>
        <button
          class="icon-btn"
          title={t("tools.colorScheme.showSchemeActions")}
          onClick={(e) => {
            const r = e.currentTarget.getBoundingClientRect();
            showContextMenu(schemeMenu(), r.left, r.bottom + 2);
          }}
        >
          <Settings size={15} />
        </button>
        <Show when={s() && !s()!.schemes.find((x) => x.name === s()!.active)?.bundled}>
          <span class="muted csch-based">{t("tools.colorScheme.basedOn", { base: s()?.base })}</span>
        </Show>
        <Show when={s()?.modifiedCount}>
          <span class="muted csch-based">{s()?.modifiedCount} changed</span>
        </Show>
      </div>
      <div class="csch-main">
        <div class="csch-list" role="listbox" aria-label={t("tools.colorScheme.attributes")}>
          <For each={groups()}>
            {(g) => (
              <>
                <div class="csch-group">{g.name}</div>
                <For each={g.attrs}>
                  {(a) => (
                    <div
                      class="csch-item"
                      role="option"
                      aria-selected={selected() === a.key}
                      classList={{ selected: selected() === a.key }}
                      onClick={() => setSelected(a.key)}
                    >
                      <span class="csch-swatch" style={{ "background-color": a.value.foreground ?? a.value.background }} />
                      <span classList={{ "csch-changed": !a.inherited && !!s()?.editable }}>{a.title}</span>
                    </div>
                  )}
                </For>
              </>
            )}
          </For>
        </div>
        <Show when={current()}>{(a) => <AttributeEditor attr={a()} />}</Show>
      </div>
      <Preview onPick={setSelected} />
    </div>
  );
}
