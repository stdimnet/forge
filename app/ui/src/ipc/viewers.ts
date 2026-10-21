/**
 * Non-text file editors (`crates/forge-viewers`, `app/src-tauri/src/commands/viewers.rs`):
 * images read through the project backend (remote projects included) and CSV/TSV tables
 * for "Edit as Table". Browser mode runs the same Rust code as WebAssembly.
 */
import { invoke } from "./index";

export interface ImageInfo {
  /** `PNG`, `JPEG`, `GIF`, `WebP`, `ICO`, `BMP` or `SVG`. */
  format: string;
  mime: string;
  width: number;
  height: number;
  /** File size in bytes. */
  size: number;
  /** Bits per pixel (PNG, BMP). */
  bitDepth?: number;
}

export interface ImageData {
  info: ImageInfo;
  base64: string;
}

export interface CsvTable {
  rows: string[][];
  /** Per cell: written in quotes. */
  quoted: boolean[][];
  delimiter: string;
  eol: string;
  trailingNewline: boolean;
  /** The widest row. */
  columns: number;
}

export type CsvEdit =
  | { op: "setCell"; row: number; column: number; value: string }
  | { op: "insertRow"; at: number }
  | { op: "removeRows"; rows: number[] }
  | { op: "insertColumn"; at: number }
  | { op: "removeColumns"; columns: number[] };

export const viewersApi = {
  /** An image of the project with its format, size and bytes (base64). */
  imageRead: (path: string) => invoke<ImageData>("image_read", { path }),
  /** Parse CSV/TSV; without a delimiter it is detected from the path and first line. */
  csvParse: (text: string, path?: string, delimiter?: string) => invoke<CsvTable>("csv_parse", { text, path, delimiter }),
  /** Apply edits; untouched cells keep their quoting. */
  csvEdit: (text: string, delimiter: string, edits: CsvEdit[]) => invoke<string>("csv_edit", { text, delimiter, edits }),
  /** Display order of the rows sorted by a column (header rows first). */
  csvSortOrder: (rows: string[][], column: number, descending: boolean, headerRows: number) =>
    invoke<number[]>("csv_sort_order", { rows, column, descending, headerRows }),
};
