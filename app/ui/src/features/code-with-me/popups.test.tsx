/**
 * The Code With Me toolbar icon, avatars and popups: tooltips do not stay over an open popup,
 * the session popup has a single participant list with the connection paths, and only one
 * popup (session popup or a status bar popup) is open at a time.
 */
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { createSignal, Show } from "solid-js";
import { render } from "solid-js/web";
import { PRESETS, type SessionView } from "@/ipc/collab";
import { StatusPopup } from "@/shell/StatusPopup";
import { applySession, popupOpen, setPopupOpen } from "./store";
import { ToolbarButton } from "./ToolbarButton";
import { ConnectionStatusItem } from "./ConnectionStatus";

const hostSession: SessionView = {
  role: "host",
  canHost: true,
  me: 0,
  participants: [
    { id: 0, name: "Robert", role: "host", color: 1, permissions: PRESETS.fullAccess },
    { id: 1, name: "Forge user", role: "guest", color: 2, permissions: PRESETS.readOnly },
  ],
  files: [],
  connections: [{ participant: 1, kind: "direct", remote: "192.168.1.42:41641", rttMs: 0 }],
};

globalThis.ResizeObserver ??= class {
  observe() {}
  disconnect() {}
  unobserve() {}
} as unknown as typeof ResizeObserver;

const tick = () => new Promise((r) => setTimeout(r, 0));
const $ = <T extends Element = HTMLElement>(sel: string) => document.querySelector<T>(sel);

/** A press as the browser dispatches it: pointerdown, mousedown, then click. */
function press(el: Element): void {
  el.dispatchEvent(new Event("pointerdown", { bubbles: true }));
  el.dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
  el.dispatchEvent(new MouseEvent("click", { bubbles: true }));
}

let dispose: (() => void) | undefined;
const [statusOpen, setStatusOpen] = createSignal(false);

beforeEach(() => {
  applySession(hostSession);
  setPopupOpen(false);
  setStatusOpen(false);
  dispose = render(
    () => (
      <>
        <ToolbarButton onStart={() => undefined} />
        <ConnectionStatusItem />
        <div class="other-status" />
        <Show when={statusOpen()}>
          <StatusPopup anchor={$(".other-status")!} class="other-popup" label="Other" onClose={() => setStatusOpen(false)}>
            <span>other</span>
          </StatusPopup>
        </Show>
      </>
    ),
    document.body,
  );
});

afterEach(() => {
  setPopupOpen(false);
  setStatusOpen(false);
  dispose?.();
  document.body.innerHTML = "";
});

describe("Code With Me tooltips", () => {
  it("drops the toolbar icon's tooltip while its popup is open and keeps its name", async () => {
    const btn = $(".cwm-toolbar-btn")!;
    expect(btn.getAttribute("title")).toBe("Code With Me: sharing with 1 guest");
    press(btn);
    await tick();
    expect($(".cwm-popup")).not.toBeNull();
    expect(btn.getAttribute("aria-expanded")).toBe("true");
    expect(btn.hasAttribute("title")).toBe(false);
    expect(btn.getAttribute("aria-label")).toBe("Code With Me: sharing with 1 guest");
  });

  it("hides an avatar's tooltip after a click until the pointer leaves", () => {
    const avatar = $('.cwm-toolbar-avatar[data-participant="1"]')!;
    expect(avatar.getAttribute("title")).toBe("Forge user · Read-only");
    avatar.dispatchEvent(new Event("pointerdown", { bubbles: true }));
    expect(avatar.hasAttribute("title")).toBe(false);
    expect(avatar.getAttribute("aria-label")).toBeTruthy();
    avatar.dispatchEvent(new Event("pointerleave"));
    expect(avatar.hasAttribute("title")).toBe(true);
  });

  it("drops the status bar item's tooltip while the session popup is open", async () => {
    const item = $('[data-status="code-with-me"]')!;
    expect(item.hasAttribute("title")).toBe(true);
    setPopupOpen(true);
    await tick();
    expect(item.hasAttribute("title")).toBe(false);
    expect(item.getAttribute("aria-expanded")).toBe("true");
    expect(item.textContent).toContain("Code With Me");
  });
});

describe("Code With Me popups", () => {
  it("lists every participant once, with the connection path", async () => {
    press($(".cwm-toolbar-btn")!);
    await tick();
    const popup = $(".cwm-popup")!;
    expect(popup.querySelectorAll('.cwm-participant[data-participant="1"]').length).toBe(1);
    expect(popup.querySelector('.cwm-participant[data-participant="1"] .cwm-path')?.textContent).toContain("Direct");
    expect(popup.querySelector('.cwm-participant[data-participant="0"] .cwm-path')).toBeNull();
    expect([...popup.querySelectorAll(".cwm-popup-label")].filter((l) => l.textContent === "Participants").length).toBe(1);
  });

  it("the status bar item toggles the session popup", async () => {
    const item = $('[data-status="code-with-me"]')!;
    press(item);
    await tick();
    expect(popupOpen()).toBe(true);
    press(item);
    await tick();
    expect(popupOpen()).toBe(false);
  });

  it("opening the session popup closes a status bar popup and the other way round", async () => {
    setStatusOpen(true);
    await tick();
    expect($(".other-popup")).not.toBeNull();
    // Opened without a click (Find Action, a shortcut): the status popup still closes.
    setPopupOpen(true);
    await tick();
    expect($(".cwm-popup")).not.toBeNull();
    expect($(".other-popup")).toBeNull();

    setStatusOpen(true);
    await tick();
    expect($(".other-popup")).not.toBeNull();
    expect(popupOpen()).toBe(false);
    expect($(".cwm-popup")).toBeNull();
  });
});
