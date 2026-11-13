/**
 * The value editor side panel of the data grid (DataGrip's Value Editor, Shift+Enter):
 * long text, JSON (formatted and validated), XML, binary (hex dump and image preview),
 * dates with a picker, enums and booleans, plus Set NULL / Set DEFAULT. The editor kind,
 * formatting, validation and hex dump come from Rust (`forge_dataedit::value`).
 */
import { createEffect, createMemo, createResource, createSignal, For, on, Show } from "solid-js";
import { AlignLeft, Check, Minimize2, X } from "lucide-solid";
import { isBinary, type Cell } from "@/ipc/db";
import { DEFAULT_MARKER, dataEditApi, type EditorKind, type ValueError } from "@/ipc/dataedit";
import { cellText, isDefault, parseInput, type GridColumn } from "./grid";
import { t as tr } from "@/i18n";

const base64OfHex = (hex: string) => {
  let bin = "";
  for (let i = 0; i + 1 < hex.length; i += 2) bin += String.fromCharCode(parseInt(hex.slice(i, i + 2), 16));
  return btoa(bin);
};

/** `2025-01-06 10:00:00` ↔ the value of `<input type="datetime-local">`. */
const pickerValue = (kind: EditorKind["kind"], text: string) => {
  if (kind === "date") return text.slice(0, 10);
  if (kind === "time") return text.slice(0, 8);
  return text.slice(0, 19).replace(" ", "T");
};

