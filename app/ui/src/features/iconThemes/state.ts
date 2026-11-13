/**
 * Icon themes: the `registerIconTheme` contribution point and the active theme.
 *
 * Every place that shows a file, symbol or UI icon (`FileIcon`, `SymbolIcon`, `UiIcon`,
 * completion, …) asks this module first. The answers come from the Rust resolver
 * (`crates/forge-icons`, through WebAssembly, so it is synchronous while rendering, in the
 * desktop app too): which icon a path, a symbol kind or a UI id gets, the light variant,
 * folder colors and tint layers. Without an active theme, or for things the theme has no
 * icon for, Forge's own lucide icons are shown.
 *
 * The active theme is the `fileIcons.theme` setting; `fileIcons.associations` adds the
 * user's own rules, and the other `fileIcons.*` settings override the theme's look
 * (`iconLook()`). The UI theme (light or dark variants), the project root (path rules) and
 * the folder colors are handed to Rust with `icons_set_options`; any change clears its
 * caches and re-renders the icons.
 */
import { createEffect, createMemo, createRoot, createSignal, on } from "solid-js";
import { getSetting } from "@/state/settings";
import type { SettingObject } from "@/ipc/settings";
import { theme as uiTheme } from "@/state/theme";
import { workspace } from "@/state/workspace";
import { editor } from "@/state/editor";
import { fileTypeOf } from "@/editor/fileTypes";
import { forgeWasm, type ForgeWasm } from "@/wasm";

export type TreeArrows = "chevron" | "triangle" | "plusMinus" | "arrow" | "none";

type Rule = { pattern: string; icon: string };

/** `crates/forge-icons` `IconThemeDef` (the `icon-theme.json` of a theme). */
export interface IconThemeDef {
  version?: number;
  icons: Record<string, string>;
  iconsLight?: Record<string, string>;
  file?: string;
  folder?: string;
  folderExpanded?: string;
  rootFolder?: string;
  rootFolderExpanded?: string;
  folderHollow?: string;
  folderHollowExpanded?: string;
  fileNames?: Record<string, string>;
  fileExtensions?: Record<string, string>;
  fileTypes?: Record<string, string>;
  pathRules?: Rule[];
  filePatterns?: Rule[];
  fileRegex?: Rule[];
  folderNames?: Record<string, string>;
  folderNamesExpanded?: Record<string, string>;
  symbols?: Record<string, string>;
  ui?: Record<string, string>;
  options?: { treeArrows?: TreeArrows; accentColor?: string; monochrome?: boolean; iconSize?: number; hollowFolders?: boolean };
}

/**
 * A theme ready to draw: its definition and the SVG text of each icon key (the icon ids,
 * and `<id>_light` for the light variants of `iconsLight`).
 */
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

/** A row of the folder colors table (`fileIcons.folderColors`). */
export interface FolderColorRow {
  pattern: string;
  color: string;
}

export const THEME_KEY = "fileIcons.theme";
export const ASSOCIATIONS_KEY = "fileIcons.associations";
export const FOLDER_COLORS_KEY = "fileIcons.folderColors";

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
  svgs: Record<string, string>;
  urls: Map<string, string>;
  /** Keys whose SVG marks a tint layer. */
  tintable: Set<string>;
}

const [active, setActive] = createSignal<Active | null>(null);
const [problems, setProblems] = createSignal<string[]>([]);
const [optionProblems, setOptionProblems] = createSignal<string[]>([]);
const [loadError, setLoadError] = createSignal<string | null>(null);
/** Bumped whenever the Rust side's answers may have changed (options, wasm loaded). */
const [revision, setRevision] = createSignal(0);
const [light, setLight] = createSignal(false);
/** The active theme's definition (icon ids for the associations editor), or null. */
export const activeIconTheme = () => active();
/** Problems of the active theme, the user's associations and folder colors (skipped ones). */
export const iconThemeProblems = () => [...problems(), ...optionProblems()];
/** Why the selected theme could not be loaded. */
export const iconThemeError = loadError;
/** Whether light variants are drawn (Rust decides from the UI theme and `fileIcons.variant`). */
export const iconVariantIsLight = light;

let wasm: ForgeWasm | null = null;
const tinted = new Map<string, string>();

