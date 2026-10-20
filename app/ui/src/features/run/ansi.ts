/**
 * ANSI escape handling for the Run console: SGR colors and styles become styled segments,
 * other control sequences (cursor movement, OSC titles) are dropped, `\r` overwrites the
 * current line like a terminal does for progress bars.
 */

export interface AnsiStyle {
  /** CSS color: a `var(--ansi-…)` reference or `rgb(…)`. */
  fg?: string;
  bg?: string;
  bold?: boolean;
  dim?: boolean;
  italic?: boolean;
  underline?: boolean;
  inverse?: boolean;
}

export interface Segment {
  text: string;
  style: AnsiStyle;
}

const NAMES = ["black", "red", "green", "yellow", "blue", "magenta", "cyan", "white"];

const named = (i: number, bright: boolean) => `var(--ansi-${bright ? "bright-" : ""}${NAMES[i]})`;

/** xterm 256-color palette entry as CSS. */
export function color256(n: number): string {
  if (n < 8) return named(n, false);
  if (n < 16) return named(n - 8, true);
  if (n < 232) {
    const v = n - 16;
    const level = (x: number) => (x === 0 ? 0 : 55 + x * 40);
    return `rgb(${level(Math.floor(v / 36))}, ${level(Math.floor(v / 6) % 6)}, ${level(v % 6)})`;
  }
  const g = 8 + (n - 232) * 10;
  return `rgb(${g}, ${g}, ${g})`;
}

function applySgr(style: AnsiStyle, params: number[]): AnsiStyle {
  const s = { ...style };
  if (params.length === 0) params = [0];
  for (let i = 0; i < params.length; i++) {
    const p = params[i];
    if (p === 0) {
      for (const k of Object.keys(s)) delete s[k as keyof AnsiStyle];
    } else if (p === 1) s.bold = true;
    else if (p === 2) s.dim = true;
    else if (p === 3) s.italic = true;
    else if (p === 4) s.underline = true;
    else if (p === 7) s.inverse = true;
    else if (p === 22) {
      delete s.bold;
      delete s.dim;
    } else if (p === 23) delete s.italic;
    else if (p === 24) delete s.underline;
    else if (p === 27) delete s.inverse;
    else if (p >= 30 && p <= 37) s.fg = named(p - 30, false);
    else if (p >= 90 && p <= 97) s.fg = named(p - 90, true);
    else if (p >= 40 && p <= 47) s.bg = named(p - 40, false);
    else if (p >= 100 && p <= 107) s.bg = named(p - 100, true);
    else if (p === 39) delete s.fg;
    else if (p === 49) delete s.bg;
    else if (p === 38 || p === 48) {
      const key = p === 38 ? "fg" : "bg";
      if (params[i + 1] === 5 && params[i + 2] !== undefined) {
        s[key] = color256(params[i + 2]);
        i += 2;
      } else if (params[i + 1] === 2 && params[i + 4] !== undefined) {
        s[key] = `rgb(${params[i + 2]}, ${params[i + 3]}, ${params[i + 4]})`;
        i += 4;
      }
    }
  }
  return s;
}

/**
 * Incremental parser: feed chunks as they arrive; escape sequences split across chunks are
 * kept until complete. Output segments may contain `\n` and `\r`.
 */
export class AnsiParser {
  private style: AnsiStyle = {};
  private pending = "";

  feed(chunk: string): Segment[] {
    const input = this.pending + chunk;
    this.pending = "";
    const out: Segment[] = [];
    let text = "";
    const flush = () => {
      if (text) out.push({ text, style: this.style });
      text = "";
    };
    let i = 0;
    while (i < input.length) {
      const c = input[i];
      if (c !== "\x1b") {
        // Drop other C0 controls except newline, carriage return and tab.
        if (c >= " " || c === "\n" || c === "\r" || c === "\t") text += c;
        i++;
        continue;
      }
      if (i + 1 >= input.length) {
        this.pending = input.slice(i);
        break;
      }
      const kind = input[i + 1];
      if (kind === "[") {
        // CSI: parameters, then a final byte in @..~
        let j = i + 2;
        while (j < input.length && !(input[j] >= "@" && input[j] <= "~")) j++;
        if (j >= input.length) {
          this.pending = input.slice(i);
          break;
        }
        if (input[j] === "m") {
          flush();
          const params = input
            .slice(i + 2, j)
            .split(/[;:]/)
            .filter((p) => p !== "")
            .map((p) => Number.parseInt(p, 10))
            .filter((n) => !Number.isNaN(n));
          this.style = applySgr(this.style, params);
        }
        i = j + 1;
      } else if (kind === "]") {
        // OSC: up to BEL or ESC \
        const bel = input.indexOf("\x07", i + 2);
        const st = input.indexOf("\x1b\\", i + 2);
        const end = bel < 0 ? st : st < 0 ? bel : Math.min(bel, st);
        if (end < 0) {
          this.pending = input.slice(i);
          break;
        }
        i = end + (input[end] === "\x07" ? 1 : 2);
      } else {
        i += 2;
      }
    }
    flush();
    return out;
  }
}

export interface ConsoleLine {
  segments: Segment[];
  /** A carriage return was seen: the next text replaces the line. */
  cr?: boolean;
}

/** Text of a console line without styles. */
export const lineText = (line: ConsoleLine) => line.segments.map((s) => s.text).join("");

/**
 * Append segments to console lines in place: `\n` starts a new line; after a `\r` the next
 * text overwrites the current line (progress bars), while `\r\n` is a plain line break.
 */
export function appendSegments(lines: ConsoleLine[], segments: Segment[], extra?: AnsiStyle): void {
  if (lines.length === 0) lines.push({ segments: [] });
  for (const seg of segments) {
    const style = extra ? { ...extra, ...seg.style } : seg.style;
    seg.text.split("\n").forEach((part, idx) => {
      if (idx > 0) lines.push({ segments: [] });
      const current = lines[lines.length - 1];
      part.split("\r").forEach((piece, k) => {
        if (k > 0) current.cr = true;
        if (!piece) return;
        if (current.cr) {
          current.segments = [];
          current.cr = false;
        }
        current.segments.push({ text: piece, style });
      });
    });
  }
}
