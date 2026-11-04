/**
 * File icon themes: the `registerIconTheme` contribution point and the active theme.
 *
 * Every place that shows a file icon (`FileIcon`, completion, …) asks `themedIcon()` first.
 * The answer comes from the Rust resolver (`crates/forge-icons`, through WebAssembly, so it
 * is synchronous while rendering, in the desktop app too). Without an active theme, or for
 * paths the theme has no icon for, Forge's own lucide icons are shown
 * (`shell/FileIcon.tsx`).
 *
 * The active theme is the `fileIcons.theme` setting; `fileIcons.associations` adds the
 * user's own rules, and `fileIcons.treeArrows`, `.monochrome`, `.accentColor` and `.size`
 * override the theme's look (`iconLook()`).
 */
import { createEffect, createRoot, createSignal, on } from "solid-js";
import { getSetting } from "@/state/settings";
import type { SettingObject } from "@/ipc/settings";
import { forgeWasm, type ForgeWasm } from "@/wasm";

export type TreeArrows = "chevron" | "triangle" | "none";

/** `crates/forge-icons` `IconThemeDef` (the `icon-theme.json` of a theme). */
export interface IconThemeDef {
  icons: Record<string, string>;
  file?: string;
  folder?: string;
  folderExpanded?: string;
  fileNames?: Record<string, string>;
  fileExtensions?: Record<string, string>;
  filePatterns?: { pattern: string; icon: string }[];
  fileRegex?: { pattern: string; icon: string }[];
  folderNames?: Record<string, string>;
  folderNamesExpanded?: Record<string, string>;
  options?: { treeArrows?: TreeArrows; accentColor?: string; monochrome?: boolean; iconSize?: number };
}

/** A theme ready to draw: its definition and the SVG text of each icon id. */
export interface IconThemeData {
  definition: IconThemeDef;
  svgs: Record<string, string>;
}

export interface IconThemeContribution {
  /** The value of the `fileIcons.theme` setting. */
  id: string;
  label: string;
  /** Who contributes it (a plugin name), shown next to the label. */
  source?: string;
  load: () => Promise<IconThemeData>;
}

/** A user association (Settings | Appearance & Behavior | File Icons). */
export interface IconAssociation {
  kind: "fileName" | "extension" | "pattern" | "regex" | "folderName";
  value: string;
  icon: string;
}

export const THEME_KEY = "fileIcons.theme";
export const ASSOCIATIONS_KEY = "fileIcons.associations";

const [themes, setThemes] = createSignal<IconThemeContribution[]>([]);
export { themes as iconThemes };

/** Add an icon theme; returns a function that removes it again. */
export function registerIconTheme(theme: IconThemeContribution): () => void {
  setThemes((list) => [...list.filter((t) => t.id !== theme.id), theme]);
  return () => setThemes((list) => list.filter((t) => t !== theme));
}

interface Active {
  id: string;
  definition: IconThemeDef;
  urls: Map<string, string>;
}

const [active, setActive] = createSignal<Active | null>(null);
const [problems, setProblems] = createSignal<string[]>([]);
const [loadError, setLoadError] = createSignal<string | null>(null);
/** The active theme's definition (icon ids for the associations editor), or null. */
export const activeIconTheme = () => active();
/** Problems of the active theme and the user's associations (skipped ones). */
export const iconThemeProblems = problems;
/** Why the selected theme could not be loaded. */
export const iconThemeError = loadError;

let wasm: ForgeWasm | null = null;

