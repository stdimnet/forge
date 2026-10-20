/** Test helpers: run a stream parser over text and read its tokens, or build an editor state. */
import { StringStream, type StreamParser } from "@codemirror/language";
import { EditorState } from "@codemirror/state";
import type { CompletionContext } from "@codemirror/autocomplete";

export type Token = [text: string, style: string | null];

/** All tokens of `text` (whitespace-only unstyled tokens are dropped). */
export function tokenize<S>(parser: StreamParser<S>, text: string): Token[][] {
  const state = parser.startState!(2);
  return text.split("\n").map((line) => {
    const out: Token[] = [];
    if (!line.length) {
      parser.blankLine?.(state, 2);
      return out;
    }
    const stream = new StringStream(line, 4, 2);
    while (!stream.eol()) {
      stream.start = stream.pos;
      const style = parser.token(stream, state);
      if (stream.pos === stream.start) throw new Error(`parser did not advance at ${stream.pos} of ${JSON.stringify(line)}`);
      const tokenText = line.slice(stream.start, stream.pos);
      if (style === null && !tokenText.trim()) continue;
      out.push([tokenText, style ?? null]);
    }
    return out;
  });
}

/** The style of the token whose text is exactly `text` on a line. */
export const styleOf = (tokens: Token[], text: string) => tokens.find(([t]) => t === text)?.[1];

export const stateOf = (doc: string, extensions: import("@codemirror/state").Extension = []) =>
  EditorState.create({ doc, extensions });

/** A completion context at the end of `doc` (or at the `|` marker). */
export function contextAt(doc: string, explicit = true, extensions: import("@codemirror/state").Extension = []): CompletionContext {
  const marker = doc.indexOf("|");
  const text = marker >= 0 ? doc.slice(0, marker) + doc.slice(marker + 1) : doc;
  const pos = marker >= 0 ? marker : text.length;
  const state = stateOf(text, extensions);
  const ctx = {
    state,
    pos,
    explicit,
    matchBefore(re: RegExp) {
      const line = state.doc.lineAt(pos);
      const start = Math.max(line.from, pos - 250);
      const m = new RegExp(`(?:${re.source})$`, re.flags).exec(state.sliceDoc(start, pos));
      return m ? { from: start + m.index, to: pos, text: m[0] } : null;
    },
  };
  return ctx as unknown as CompletionContext;
}
