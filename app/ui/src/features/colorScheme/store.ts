/**
 * The color scheme as Rust computes it (`forge-colorscheme`): the schemes, the attributes of
 * the active one and the CSS variables that make the editor show it. The variables are set on
 * <html> here; while the scheme follows the theme, none are set and the theme CSS applies.
 */
import { createEffect, createRoot, createSignal, on } from "solid-js";
import { colorSchemeApi, type SchemeChange, type SchemesSnapshot, type ThemeKind } from "@/ipc/colorScheme";
import { getSetting, settingsState } from "@/state/settings";
import { isDarkTheme, isHighContrastTheme, theme } from "@/state/theme";

const [snapshot, setSnapshot] = createSignal<SchemesSnapshot | null>(null);
export { snapshot as colorSchemes };

/** Whether the UI theme is a dark one (the scheme that follows the theme is Ember then). */
export const themeIsDark = (): boolean => isDarkTheme(theme());

/** The kind of UI theme, which decides the scheme that follows the theme. */
export const themeKind = (): ThemeKind => ({ dark: isDarkTheme(theme()), highContrast: isHighContrastTheme(theme()) });

let applied: string[] = [];

/** Set the scheme's CSS variables on the document (and remove the previous ones). */
export function applySchemeCss(s: SchemesSnapshot, root: HTMLElement = document.documentElement): void {
  for (const name of applied) root.style.removeProperty(name);
  applied = s.css.map((v) => v.name);
  for (const v of s.css) root.style.setProperty(v.name, v.value);
}

let seq = 0;

function adopt(request: Promise<SchemesSnapshot>): Promise<void> {
  const n = ++seq;
  return request.then(
    (s) => {
      if (n !== seq) return;
      setSnapshot(s);
      applySchemeCss(s);
    },
    (e) => console.warn("color scheme not loaded", e),
  );
}

export const refreshColorSchemes = (): Promise<void> => adopt(colorSchemeApi.state(themeKind()));

export const changeColorScheme = (change: SchemeChange): Promise<void> =>
  adopt(colorSchemeApi.change(change, themeKind()));

/** Import an .icls file (text in browser mode, a picked path in the desktop app). */
export const importColorScheme = (source: { text?: string; path?: string }): Promise<void> =>
  colorSchemeApi.import(source, themeKind()).then((s) => {
    seq++;
    setSnapshot(s);
    applySchemeCss(s);
  });

/** Why `name` cannot name a user scheme (null when it can); `except` is its current name. */
export function schemeNameProblem(name: string, except?: string): string | null {
  const n = name.trim();
  if (!n) return "Enter a name";
  if (/[\t\n]/.test(n)) return "The name cannot contain tabs or line breaks";
  const taken = (snapshot()?.schemes ?? []).some((s) => s.name === n && s.name !== except);
  return taken ? `A scheme named "${n}" already exists` : null;
}

/** A free name like Rust's `free_name`, for the Duplicate dialog. */
export function freeSchemeName(base: string): string {
  if (!schemeNameProblem(base)) return base;
  for (let n = 2; ; n++) if (!schemeNameProblem(`${base} ${n}`)) return `${base} ${n}`;
}

let installed = false;

/** Follow the scheme settings and the theme; call once from the main window. */
export function installColorScheme(): void {
  if (installed) return;
  installed = true;
  createRoot(() =>
    createEffect(
      on(
        () => [settingsState.loaded, getSetting("editor.colorScheme.active"), getSetting("editor.colorScheme.schemes"), theme()],
        ([loaded]) => {
          if (loaded) void refreshColorSchemes();
        },
      ),
    ),
  );
}
