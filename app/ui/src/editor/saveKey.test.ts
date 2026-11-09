import { describe, expect, it } from "vitest";

// Every non-test source of the UI, as text.
const sources = import.meta.glob<string>(["../**/*.{ts,tsx}", "!../**/*.test.{ts,tsx}", "!../wasm/**", "!../mock/**"], {
  query: "?raw",
  import: "default",
  eager: true,
});

describe("saving from the editor", () => {
  it("goes through the keymap (SaveAll), not a CodeMirror Mod-s binding", () => {
    expect(Object.keys(sources).length).toBeGreaterThan(100);
    const bound = Object.entries(sources)
      .filter(([, text]) => /key:\s*["']Mod-s["']/.test(text))
      .map(([file]) => file);
    expect(bound).toEqual([]);
  });
});
