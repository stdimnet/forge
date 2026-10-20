import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import "@/mock/settingsMock";
import "@/mock/keymapMock";
import { registerCommand } from "@/state/commands";
import { balloons, focusContextOf, installKeymap, resolveCommand, runAction, suspendKeymap } from "./dispatch";
import { resetKeymap, setActionShortcuts, setKeymapId, startKeymap } from "./store";

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
      <section class="tool-window"><button id="tw">x</button>
        <div class="xterm"><textarea id="term"></textarea></div></section>
      <button id="plain">y</button>`;
    const q = (s: string) => document.querySelector(s);
    expect(focusContextOf(q(".cm-content"))).toBe("editor");
    expect(focusContextOf(q("#cm-search"))).toBe("input");
    expect(focusContextOf(q("#text"))).toBe("input");
    expect(focusContextOf(q("#check"))).toBe("global");
    expect(focusContextOf(q("#tw"))).toBe("toolwindow");
    expect(focusContextOf(q("#term"))).toBe("terminal");
    expect(focusContextOf(q("#plain"))).toBe("global");
    expect(focusContextOf(null)).toBe("global");
  });
});

describe("installed keymap", () => {
  const gotoClass = vi.fn();
  const search = vi.fn();
  beforeAll(async () => {
    await installKeymap(window);
    await setKeymapId("$default");
    await resetKeymap();
    registerCommand({ id: "GotoClass", title: "Class", run: gotoClass });
    registerCommand({ id: "SearchEverywhere", title: "Search Everywhere", run: search });
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

  it("can be suspended while recording shortcuts", () => {
    const resume = suspendKeymap();
    const e = press(document.getElementById("btn")!, { key: "n", code: "KeyN", ctrlKey: true });
    resume();
    expect(e.defaultPrevented).toBe(false);
    expect(gotoClass).not.toHaveBeenCalled();
  });
});
