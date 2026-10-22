/**
 * Emmet and color values computed in Rust (`crates/forge-emmet`, `crates/forge-colors`,
 * `commands/emmet.rs`). Offsets are UTF-16, like CodeMirror's. The `*Sync` variants run the
 * same Rust code as WebAssembly for work that must answer at once: the expand key, the
 * abbreviation preview, color swatches after each edit and the color picker.
 */
import { invoke } from "./index";
import { callWasmSync, forgeWasm, type ForgeWasm } from "@/wasm";

/** The `editor.emmet.*` settings and the editor's indent unit. */
export interface EmmetOptions {
  html: boolean;
  css: boolean;
  jsx: boolean;
  jsxClassAttr: string;
  hrefRecognition: boolean;
  indent: string;
}

export interface EmmetField {
  from: number;
  to: number;
  index: number;
}

/** Replace `from..to` with `text`; `fields` are tab stops inside `text`. */
export interface EmmetExpansion {
  from: number;
  to: number;
  text: string;
  fields: EmmetField[];
  end: number;
  abbreviation: string;
  syntax: "html" | "jsx" | "css";
}

export interface EmmetEdit {
  from: number;
  to: number;
  text: string;
}

export interface EmmetExpandRequest {
  text: string;
  offset: number;
  languageId?: string;
  options: EmmetOptions;
  /** Only abbreviations that are clearly Emmet (the abbreviation preview). */
  preview?: boolean;
}

export interface EmmetWrapRequest {
  text: string;
  from: number;
  to: number;
  abbreviation: string;
  languageId?: string;
  options: EmmetOptions;
}

export interface EmmetTagRequest {
  text: string;
  offset: number;
  abbreviation?: string;
  options: EmmetOptions;
}

export interface Rgba {
  r: number;
  g: number;
  b: number;
  a: number;
}

/** A color in a document. */
export interface ColorMatch {
  from: number;
  to: number;
  color: Rgba;
  /** `#rrggbb` / `#rrggbbaa`. */
  hex: string;
  /** The color as written. */
  text: string;
  /** The picker may write it back (not for Tailwind classes). */
  editable: boolean;
}

let wasm: ForgeWasm | null = null;
void forgeWasm().then(
  (w) => (wasm = w),
  () => undefined,
);

/** In-process call; `undefined` while the WebAssembly module is still loading. */
function sync<T>(fn: (w: ForgeWasm) => (json: string) => string, input: unknown): T | undefined {
  if (!wasm) return undefined;
  return callWasmSync<T>(fn(wasm), input);
}

export const emmetApi = {
  expand: (request: EmmetExpandRequest) => invoke<EmmetExpansion | null>("emmet_expand", { request }),
  wrap: (request: EmmetWrapRequest) => invoke<EmmetExpansion>("emmet_wrap", { request }),
  editPoint: (request: { text: string; offset: number; forward: boolean }) =>
    invoke<number | null>("emmet_edit_point", { request }),
  updateTag: (request: EmmetTagRequest) => invoke<EmmetEdit[]>("emmet_update_tag", { request }),
  removeTag: (request: EmmetTagRequest) => invoke<EmmetEdit[]>("emmet_remove_tag", { request }),
  expandSync: (request: EmmetExpandRequest) => sync<EmmetExpansion | null>((w) => w.emmet_expand, request),
  /** Throws with Rust's message when the abbreviation is invalid. */
  wrapSync: (request: EmmetWrapRequest) => sync<EmmetExpansion>((w) => w.emmet_wrap, request),
};

export const colorsApi = {
  find: (request: { text: string; languageId?: string }) => invoke<ColorMatch[]>("colors_find", { request }),
  format: (request: { color: Rgba; original: string }) => invoke<string>("colors_format", { request }),
  parse: (text: string) => invoke<{ color: Rgba; hex: string } | null>("colors_parse", { text }),
  findSync: (request: { text: string; languageId?: string }) => sync<ColorMatch[]>((w) => w.colors_find, request),
  formatSync: (request: { color: Rgba; original: string }) => sync<string>((w) => w.colors_format, request),
  parseSync: (text: string) => sync<{ color: Rgba; hex: string } | null>((w) => w.colors_parse, { text }),
};
