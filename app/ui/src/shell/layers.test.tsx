import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { createSignal, Show } from "solid-js";
import { render } from "solid-js/web";
import { installDialogAccessibility, openDialogs } from "@/features/accessibility/dialogs";
import { LAYER_BASE, LAYER_KINDS, Layer, layerOf, layerZ, openLayer, openLayers, useLayer, type Layer as LayerT } from "./layers";

/** Node's `fs` (vitest runs in Node; app/ui has no @types/node). */
const proc = (globalThis as unknown as { process: { cwd(): string; getBuiltinModule(id: string): unknown } }).process;
const fs = proc.getBuiltinModule("node:fs") as { readFileSync(path: string, encoding: "utf8"): string };
// vitest runs in app/ui.
const themeCss = fs.readFileSync(`${proc.cwd()}/src/theme/theme.css`, "utf8");

const tick = (ms = 0) => new Promise((r) => setTimeout(r, ms));

beforeAll(() => installDialogAccessibility(document));

afterEach(() => {
  for (const l of openLayers()) l.close();
  for (const el of [...document.body.children]) if (!(el as HTMLElement).dataset.focusGuard) el.remove();
});

describe("layer scale", () => {
  it("matches the --z-* variables of theme.css, lowest first", () => {
    const values = LAYER_KINDS.map((k) => Number(new RegExp(`--z-${k}:\\s*(\\d+);`).exec(themeCss)?.[1]));
    expect(values).toEqual(LAYER_KINDS.map((k) => LAYER_BASE[k]));
    expect([...values].sort((a, b) => a - b)).toEqual(values);
  });
});

describe("overlay stack", () => {
  it("puts a layer above its base, its parent and earlier layers of the same or a lower kind", () => {
    expect(layerZ("popup", null, [])).toBe(501);
    // A popup opened from a modal dialog is above the dialog.
    expect(layerZ("popup", { z: 601 }, [{ kind: "modal", z: 601 }])).toBe(602);
    // The most recent dialog is above the earlier one; a notification stays above both.
    expect(layerZ("modal", null, [{ kind: "modal", z: 601 }, { kind: "notification", z: 701 }])).toBe(602);
    // A higher kind opened earlier does not lift a lower one.
    expect(layerZ("popup", null, [{ kind: "context-menu", z: 801 }])).toBe(501);
    // Nothing but the system layer reaches it.
    expect(layerZ("popup", { z: 5000 }, [])).toBe(LAYER_BASE.system - 1);
  });

  it("releases a layer when it closes", () => {
    const dialog = openLayer("modal");
    const nested = openLayer("modal", dialog);
    expect(nested.z).toBeGreaterThan(dialog.z);
    nested.close();
    nested.close();
    expect(openLayers()).toEqual([dialog]);
    expect(openLayer("popup", nested).parent).toBeNull();
  });
});

describe("<Layer>", () => {
  it("renders through a portal into the overlay host, above the layer around it", () => {
    let inner: LayerT | undefined;
    const Probe = () => {
      inner = useLayer();
      return <span class="probe" />;
    };
    const host = document.createElement("div");
    document.body.appendChild(host);
    const dispose = render(
      () => (
        <Layer kind="modal">
          <div class="dlg">
            <Layer kind="popup">
              <Probe />
            </Layer>
          </div>
        </Layer>
      ),
      host,
    );
    const overlays = document.getElementById("forge-overlays")!;
    const [outer, popup] = [...overlays.querySelectorAll<HTMLElement>(".forge-layer")];
    expect(host.querySelector(".dlg")).toBeNull();
    expect(outer.dataset.layerKind).toBe("modal");
    expect(popup.dataset.layerKind).toBe("popup");
    expect(Number(popup.style.zIndex)).toBeGreaterThan(Number(outer.style.zIndex));
    expect(inner?.parent?.kind).toBe("modal");
    expect(layerOf(popup.querySelector(".probe"))).toBe(inner);
    dispose();
    expect(openLayers()).toEqual([]);
    expect(overlays.children).toHaveLength(0);
  });

  it("takes the layer of the focused element as its parent", () => {
    const dialog = openLayer("modal");
    const outside = document.createElement("div");
    outside.dataset.layerId = String(dialog.id);
    outside.innerHTML = "<button>Open</button>";
    document.body.appendChild(outside);
    outside.querySelector("button")!.focus();
    const [open, setOpen] = createSignal(true);
    const host = document.createElement("div");
    document.body.appendChild(host);
    render(
      () => (
        <Show when={open()}>
          <Layer kind="popup">
            <div class="list" />
          </Layer>
        </Show>
      ),
      host,
    );
    const popup = openLayers().at(-1)!;
    expect(popup.parent).toBe(dialog);
    expect(popup.z).toBeGreaterThan(dialog.z);
    setOpen(false);
    expect(popup.isOpen).toBe(false);
  });
});

describe("adopted overlays", () => {
  it("layers backdrops and menus that features render themselves, and releases them", async () => {
    const host = document.createElement("div");
    host.innerHTML = `<div class="modal-backdrop" style="position: fixed"><div class="dialog"><div class="dialog-title">Rename</div><input /></div></div>`;
    document.body.appendChild(host);
    await tick();
    const backdrop = host.querySelector<HTMLElement>(".modal-backdrop")!;
    expect(openDialogs()).toContain(backdrop.firstElementChild);
    expect(backdrop.dataset.layerKind).toBe("modal");
    const dialogZ = Number(backdrop.style.zIndex);
    expect(dialogZ).toBeGreaterThan(LAYER_BASE.modal);

    // A menu opened from the dialog (focus inside it), rendered somewhere else.
    backdrop.querySelector("input")!.focus();
    const menuHost = document.createElement("div");
    menuHost.innerHTML = `<div role="menu" style="position: fixed"><div role="menuitem">Copy</div></div><div role="listbox">inline</div>`;
    document.body.appendChild(menuHost);
    await tick();
    const menu = menuHost.querySelector<HTMLElement>("[role=menu]")!;
    expect(menu.dataset.layerKind).toBe("context-menu");
    expect(layerOf(menu)?.parent?.z).toBe(dialogZ);
    // Local (not fixed) lists stay out of the stack.
    expect(menuHost.querySelector<HTMLElement>("[role=listbox]")!.dataset.layerId).toBeUndefined();

    menuHost.remove();
    host.remove();
    await tick();
    expect(openLayers()).toEqual([]);
  });

  it("traps focus in the modal dialog painted on top", async () => {
    const top = openLayer("modal");
    const a = document.createElement("div");
    a.innerHTML = `<div class="modal-backdrop" data-layer-id="${top.id}"><div class="dialog"><button class="in-top">OK</button></div></div>`;
    const b = document.createElement("div");
    b.innerHTML = `<div class="modal-backdrop"><div class="dialog"><button class="in-other">OK</button></div></div>`;
    const outside = document.createElement("button");
    outside.className = "outside";
    document.body.append(a, b, outside);
    await tick(80);
    // `b` opened later but is not on the stack: Tab out of the window wraps into the layered one.
    outside.focus();
    expect(document.activeElement?.className).toBe("in-top");
  });
});
