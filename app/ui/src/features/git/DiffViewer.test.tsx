/**
 * The diff viewer takes Next/Previous Difference from the keymap and closes on Escape only
 * when the key comes from inside it, not from a dialog opened above it (#217).
 */
import { afterEach, describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import { DiffViewer } from "./DiffViewer";
import { activeDiffStepper, setMergeActions } from "./conflicts";
import type { DiffRequest } from "./overlays";

globalThis.ResizeObserver ??= class {
  observe() {}
  disconnect() {}
  unobserve() {}
} as unknown as typeof ResizeObserver;

const request: DiffRequest = {
  path: "app/Models/User.php",
  left: { title: "HEAD", content: "<?php\n$a = 1;\n$b = 2;\n" },
  right: { title: "Working copy", content: "<?php\n$a = 1;\n$b = 3;\n" },
  editable: false,
};

let dispose: (() => void) | undefined;
afterEach(() => {
  dispose?.();
  dispose = undefined;
  document.body.innerHTML = "";
  setMergeActions(null);
});

const tick = () => new Promise((r) => setTimeout(r, 0));
const escape = (target: EventTarget) => target.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }));

function mount() {
  const onClose = vi.fn();
  const host = document.createElement("div");
  document.body.append(host);
  dispose = render(() => <DiffViewer request={request} onClose={onClose} />, host);
  return { onClose, viewer: document.querySelector<HTMLElement>(".diff-viewer")! };
}

describe("DiffViewer", () => {
  it("is the diff focus context and takes the focus when it opens", async () => {
    const { viewer } = mount();
    await tick();
    expect(viewer.dataset.focusContext).toBe("diff");
    expect(viewer.contains(document.activeElement)).toBe(true);
  });

  it("closes on Escape from inside, not from a dialog above it", async () => {
    const { onClose, viewer } = mount();
    await tick();
    // A dialog opened above the viewer lives in the overlay host, outside the viewer.
    const dialog = document.createElement("div");
    dialog.setAttribute("role", "dialog");
    dialog.innerHTML = "<input />";
    document.body.append(dialog);
    escape(dialog.querySelector("input")!);
    await tick();
    expect(onClose).not.toHaveBeenCalled();
    escape(viewer.querySelector(".cm-content") ?? viewer);
    await tick();
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("is what NextDiff / PreviousDiff step through while open, before the merge window", async () => {
    const merge = { step: vi.fn(), applyAll: vi.fn(), magic: vi.fn() };
    setMergeActions(merge);
    expect(activeDiffStepper()).toBe(merge);
    mount();
    await tick();
    expect(activeDiffStepper()).not.toBe(merge);
    expect(() => activeDiffStepper()!.step(true)).not.toThrow();
    expect(merge.step).not.toHaveBeenCalled();
    dispose?.();
    dispose = undefined;
    expect(activeDiffStepper()).toBe(merge);
  });
});
