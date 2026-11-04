import { Show } from "solid-js";
import type { ToolWindowAnchor } from "@/features/registry";
import { dockedIn, layout, openInArea } from "@/state/layout";
import { workspace } from "@/state/workspace";
import { appearanceMode, isFullScreen } from "@/state/appearanceModes";
import { slotFor } from "@/state/toolWindowLayout";
import { MainToolbar } from "./MainToolbar";
import { ToolStripe } from "./ToolStripe";
import { ToolWindowPanel } from "./ToolWindowPanel";
import { SplitDivider, Splitter } from "./Splitter";
import { EditorArea } from "./EditorArea";
import { StatusBar } from "./StatusBar";
import { UndockedToolWindows } from "./UndockedToolWindows";
import { drawsControls } from "./window/chrome";
import { WindowTitleBar } from "./window/WindowFrame";

/**
 * The tool windows of one area: the primary group's window and the split group's window,
 * stacked on the sides (Left Top above Left Bottom) or side by side at the bottom (Bottom
 * Left, Bottom Right), like PHPStorm's New UI.
 */
function ToolArea(props: { anchor: ToolWindowAnchor }) {
  const both = () => openInArea(props.anchor).length === 2;
  const primary = slotFor(props.anchor);
  return (
    <div class={`tool-area tool-area-${props.anchor}`} classList={{ split: both() }}>
      <Show when={dockedIn(primary)}>
        <div class="tool-area-part" style={both() ? { flex: `0 0 ${layout.splits[props.anchor] / 10}%` } : undefined}>
          <ToolWindowPanel slot={primary} />
        </div>
      </Show>
      <Show when={both()}>
        <SplitDivider anchor={props.anchor} />
      </Show>
      <Show when={dockedIn(slotFor(props.anchor, true))}>
        <div class="tool-area-part">
          <ToolWindowPanel slot={slotFor(props.anchor, true)} />
        </div>
      </Show>
    </div>
  );
}

/** The main IDE window: toolbar, stripes, side panels, editor, bottom panel, status bar. */
export function Workbench() {
  return (
    <div class="workbench">
      {/* Presentation / Distraction-Free / Zen Mode hide the main toolbar; a window that is
          not full screen keeps a title bar to move it. */}
      <Show when={appearanceMode() && !isFullScreen() && drawsControls()}>
        <WindowTitleBar title={workspace()?.name ?? "Forge"} />
      </Show>
      <h1 class="sr-only">{workspace()?.name ? `${workspace()!.name} – Forge` : "Forge"}</h1>
      <MainToolbar />
      <div class="workbench-body">
        <ToolStripe side="left" />
        <div class="workbench-center">
          <div class="workbench-main">
            <Show when={openInArea("left").length}>
              <div class="side-panel" style={{ width: `${layout.size.left}px` }}>
                <ToolArea anchor="left" />
              </div>
              <Splitter anchor="left" />
            </Show>
            <EditorArea />
            <Show when={openInArea("right").length}>
              <Splitter anchor="right" />
              <div class="side-panel" style={{ width: `${layout.size.right}px` }}>
                <ToolArea anchor="right" />
              </div>
            </Show>
          </div>
          <Show when={openInArea("bottom").length}>
            <Splitter anchor="bottom" />
            <div class="bottom-panel" style={{ height: `${layout.size.bottom}px` }}>
              <ToolArea anchor="bottom" />
            </div>
          </Show>
          <UndockedToolWindows />
        </div>
        <ToolStripe side="right" />
      </div>
      <StatusBar />
    </div>
  );
}
