/**
 * Minimize / maximize / close buttons drawn by Forge, styled after the platform: Windows 11
 * rectangles, macOS traffic lights, GNOME (Adwaita) round buttons, KDE Breeze glyphs or a
 * generic Linux look. Glyphs are small inline SVGs because the OS glyphs differ from any icon
 * set (Breeze minimizes with a chevron, macOS shows glyphs only on hover, …).
 */
import { For, Match, Switch, type JSX } from "solid-js";
import type { ControlsStyle, WindowButton } from "@/ipc/window";
import { maximized, windowActions } from "./chrome";
import "./window.css";

const TITLES: Record<WindowButton, string> = { minimize: "Minimize", maximize: "Maximize", close: "Close" };

function Svg(props: { size: number; children: JSX.Element }) {
  return (
    <svg
      class="wc-glyph"
      width={props.size}
      height={props.size}
      viewBox={`0 0 ${props.size} ${props.size}`}
      fill="none"
      stroke="currentColor"
      aria-hidden="true"
    >
      {props.children}
    </svg>
  );
}

/** Windows 11 / Segoe Fluent glyphs (10×10, hairline strokes). */
function WindowsGlyph(props: { button: WindowButton; restore: boolean }) {
  return (
    <Svg size={10}>
      <Switch>
        <Match when={props.button === "minimize"}>
          <path d="M0 5.5h10" />
        </Match>
        <Match when={props.button === "maximize" && props.restore}>
          <path d="M2.5 2.5V.5h7v7h-2M.5 2.5h7v7h-7z" />
        </Match>
        <Match when={props.button === "maximize"}>
          <rect x="0.5" y="0.5" width="9" height="9" />
        </Match>
        <Match when={props.button === "close"}>
          <path d="M.5.5l9 9M9.5.5l-9 9" />
        </Match>
      </Switch>
    </Svg>
  );
}

/** Adwaita symbolic icons (16×16). */
function GnomeGlyph(props: { button: WindowButton; restore: boolean }) {
  return (
    <Svg size={16}>
      <Switch>
        <Match when={props.button === "minimize"}>
          <path d="M4.5 11.25h7" stroke-width="1.5" stroke-linecap="round" />
        </Match>
        <Match when={props.button === "maximize" && props.restore}>
          <rect x="5.25" y="5.25" width="5.5" height="5.5" rx="1" stroke-width="1.5" />
        </Match>
        <Match when={props.button === "maximize"}>
          <rect x="4.25" y="4.25" width="7.5" height="7.5" rx="1.25" stroke-width="1.5" />
        </Match>
        <Match when={props.button === "close"}>
          <path d="M5 5l6 6M11 5l-6 6" stroke-width="1.5" stroke-linecap="round" />
        </Match>
      </Switch>
    </Svg>
  );
}

/** Breeze glyphs (18×18): chevrons for minimize/maximize, a diamond for restore. */
function KdeGlyph(props: { button: WindowButton; restore: boolean }) {
  return (
    <Svg size={18}>
      <Switch>
        <Match when={props.button === "minimize"}>
          <path d="M4.5 7l4.5 4.5L13.5 7" stroke-width="1.4" stroke-linejoin="round" />
        </Match>
        <Match when={props.button === "maximize" && props.restore}>
          <path d="M9 4.5L13.5 9 9 13.5 4.5 9z" stroke-width="1.4" stroke-linejoin="round" />
        </Match>
        <Match when={props.button === "maximize"}>
          <path d="M4.5 11l4.5-4.5 4.5 4.5" stroke-width="1.4" stroke-linejoin="round" />
        </Match>
        <Match when={props.button === "close"}>
          <path d="M5.5 5.5l7 7M12.5 5.5l-7 7" stroke-width="1.4" stroke-linecap="round" />
        </Match>
      </Switch>
    </Svg>
  );
}

/** macOS traffic light glyphs (8×8), shown while the pointer is over the group. */
function MacGlyph(props: { button: WindowButton; restore: boolean }) {
  return (
    <Svg size={8}>
      <Switch>
        <Match when={props.button === "minimize"}>
          <path d="M1.5 4h5" stroke-width="1.2" />
        </Match>
        <Match when={props.button === "maximize" && props.restore}>
          <path d="M4 .8v3.2H.8z M4 7.2V4h3.2z" fill="currentColor" stroke="none" />
        </Match>
        <Match when={props.button === "maximize"}>
          <path d="M1.2 1.2h4L1.2 5.2z M6.8 6.8h-4l4-4z" fill="currentColor" stroke="none" />
        </Match>
        <Match when={props.button === "close"}>
          <path d="M2 2l4 4M6 2L2 6" stroke-width="1.2" />
        </Match>
      </Switch>
    </Svg>
  );
}

function Glyph(props: { style: ControlsStyle; button: WindowButton; restore: boolean }) {
  return (
    <Switch fallback={<WindowsGlyph button={props.button} restore={props.restore} />}>
      <Match when={props.style === "gnome"}>
        <GnomeGlyph button={props.button} restore={props.restore} />
      </Match>
      <Match when={props.style === "kde"}>
        <KdeGlyph button={props.button} restore={props.restore} />
      </Match>
      <Match when={props.style === "macos"}>
        <MacGlyph button={props.button} restore={props.restore} />
      </Match>
    </Switch>
  );
}

export interface WindowControlsProps {
  style: ControlsStyle;
  buttons: WindowButton[];
  side: "left" | "right";
  /** Dialog windows cannot be minimized (the macOS button shows disabled). */
  dialog?: boolean;
  /** Close action; defaults to closing the window. */
  onClose?: () => void;
}

export function WindowControls(props: WindowControlsProps) {
  const run = (button: WindowButton) => {
    if (button === "close") (props.onClose ?? windowActions.close)();
    else if (button === "minimize") windowActions.minimize();
    else windowActions.toggleMaximize();
  };
  return (
    <div class={`wc wc-${props.style} wc-${props.side}`} data-window-controls={props.style} data-no-drag>
      <For each={props.buttons}>
        {(button) => (
          <button
            class={`wc-btn wc-${button}`}
            title={button === "maximize" && maximized() ? "Restore" : TITLES[button]}
            aria-label={TITLES[button]}
            disabled={props.dialog && button === "minimize"}
            tabIndex={-1}
            onMouseDown={(e) => e.stopPropagation()}
            onClick={() => run(button)}
          >
            <Glyph style={props.style} button={button} restore={maximized()} />
          </button>
        )}
      </For>
    </div>
  );
}
