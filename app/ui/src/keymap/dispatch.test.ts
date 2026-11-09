import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import "@/mock/settingsMock";
import "@/mock/keymapMock";
import { registerCommand } from "@/state/commands";
import { emitMock } from "@/ipc";
import { SIDE_BUTTON_EVENT, balloons, focusContextOf, installKeymap, resolveCommand, runAction, suspendKeymap } from "./dispatch";
import { resetKeymap, setActionShortcuts, setKeymapId, startKeymap } from "./store";
import { setLocale } from "@/i18n";

function press(target: EventTarget, init: KeyboardEventInit) {
  const e = new KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...init });
  target.dispatchEvent(e);
  target.dispatchEvent(new KeyboardEvent("keyup", { bubbles: true, cancelable: true, ...init }));
  return e;
}

describe("runAction", () => {
  beforeAll(() => startKeymap());

  it("runs registered commands", async () => {
    const run = vi.fn();
    registerCommand({ id: "Annotate", title: "Annotate", run });
    await runAction("Annotate");
    expect(run).toHaveBeenCalledOnce();
  });

  it("follows aliases to existing Forge command ids", async () => {
    const run = vi.fn();
    registerCommand({ id: "run.run", title: "Run", run });
    expect(resolveCommand("Run")).toBe("run.run");
    await runAction("Run");
    expect(run).toHaveBeenCalledOnce();
  });

  it("shows a 'not available yet' balloon for unimplemented actions", async () => {
    await runAction("Vcs.Push");
    expect(balloons().at(-1)?.text).toBe("Push is not available yet");
  });
});

describe("focusContextOf", () => {
  it("classifies editors, inputs and tool windows", () => {
    document.body.innerHTML = `
      <div class="cm-editor"><div class="cm-content" contenteditable="true"></div><input id="cm-search" /></div>
      <input id="text" /><input id="check" type="checkbox" />
      <section class="tool-window"><button id="tw">x</button><input id="tw-search" />
        <textarea id="message" data-focus-context="toolwindow-input"></textarea>
        <div class="xterm"><textarea id="term"></textarea></div></section>
      <button id="plain">y</button>`;
    const q = (s: string) => document.querySelector(s);
    expect(focusContextOf(q(".cm-content"))).toBe("editor");
    expect(focusContextOf(q("#cm-search"))).toBe("input");
    expect(focusContextOf(q("#text"))).toBe("input");
    expect(focusContextOf(q("#check"))).toBe("global");
    expect(focusContextOf(q("#tw"))).toBe("toolwindow");
    expect(focusContextOf(q("#tw-search"))).toBe("input");
    expect(focusContextOf(q("#message"))).toBe("toolwindow-input");
    expect(focusContextOf(q("#term"))).toBe("terminal");
    expect(focusContextOf(q("#plain"))).toBe("global");
    expect(focusContextOf(null)).toBe("global");
  });
});

