import { describe, expect, it } from "vitest";
import {
  canonicalShortcut,
  eventKey,
  formatShortcutText,
  parseShortcut,
  parseStroke,
  shortcutId,
  strokeFromEvent,
  strokeId,
  type KeyEventLike,
} from "./shortcut";

const ev = (code: string, key: string, mods: Partial<KeyEventLike> = {}): KeyEventLike => ({
  code,
  key,
  ctrlKey: false,
  altKey: false,
  shiftKey: false,
  metaKey: false,
  ...mods,
});

describe("parseStroke", () => {
  it("parses modifiers in any order and normalizes the key", () => {
    expect(strokeId(parseStroke("shift+ctrl+a"))).toBe("Ctrl+Shift+A");
    expect(strokeId(parseStroke("Ctrl+Alt+Shift+T"))).toBe("Ctrl+Alt+Shift+T");
    expect(strokeId(parseStroke("Alt+Insert"))).toBe("Alt+Insert");
    expect(strokeId(parseStroke("Ctrl+F12"))).toBe("Ctrl+F12");
    expect(strokeId(parseStroke("Alt+Shift+Up"))).toBe("Alt+Shift+Up");
  });

  it("understands macOS names and symbols", () => {
    expect(strokeId(parseStroke("Cmd+Shift+A"))).toBe("Shift+Meta+A");
    expect(strokeId(parseStroke("⌘+,"))).toBe("Meta+,");
    expect(strokeId(parseStroke("Opt+Enter"))).toBe("Alt+Enter");
  });

  it("maps Mod to Cmd on macOS and Ctrl elsewhere", () => {
    expect(strokeId(parseStroke("Mod+S", "mac"))).toBe("Meta+S");
    expect(strokeId(parseStroke("Mod+S", "other"))).toBe("Ctrl+S");
  });

  it("handles punctuation and numpad keys including '+'", () => {
    expect(strokeId(parseStroke("Ctrl+/"))).toBe("Ctrl+/");
    expect(strokeId(parseStroke("Ctrl+-"))).toBe("Ctrl+-");
    expect(strokeId(parseStroke("Ctrl+="))).toBe("Ctrl+=");
    expect(strokeId(parseStroke("Ctrl+NumPad+"))).toBe("Ctrl+NumPad+");
    expect(strokeId(parseStroke("Ctrl+Shift+NumPad-"))).toBe("Ctrl+Shift+NumPad-");
    expect(strokeId(parseStroke("Alt+`"))).toBe("Alt+`");
    expect(strokeId(parseStroke("Ctrl+Shift+'"))).toBe("Ctrl+Shift+'");
  });

  it("accepts key aliases", () => {
    expect(strokeId(parseStroke("Shift+Esc"))).toBe("Shift+Escape");
    expect(strokeId(parseStroke("ctrl+back_space"))).toBe("Ctrl+Backspace");
    expect(strokeId(parseStroke("Alt+ArrowLeft"))).toBe("Alt+Left");
  });

  it("rejects garbage", () => {
    expect(() => parseStroke("Ctrl+Foo")).toThrow();
    expect(() => parseStroke("Hyper+A")).toThrow();
    expect(() => parseStroke("")).toThrow();
  });
});

describe("parseShortcut", () => {
  it("parses double taps", () => {
    expect(parseShortcut("Shift Shift")).toEqual({ kind: "double", modifier: "Shift" });
    expect(parseShortcut("Double Shift")).toEqual({ kind: "double", modifier: "Shift" });
    expect(parseShortcut("Ctrl Ctrl")).toEqual({ kind: "double", modifier: "Ctrl" });
    expect(() => parseShortcut("Double A")).toThrow();
  });

  it("parses chords with space or comma", () => {
    const a = parseShortcut("Ctrl+K Ctrl+C");
    const b = parseShortcut("Ctrl+K, Ctrl+C");
    expect(a.kind).toBe("keys");
    expect(shortcutId(a)).toBe("Ctrl+K Ctrl+C");
    expect(shortcutId(b)).toBe("Ctrl+K Ctrl+C");
  });

  it("rejects three-stroke and key-less shortcuts", () => {
    expect(() => parseShortcut("Ctrl+K Ctrl+C Ctrl+D")).toThrow();
    expect(() => parseShortcut("Ctrl+Shift")).toThrow();
    expect(() => parseShortcut("Shift Ctrl")).toThrow();
  });

  it("canonicalizes equivalent spellings", () => {
    expect(canonicalShortcut("shift+ctrl+a")).toBe(canonicalShortcut("Ctrl+Shift+A"));
    expect(canonicalShortcut("nonsense+")).toBeNull();
  });
});

describe("formatting", () => {
  it("formats Windows/Linux text", () => {
    expect(formatShortcutText("Ctrl+Shift+A", "other")).toBe("Ctrl+Shift+A");
    expect(formatShortcutText("Double Shift", "other")).toBe("Double Shift");
    expect(formatShortcutText("Ctrl+K Ctrl+C", "other")).toBe("Ctrl+K, Ctrl+C");
    expect(formatShortcutText("Ctrl+NumPad+", "other")).toBe("Ctrl+NumPad +");
  });

  it("formats macOS symbols in PHPStorm order", () => {
    expect(formatShortcutText("Cmd+Shift+A", "mac")).toBe("⇧⌘A");
    expect(formatShortcutText("Ctrl+Alt+Shift+Cmd+Up", "mac")).toBe("⌃⌥⇧⌘↑");
    expect(formatShortcutText("Alt+Enter", "mac")).toBe("⌥⏎");
    expect(formatShortcutText("Double Shift", "mac")).toBe("Double⇧");
  });
});

describe("events", () => {
  it("uses physical codes so Alt/Option chars do not matter", () => {
    expect(eventKey(ev("KeyJ", "∆", { altKey: true }))).toBe("J");
    expect(eventKey(ev("Digit1", "¡", { altKey: true }))).toBe("1");
    expect(eventKey(ev("Slash", "/"))).toBe("/");
    expect(eventKey(ev("NumpadAdd", "+"))).toBe("NumPad+");
    expect(eventKey(ev("ArrowUp", "ArrowUp"))).toBe("Up");
    expect(eventKey(ev("F12", "F12"))).toBe("F12");
    expect(eventKey(ev("ShiftLeft", "Shift", { shiftKey: true }))).toBe("");
  });

  it("falls back to key when code is unknown", () => {
    expect(eventKey(ev("", "Escape"))).toBe("Escape");
  });

  it("builds strokes from events", () => {
    const s = strokeFromEvent(ev("KeyA", "A", { ctrlKey: true, shiftKey: true }));
    expect(strokeId(s)).toBe("Ctrl+Shift+A");
  });
});

describe("comma key vs. chord separator", () => {
  it("keeps ',' as a key", () => {
    expect(shortcutId(parseShortcut("Cmd+,"))).toBe("Meta+,");
    expect(shortcutId(parseShortcut("Cmd+, Cmd+."))).toBe("Meta+, Meta+.");
  });
});