const svgUrl = (svg: string) => `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;

/** Load and activate the theme with `id` ("" for Forge's own icons). */
let seq = 0;

async function activate(id: string, associations: IconAssociation[]): Promise<void> {
  const mine = ++seq;
  const theme = id ? themes().find((t) => t.id === id) : undefined;
  if (!wasm) {
    wasm = await forgeWasm();
    applyOptions();
  }
  if (mine !== seq) return;
  tinted.clear();
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
    const w = wasm;
    const tintable = new Set(Object.entries(data.svgs).filter(([, svg]) => w.icons_has_tint_layer(svg)).map(([k]) => k));
    setProblems(found);
    setLoadError(null);
    setActive({ id, definition: data.definition, svgs: data.svgs, urls, tintable });
  } catch (e) {
    if (mine !== seq) return;
    wasm.icons_set_theme(JSON.stringify({ theme: null }));
    setActive(null);
    setLoadError(e instanceof Error ? e.message : String(e));
  }
}

const options = () => ({
  uiTheme: uiTheme(),
  variant: getSetting<string>("fileIcons.variant", "auto"),
  projectRoot: workspace()?.root ?? null,
  folderColors: (getSetting<SettingObject[]>(FOLDER_COLORS_KEY, []) as unknown as FolderColorRow[]) ?? [],
});

function applyOptions(): void {
  if (!wasm) return;
  const out = JSON.parse(wasm.icons_set_options(JSON.stringify(options()))) as { light: boolean; problems: string[] };
  setLight(out.light);
  setOptionProblems(out.problems);
  tinted.clear();
  setRevision((r) => r + 1);
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
  // The UI theme (light / dark variants, live), the project root and the folder colors.
  createEffect(on(() => JSON.stringify(options()), applyOptions, { defer: true }));
});

export type IconKind = "file" | "folder" | "folderOpen";
const KIND_CODE: Record<IconKind, number> = { file: 0, folder: 1, folderOpen: 2 };

/** Extra facts about a folder: the project root (Project view), or one that holds open files. */
export interface IconOptions {
  root?: boolean;
  hollow?: boolean;
}

/** The key (icon id, or `<id>_light`) the active theme draws for `path`, or null. */
export function themedIconKey(path: string, kind: IconKind = "file", opts: IconOptions = {}): string | null {
  revision();
  const a = active();
  if (!a || !wasm || !path) return null;
  const flags = KIND_CODE[kind] | (opts.root ? 4 : 0) | (opts.hollow && kind !== "file" ? 8 : 0);
  // The detected file type (a `#!` line of an extensionless script) only when it matters.
  const fileType = kind === "file" && wasm.icons_needs_file_type(path) ? (fileTypeOf(path).id ?? "") : "";
  const key = wasm.icons_resolve(path, flags, fileType);
  return key && a.urls.has(key) ? key : null;
}

/** The icon of the active theme for `path` (a path or a file name), as an image URL, or null. */
export function themedIcon(path: string, kind: IconKind = "file", opts: IconOptions = {}): string | null {
  const key = themedIconKey(path, kind, opts);
  return key ? active()!.urls.get(key)! : null;
}

/** The URL of an icon key of the active theme. */
export const iconUrl = (key: string): string | null => active()?.urls.get(key) ?? null;

/** A themed code symbol icon: its key and the keys of its marker overlays. */
export interface ThemedSymbol {
  icon: string;
  markers: string[];
}

/** The active theme's icon for a symbol kind (`forge_icons::ids::SYMBOL_KINDS`), or null. */
export function themedSymbol(kind: string, modifiers: { isAbstract?: boolean; isFinal?: boolean; isStatic?: boolean } = {}): ThemedSymbol | null {
  revision();
  const a = active();
  if (!a || !wasm) return null;
  const bits = (modifiers.isAbstract ? 1 : 0) | (modifiers.isFinal ? 2 : 0) | (modifiers.isStatic ? 4 : 0);
  const out = wasm.icons_symbol(kind, bits);
  if (!out) return null;
  const sym = JSON.parse(out) as ThemedSymbol;
  return a.urls.has(sym.icon) ? { icon: sym.icon, markers: sym.markers.filter((m) => a.urls.has(m)) } : null;
}

/** The active theme's key for a UI id (`toolwindow.project`, `action.run`, …), or null. */
export function themedUiIcon(id: string): string | null {
  revision();
  const a = active();
  if (!a || !wasm) return null;
  const key = wasm.icons_ui(id);
  return key && a.urls.has(key) ? key : null;
}

/** The user's color for the folder at `path` (`fileIcons.folderColors`), or null. */
export function folderColor(path: string): string | null {
  revision();
  if (!wasm || !path) return null;
  return wasm.icons_folder_color(path) || null;
}

const openFolders = createRoot(() =>
  createMemo(() => {
    revision();
    if (!wasm || !iconLook().hollow) return new Set<string>();
    const openFiles = editor.tabs.map((t) => t.path).filter((p) => !p.includes("://"));
    return new Set(JSON.parse(wasm.icons_open_folders(JSON.stringify({ openFiles, projectRoot: workspace()?.root ?? null }))) as string[]);
  }),
);

/** Whether the folder at `path` (relative to the project) is drawn hollow: it holds open files. */
export const isHollowFolder = (path: string): boolean => !!path && openFolders().has(path);

export type TintMode = "layer" | "mask";

export interface IconLook {
  arrows: TreeArrows;
  monochrome: boolean;
  accent: string | null;
  size: number | null;
  /** Recolor only the tint layer of icons that mark one, or always the whole icon. */
  tintMode: TintMode;
  /** 0 to 1. */
  saturation: number;
  /** 0.1 to 1. */
  opacity: number;
  /** Folders that hold open files are drawn hollow. */
  hollow: boolean;
}

const triState = (value: string, theme: boolean | undefined) => (value === "theme" ? !!theme : value === "on");
const HEX = /^#(?:[0-9a-f]{3}|[0-9a-f]{6})$/i;