describe("installed keymap", () => {
  const gotoClass = vi.fn();
  const search = vi.fn();
  const back = vi.fn();
  const forward = vi.fn();
  beforeAll(async () => {
    await installKeymap(window);
    await setKeymapId("$default");
    await resetKeymap();
    registerCommand({ id: "GotoClass", title: "Class", run: gotoClass });
    registerCommand({ id: "SearchEverywhere", title: "Search Everywhere", run: search });
    registerCommand({ id: "Back", title: "Back", run: back });
    registerCommand({ id: "Forward", title: "Forward", run: forward });
  });
  beforeEach(() => {
    document.body.innerHTML = `<input id="field" /><button id="btn">b</button>`;
  });
  afterEach(() => vi.clearAllMocks());

  it("dispatches shortcuts and prevents the browser default", () => {
    const e = press(document.getElementById("btn")!, { key: "n", code: "KeyN", ctrlKey: true });
    expect(e.defaultPrevented).toBe(true);
    expect(gotoClass).toHaveBeenCalledOnce();
  });

  it("leaves typing in inputs alone", () => {
    const e = press(document.getElementById("field")!, { key: "n", code: "KeyN" });
    expect(e.defaultPrevented).toBe(false);
  });

  it("lets the terminal keep shell keys", () => {
    document.body.innerHTML = `<div class="xterm"><textarea id="term"></textarea></div>`;
    const e = press(document.getElementById("term")!, { key: "n", code: "KeyN", ctrlKey: true });
    expect(e.defaultPrevented).toBe(false);
    expect(gotoClass).not.toHaveBeenCalled();
  });

  it("waits for the second stroke of a chord", async () => {
    await setActionShortcuts("GotoClass", ["Ctrl+Alt+Shift+K Ctrl+N"]);
    const btn = document.getElementById("btn")!;
    expect(press(btn, { key: "k", code: "KeyK", ctrlKey: true, altKey: true, shiftKey: true }).defaultPrevented).toBe(true);
    expect(gotoClass).not.toHaveBeenCalled();
    expect(press(btn, { key: "n", code: "KeyN", ctrlKey: true }).defaultPrevented).toBe(true);
    expect(gotoClass).toHaveBeenCalledOnce();
    await resetKeymap();
  });

  it("recognizes Double Shift", () => {
    const btn = document.getElementById("btn")!;
    press(btn, { key: "Shift", code: "ShiftLeft", shiftKey: true });
    press(btn, { key: "Shift", code: "ShiftLeft", shiftKey: true });
    expect(search).toHaveBeenCalledOnce();
  });

  it("leaves keys a widget owns to the widget", () => {
    document.body.innerHTML = `<div data-own-keys="Ctrl+N Alt+Enter"><textarea id="own"></textarea></div><button id="btn">b</button>`;
    const owned = press(document.getElementById("own")!, { key: "n", code: "KeyN", ctrlKey: true });
    expect(owned.defaultPrevented).toBe(false);
    expect(gotoClass).not.toHaveBeenCalled();
    press(document.getElementById("btn")!, { key: "n", code: "KeyN", ctrlKey: true });
    expect(gotoClass).toHaveBeenCalledOnce();
  });

  it("can be suspended while recording shortcuts", () => {
    const resume = suspendKeymap();
    const e = press(document.getElementById("btn")!, { key: "n", code: "KeyN", ctrlKey: true });
    resume();
    expect(e.defaultPrevented).toBe(false);
    expect(gotoClass).not.toHaveBeenCalled();
  });

  /** A side button click as the webviews report it: mousedown, mouseup and (Chromium) auxclick. */
  function sideClick(target: EventTarget, button: number) {
    const init = { bubbles: true, cancelable: true, button, detail: 1 };
    const down = new MouseEvent("mousedown", init);
    const up = new MouseEvent("mouseup", init);
    const aux = new MouseEvent("auxclick", init);
    target.dispatchEvent(down);
    target.dispatchEvent(up);
    target.dispatchEvent(aux);
    return { down, up, aux };
  }

  it("runs Back and Forward on the mouse side buttons once and cancels the webview's navigation (#222)", async () => {
    const btn = document.getElementById("btn")!;
    const { down, up, aux } = sideClick(btn, 3);
    await Promise.resolve();
    expect(back).toHaveBeenCalledOnce();
    expect([down.defaultPrevented, up.defaultPrevented, aux.defaultPrevented]).toEqual([true, true, true]);
    sideClick(btn, 4);
    await Promise.resolve();
    expect(forward).toHaveBeenCalledOnce();
  });

  it("runs the side buttons in text fields and the terminal too (#222)", async () => {
    document.body.innerHTML = `<input id="field" /><div class="xterm"><textarea id="term"></textarea></div>`;
    sideClick(document.getElementById("field")!, 3);
    sideClick(document.getElementById("term")!, 4);
    await Promise.resolve();
    expect(back).toHaveBeenCalledOnce();
    expect(forward).toHaveBeenCalledOnce();
  });

  it("follows a rebound side button (#222)", async () => {
    await setActionShortcuts("Back", ["Ctrl+Alt+Left"]);
    await setActionShortcuts("GotoClass", ["Ctrl+N", "Button4 Click"]);
    sideClick(document.getElementById("btn")!, 3);
    await Promise.resolve();
    expect(back).not.toHaveBeenCalled();
    expect(gotoClass).toHaveBeenCalledOnce();
    await resetKeymap();
  });

  it("runs side button clicks the desktop shell reports (WebKitGTK, #222)", async () => {
    const click = { ctrlKey: false, altKey: false, shiftKey: false, metaKey: false, x: 1, y: 1 };
    emitMock(SIDE_BUTTON_EVENT, { ...click, button: 3 });
    emitMock(SIDE_BUTTON_EVENT, { ...click, button: 4 });
    await Promise.resolve();
    expect(back).toHaveBeenCalledOnce();
    expect(forward).toHaveBeenCalledOnce();
    const resume = suspendKeymap();
    emitMock(SIDE_BUTTON_EVENT, { ...click, button: 3 });
    resume();
    await Promise.resolve();
    expect(back).toHaveBeenCalledOnce();
  });
});

describe("action balloons", () => {
  afterEach(() => setLocale("en"));

  it("follow the IDE language", async () => {
    setLocale("de");
    registerCommand({
      id: "Annotate",
      title: "Annotate",
      run: () => {
        throw new Error("boom");
      },
    });
    await runAction("Annotate");
    expect(balloons().at(-1)?.text).toMatch(/ fehlgeschlagen$/);
    await runAction("Vcs.Push");
    expect(balloons().at(-1)?.text).toMatch(/ ist noch nicht verfügbar$/);
    expect(balloons().at(-1)?.detail).toBe("Diese Aktion kommt mit einem späteren Forge-Update.");
  });
});
