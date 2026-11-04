/**
 * PHPStorm's color picker: a saturation/brightness square, hue and opacity sliders, RGB or
 * HSB fields with opacity, a hex field and the recently used colors. Every change is written
 * into the document at once, in the notation the color was written in (Rust formats it:
 * `colors_format`). Enter or a click outside closes the picker, Escape restores the color.
 */
import { createMemo, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { createStore } from "solid-js/store";
import type { EditorView } from "@codemirror/view";
import { colorsApi, type Rgba } from "@/ipc/emmet";
import { getSetting, saveSetting } from "@/state/settings";
import { cssOf, hexOf, hsvToRgb, rgbToHsv, type Hsva } from "./hsv";
import { t as tr } from "@/i18n";

export const RECENT_KEY = "editor.colorPicker.recentColors";
const MAX_RECENT = 12;

export interface PickerTarget {
  view: EditorView;
  /** The color text in the document (`from === to` inserts a new color). */
  from: number;
  to: number;
  original: string;
  color: Rgba;
  /** Tailwind classes and other read-only colors only show the picker. */
  editable: boolean;
  x: number;
  y: number;
}

interface PickerState extends PickerTarget {
  hsva: Hsva;
  /** The text first written; Escape restores it. */
  initial: string;
  changed: boolean;
}

const [state, setState] = createStore<{ picker: PickerState | null }>({ picker: null });

export const colorPickerOpen = () => !!state.picker;

export function openColorPicker(t: PickerTarget): void {
  setState("picker", { ...t, hsva: rgbToHsv(t.color), initial: t.original, changed: false });
}

/** Write the color into the document in the original notation. */
function write(rgba: Rgba): void {
  const p = state.picker;
  if (!p || !p.editable) return;
  const text = colorsApi.formatSync({ color: rgba, original: p.original }) ?? hexOf(rgba);
  if (text === p.view.state.sliceDoc(p.from, p.to) && p.from !== p.to) return;
  p.view.dispatch({ changes: { from: p.from, to: p.to, insert: text }, userEvent: "input.color" });
  setState("picker", { to: p.from + text.length, original: text, changed: true });
}

export function closeColorPicker(restore = false): void {
  const p = state.picker;
  if (!p) return;
  if (restore && p.changed) {
    p.view.dispatch({ changes: { from: p.from, to: p.to, insert: p.initial }, userEvent: "input.color" });
  } else if (p.changed) {
    const hex = hexOf(hsvToRgb(p.hsva));
    const recent = getSetting<string[]>(RECENT_KEY, []) ?? [];
    void saveSetting(RECENT_KEY, [hex, ...recent.filter((c) => c !== hex)].slice(0, MAX_RECENT)).catch(() => undefined);
  }
  setState("picker", null);
  p.view.focus();
}

function setHsva(next: Hsva): void {
  setState("picker", "hsva", next);
  write(hsvToRgb(next));
}

/** Track a pointer drag over `el`, reporting the position as 0..1 fractions. */
function drag(el: HTMLElement, onMove: (x: number, y: number) => void) {
  return (e: PointerEvent) => {
    e.preventDefault();
    el.setPointerCapture?.(e.pointerId);
    const report = (ev: PointerEvent) => {
      const r = el.getBoundingClientRect();
      const x = r.width ? (ev.clientX - r.left) / r.width : 0;
      const y = r.height ? (ev.clientY - r.top) / r.height : 0;
      onMove(Math.min(1, Math.max(0, x)), Math.min(1, Math.max(0, y)));
    };
    report(e);
    const move = (ev: PointerEvent) => report(ev);
    const up = () => {
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerup", up);
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
  };
}

const round = (v: number) => Math.round(v);

function Picker(props: { picker: PickerState }) {
  let el!: HTMLDivElement;
  let sv!: HTMLDivElement;
  let hue!: HTMLDivElement;
  let alpha!: HTMLDivElement;
  const [mode, setMode] = createSignal<"RGB" | "HSB">("RGB");
  const [pos, setPos] = createSignal({ x: props.picker.x, y: props.picker.y });
  const hsva = () => props.picker.hsva;
  const rgba = createMemo(() => hsvToRgb(hsva()));
  const hex = createMemo(() => hexOf(rgba()));
  const recent = () => (getSetting<string[]>(RECENT_KEY, []) ?? []).slice(0, MAX_RECENT);

  const away = (e: MouseEvent) => {
    if (!el.contains(e.target as Node)) closeColorPicker();
  };
  onMount(() => {
    const r = el.getBoundingClientRect();
    const x = Math.max(8, Math.min(props.picker.x, window.innerWidth - r.width - 8));
    const y = props.picker.y + r.height > window.innerHeight - 8 ? Math.max(8, props.picker.y - r.height - 24) : props.picker.y;
    setPos({ x, y });
    window.addEventListener("mousedown", away, true);
    el.focus();
  });
  onCleanup(() => window.removeEventListener("mousedown", away, true));

  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Escape") closeColorPicker(true);
    else if (e.key === "Enter") {
      (document.activeElement as HTMLElement | null)?.blur?.();
      closeColorPicker();
    } else return;
    e.preventDefault();
    e.stopPropagation();
  };

  const setChannel = (i: number, raw: string) => {
    const n = Number(raw);
    if (!Number.isFinite(n)) return;
    if (mode() === "RGB") {
      const c = { ...rgba() };
      const v = Math.min(255, Math.max(0, n));
      if (i === 0) c.r = v;
      else if (i === 1) c.g = v;
      else c.b = v;
      const next = rgbToHsv(c);
      // Grays have no hue: keep the slider where it is.
      setHsva({ ...next, h: next.s === 0 ? hsva().h : next.h });
    } else {
      const next = { ...hsva() };
      if (i === 0) next.h = Math.min(360, Math.max(0, n)) % 360;
      else if (i === 1) next.s = Math.min(100, Math.max(0, n)) / 100;
      else next.v = Math.min(100, Math.max(0, n)) / 100;
      setHsva(next);
    }
  };
  const channels = () =>
    mode() === "RGB"
      ? [
          ["R", round(rgba().r), 255],
          ["G", round(rgba().g), 255],
          ["B", round(rgba().b), 255],
        ]
      : [
          ["H", round(hsva().h), 360],
          ["S", round(hsva().s * 100), 100],
          ["B", round(hsva().v * 100), 100],
        ];

  const setHex = (text: string) => {
    const parsed = colorsApi.parseSync(text);
    if (parsed) setHsva(rgbToHsv(parsed.color));
  };
  const pickRecent = (text: string) => setHex(text);

  const readOnly = () => !props.picker.editable;

  return (
    <div
      ref={el}
      class="color-picker"
      classList={{ "read-only": readOnly() }}
      role="dialog"
      aria-label={tr("tools.colors.colorPicker")}
      tabindex={-1}
      style={{ left: `${pos().x}px`, top: `${pos().y}px` }}
      onKeyDown={onKey}
    >
      <div
        ref={sv}
        class="cp-sv"
        style={{ "background-color": `hsl(${hsva().h}, 100%, 50%)` }}
        onPointerDown={(e) => !readOnly() && drag(sv, (x, y) => setHsva({ ...hsva(), s: x, v: 1 - y }))(e)}
      >
        <div class="cp-sv-white" />
        <div class="cp-sv-black" />
        <div class="cp-sv-thumb" style={{ left: `${hsva().s * 100}%`, top: `${(1 - hsva().v) * 100}%`, "background-color": cssOf({ ...rgba(), a: 1 }) }} />
      </div>
      <div class="cp-row">
        <div class="cp-preview" title={hex()}>
          <div class="cp-preview-color" style={{ "background-color": cssOf(rgba()) }} />
        </div>
        <div class="cp-sliders">
          <div ref={hue} class="cp-slider cp-hue" onPointerDown={(e) => !readOnly() && drag(hue, (x) => setHsva({ ...hsva(), h: x * 359.9 }))(e)}>
            <div class="cp-slider-thumb" style={{ left: `${(hsva().h / 360) * 100}%` }} />
          </div>
          <div ref={alpha} class="cp-slider cp-alpha" onPointerDown={(e) => !readOnly() && drag(alpha, (x) => setHsva({ ...hsva(), a: Math.round(x * 100) / 100 }))(e)}>
            <div class="cp-alpha-fill" style={{ background: `linear-gradient(to right, ${cssOf({ ...rgba(), a: 0 })}, ${cssOf({ ...rgba(), a: 1 })})` }} />
            <div class="cp-slider-thumb" style={{ left: `${hsva().a * 100}%` }} />
          </div>
        </div>
      </div>
      <div class="cp-fields">
        <select class="cp-mode" value={mode()} onChange={(e) => setMode(e.currentTarget.value as "RGB" | "HSB")} disabled={readOnly()}>
          <option value="RGB">RGB</option>
          <option value="HSB">HSB</option>
        </select>
        <For each={channels()}>
          {([label, value, max], i) => (
            <input
              class="cp-num"
              type="number"
              min={0}
              max={max as number}
              aria-label={label as string}
              title={label as string}
              value={value as number}
              disabled={readOnly()}
              onChange={(e) => setChannel(i(), e.currentTarget.value)}
            />
          )}
        </For>
        <input
          class="cp-num cp-alpha-num"
          type="number"
          min={0}
          max={100}
          aria-label={tr("tools.colors.opacity")}
          title={tr("tools.colors.opacity2")}
          value={round(hsva().a * 100)}
          disabled={readOnly()}
          onChange={(e) => setHsva({ ...hsva(), a: Math.min(100, Math.max(0, Number(e.currentTarget.value) || 0)) / 100 })}
        />
        <span class="cp-percent">%</span>
      </div>
      <div class="cp-fields">
        <span class="cp-label">{tr("tools.colors.hex")}</span>
        <input class="cp-hex" spellcheck={false} value={hex()} disabled={readOnly()} onChange={(e) => setHex(e.currentTarget.value)} />
      </div>
      <Show when={recent().length && !readOnly()}>
        <div class="cp-recent-title">{tr("tools.colors.recentlyUsed")}</div>
        <div class="cp-recent">
          <For each={recent()}>
            {(c) => (
              <button class="cp-recent-color" title={c} aria-label={c} onClick={() => pickRecent(c)}>
                <span style={{ "background-color": c }} />
              </button>
            )}
          </For>
        </div>
      </Show>
      <Show when={readOnly()}>
        <div class="cp-note">{props.picker.original} · {hex()}</div>
      </Show>
    </div>
  );
}

export function ColorPickerHost() {
  return (
    <Show when={state.picker} keyed>
      {(p) => <Picker picker={p} />}
    </Show>
  );
}