/** The look of icons and trees: the user's settings over the theme's options. */
export function iconLook(): IconLook {
  const o = active()?.definition.options ?? {};
  const arrows = getSetting<string>("fileIcons.treeArrows", "theme");
  const accent = getSetting<string>("fileIcons.accentColor", "").trim();
  const size = getSetting<number>("fileIcons.size", 0);
  const percent = (key: string, min: number) => Math.min(100, Math.max(min, getSetting<number>(key, 100))) / 100;
  return {
    arrows: (arrows === "theme" ? o.treeArrows ?? "chevron" : arrows) as TreeArrows,
    monochrome: triState(getSetting<string>("fileIcons.monochrome", "theme"), o.monochrome),
    accent: HEX.test(accent) ? accent : o.accentColor ?? null,
    size: size > 0 ? size : o.iconSize ?? null,
    tintMode: getSetting<string>("fileIcons.tintMode", "layer") === "mask" ? "mask" : "layer",
    saturation: percent("fileIcons.saturation", 0),
    opacity: percent("fileIcons.opacity", 10),
    hollow: triState(getSetting<string>("fileIcons.hollowFolders", "theme"), o.hollowFolders),
  };
}

/** A theme color token as `#rrggbb` (tinted SVGs are images: CSS variables do not reach them). */
function tokenHex(token: string): string | null {
  if (typeof getComputedStyle !== "function") return null;
  const v = getComputedStyle(document.documentElement).getPropertyValue(token).trim();
  if (HEX.test(v)) return v;
  const m = v.match(/^rgba?\(\s*(\d+)[,\s]+(\d+)[,\s]+(\d+)/);
  return m ? `#${m.slice(1, 4).map((n) => Number(n).toString(16).padStart(2, "0")).join("")}` : null;
}

/** The URL of `key` with its tint layer in `color` (cached until the theme or options change). */
function tintedUrl(key: string, color: string): string | null {
  const a = active();
  if (!a || !wasm || !a.tintable.has(key)) return null;
  const cacheKey = `${key}\n${color}`;
  let url = tinted.get(cacheKey);
  if (url === undefined) {
    const svg = wasm.icons_tint(a.svgs[key] ?? "", color);
    url = svg ? svgUrl(svg) : "";
    tinted.set(cacheKey, url);
  }
  return url || null;
}

/**
 * A themed icon as a DOM element, for places outside Solid (CodeMirror completion), or null
 * when the active theme has none for `path`.
 */
export function themedIconElement(path: string, kind: IconKind, size: number): HTMLElement | null {
  const key = themedIconKey(path, kind);
  if (!key) return null;
  const el = document.createElement("span");
  for (const [k, v] of Object.entries(iconStyle(key, kind, size))) el.style.setProperty(k, v);
  el.className = "file-icon themed-file-icon";
  return el;
}

/** A themed symbol icon as a DOM element (completion), or null for Forge's own icon. */
export function themedSymbolElement(kind: string, size: number): HTMLElement | null {
  const sym = themedSymbol(kind);
  if (!sym) return null;
  const el = document.createElement("span");
  for (const [k, v] of Object.entries(iconStyle(sym.icon, "symbol", size))) el.style.setProperty(k, v);
  el.className = "themed-symbol-icon";
  el.setAttribute("aria-hidden", "true");
  return el;
}

const image = (url: string) => ({ "background-image": `url("${url}")`, "background-size": "contain", "background-repeat": "no-repeat", "background-position": "center" });

/**
 * Style a themed icon `key`: an image, its tint layer recolored, or a one-color mask, with
 * the saturation and opacity options. Folders take `color` (a folder color) over the accent.
 */
export function iconStyle(key: string, kind: IconKind | "symbol" | "ui", size: number, color?: string | null): Record<string, string> {
  const url = iconUrl(key) ?? "";
  const look = iconLook();
  const px = `${look.size ?? size}px`;
  const base: Record<string, string> = { width: px, height: px, "flex-shrink": "0", display: "inline-block" };
  const filters: string[] = [];
  if (look.saturation < 1) filters.push(`saturate(${look.saturation})`);
  if (look.opacity < 1) base.opacity = String(look.opacity);
  const folder = kind === "folder" || kind === "folderOpen";
  const folderTint = folder ? color || look.accent : null;
  const tint = look.monochrome ? folderTint || "var(--fg-muted)" : folderTint;
  const done = (style: Record<string, string>) => (filters.length ? { ...style, filter: filters.join(" ") } : style);
  if (!tint) return done({ ...base, ...image(url) });
  if (look.tintMode === "layer") {
    const hex = tint.startsWith("var(") ? tokenHex(tint.slice(4, -1)) : tint;
    const layered = hex ? tintedUrl(key, hex) : null;
    if (layered) {
      // Monochrome: the rest of the icon in gray, unless the layer carries a folder color.
      if (look.monochrome && !folderTint) filters.push("grayscale(1)");
      return done({ ...base, ...image(layered) });
    }
  }
  return done({
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
  });
}
