/**
 * Applies settings to the running UI: fonts and sizes become CSS variables, a few options
 * become `data-*` attributes on <html> (styled in settings.css), and the theme setting is
 * kept in sync with the toolbar theme toggle.
 */
import { createEffect, createRoot, on, untrack } from "solid-js";
import { setTheme, theme, type ThemeName } from "@/state/theme";
import { getSetting, saveSetting, setLocalSetting, settingsState } from "@/state/settings";

const MONO_FALLBACK = `"JetBrains Mono", "Fira Code", "Cascadia Code", Menlo, Consolas, monospace`;
const UI_FALLBACK = `-apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif`;

/** CSS font-family value with the chosen family first. */
export function fontStack(family: string, fallback: string): string {
  if (!family) return fallback;
  const generic = ["monospace", "system-ui", "sans-serif"].includes(family);
  const first = generic ? family : `"${family}"`;
  const rest = fallback
    .split(",")
    .map((f) => f.trim())
    .filter((f) => f.replace(/"/g, "") !== family);
  return [first, ...rest].join(", ");
}

/** CSS variables derived from settings (exported for tests). */
export function cssVars(get: <T>(key: string) => T): Record<string, string> {
  const uiSize = get<number>("appearance.uiFont.size");
  return {
    "--font-mono": fontStack(get<string>("editor.font.family"), MONO_FALLBACK),
    "--fs-editor": `${get<number>("editor.font.size")}px`,
    "--fs-presentation": `${get<number>("appearance.presentationFontSize") ?? 24}px`,
    "--editor-line-height": String(get<number>("editor.font.lineHeight")),
    "--editor-ligatures": get<boolean>("editor.font.ligatures") ? "normal" : "none",
    "--font-ui": fontStack(get<string>("appearance.uiFont.family"), UI_FALLBACK),
    "--fs-ui": `${uiSize}px`,
    "--fs-small": `${uiSize - 1}px`,
  };
}

/** Boolean `data-*` flags on <html>, consumed by settings.css. */
export function rootFlags(get: <T>(key: string) => T): Record<string, boolean> {
  return {
    compact: get<boolean>("appearance.compactMode"),
    hideStripes: !get<boolean>("appearance.showToolWindowBars"),
    noCaretBlink: !get<boolean>("editor.caret.blinking"),
    smoothScroll: get<boolean>("editor.smoothScrolling"),
    smoothLists: get<boolean>("appearance.smoothScrolling"),
    toolWindowNumbers: get<boolean>("appearance.showToolWindowNumbers"),
    showWhitespace: get<boolean>("editor.showWhitespace"),
    noCaretRow: !get<boolean>("editor.highlightCurrentLine"),
    noLineNumbers: !get<boolean>("editor.showLineNumbers"),
  };
}

export function applySettingsToDocument(): void {
  createRoot(() => {
    const get = <T,>(key: string) => getSetting(key) as T;
    createEffect(() => {
      if (!settingsState.loaded) return;
      const root = document.documentElement;
      for (const [name, value] of Object.entries(cssVars(get))) root.style.setProperty(name, value);
      for (const [name, on] of Object.entries(rootFlags(get))) {
        if (on) root.dataset[name] = "";
        else delete root.dataset[name];
      }
    });

    // Theme: an explicit setting wins on startup; otherwise adopt the current theme.
    // Afterwards the setting drives the theme.
    createEffect(
      on(
        () => settingsState.loaded,
        (loaded) => {
          if (!loaded) return;
          const explicit = "appearance.theme" in settingsState.user || "appearance.theme" in settingsState.project;
          if (!explicit) setLocalSetting("appearance.theme", untrack(theme));
          createEffect(() => {
            const t = getSetting<string>("appearance.theme");
            if (t && t !== untrack(theme)) setTheme(t as ThemeName);
          });
        },
      ),
    );
    // The toolbar toggle changes the theme directly; persist it as the setting.
    createEffect(
      on(
        theme,
        (t) => {
          if (settingsState.loaded && getSetting("appearance.theme") !== t) void saveSetting("appearance.theme", t);
        },
        { defer: true },
      ),
    );
  });
}
