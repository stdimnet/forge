import { describe, expect, it } from "vitest";
import {
  KeymapEngine,
  allConflicts,
  allowedInInput,
  allowedInTerminal,
  buildBindings,
  contextMatches,
  findConflicts,
  type Binding,
} from "./engine";
import { parseShortcut, parseStroke, type KeyEventLike } from "./shortcut";
import type { ActionContext } from "./phpstormKeymap";

const CODES: Record<string, string> = { "/": "Slash", Up: "ArrowUp", Insert: "Insert", Enter: "Enter", F6: "F6", Escape: "Escape" };

/** Build a keydown-like event from "Ctrl+Shift+A". */
function key(text: string, extra: Partial<KeyEventLike> = {}): KeyEventLike {
  const s = parseStroke(text);
  const code = /^[A-Z]$/.test(s.key) ? `Key${s.key}` : /^\d$/.test(s.key) ? `Digit${s.key}` : (CODES[s.key] ?? s.key);
  return { code, key: s.key, ctrlKey: s.ctrl, altKey: s.alt, shiftKey: s.shift, metaKey: s.meta, ...extra };
}

const mod = (name: "Shift" | "Control" | "Alt" | "Meta", down = true): KeyEventLike => ({
  code: `${name}Left`,
  key: name,
  ctrlKey: name === "Control" && down,
  altKey: name === "Alt" && down,
  shiftKey: name === "Shift" && down,
  metaKey: name === "Meta" && down,
});

const contexts: Record<string, ActionContext> = { Generate: "editor", EditorDuplicate: "editor", Rerun: "toolwindow" };

function makeEngine(map: Record<string, string[]>, now?: () => number) {
  const bindings = buildBindings(map, (id) => contexts[id] ?? "any", "other");
  return new KeymapEngine(bindings, { now });
}

describe("KeymapEngine single strokes", () => {
  const engine = makeEngine({
    GotoAction: ["Ctrl+Shift+A"],
    Generate: ["Alt+Insert"],
    NewElement: ["Alt+Insert"],
    RenameElement: ["Shift+F6"],
    EditorDuplicate: ["Ctrl+D"],
    Rerun: ["Ctrl+F5"],
    JumpToLastWindow: ["F12"],
  });

  it("resolves bound keystrokes", () => {
    expect(engine.handleKeyDown(key("Ctrl+Shift+A"), "global")).toEqual({ action: "GotoAction", consumed: true });
  });

  it("ignores unbound keystrokes", () => {
    expect(engine.handleKeyDown(key("Ctrl+Shift+Q"), "global")).toEqual({ consumed: false });
  });

  it("prefers context-specific bindings in their context", () => {
    expect(engine.handleKeyDown(key("Alt+Insert"), "editor").action).toBe("Generate");
    expect(engine.handleKeyDown(key("Alt+Insert"), "global").action).toBe("NewElement");
  });

  it("does not fire editor actions outside the editor", () => {
    expect(engine.handleKeyDown(key("Ctrl+D"), "global")).toEqual({ consumed: false });
    expect(engine.handleKeyDown(key("Ctrl+D"), "editor").action).toBe("EditorDuplicate");
  });

  it("fires tool window actions only in tool windows", () => {
    expect(engine.handleKeyDown(key("Ctrl+F5"), "editor").consumed).toBe(false);
    expect(engine.handleKeyDown(key("Ctrl+F5"), "toolwindow").action).toBe("Rerun");
  });

  it("lets text inputs keep plain keys but allows real shortcuts", () => {
    const plain = makeEngine({ X: ["F"], Y: ["Shift+F"], Z: ["Ctrl+A"] });
    expect(plain.handleKeyDown(key("F"), "input").consumed).toBe(false);
    expect(plain.handleKeyDown(key("Shift+F"), "input").consumed).toBe(false);
    expect(plain.handleKeyDown(key("Ctrl+A"), "input").consumed).toBe(false);
    expect(plain.handleKeyDown(key("F"), "global").action).toBe("X");
    expect(engine.handleKeyDown(key("Ctrl+Shift+A"), "input").action).toBe("GotoAction");
    expect(engine.handleKeyDown(key("Shift+F6"), "input").action).toBe("RenameElement");
  });
});

describe("KeymapEngine chords", () => {
  it("waits for the second keystroke", () => {
    const engine = makeEngine({ CommentLine: ["Ctrl+K Ctrl+C"], Uncomment: ["Ctrl+K Ctrl+U"] });
    expect(engine.handleKeyDown(key("Ctrl+K"), "editor")).toEqual({ consumed: true });
    expect(engine.chordPending).toBe(true);
    expect(engine.handleKeyDown(key("Ctrl+U"), "editor")).toEqual({ action: "Uncomment", consumed: true });
    expect(engine.chordPending).toBe(false);
  });

  it("swallows an unknown second keystroke and recovers", () => {
    const engine = makeEngine({ CommentLine: ["Ctrl+K Ctrl+C"], Find: ["Ctrl+F"] });
    engine.handleKeyDown(key("Ctrl+K"), "global");
    expect(engine.handleKeyDown(key("Ctrl+F"), "global")).toEqual({ consumed: true });
    expect(engine.handleKeyDown(key("Ctrl+F"), "global").action).toBe("Find");
  });

  it("ignores pure modifier presses between chord strokes", () => {
    const engine = makeEngine({ X: ["Ctrl+K Shift+C"] });
    engine.handleKeyDown(key("Ctrl+K"), "global");
    engine.handleKeyDown(mod("Shift"), "global");
    expect(engine.handleKeyDown(key("Shift+C"), "global").action).toBe("X");
  });
});

