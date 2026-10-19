import { createSignal } from "solid-js";

export type ThemeName = "dark" | "light";

const stored = (() => {
  try {
    return localStorage.getItem("forge.theme") as ThemeName | null;
  } catch {
    return null;
  }
})();

const [theme, setThemeSignal] = createSignal<ThemeName>(stored ?? "dark");

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

export const toggleTheme = () => setTheme(theme() === "dark" ? "light" : "dark");
