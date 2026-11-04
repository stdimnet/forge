import { For, Show, createSignal } from "solid-js";
import { Portal } from "solid-js/web";
import { Ellipsis } from "lucide-solid";
import { findToolWindow, type ToolWindow } from "@/features/registry";
import { isToolWindowVisible, moveToolWindowTo, stripeToolWindows, toggleToolWindow, type ToolWindowSlot } from "@/state/layout";
import { STRIPE_SLOTS, dropBefore, dropIndex } from "@/state/toolWindowLayout";
import { openStripeMenu } from "./toolWindowMenu";
import { openMoreToolWindows } from "./MoreToolWindows";
import { t } from "@/i18n";

/** Where a dragged stripe button would land. */
interface DropTarget {
  slot: ToolWindowSlot;
  index: number;
  /** Position of the drop indicator inside the group (px from its top). */
  y: number;
}

interface Drag {
  id: string;
  x: number;
  y: number;
  target: DropTarget | null;
}

/** Pixels the pointer must move before a press on a stripe button becomes a drag. */
const DRAG_THRESHOLD = 5;
/** How far beside a stripe a drop still counts. */
const STRIPE_TOLERANCE = 24;

const [drag, setDrag] = createSignal<Drag | null>(null);

/** The stripe group under the pointer: the nearest group of the stripe the pointer is over. */
function findTarget(x: number, y: number): DropTarget | null {
  const stripe = [...document.querySelectorAll<HTMLElement>(".tool-stripe")].find((s) => {
    const r = s.getBoundingClientRect();
    return r.width > 0 && x >= r.left - STRIPE_TOLERANCE && x <= r.right + STRIPE_TOLERANCE;
  });
  if (!stripe) return null;
  let best: { el: HTMLElement; distance: number } | null = null;
  for (const el of stripe.querySelectorAll<HTMLElement>("[data-stripe-slot]")) {
    const r = el.getBoundingClientRect();
    const distance = y < r.top ? r.top - y : y > r.bottom ? y - r.bottom : 0;
    if (!best || distance < best.distance) best = { el, distance };
  }
  if (!best) return null;
  const group = best.el;
  const top = group.getBoundingClientRect().top;
  const buttons = [...group.querySelectorAll<HTMLElement>("[data-tool-window]")].map((b) => {
    const r = b.getBoundingClientRect();
    return { start: r.top, end: r.bottom };
  });
  const index = dropIndex(buttons, y);
  const gap = 2;
  const lineY =
    index < buttons.length ? buttons[index].start - top - gap : buttons.length ? buttons[buttons.length - 1].end - top + gap : 0;
  return { slot: group.dataset.stripeSlot as ToolWindowSlot, index, y: lineY };
}

/** Press on a stripe button: a click toggles the window, a drag moves the button. */
function startPress(e: PointerEvent, tw: ToolWindow): void {
  if (e.button !== 0) return;
  const startX = e.clientX;
  const startY = e.clientY;
  let dragging = false;
  const move = (ev: PointerEvent) => {
    if (!dragging && Math.hypot(ev.clientX - startX, ev.clientY - startY) < DRAG_THRESHOLD) return;
    dragging = true;
    setDrag({ id: tw.id, x: ev.clientX, y: ev.clientY, target: findTarget(ev.clientX, ev.clientY) });
  };
  const up = () => {
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", up);
    const d = drag();
    setDrag(null);
    if (!dragging) return toggleToolWindow(tw.id);
    if (!d?.target) return;
    const group = stripeToolWindows(d.target.slot).map((w) => w.id);
    moveToolWindowTo(tw.id, d.target.slot, dropBefore(group, tw.id, d.target.index));
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up);
}

