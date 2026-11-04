import { describe, expect, it } from "vitest";
import { checkTheme, contrastRatio, levelOf, parseColor, themeTokens } from "./contrast";
import { THEMES } from "@/state/theme";

/** theme.css as text: vitest runs in Node with app/ui as working directory. */
function readThemeCss(): string {
  const proc = (globalThis as unknown as {
    process: { cwd(): string; getBuiltinModule(id: string): { readFileSync(path: string, enc: string): string } };
  }).process;
  return proc.getBuiltinModule("node:fs").readFileSync(`${proc.cwd()}/src/theme/theme.css`, "utf8");
}

const tokens = themeTokens(readThemeCss());

/**
 * Colors that belong to the Solarized palette itself (its accent and syntax hues are fixed by
 * design and stay below 4.5:1 on its backgrounds); Forge's own themes have no exceptions.
 */
const PALETTE_EXCEPTIONS: Record<string, RegExp> = {
  "solarized-dark": /^--(syn-|accent-fg|danger|info|vcs-)/,
  "solarized-light": /^--(syn-|accent-fg|danger|warning|success|info|vcs-)/,
};

describe("contrast math", () => {
  it("computes WCAG ratios", () => {
    expect(contrastRatio([0, 0, 0], [255, 255, 255])).toBeCloseTo(21, 5);
    expect(contrastRatio([255, 255, 255], [255, 255, 255])).toBe(1);
    // #767676 on white is the classic 4.54:1.
    expect(contrastRatio([0x76, 0x76, 0x76], [255, 255, 255])).toBeCloseTo(4.54, 2);
  });

  it("parses hex and rgba colors", () => {
    expect(parseColor("#abc")).toEqual([0xaa, 0xbb, 0xcc, 1]);
    expect(parseColor("rgba(255, 122, 26, 0.14)")).toEqual([255, 122, 26, 0.14]);
    expect(parseColor("var(--x)")).toBeNull();
  });
});

describe("theme contrast", () => {
  it("defines every theme", () => {
    for (const theme of THEMES) expect(tokens.has(theme), theme).toBe(true);
  });

  for (const theme of THEMES) {
    const level = levelOf(theme);
    it(`${theme} meets WCAG ${level}`, () => {
      const skip = PALETTE_EXCEPTIONS[theme];
      const failures = checkTheme(tokens.get(theme)!, level)
        .filter((r) => !r.ok && !(skip && skip.test(r.fg)))
        .map((r) => `${r.fg} on ${r.bg}: ${r.ratio.toFixed(2)} < ${r.min}`);
      expect(failures).toEqual([]);
    });
  }

  it("checks the high-contrast themes at AAA", () => {
    expect(levelOf("high-contrast")).toBe("AAA");
    expect(levelOf("high-contrast-light")).toBe("AAA");
    expect(levelOf("dark")).toBe("AA");
  });
});