export function ValueEditor(props: {
  column: GridColumn;
  value: Cell | undefined;
  /** Row label for the header, e.g. `row 12`. */
  where: string;
  editable: boolean;
  onApply: (value: Cell) => void;
  onClose: () => void;
}) {
  const [kind] = createResource(
    () => ({ type: props.column.dataType ?? props.column.typeName, value: props.value ?? null }),
    ({ type, value }) => dataEditApi.editor(type, isDefault(value) ? null : value).catch((): EditorKind => ({ kind: "text" })),
  );
  const [text, setText] = createSignal("");
  const [error, setError] = createSignal<ValueError | null>(null);
  const [dirty, setDirty] = createSignal(false);

  const k = () => kind()?.kind ?? "text";
  const original = () => (props.value === undefined || props.value === null || isDefault(props.value) ? "" : cellText(props.value));

  // A new cell or a new kind: show its value (JSON and XML formatted).
  createEffect(
    on([() => props.value, kind], async () => {
      setDirty(false);
      setError(null);
      const t = original();
      setText(t);
      const kk = k();
      if ((kk === "json" || kk === "xml") && t.trim()) {
        const f = await dataEditApi.format(kk, t).catch(() => null);
        if (f?.text != null && !dirty()) setText(f.text);
        setError(f?.error ?? null);
      }
    }),
  );

  // Validate JSON / XML while typing.
  let timer: ReturnType<typeof setTimeout> | undefined;
  createEffect(
    on(text, (t) => {
      const kk = k();
      if (kk !== "json" && kk !== "xml") return;
      clearTimeout(timer);
      timer = setTimeout(() => {
        if (!t.trim()) return setError(null);
        void dataEditApi.format(kk, t).then((f) => setError(f.error ?? null));
      }, 150);
    }),
  );

  const [binary] = createResource(
    () => (props.value && isBinary(props.value) ? props.value : null),
    (v) => dataEditApi.binary(v, 4096),
  );

  const reformat = async (indent: number) => {
    const kk = k();
    if (kk !== "json" && kk !== "xml") return;
    const f = await dataEditApi.format(kk, text(), indent);
    if (f.text != null) {
      setText(f.text);
      setDirty(true);
    }
    setError(f.error ?? null);
  };

  const edit = (t: string) => {
    setText(t);
    setDirty(true);
  };

  const [applyError, setApplyError] = createSignal<string | null>(null);

  const apply = async () => {
    setApplyError(null);
    const kk = kind();
    const t = text();
    let v: Cell = t;
    if (!kk) return;
    if (kk.kind === "json" || kk.kind === "xml") {
      if (error()) return setApplyError("Fix the document first");
      // Store compact JSON when the cell held compact JSON.
      if (kk.kind === "json" && !original().includes("\n")) {
        const f = await dataEditApi.format("json", t, 0);
        if (f.text != null) v = f.text;
      }
    } else if (kk.kind === "date" || kk.kind === "dateTime" || kk.kind === "time") {
      const norm = await dataEditApi.temporal(kk, t);
      if (norm === null) return setApplyError(`Not a valid ${kk.kind === "dateTime" ? "date and time" : kk.kind}`);
      v = norm;
    } else if (kk.kind === "number") {
      v = parseInput(t, "number", props.value ?? null);
    } else if (kk.kind === "bool") {
      v = typeof props.value === "number" ? (t === "true" ? 1 : 0) : t === "true";
    }
    props.onApply(v);
    setDirty(false);
  };

  const enumValues = () => {
    const kk = kind();
    return kk?.kind === "enum" ? kk.values : [];
  };
  const multiple = () => {
    const kk = kind();
    return kk?.kind === "enum" && kk.multiple;
  };
  const setMembers = createMemo(() => new Set(text().split(",").filter(Boolean)));

  return (
    <div class="db-side db-value" data-testid="db-value-editor">
      <div class="db-side-head">
        <span class="db-side-title">{props.column.name}</span>
        <span class="muted db-value-type">
          {props.column.dataType ?? props.column.typeName} · {props.where}
        </span>
        <span class="db-spacer" />
        <Show when={k() === "json" || k() === "xml"}>
          <button class="icon-btn" title={tr("database.valueEditor.reformat")} onClick={() => void reformat(2)}>
            <AlignLeft size={14} />
          </button>
          <Show when={k() === "json"}>
            <button class="icon-btn" title={tr("database.valueEditor.minify")} onClick={() => void reformat(0)}>
              <Minimize2 size={14} />
            </button>
          </Show>
        </Show>
        <button class="icon-btn" title={tr("common.close")} onClick={props.onClose}>
          <X size={14} />
        </button>
      </div>
      <div class="db-side-body db-value-body">
        <Show when={isDefault(props.value)}>
          <div class="db-pending-note">{tr("tools.database.setToDefault", { value: props.column.defaultValue ?? "generated" })}</div>
        </Show>
        <Show when={props.value === null}>
          <div class="db-pending-note muted">{tr("database.valueEditor.theValueIsNull")}</div>
        </Show>
        <Show when={k() === "binary"}>
          <Show when={binary()}>
            {(b) => (
              <>
                <div class="muted db-value-meta">
                  {b().size} bytes{b().mime ? ` · ${b().mime}` : ""}
                </div>
                <Show when={b().mime && props.value && isBinary(props.value)}>
                  <div class="db-value-image">
                    <img alt={tr("database.valueEditor.imagePreview")} src={`data:${b().mime};base64,${base64OfHex((props.value as { $hex: string }).$hex)}`} />
                  </div>
                </Show>
                <div class="db-hex" data-testid="db-hex">
                  <For each={b().lines}>
                    {(l) => (
                      <div class="db-hex-line">
                        <span class="db-hex-off">{l.offset}</span>
                        <span class="db-hex-bytes">{l.hex}</span>
                        <span class="db-hex-ascii">{l.ascii}</span>
                      </div>
                    )}
                  </For>
                  <Show when={b().truncated}>
                    <div class="muted">…</div>
                  </Show>
                </div>
              </>
            )}
          </Show>
        </Show>
        <Show when={k() === "enum"}>
          <Show
            when={multiple()}
            fallback={
              <select class="db-value-select" disabled={!props.editable} value={text()} onChange={(e) => edit(e.currentTarget.value)}>
                <Show when={!enumValues().includes(text())}>
                  <option value={text()}>{text() || tr("tools.database.empty")}</option>
                </Show>
                <For each={enumValues()}>{(v) => <option value={v}>{v}</option>}</For>
              </select>
            }
          >
            <div class="db-value-set">
              <For each={enumValues()}>
                {(v) => (
                  <label>
                    <input
                      type="checkbox"
                      disabled={!props.editable}
                      checked={setMembers().has(v)}
                      onChange={(e) => {
                        const next = new Set(setMembers());
                        if (e.currentTarget.checked) next.add(v);
                        else next.delete(v);
                        edit(enumValues().filter((x) => next.has(x)).join(","));
                      }}
                    />
                    {v}
                  </label>
                )}
              </For>
            </div>
          </Show>
        </Show>
        <Show when={k() === "bool"}>
          <select class="db-value-select" disabled={!props.editable} value={text() === "1" || text() === "true" ? "true" : "false"} onChange={(e) => edit(e.currentTarget.value)}>
            <option value="true">true</option>
            <option value="false">false</option>
          </select>
        </Show>
        <Show when={k() === "date" || k() === "dateTime" || k() === "time"}>
          <div class="db-value-date">
            <input
              type={k() === "date" ? "date" : k() === "time" ? "time" : "datetime-local"}
              step="1"
              disabled={!props.editable}
              value={pickerValue(k(), text())}
              onInput={(e) => edit(e.currentTarget.value.replace("T", " "))}
            />
            <input class="db-value-input" spellcheck={false} disabled={!props.editable} value={text()} onInput={(e) => edit(e.currentTarget.value)} />
          </div>
        </Show>
        <Show when={k() === "number"}>
          <input class="db-value-input" spellcheck={false} disabled={!props.editable} value={text()} onInput={(e) => edit(e.currentTarget.value)} />
        </Show>
        <Show when={k() === "text" || k() === "json" || k() === "xml"}>
          <textarea
            class="db-value-text"
            classList={{ invalid: !!error() }}
            spellcheck={false}
            readOnly={!props.editable}
            value={text()}
            data-testid="db-value-text"
            onInput={(e) => edit(e.currentTarget.value)}
          />
        </Show>
        <Show when={error()}>
          {(e) => (
            <div class="db-value-error" data-testid="db-value-error">
              {tr("tools.database.lineColumn", { value: k() === "json" ? tr("database.valueEditor.invalidJson") : tr("database.valueEditor.invalidXml"), message: e().message, line: e().line, column: e().column })}
            </div>
          )}
        </Show>
        <Show when={(k() === "json" || k() === "xml") && !error() && text().trim()}>
          <div class="db-value-ok">
            <Check size={12} /> {tr("tools.database.valid", { value: k() === "json" ? "JSON" : "XML" })}
          </div>
        </Show>
        <Show when={applyError()}>
          <div class="db-value-error">{applyError()}</div>
        </Show>
      </div>
      <Show when={props.editable}>
        <div class="db-side-foot">
          <Show when={props.column.nullable !== false}>
            <button class="db-btn" onClick={() => props.onApply(null)}>
              {tr("tools.database.setNull")}
            </button>
          </Show>
          <Show when={props.column.defaultValue != null || props.column.autoIncrement}>
            <button class="db-btn" onClick={() => props.onApply(DEFAULT_MARKER)}>
              {tr("tools.database.setDefault")}
            </button>
          </Show>
          <span class="db-spacer" />
          <Show when={k() !== "binary"}>
            <button class="db-btn primary" disabled={!dirty()} onClick={() => void apply()}>
              {tr("common.apply")}
            </button>
          </Show>
        </div>
      </Show>
    </div>
  );
}