function StripeButton(props: { tw: ToolWindow }) {
  const active = () => isToolWindowVisible(props.tw.id);
  // The digit of an Alt+<n> shortcut, shown with "Show tool window numbers".
  const number = () => props.tw.shortcut?.match(/^Alt\+(\d)$/)?.[1];
  return (
    <button
      class="icon-btn stripe-btn"
      classList={{ active: active(), dragged: drag()?.id === props.tw.id }}
      title={props.tw.shortcut ? `${props.tw.title} (${props.tw.shortcut})` : props.tw.title}
      aria-label={props.tw.title}
      aria-pressed={active()}
      aria-keyshortcuts={props.tw.shortcut}
      data-tool-window={props.tw.id}
      onPointerDown={(e) => startPress(e, props.tw)}
      // Mouse clicks are handled on pointer up (they may be drags); this is Enter/Space.
      onClick={(e) => e.detail === 0 && toggleToolWindow(props.tw.id)}
      onContextMenu={(e) => {
        e.preventDefault();
        openStripeMenu(props.tw.id, e.clientX, e.clientY);
      }}
    >
      <props.tw.icon size={18} stroke-width={1.75} />
      <Show when={number()}>
        <span class="stripe-num">{number()}</span>
      </Show>
      <Show when={!active() && props.tw.badge}>
        {(badge) => <span class="stripe-badge">{badge()()}</span>}
      </Show>
    </button>
  );
}

function StripeGroup(props: { slot: ToolWindowSlot }) {
  const target = () => (drag()?.target?.slot === props.slot ? drag()!.target : null);
  return (
    <div class="stripe-group" classList={{ "drop-target": !!target() }} data-stripe-slot={props.slot} role="group">
      <For each={stripeToolWindows(props.slot)}>{(tw) => <StripeButton tw={tw} />}</For>
      <Show when={target()}>{(t) => <div class="stripe-drop-indicator" style={{ top: `${t().y}px` }} />}</Show>
    </div>
  );
}

/**
 * Vertical icon stripe at the window edge (PHPStorm New UI). The left stripe holds Left Top
 * and Left Bottom, the ⋯ button for all tool windows and, at its foot, Bottom Left; the right
 * stripe holds Right Top, Right Bottom and, at its foot, Bottom Right. Buttons are dragged to
 * reorder them or to move a tool window to another group.
 */
export function ToolStripe(props: { side: "left" | "right" }) {
  const slots = STRIPE_SLOTS[props.side];
  const splitShown = () => stripeToolWindows(slots.top[1]).length > 0 || !!drag();
  return (
    <nav
      class={`tool-stripe tool-stripe-${props.side}`}
      classList={{ dragging: !!drag() }}
      aria-label={t(props.side === "left" ? "a11y.stripe.left" : "a11y.stripe.right")}
    >
      <div class="stripe-part">
        <StripeGroup slot={slots.top[0]} />
        <Show when={splitShown()}>
          <div class="stripe-separator" />
        </Show>
        <StripeGroup slot={slots.top[1]} />
        <Show when={props.side === "left"}>
          <button
            class="icon-btn stripe-btn stripe-more"
            title={t("toolwindow.more")}
            aria-label={t("toolwindow.more")}
            aria-haspopup="menu"
            data-more-tool-windows
            onClick={(e) => {
              const r = e.currentTarget.getBoundingClientRect();
              openMoreToolWindows(r.right + 4, r.top);
            }}
          >
            <Ellipsis size={18} stroke-width={1.75} />
          </button>
        </Show>
      </div>
      <div class="stripe-part">
        <StripeGroup slot={slots.foot} />
      </div>
      <Show when={props.side === "left" && drag()}>
        {(d) => (
          <Portal>
            <div class="stripe-drag-ghost" style={{ left: `${d().x + (d().x > window.innerWidth - 60 ? -40 : 10)}px`, top: `${d().y + 10}px` }}>
              {(() => {
                const tw = findToolWindow(d().id);
                return tw ? <tw.icon size={18} stroke-width={1.75} /> : null;
              })()}
            </div>
          </Portal>
        )}
      </Show>
    </nav>
  );
}