const svgUrl = (svg: string) => `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;

/** Load and activate the theme with `id` ("" for Forge's own icons). */
let seq = 0;

async function activate(id: string, associations: IconAssociation[]): Promise<void> {
  const mine = ++seq;
  const theme = id ? themes().find((t) => t.id === id) : undefined;
  wasm ??= await forgeWasm();
  if (mine !== seq) return;
  if (!theme) {
    wasm.icons_set_theme(JSON.stringify({ theme: null }));
    setActive(null);
    setProblems([]);
    // A theme of a plugin that is not registered (yet) stays selected: it activates when
    // the plugin registers it.
    setLoadError(null);
    return;
  }
  try {
    const data = await theme.load();
    // A newer selection won while this one loaded.
    if (mine !== seq) return;
    const found = JSON.parse(wasm.icons_set_theme(JSON.stringify({ theme: data.definition, associations }))) as string[];
    const urls = new Map(Object.entries(data.svgs).map(([icon, svg]) => [icon, svgUrl(svg)]));
    setProblems(found);
    setLoadError(null);
    setActive({ id, definition: data.definition, urls });
  } catch (e) {
    if (mine !== seq) return;
    wasm.icons_set_theme(JSON.stringify({ theme: null }));
    setActive(null);
    setLoadError(e instanceof Error ? e.message : String(e));
  }
}

createRoot(() => {
  createEffect(
    on(
      [() => getSetting<string>(THEME_KEY, ""), () => getSetting<SettingObject[]>(ASSOCIATIONS_KEY, []) as unknown as IconAssociation[], themes],
      ([id, associations]) => {
        // Also re-runs when themes are (un)registered, e.g. after a plugin was enabled.
        void activate(id, Array.isArray(associations) ? associations : []);
      },
    ),
  );
});

export type IconKind = "file" | "folder" | "folderOpen";
const KIND_CODE: Record<IconKind, number> = { file: 0, folder: 1, folderOpen: 2 };

/** The icon of the active theme for `path` (a path or a file name), as an image URL, or null. */
export function themedIcon(path: string, kind: IconKind = "file"): string | null {
  const a = active();
  if (!a || !wasm || !path) return null;
  const id = wasm.icons_resolve(path, KIND_CODE[kind]);
  return (id && a.urls.get(id)) || null;
}

/** The look of file icons and trees: the user's settings over the theme's options. */
export function iconLook(): { arrows: TreeArrows; monochrome: boolean; accent: string | null; size: number | null } {
  const o = active()?.definition.options ?? {};
  const arrows = getSetting<string>("fileIcons.treeArrows", "theme");
  const mono = getSetting<string>("fileIcons.monochrome", "theme");
  const accent = getSetting<string>("fileIcons.accentColor", "").trim();
  const size = getSetting<number>("fileIcons.size", 0);
  return {
    arrows: (arrows === "theme" ? o.treeArrows ?? "chevron" : arrows) as TreeArrows,
    monochrome: mono === "theme" ? !!o.monochrome : mono === "on",
    accent: /^#(?:[0-9a-f]{3}|[0-9a-f]{6})$/i.test(accent) ? accent : o.accentColor ?? null,
    size: size > 0 ? size : o.iconSize ?? null,
  };
}

/**
 * A themed icon as a DOM element, for places outside Solid (CodeMirror completion), or null
 * when the active theme has none for `path`.
 */
export function themedIconElement(path: string, kind: IconKind, size: number): HTMLElement | null {
  const url = themedIcon(path, kind);
  if (!url) return null;
  const el = document.createElement("span");
  applyIconStyle(el.style, url, kind, size);
  el.className = "file-icon themed-file-icon";
  return el;
}

/** Style a themed icon: an image, or a color mask when monochrome or accent-colored. */
export function iconStyle(url: string, kind: IconKind, size: number): Record<string, string> {
  const look = iconLook();
  const px = `${look.size ?? size}px`;
  const base: Record<string, string> = { width: px, height: px, "flex-shrink": "0", display: "inline-block" };
  const tint = look.monochrome ? (kind !== "file" && look.accent) || "var(--fg-muted)" : kind !== "file" ? look.accent : null;
  if (!tint) return { ...base, "background-image": `url("${url}")`, "background-size": "contain", "background-repeat": "no-repeat", "background-position": "center" };
  return {
    ...base,
    "background-color": tint,
    "mask-image": `url("${url}")`,
    "mask-size": "contain",
    "mask-repeat": "no-repeat",
    "mask-position": "center",
    "-webkit-mask-image": `url("${url}")`,
    "-webkit-mask-size": "contain",
    "-webkit-mask-repeat": "no-repeat",
    "-webkit-mask-position": "center",
  };
}

function applyIconStyle(style: CSSStyleDeclaration, url: string, kind: IconKind, size: number): void {
  for (const [k, v] of Object.entries(iconStyle(url, kind, size))) style.setProperty(k, v);
}
