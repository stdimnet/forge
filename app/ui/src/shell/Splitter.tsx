import type { ToolWindowAnchor } from "@/features/registry";
import { layout, resizePanel } from "@/state/layout";

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
