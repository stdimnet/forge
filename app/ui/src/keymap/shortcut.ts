/**
 * Keyboard shortcut model: parsing of PHPStorm-style strings, formatting for display
 * (Windows/Linux text or macOS symbols) and conversion from DOM keyboard events.
 *
 *   "Ctrl+Shift+A"        one keystroke
 *   "Ctrl+K Ctrl+C"       chord (two keystrokes; "Ctrl+K, Ctrl+C" is accepted too)
 *   "Shift Shift"         double-tap of a modifier ("Double Shift" is accepted too)
 *   "Cmd+," / "Meta+,"    macOS command key; "Mod+S" is Cmd on macOS and Ctrl elsewhere
 */

export type Platform = "mac" | "other";
export type Modifier = "Ctrl" | "Alt" | "Shift" | "Meta";

export interface KeyStroke {
  ctrl: boolean;
  alt: boolean;
  shift: boolean;
  meta: boolean;
  /** Canonical key name ("A", "1", "F12", "Up", "Enter", "/", "NumPad+") or "" for a bare modifier. */
  key: string;
}

export type Shortcut =
  | { kind: "keys"; strokes: KeyStroke[] }
  | { kind: "double"; modifier: Modifier };

export const detectPlatform = (): Platform =>
  typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent)
    ? "mac"
    : "other";

const MODIFIER_ALIASES: Record<string, Modifier | "Mod"> = {
  ctrl: "Ctrl",
  control: "Ctrl",
  "⌃": "Ctrl",
  alt: "Alt",
  opt: "Alt",
  option: "Alt",
  "⌥": "Alt",
  shift: "Shift",
  "⇧": "Shift",
  meta: "Meta",
  cmd: "Meta",
  command: "Meta",
  "⌘": "Meta",
  win: "Meta",
  mod: "Mod",
};

const KEY_ALIASES: Record<string, string> = {
  esc: "Escape",
  escape: "Escape",
  del: "Delete",
  delete: "Delete",
  ins: "Insert",
  insert: "Insert",
  return: "Enter",
  enter: "Enter",
  backspace: "Backspace",
  back_space: "Backspace",
  tab: "Tab",
  space: "Space",
  home: "Home",
  end: "End",
  pageup: "PageUp",
  page_up: "PageUp",
  pgup: "PageUp",
  pagedown: "PageDown",
  page_down: "PageDown",
  pgdn: "PageDown",
  up: "Up",
  down: "Down",
  left: "Left",
  right: "Right",
  arrowup: "Up",
  arrowdown: "Down",
  arrowleft: "Left",
  arrowright: "Right",
  backquote: "`",
  back_quote: "`",
  minus: "-",
  equals: "=",
  equal: "=",
  plus: "=",
  slash: "/",
  backslash: "\\",
  back_slash: "\\",
  period: ".",
  comma: ",",
  semicolon: ";",
  quote: "'",
  openbracket: "[",
  open_bracket: "[",
  closebracket: "]",
  close_bracket: "]",
  contextmenu: "ContextMenu",
  "numpad+": "NumPad+",
  "numpad-": "NumPad-",
  "numpad*": "NumPad*",
  "numpad/": "NumPad/",
  "numpad.": "NumPad.",
  add: "NumPad+",
  subtract: "NumPad-",
  multiply: "NumPad*",
  divide: "NumPad/",
};

/** Normalize a key token from a shortcut string to its canonical name. */
export function normalizeKey(token: string): string | null {
  const t = token.trim();
  if (!t) return null;
  const alias = KEY_ALIASES[t.toLowerCase()];
  if (alias) return alias;
  if (/^f([1-9]|1\d|2[0-4])$/i.test(t)) return t.toUpperCase();
  if (/^numpad\d$/i.test(t)) return `NumPad${t.slice(-1)}`;
  if (t.length === 1) return t.toUpperCase();
  return null;
}

function emptyStroke(): KeyStroke {
  return { ctrl: false, alt: false, shift: false, meta: false, key: "" };
}

function splitStrokeTokens(s: string): string[] {
  // "+" separates tokens, but a trailing "+" is the key itself ("Ctrl+NumPad+", "Ctrl++").
  let tail: string | null = null;
  if (/numpad\+$/i.test(s)) {
    tail = "NumPad+";
    s = s.slice(0, -7);
  } else if (s.endsWith("++") || s === "+") {
    tail = "+";
    s = s.slice(0, s === "+" ? -1 : -2);
  }
  const tokens = s.split("+").filter((t) => t.length > 0);
  if (tail) tokens.push(tail);
  return tokens;
}

