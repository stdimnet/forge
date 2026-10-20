/**
 * Run console: ANSI-styled lines, clickable file references (stack traces, `file:line`),
 * stdin input typed directly into the console, scroll-to-end and soft wraps.
 */
import { createEffect, createSignal, For, on, Show, type JSX } from "solid-js";
import { workspace } from "@/state/workspace";
import { revealLocation } from "@/features/lsp/navigation";
import { showBalloon } from "@/keymap";
import { lineText, type AnsiStyle, type ConsoleLine, type Segment } from "./ansi";
import { findFileLinks, toWorkspacePath, type FileLink } from "./fileLinks";

export async function openFileLink(link: Pick<FileLink, "path" | "line" | "column">, cwd?: string): Promise<void> {
  const root = workspace()?.root;
  const cwdRel = cwd ? (toWorkspacePath(cwd, root) ?? "") : "";
  const rel = toWorkspacePath(link.path, root, cwdRel);
  if (!rel) {
    showBalloon(`${link.path} is outside the project`);
    return;
  }
  const ok = await revealLocation(rel, { line: Math.max(0, (link.line ?? 1) - 1), character: Math.max(0, (link.column ?? 1) - 1) });
  if (!ok) showBalloon(`Cannot open ${rel}`);
}

export function styleOf(s: AnsiStyle): JSX.CSSProperties | undefined {
  if (!s.fg && !s.bg && !s.bold && !s.dim && !s.italic && !s.underline && !s.inverse) return undefined;
  const css: JSX.CSSProperties = {};
  const fg = s.inverse ? (s.bg ?? "var(--bg-panel)") : s.fg;
  const bg = s.inverse ? (s.fg ?? "var(--fg)") : s.bg;
  if (fg) css.color = fg;
  if (bg) css.background = bg;
  if (s.bold) css["font-weight"] = "600";
  if (s.dim) css.opacity = "0.7";
  if (s.italic) css["font-style"] = "italic";
  if (s.underline) css["text-decoration"] = "underline";
  return css;
}

interface Piece {
  text: string;
  style: AnsiStyle;
  link?: FileLink;
}

/** Split a line's segments at file link boundaries. */
export function linePieces(line: ConsoleLine): Piece[] {
  const text = lineText(line);
  const links = /[\\/.]/.test(text) ? findFileLinks(text) : [];
  if (!links.length) return line.segments;
  const out: Piece[] = [];
  let pos = 0;
  for (const seg of line.segments as Segment[]) {
    const segEnd = pos + seg.text.length;
    let cursor = pos;
    while (cursor < segEnd) {
      const link = links.find((l) => l.start <= cursor && cursor < l.end);
      const next = link
        ? Math.min(link.end, segEnd)
        : Math.min(segEnd, ...links.filter((l) => l.start > cursor).map((l) => l.start), segEnd);
      out.push({ text: seg.text.slice(cursor - pos, next - pos), style: seg.style, link });
      cursor = next;
    }
    pos = segEnd;
  }
  return out;
}

export interface ConsoleProps {
  lines: () => ConsoleLine[];
  /** Changes whenever lines change (lines are mutated in place). */
  version: () => number;
  softWrap: boolean;
  scrollToEnd: boolean;
  cwd?: string;
  /** Stdin: called with text including the trailing newline; omitted when not running. */
  onInput?: (text: string) => void;
  /** Extra content after the lines (e.g. a failure block). */
  footer?: JSX.Element;
  emptyText?: string;
}

export function Console(props: ConsoleProps) {
  let el!: HTMLDivElement;
  const [input, setInput] = createSignal("");
  const snapshot = () => {
    props.version();
    return [...props.lines()];
  };
  createEffect(
    on(
      () => [props.version(), props.scrollToEnd] as const,
      ([, follow]) => {
        if (follow) requestAnimationFrame(() => (el.scrollTop = el.scrollHeight));
      },
    ),
  );
  const onKeyDown = (e: KeyboardEvent) => {
    if (!props.onInput || e.ctrlKey || e.metaKey || e.altKey) return;
    if (e.key === "Enter") {
      props.onInput(`${input()}\n`);
      setInput("");
    } else if (e.key === "Backspace") setInput((s) => s.slice(0, -1));
    else if (e.key.length === 1) setInput((s) => s + e.key);
    else return;
    e.preventDefault();
  };
  return (
    <div
      ref={el}
      class="run-console"
      classList={{ wrap: props.softWrap }}
      tabIndex={0}
      onKeyDown={onKeyDown}
      data-testid="run-console"
    >
      <For each={snapshot()}>
        {(line, i) => (
          <div class="run-line">
            <For each={linePieces(line)}>
              {(p) =>
                p.link ? (
                  <a
                    class="run-link"
                    style={styleOf(p.style)}
                    onClick={() => void openFileLink(p.link!, props.cwd)}
                    title="Open in editor"
                  >
                    {p.text}
                  </a>
                ) : (
                  <span style={styleOf(p.style)}>{p.text}</span>
                )
              }
            </For>
            <Show when={props.onInput && i() === snapshot().length - 1}>
              <span class="run-input">{input()}</span>
              <span class="run-caret" />
            </Show>
            {"​"}
          </div>
        )}
      </For>
      <Show when={snapshot().length === 0 && props.emptyText}>
        <div class="run-empty muted">{props.emptyText}</div>
      </Show>
      {props.footer}
    </div>
  );
}
