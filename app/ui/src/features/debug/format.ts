/** Presentation of debugger values, frames and inline editor values (PHPStorm style). */
import type { Frame, Property } from "./api";

const MAX_STRING = 160;

function quote(s: string): string {
  const escaped = s.replace(/\\/g, "\\\\").replace(/\n/g, "\\n").replace(/\r/g, "\\r").replace(/\t/g, "\\t");
  return `"${escaped.length > MAX_STRING ? `${escaped.slice(0, MAX_STRING)}…` : escaped}"`;
}

/** `{int}`, `{App\Models\Order}`, `{array}` … or "" for strings/bools/null (shown bare). */
export function typeLabel(p: Property): string {
  switch (p.type) {
    case "object":
      return `{${p.classname ?? "object"}}`;
    case "array":
      return "{array}";
    case "int":
    case "float":
      return `{${p.type}}`;
    case "resource":
      return "{resource}";
    case "closure":
      return "{Closure}";
    default:
      return "";
  }
}

/** The value part: `42`, `"text"`, `true`, `[3]`. */
export function valueText(p: Property): string {
  switch (p.type) {
    case "object":
    case "array":
      return `[${p.numChildren}]`;
    case "string":
      return quote(p.value ?? "");
    case "bool":
      return p.value === "1" || p.value === "true" ? "true" : "false";
    case "null":
      return "null";
    case "uninitialized":
      return "uninitialized";
    default:
      return p.value ?? "";
  }
}

/** One-line presentation, e.g. `{App\Models\Order} [3]` or `"PAID"`. */
export function formatValue(p: Property): string {
  const t = typeLabel(p);
  const v = valueText(p);
  return t ? `${t} ${v}` : v;
}

/** Short class name for compact places (inline values): `{Order} [3]`. */
export function formatValueShort(p: Property): string {
  if (p.type === "object") return `{${(p.classname ?? "object").split("\\").pop()}} [${p.numChildren}]`;
  if (p.type === "array") return formatValue(p);
  return valueText(p);
}

/** Name as PHPStorm shows it in the tree: variables keep `$`, array keys/properties not. */
export const displayName = (p: Property) => p.name;

export function basename(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

/** `OrderController.php:24, App\Http\Controllers\OrderController->show()` */
export function frameLabel(f: Frame): { location: string; where: string } {
  const file = f.file ?? f.fileUri.replace(/^file:\/\//, "");
  const where = f.where === "{main}" ? "{main}()" : f.where.endsWith(")") ? f.where : `${f.where}()`;
  return { location: `${basename(file)}:${f.line}`, where };
}

const VAR_RE = /\$[A-Za-z_\u0080-￿][\w\u0080-￿]*/g;

/**
 * First line of the named function or method containing `line` (1-based), searching at most
 * `limit` lines up. Closures and arrow functions do not start a scope here.
 */
export function scopeStart(lines: string[], line: number, limit = 80): number {
  for (let l = line; l >= Math.max(1, line - limit); l--) {
    if (/\bfunction\s+&?[A-Za-z_]\w*\s*\(/.test(lines[l - 1] ?? "")) return l;
  }
  return Math.max(1, line - 30);
}

/**
 * Values to show after code lines while suspended: for every line from the start of the
 * current function to the execution line, the variables it mentions that are in scope.
 * Returns 1-based line → "$order: {Order} [3]   $total: 149.9".
 */
export function inlineValues(lines: string[], execLine: number, vars: Property[]): Map<number, string> {
  const byName = new Map(
    vars.filter((v) => v.type !== "uninitialized").map((v) => [v.name.startsWith("$") ? v.name : `$${v.name}`, v]),
  );
  const out = new Map<number, string>();
  if (!byName.size || execLine < 1) return out;
  const start = scopeStart(lines, execLine);
  for (let l = start; l <= Math.min(execLine, lines.length); l++) {
    const text = lines[l - 1].replace(/\/\/.*$|#(?!\[).*$/, "");
    const seen = new Set<string>();
    const parts: string[] = [];
    for (const m of text.matchAll(VAR_RE)) {
      const name = m[0];
      const v = byName.get(name);
      if (!v || seen.has(name)) continue;
      seen.add(name);
      parts.push(`${name}: ${formatValueShort(v)}`);
    }
    if (parts.length) out.set(l, parts.join("   "));
  }
  return out;
}

/** Identifier or `$variable` (with `->prop` chains) around a column, for Evaluate/Quick Evaluate. */
export function expressionAt(line: string, col: number): string {
  const re = /\$?[A-Za-z_\u0080-￿][\w\u0080-￿]*(?:(?:->|::)[A-Za-z_\u0080-￿][\w\u0080-￿]*)*/g;
  for (const m of line.matchAll(re)) {
    const from = m.index ?? 0;
    if (col >= from && col <= from + m[0].length) return m[0];
  }
  return "";
}

/** The PHP literal Set Value starts with: `'text'`, `42`, `true`, `null` ("" for compound values). */
export function phpLiteral(p: Property): string {
  switch (p.type) {
    case "string":
      return `'${(p.value ?? "").replace(/\\/g, "\\\\").replace(/'/g, "\\'")}'`;
    case "bool":
      return p.value === "1" || p.value === "true" ? "true" : "false";
    case "null":
    case "uninitialized":
      return "null";
    case "int":
    case "float":
      return p.value ?? "0";
    default:
      return "";
  }
}

/** The text Copy Value puts on the clipboard for a scalar (strings unquoted). */
export function copyText(p: Property): string {
  if (p.type === "string") return p.value ?? "";
  return valueText(p);
}