/** Parse one keystroke such as "Ctrl+Alt+Shift+T". Throws on unknown tokens. */
export function parseStroke(text: string, platform: Platform = "other"): KeyStroke {
  const stroke = emptyStroke();
  const tokens = splitStrokeTokens(text.trim());
  if (!tokens.length) throw new Error(`empty keystroke`);
  tokens.forEach((raw, i) => {
    const mod = MODIFIER_ALIASES[raw.trim().toLowerCase()];
    if (mod && (i < tokens.length - 1 || tokens.length === 1 || !stroke.key)) {
      const m = mod === "Mod" ? (platform === "mac" ? "Meta" : "Ctrl") : mod;
      stroke[m.toLowerCase() as "ctrl" | "alt" | "shift" | "meta"] = true;
      return;
    }
    if (i !== tokens.length - 1) throw new Error(`unknown modifier "${raw}" in "${text}"`);
    const key = normalizeKey(raw);
    if (key == null) throw new Error(`unknown key "${raw}" in "${text}"`);
    stroke.key = key;
  });
  return stroke;
}

const modifierCount = (s: KeyStroke) => +s.ctrl + +s.alt + +s.shift + +s.meta;

function soleModifier(s: KeyStroke): Modifier | null {
  if (s.key || modifierCount(s) !== 1) return null;
  return s.ctrl ? "Ctrl" : s.alt ? "Alt" : s.shift ? "Shift" : "Meta";
}

/** Parse a full shortcut: single keystroke, two-stroke chord or modifier double-tap. */
export function parseShortcut(text: string, platform: Platform = "other"): Shortcut {
  const trimmed = text.trim();
  const dbl = /^double\s+(\S+)$/i.exec(trimmed);
  if (dbl) {
    const mod = soleModifier(parseStroke(dbl[1], platform));
    if (!mod) throw new Error(`"${text}": only modifiers can be double-tapped`);
    return { kind: "double", modifier: mod };
  }
  // Strokes are separated by whitespace; a trailing "," on a stroke ("Ctrl+K, Ctrl+C") is a
  // separator unless it is the key itself ("Cmd+,").
  const parts = trimmed
    .split(/\s+/)
    .map((p) => (p.length > 1 && p.endsWith(",") && !p.endsWith("+,") ? p.slice(0, -1) : p))
    .filter((p) => p && p !== ",");
  if (!parts.length) throw new Error("empty shortcut");
  if (parts.length > 2) throw new Error(`"${text}": at most two keystrokes are supported`);
  const strokes = parts.map((p) => parseStroke(p, platform));
  if (strokes.length === 2) {
    const [a, b] = strokes.map(soleModifier);
    if (a && a === b) return { kind: "double", modifier: a };
  }
  if (strokes.some((s) => !s.key)) throw new Error(`"${text}": a keystroke needs a key`);
  return { kind: "keys", strokes };
}

export function tryParseShortcut(text: string, platform: Platform = "other"): Shortcut | null {
  try {
    return parseShortcut(text, platform);
  } catch {
    return null;
  }
}

/** Canonical, platform-independent id of a keystroke ("Ctrl+Alt+Shift+Meta+Key"). */
export function strokeId(s: KeyStroke): string {
  const parts: string[] = [];
  if (s.ctrl) parts.push("Ctrl");
  if (s.alt) parts.push("Alt");
  if (s.shift) parts.push("Shift");
  if (s.meta) parts.push("Meta");
  if (s.key) parts.push(s.key);
  return parts.join("+");
}

/** Canonical id of a shortcut, used for lookups and conflict detection. */
export function shortcutId(s: Shortcut): string {
  return s.kind === "double" ? `Double ${s.modifier}` : s.strokes.map(strokeId).join(" ");
}

/** Canonicalize a user-entered shortcut string; returns null when it does not parse. */
export function canonicalShortcut(text: string, platform: Platform = "other"): string | null {
  const s = tryParseShortcut(text, platform);
  return s ? shortcutId(s) : null;
}

const MAC_KEY_SYMBOLS: Record<string, string> = {
  Enter: "⏎",
  Backspace: "⌫",
  Delete: "⌦",
  Escape: "⎋",
  Tab: "⇥",
  Up: "↑",
  Down: "↓",
  Left: "←",
  Right: "→",
  PageUp: "⇞",
  PageDown: "⇟",
  Home: "↖",
  End: "↘",
  Space: "Space",
};
const TEXT_KEY_NAMES: Record<string, string> = {
  Up: "Up",
  Down: "Down",
  Left: "Left",
  Right: "Right",
  "NumPad+": "NumPad +",
  "NumPad-": "NumPad -",
  "NumPad*": "NumPad *",
  "NumPad/": "NumPad /",
  "NumPad.": "NumPad .",
};
const MAC_MODIFIER_SYMBOLS: Record<Modifier, string> = { Ctrl: "⌃", Alt: "⌥", Shift: "⇧", Meta: "⌘" };