describe("KeymapEngine double taps", () => {
  function tapEngine() {
    let t = 0;
    const engine = makeEngine({ SearchEverywhere: ["Shift Shift"] }, () => t);
    const tap = (at: number, focus: "global" | "input" = "global") => {
      t = at;
      engine.handleKeyDown(mod("Shift"), focus);
      return engine.handleKeyUp(mod("Shift", false), focus);
    };
    return { engine, tap, setTime: (v: number) => (t = v) };
  }

  it("fires on two quick taps", () => {
    const { tap } = tapEngine();
    expect(tap(0).action).toBeUndefined();
    expect(tap(200)).toEqual({ action: "SearchEverywhere", consumed: true });
  });

  it("works while typing in an input", () => {
    const { tap } = tapEngine();
    tap(0, "input");
    expect(tap(150, "input").action).toBe("SearchEverywhere");
  });

  it("does not fire when taps are too slow", () => {
    const { tap } = tapEngine();
    tap(0);
    expect(tap(900).action).toBeUndefined();
    expect(tap(1000).action).toBe("SearchEverywhere");
  });

  it("does not fire when Shift was used as a modifier", () => {
    const { engine, tap } = tapEngine();
    tap(0);
    engine.handleKeyDown(mod("Shift"), "global");
    engine.handleKeyDown(key("Shift+A"), "global");
    expect(engine.handleKeyUp(mod("Shift", false), "global").action).toBeUndefined();
  });

  it("is broken by another key between the taps", () => {
    const { engine, tap } = tapEngine();
    tap(0);
    engine.handleKeyDown(key("A"), "global");
    expect(tap(100).action).toBeUndefined();
  });

  it("is broken by interrupt (mouse click)", () => {
    const { engine, tap } = tapEngine();
    tap(0);
    engine.interrupt();
    expect(tap(100).action).toBeUndefined();
  });

  it("ignores auto-repeat of a held modifier", () => {
    const { engine, tap } = tapEngine();
    tap(0);
    engine.handleKeyDown({ ...mod("Shift"), repeat: true }, "global");
    expect(engine.handleKeyUp(mod("Shift", false), "global").action).toBeUndefined();
  });
});

describe("conflicts", () => {
  const bindings: Binding[] = buildBindings(
    {
      GotoClass: ["Ctrl+N"],
      CheckinProject: ["Ctrl+K"],
      Generate: ["Alt+Insert"],
      NewElement: ["Alt+Insert"],
      Chord: ["Ctrl+K Ctrl+C"],
    },
    (id) => contexts[id] ?? "any",
    "other",
  );

  it("finds identical shortcuts", () => {
    const c = findConflicts(bindings, parseShortcut("Ctrl+N"), "Other");
    expect(c.map((x) => [x.actionId, x.kind])).toEqual([["GotoClass", "same"]]);
  });

  it("does not report the action itself", () => {
    expect(findConflicts(bindings, parseShortcut("Ctrl+N"), "GotoClass")).toEqual([]);
  });

  it("finds chord prefix clashes both ways", () => {
    expect(findConflicts(bindings, parseShortcut("Ctrl+K Ctrl+X"), "New").map((c) => c.actionId)).toEqual([
      "CheckinProject",
    ]);
    expect(findConflicts(bindings, parseShortcut("Ctrl+K"), "New").map((c) => c.actionId)).toEqual([
      "CheckinProject",
      "Chord",
    ]);
  });

  it("lists all conflicting shortcuts", () => {
    const all = allConflicts(bindings);
    expect(all.get("Alt+Insert")).toEqual(["Generate", "NewElement"]);
    expect(all.has("Ctrl+N")).toBe(false);
  });
});

describe("helpers", () => {
  it("contextMatches", () => {
    expect(contextMatches("any", "input")).toBe(true);
    expect(contextMatches("editor", "editor")).toBe(true);
    expect(contextMatches("editor", "input")).toBe(false);
    expect(contextMatches("toolwindow", "global")).toBe(false);
  });

  it("allowedInInput", () => {
    expect(allowedInInput(parseStroke("Ctrl+Shift+A"))).toBe(true);
    expect(allowedInInput(parseStroke("F2"))).toBe(true);
    expect(allowedInInput(parseStroke("Ctrl+V"))).toBe(false);
    expect(allowedInInput(parseStroke("Meta+Left"))).toBe(false);
    expect(allowedInInput(parseStroke("Shift+A"))).toBe(false);
    expect(allowedInInput(parseStroke("Escape"))).toBe(false);
  });

  it("leaves shell keys to the terminal but keeps IDE function keys and tool windows", () => {
    const allowed = (s: string) => allowedInTerminal(parseStroke(s));
    for (const s of ["Ctrl+C", "Ctrl+D", "Ctrl+R", "Ctrl+E", "Alt+B", "Escape", "Tab", "Ctrl+Shift+C", "Ctrl+Shift+V"])
      expect(allowed(s), s).toBe(false);
    for (const s of ["Alt+F12", "Shift+F10", "Ctrl+F2", "Alt+4", "Ctrl+Shift+A", "Ctrl+Alt+S", "Ctrl+Shift+T"])
      expect(allowed(s), s).toBe(true);
    const engine = new KeymapEngine();
    engine.setBindings(buildBindings({ RecentFiles: ["Ctrl+E"], Run: ["Shift+F10"] }, () => "any", "other"));
    expect(engine.handleKeyDown(key("Ctrl+E"), "terminal")).toEqual({ consumed: false });
    expect(engine.handleKeyDown(key("Shift+F10"), "terminal").action).toBe("Run");
  });

  it("buildBindings skips unparsable strings", () => {
    expect(buildBindings({ A: ["Ctrl+Nope", "Ctrl+A"] }, () => "any", "other")).toHaveLength(1);
  });
});
