import { Show } from "solid-js";
import { layout } from "@/state/layout";
import { MainToolbar } from "./MainToolbar";
import { ToolStripe } from "./ToolStripe";
import { ToolWindowPanel } from "./ToolWindowPanel";
import { Splitter } from "./Splitter";
import { EditorArea } from "./EditorArea";
import { StatusBar } from "./StatusBar";

/** The main IDE window: toolbar, stripes, side panels, editor, bottom panel, status bar. */
export function Workbench() {
  return (
    <div class="workbench">
      <MainToolbar />
      <div class="workbench-body">
        <ToolStripe side="left" />
        <div class="workbench-center">
          <div class="workbench-main">
            <Show when={layout.active.left}>
              <div class="side-panel" style={{ width: `${layout.size.left}px` }}>
                <ToolWindowPanel anchor="left" />
              </div>
              <Splitter anchor="left" />
            </Show>
            <EditorArea />
            <Show when={layout.active.right}>
              <Splitter anchor="right" />
              <div class="side-panel" style={{ width: `${layout.size.right}px` }}>
                <ToolWindowPanel anchor="right" />
              </div>
            </Show>
          </div>
          <Show when={layout.active.bottom}>
            <Splitter anchor="bottom" />
            <div class="bottom-panel" style={{ height: `${layout.size.bottom}px` }}>
              <ToolWindowPanel anchor="bottom" />
            </div>
          </Show>
        </div>
        <ToolStripe side="right" />
      </div>
      <StatusBar />
    </div>
  );
}
