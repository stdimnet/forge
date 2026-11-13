import { beforeEach, describe, expect, it, vi } from "vitest";

/** `theme.ts` reads localStorage once at import time, so each case imports a fresh copy. */
async function load(stored: string | null) {
  vi.resetModules();
  localStorage.clear();
  if (stored !== null) localStorage.setItem("forge.theme", stored);
  return import("./theme");
}

describe("theme", () => {
  beforeEach(() => {
    delete document.documentElement.dataset.theme;
  });

  it("starts with the stored theme", async () => {
    const { theme } = await load("solarized-dark");
    expect(theme()).toBe("solarized-dark");
  });

  it("falls back to dark for a missing or unknown stored value", async () => {
    expect((await load(null)).theme()).toBe("dark");
    expect((await load("neon")).theme()).toBe("dark");
  });

  it("toggles within the theme family", async () => {
    const { theme, setTheme, toggleTheme } = await load(null);
    toggleTheme();
    expect(theme()).toBe("light");
    toggleTheme();
    expect(theme()).toBe("dark");
    setTheme("solarized-dark");
    toggleTheme();
    expect(theme()).toBe("solarized-light");
    toggleTheme();
    expect(theme()).toBe("solarized-dark");
    setTheme("high-contrast");
    toggleTheme();
    expect(theme()).toBe("high-contrast-light");
    toggleTheme();
    expect(theme()).toBe("high-contrast");
  });

  it("knows dark and high-contrast themes", async () => {
    const { isDarkTheme, isHighContrastTheme } = await load(null);
    expect(isDarkTheme("high-contrast")).toBe(true);
    expect(isDarkTheme("high-contrast-light")).toBe(false);
    expect(isHighContrastTheme("high-contrast-light")).toBe(true);
    expect(isHighContrastTheme("solarized-dark")).toBe(false);
  });

  it("applies and persists the theme", async () => {
    const { setTheme } = await load(null);
    setTheme("solarized-light");
    expect(document.documentElement.dataset.theme).toBe("solarized-light");
    expect(localStorage.getItem("forge.theme")).toBe("solarized-light");
  });
});
