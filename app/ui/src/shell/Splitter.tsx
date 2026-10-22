import type { ToolWindowAnchor } from "@/features/registry";
import { layout, resizePanel, resizeSplit } from "@/state/layout";

/** Drag handle that resizes the panel on the given side. */
export function Splitter(props: { anchor: ToolWindowAnchor }) {
  const onPointerDown = (e: PointerEvent) => {
    e.preventDefault();
    const start = props.anchor === "bottom" ? e.clientY : e.clientX;
    const startSize = layout.size[props.anchor];
    const move = (ev: PointerEvent) => {
      const pos = props.anchor === "bottom" ? ev.clientY : ev.clientX;
      const delta = props.anchor === "left" ? pos - start : start - pos;
      resizePanel(props.anchor, startSize + delta);
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };
  return (
    <div
      class={`splitter splitter-${props.anchor === "bottom" ? "h" : "v"}`}
      onPointerDown={onPointerDown}
    />
  );
}

/**
 * Drag handle between the two tool windows of one area (Left Top / Left Bottom stacked,
 * Bottom Left / Bottom Right side by side); it changes the area's split ratio.
 */
export function SplitDivider(props: { anchor: ToolWindowAnchor }) {
  let el!: HTMLDivElement;
  const vertical = () => props.anchor === "bottom";
  const onPointerDown = (e: PointerEvent) => {
    e.preventDefault();
    const area = el.parentElement!.getBoundingClientRect();
    const move = (ev: PointerEvent) => {
      const ratio = vertical() ? (ev.clientX - area.left) / area.width : (ev.clientY - area.top) / area.height;
      resizeSplit(props.anchor, ratio * 1000);
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };
  return <div ref={el} class={`splitter splitter-${vertical() ? "v" : "h"}`} data-split-divider={props.anchor} onPointerDown={onPointerDown} />;
}
