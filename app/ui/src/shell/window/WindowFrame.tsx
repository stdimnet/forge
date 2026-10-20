/**
 * The frame of a dialog: Forge's title bar (title + platform-styled controls) above the
 * dialog's content. A dialog window fills its OS window with it; in browser mode the same
 * frame floats in-page over a backdrop. With native decorations the OS draws the title bar of
 * a dialog window, so the frame shows only the content.
 */
import { For, onMount, Show, type JSX } from "solid-js";
import { isTauri } from "@/ipc";
import type { ButtonLayout } from "@/ipc/window";
import { chrome, dragRegion, drawsControls, maximized, startResize, type ResizeDirection } from "./chrome";
import { WindowControls } from "./WindowControls";

const FALLBACK_BUTTONS: ButtonLayout = { left: [], right: ["close"] };

export interface WindowFrameProps {
  title: string;
  /** Rendered in-page (browser mode) instead of filling an OS window. */
  inPage?: boolean;
  /** In-page size in CSS pixels. */
  width?: number;
  height?: number;
  /** Close button, Escape (unless the content handled it) and the OS close request. */
  onRequestClose: () => void;
  children: JSX.Element;
}

export function WindowFrame(props: WindowFrameProps) {
  const style = () => chrome()?.style ?? "linux";
  const buttons = () => chrome()?.dialogButtons ?? FALLBACK_BUTTONS;
  // In-page dialogs always need a title bar; a window only when Forge draws it.
  const titleBar = () => props.inPage || drawsControls();
  let el!: HTMLDivElement;
  onMount(() => {
    if (!el.contains(document.activeElement)) el.focus();
  });
  const onKeyDown = (e: KeyboardEvent) => {
    // Content that handles Escape itself stops the event before it gets here.
    if (e.key === "Escape" && !e.defaultPrevented) {
      e.preventDefault();
      props.onRequestClose();
    }
  };
  return (
    <div
      ref={el}
      classList={{
        wf: true,
        [`wf-${style()}`]: true,
        "wf-in-page": props.inPage,
        "wf-window": !props.inPage,
        "wf-bordered": !props.inPage && drawsControls(),
      }}
      style={props.inPage ? { width: `${props.width ?? 800}px`, height: `${props.height ?? 600}px` } : undefined}
      role="dialog"
      aria-label={props.title}
      tabIndex={-1}
      onKeyDown={onKeyDown}
    >
      <Show when={titleBar()}>
        <header
          class="wf-titlebar"
          classList={{ "wf-title-left": chrome()?.titleAlign === "left" }}
          onMouseDown={(e) => !props.inPage && dragRegion.onMouseDown(e)}
        >
          <Show when={buttons().left.length > 0}>
            <WindowControls style={style()} buttons={buttons().left} side="left" dialog onClose={props.onRequestClose} />
          </Show>
          <span class="wf-title">{props.title}</span>
          <Show when={buttons().right.length > 0}>
            <WindowControls style={style()} buttons={buttons().right} side="right" dialog onClose={props.onRequestClose} />
          </Show>
        </header>
      </Show>
      <div class="wf-body">{props.children}</div>
      <Show when={!props.inPage}>
        <ResizeEdges />
      </Show>
    </div>
  );
}

const EDGES: [ResizeDirection, string][] = [
  ["North", "n"],
  ["South", "s"],
  ["East", "e"],
  ["West", "w"],
  ["NorthEast", "ne"],
  ["NorthWest", "nw"],
  ["SouthEast", "se"],
  ["SouthWest", "sw"],
];

/** Resize handles along the edges of an undecorated window. */
export function ResizeEdges() {
  return (
    <Show when={isTauri() && drawsControls() && !maximized()}>
      <For each={EDGES}>
        {([dir, cls]) => (
          <div
            class={`wf-resize wf-resize-${cls}`}
            onMouseDown={(e) => {
              if (e.button !== 0) return;
              e.preventDefault();
              startResize(dir);
            }}
          />
        )}
      </For>
    </Show>
  );
}

/** Title bar of a window without the main toolbar (the Welcome screen). */
export function WindowTitleBar(props: { title: string }) {
  const style = () => chrome()?.style ?? "linux";
  const buttons = () => chrome()?.buttons ?? FALLBACK_BUTTONS;
  return (
    <header
      classList={{ "wf-titlebar": true, "wf-window-titlebar": true, [`wf-${style()}`]: true, "wf-title-left": chrome()?.titleAlign === "left" }}
      onMouseDown={dragRegion.onMouseDown}
    >
      <Show when={buttons().left.length > 0}>
        <WindowControls style={style()} buttons={buttons().left} side="left" />
      </Show>
      <span class="wf-title">{props.title}</span>
      <Show when={buttons().right.length > 0}>
        <WindowControls style={style()} buttons={buttons().right} side="right" />
      </Show>
    </header>
  );
}