/** Display text for a keystroke, PHPStorm style ("Ctrl+Shift+A" or "⇧⌘A"). */
export function formatStroke(s: KeyStroke, platform: Platform): string {
  if (platform === "mac") {
    let out = "";
    if (s.ctrl) out += "⌃";
    if (s.alt) out += "⌥";
    if (s.shift) out += "⇧";
    if (s.meta) out += "⌘";
    return out + (MAC_KEY_SYMBOLS[s.key] ?? s.key);
  }
  const parts: string[] = [];
  if (s.ctrl) parts.push("Ctrl");
  if (s.alt) parts.push("Alt");
  if (s.shift) parts.push("Shift");
  if (s.meta) parts.push("Meta");
  if (s.key) parts.push(TEXT_KEY_NAMES[s.key] ?? s.key);
  return parts.join("+");
}

export function formatShortcut(s: Shortcut, platform: Platform): string {
  if (s.kind === "double") {
    return platform === "mac" ? `Double${MAC_MODIFIER_SYMBOLS[s.modifier]}` : `Double ${s.modifier}`;
  }
  return s.strokes.map((k) => formatStroke(k, platform)).join(", ");
}

/** Format a shortcut string for display; unparsable strings are returned unchanged. */
export function formatShortcutText(text: string, platform: Platform): string {
  const s = tryParseShortcut(text, platform);
  return s ? formatShortcut(s, platform) : text;
}

/** The subset of KeyboardEvent the keymap needs (makes the engine testable). */
export interface KeyEventLike {
  key: string;
  code: string;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  metaKey: boolean;
  repeat?: boolean;
}

const CODE_KEYS: Record<string, string> = {
  Minus: "-",
  Equal: "=",
  BracketLeft: "[",
  BracketRight: "]",
  Backslash: "\\",
  IntlBackslash: "\\",
  Semicolon: ";",
  Quote: "'",
  Backquote: "`",
  Comma: ",",
  Period: ".",
  Slash: "/",
  Space: "Space",
  Enter: "Enter",
  NumpadEnter: "Enter",
  Escape: "Escape",
  Backspace: "Backspace",
  Tab: "Tab",
  Delete: "Delete",
  Insert: "Insert",
  Home: "Home",
  End: "End",
  PageUp: "PageUp",
  PageDown: "PageDown",
  ArrowUp: "Up",
  ArrowDown: "Down",
  ArrowLeft: "Left",
  ArrowRight: "Right",
  ContextMenu: "ContextMenu",
  NumpadAdd: "NumPad+",
  NumpadSubtract: "NumPad-",
  NumpadMultiply: "NumPad*",
  NumpadDivide: "NumPad/",
  NumpadDecimal: "NumPad.",
};

const MODIFIER_KEYS = new Set(["Shift", "Control", "Alt", "Meta", "AltGraph", "OS", "Hyper", "Super"]);

/**
 * Key name of an event. Uses the physical `code` for letters/digits/punctuation so that
 * shortcuts work regardless of layout and of Alt/Option producing special characters.
 */
export function eventKey(e: KeyEventLike): string {
  if (MODIFIER_KEYS.has(e.key)) return "";
  const c = e.code ?? "";
  if (/^Key[A-Z]$/.test(c)) return c.slice(3);
  if (/^Digit\d$/.test(c)) return c.slice(5);
  if (/^Numpad\d$/.test(c)) return `NumPad${c.slice(6)}`;
  if (/^F\d{1,2}$/.test(c)) return c;
  if (CODE_KEYS[c]) return CODE_KEYS[c];
  return normalizeKey(e.key) ?? e.key;
}

export function strokeFromEvent(e: KeyEventLike): KeyStroke {
  return { ctrl: e.ctrlKey, alt: e.altKey, shift: e.shiftKey, meta: e.metaKey, key: eventKey(e) };
}

/** Which modifier a bare modifier key event is about (for double-tap detection). */
export function modifierOfEvent(e: KeyEventLike): Modifier | null {
  switch (e.key) {
    case "Shift":
      return "Shift";
    case "Control":
      return "Ctrl";
    case "Alt":
      return "Alt";
    case "Meta":
    case "OS":
      return "Meta";
    default:
      return null;
  }
}
