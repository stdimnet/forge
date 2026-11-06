/**
 * Tool windows that are open but not docked (View Mode): Undock slides the window over the
 * editor at its place, Float shows it as a panel the user moves (drag the header) and
 * resizes (drag the corner). Window mode opens a separate window (`features/windows`).
 */
import { createSignal, For, Show } from "solid-js";
import { findToolWindow } from "@/features/registry";
import { layout, moveFloatingToolWindow, undockedWindows, viewModeOf } from "@/state/layout";
import { anchorOf, clampFloating, defaultFloatingBounds, placementOf, type FloatingBounds, type ToolWindowSlot } from "@/state/toolWindowLayout";
import { ToolWindowPanel } from "./ToolWindowPanel";
import { createLayer, layerStyle } from "./layers";
import { t } from "@/i18n";

/** Elements in the header that keep their own mouse handling. */
const INTERACTIVE = "button, a, input, select, textarea, [role='button'], [role='tab']";

/** The area floating windows live in (the editor and tool window part of the workbench). */
function areaOf(el: HTMLElement | undefined): { width: number; height: number } {
  const parent = el?.parentElement ?? document.querySelector<HTMLElement>(".workbench-center");
  return { width: parent?.clientWidth ?? window.innerWidth, height: parent?.clientHeight ?? window.innerHeight };
}

function FloatingPanel(props: { id: string; slot: ToolWindowSlot }) {
  let el: HTMLDivElement | undefined;
  // Its own layer: the most recently opened floating window is on top, and popups opened from
  // it (the layer of the focused element) open above it.
  const layer = createLayer("floating-tool-window");
  const [live, setLive] = createSignal<FloatingBounds | null>(null);
  const stored = () => {
    const tw = findToolWindow(props.id);
    return tw ? placementOf(tw, layout.placements).floating : undefined;
  };
  const bounds = (): FloatingBounds =>
    live() ?? clampFloating(stored() ?? defaultFloatingBounds(anchorOf(props.slot), areaOf(el)), areaOf(el));

  /** Follow the pointer from `start`; `apply` turns the offset into new bounds. */
  const track = (e: PointerEvent, apply: (start: FloatingBounds, dx: number, dy: number) => FloatingBounds) => {
    e.preventDefault();
    const start = bounds();
    const [x0, y0] = [e.clientX, e.clientY];
    const target = e.currentTarget as HTMLElement;
    target.setPointerCapture(e.pointerId);
    const move = (ev: PointerEvent) => setLive(clampFloating(apply(start, ev.clientX - x0, ev.clientY - y0), areaOf(el)));
    const up = () => {
      target.removeEventListener("pointermove", move);
      target.removeEventListener("pointerup", up);
      const done = live();
      if (done) moveFloatingToolWindow(props.id, done);
      setLive(null);
    };
    target.addEventListener("pointermove", move);
    target.addEventListener("pointerup", up);
  };

  const onHeaderDown = (e: PointerEvent) => {
    const t = e.target as Element;
    if (e.button !== 0 || !t.closest(".tool-window-header") || t.closest(INTERACTIVE)) return;
    track(e, (s, dx, dy) => ({ ...s, x: s.x + dx, y: s.y + dy }));
  };

  return (
    <div
      ref={el}
      class="floating-tool-window"
      data-floating-tool-window={props.id}
      data-layer-id={layer.id}
      data-layer-kind={layer.kind}
      style={{
        ...layerStyle(layer),
        left: `${bounds().x}px`,
        top: `${bounds().y}px`,
        width: `${bounds().width}px`,
        height: `${bounds().height}px`,
      }}
      onPointerDown={onHeaderDown}
    >
      <ToolWindowPanel slot={props.slot} />
      <div
        class="floating-resize"
        title={t("toolwindow.resize")}
        onPointerDown={(e) => {
          e.stopPropagation();
          track(e, (s, dx, dy) => ({ ...s, width: s.width + dx, height: s.height + dy }));
        }}
      />
    </div>
  );
}

function UndockedPanel(props: { slot: ToolWindowSlot }) {
  const anchor = () => anchorOf(props.slot);
  const style = () => {
    const a = anchor();
    return a === "bottom" ? { height: `${layout.size.bottom}px` } : { width: `${layout.size[a]}px` };
  };
  return (
    <div class={`undocked-tool-window undocked-${anchor()}`} style={style()}>
      <ToolWindowPanel slot={props.slot} />
    </div>
  );
}

/** The layer over the editor area that shows undocked and floating tool windows. */
export function UndockedToolWindows() {
  const ids = () => undockedWindows().filter((w) => w.mode !== "window").map((w) => w.id);
  const slotOf = (id: string) => undockedWindows().find((w) => w.id === id)?.slot;
  return (
    <For each={ids()}>
      {(id) => (
        <Show when={slotOf(id)} keyed>
          {(slot) => (
            <Show when={viewModeOf(id) === "float"} fallback={<UndockedPanel slot={slot} />}>
              <FloatingPanel id={id} slot={slot} />
            </Show>
          )}
        </Show>
      )}
    </For>
  );
}
