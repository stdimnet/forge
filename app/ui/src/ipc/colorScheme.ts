/**
 * Settings | Editor | Color Scheme (`app/src-tauri/src/commands/color_scheme.rs`,
 * `crates/forge-colorscheme`): the schemes, the attributes of the active one with their CSS,
 * the preview, changes stored in the settings and IntelliJ `.icls` import/export. In browser
 * mode the same Rust code runs as WebAssembly (`mock/colorSchemeMock.ts`).
 */
import { invoke } from "./index";

export type Effect = "underline" | "boldUnderline" | "waveUnderline" | "strikethrough" | "dottedLine" | "bordered";

/** How the text of one attribute looks; colors are `#rrggbb`. */
export interface TextAttributes {
  foreground?: string;
  background?: string;
  bold?: boolean;
  italic?: boolean;
  effect?: Effect;
  effectColor?: string;
}

/** Inline CSS computed in Rust. */
export interface CssStyle {
  color?: string;
  backgroundColor?: string;
  fontWeight?: string;
  fontStyle?: string;
  textDecoration?: string;
  boxShadow?: string;
}

export interface AttributeView {
  key: string;
  title: string;
  /** Tree group ("General", "Language Defaults"). */
  group: string;
  fields: { foreground: boolean; background: boolean; font: boolean; effect: boolean };
  value: TextAttributes;
  /** Whether the value comes from the parent scheme. */
  inherited: boolean;
  parentValue: TextAttributes;
  style: CssStyle;
}

export interface PreviewLine {
  segments: { text: string; attribute?: string; overlay?: string }[];
  caretRow: boolean;
}

export interface SchemesSnapshot {
  /** The `editor.colorScheme.active` setting ("" follows the theme). */
  selected: string;
  followsTheme: boolean;
  /** The scheme shown and edited. */
  active: string;
  /** "Ember" or "Anvil": the bundled scheme it is or derives from. */
  base: string;
  /** Only user schemes can be changed. */
  editable: boolean;
  modifiedCount: number;
  schemes: { name: string; parent?: string; bundled: boolean }[];
  attributes: AttributeView[];
  effects: { id: Effect; title: string }[];
  preview: PreviewLine[];
  /** CSS variables for the document; empty while following the theme. */
  css: { name: string; value: string }[];
}

export type SchemeChange =
  | { op: "select"; scheme: string }
  | { op: "duplicate"; name?: string }
  | { op: "rename"; scheme: string; name: string }
  | { op: "delete"; scheme: string }
  /** Change an attribute of the active user scheme; no value resets it to the parent's. */
  | { op: "setAttribute"; key: string; value?: TextAttributes }
  | { op: "resetAll" };

export const colorSchemeApi = {
  /** `dark`: whether the UI theme is dark (decides the scheme that follows the theme). */
  state: (dark: boolean) => invoke<SchemesSnapshot>("color_scheme_state", { dark }),
  change: (change: SchemeChange, dark: boolean) => invoke<SchemesSnapshot>("color_scheme_change", { change, dark }),
  /** Import an .icls file from its text or (desktop) a picked path; selects the new scheme. */
  import: (source: { text?: string; path?: string }, dark: boolean) =>
    invoke<SchemesSnapshot>("color_scheme_import", { ...source, dark }),
  /** The scheme as .icls text; with `path` (desktop) it is also written there. */
  export: (name: string, path?: string) => invoke<string>("color_scheme_export", { name, path }),
};
