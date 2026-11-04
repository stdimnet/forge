import { createSignal } from "solid-js";

export type ThemeName = "dark" | "light" | "solarized-dark" | "solarized-light" | "high-contrast" | "high-contrast-light";

export const THEMES: readonly ThemeName[] = [
  "dark",
  "light",
  "solarized-dark",
  "solarized-light",
  "high-contrast",
  "high-contrast-light",
];

/** Whether a theme is dark (the editor color scheme that follows it is a dark one). */
export const isDarkTheme = (t: ThemeName): boolean => t === "dark" || t === "solarized-dark" || t === "high-contrast";

/** Whether a theme is one of the High Contrast themes (WCAG AAA, borders, strong focus rings). */
export const isHighContrastTheme = (t: ThemeName): boolean => t.startsWith("high-contrast");

const isTheme = (v: unknown): v is ThemeName => THEMES.includes(v as ThemeName);

const stored = (() => {
  try {
    return localStorage.getItem("forge.theme") as ThemeName | null;
  } catch {
    return null;
  }
})();

const [theme, setThemeSignal] = createSignal<ThemeName>(isTheme(stored) ? stored : "dark");

export { theme };

export function setTheme(t: ThemeName): void {
  setThemeSignal(t);
  document.documentElement.dataset.theme = t;
  try {
    localStorage.setItem("forge.theme", t);
  } catch {
    /* storage unavailable: theme still applies for this session */
  }
}

/** Switch between the dark and light variant of the current theme family. */
export const toggleTheme = () =>
  setTheme(
    (
      {
        dark: "light",
        light: "dark",
        "solarized-dark": "solarized-light",
        "solarized-light": "solarized-dark",
        "high-contrast": "high-contrast-light",
        "high-contrast-light": "high-contrast",
      } as const
    )[theme()],
  );
