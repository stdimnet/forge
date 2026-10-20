/**
 * E-Mail (`.eml`, RFC 5322 / MIME): header and body highlighting that follows multipart
 * boundaries, header name and value completion, hover decoding of RFC 2047 encoded words,
 * folding of header blocks and MIME parts, and checks for malformed headers and missing
 * closing boundaries.
 */
import { foldService, type StreamParser, type StringStream } from "@codemirror/language";
import type { EditorState, Extension, Text } from "@codemirror/state";
import type { Completion, CompletionContext, CompletionResult } from "@codemirror/autocomplete";
import { registerEditorExtension } from "@/editor/extensions";
import { languageIdFor, registerLanguage } from "@/editor/languages";
import { docDom, hoverDocs, streamLanguage, syntaxDiagnostics, type SyntaxProblem } from "./shared";

// ---------------------------------------------------------------------------------------
// Pure helpers
// ---------------------------------------------------------------------------------------

/** One RFC 2047 encoded word: `=?charset?B|Q?payload?=`. Use with a fresh `g` flag per scan. */
export const ENCODED_WORD_SOURCE = "=\\?([^?\\s*]+)(?:\\*[^?\\s]*)?\\?([bBqQ])\\?([^?\\s]*)\\?=";

const decodeBytes = (bytes: Uint8Array, charset: string): string => {
  for (const cs of [charset.toLowerCase(), "utf-8"]) {
    try {
      return new TextDecoder(cs, { fatal: false }).decode(bytes);
    } catch {
      /* unknown charset: try the next one */
    }
  }
  return Array.from(bytes, (b) => String.fromCharCode(b)).join("");
};

/** Decode one encoded word; null when the payload is not valid for its encoding. */
export function decodeEncodedWord(charset: string, encoding: string, payload: string): string | null {
  try {
    const bytes: number[] = [];
    if (/^b$/i.test(encoding)) {
      let b64 = payload.replace(/\s/g, "");
      while (b64.length % 4) b64 += "=";
      for (const ch of atob(b64)) bytes.push(ch.charCodeAt(0));
    } else {
      const text = payload.replace(/_/g, " ");
      for (let i = 0; i < text.length; i++) {
        if (text[i] === "=" && /^[0-9a-fA-F]{2}$/.test(text.slice(i + 1, i + 3))) {
          bytes.push(parseInt(text.slice(i + 1, i + 3), 16));
          i += 2;
        } else bytes.push(text.charCodeAt(i) & 0xff);
      }
    }
    return decodeBytes(Uint8Array.from(bytes), charset);
  } catch {
    return null;
  }
}

/** Replace all encoded words in `text`; whitespace between adjacent encoded words is dropped. */
export function decodeEncodedWords(text: string): string {
  const joined = text.replace(/(\?=)[ \t\r\n]+(?==\?)/g, "$1");
  return joined.replace(new RegExp(ENCODED_WORD_SOURCE, "g"), (m, cs: string, enc: string, payload: string) => {
    return decodeEncodedWord(cs, enc, payload) ?? m;
  });
}

/** The multipart boundary of a Content-Type header value, if it is a multipart type. */
export function parseBoundary(contentType: string): string | null {
  if (!/^\s*multipart\//i.test(contentType)) return null;
  const m = /boundary\s*=\s*(?:"([^"]*)"|([^\s;]+))/i.exec(contentType);
  const boundary = m?.[1] ?? m?.[2];
  return boundary ? boundary : null;
}

export type TransferEncoding = "plain" | "base64" | "qp";

export function transferEncoding(value: string): TransferEncoding {
  const v = value.trim().toLowerCase();
  if (v === "base64") return "base64";
  if (v === "quoted-printable") return "qp";
  return "plain";
}

export interface HeaderDoc {
  name: string;
  doc: string;
  /** Suggested values after the colon. */
  values?: string[];
}

export const HEADERS: HeaderDoc[] = [
  { name: "From", doc: "The author(s) of the message." },
  { name: "To", doc: "Primary recipients." },
  { name: "Cc", doc: "Carbon-copy recipients." },
  { name: "Bcc", doc: "Blind carbon-copy recipients; normally removed before delivery." },
  { name: "Subject", doc: "The topic of the message." },
  { name: "Date", doc: "When the message was written, e.g. `Tue, 20 Oct 2026 08:55:56 +0000`." },
  { name: "Message-ID", doc: "A globally unique identifier of this message, like `<id@host>`." },
  { name: "In-Reply-To", doc: "The `Message-ID` of the message this one replies to." },
  { name: "References", doc: "The `Message-ID`s of the whole conversation thread." },
  { name: "Reply-To", doc: "Where replies should go instead of `From`." },
  { name: "Sender", doc: "The agent that actually transmitted the message, when different from `From`." },
  { name: "Return-Path", doc: "The envelope sender; added by the final delivery system." },
  { name: "Received", doc: "A trace line added by each server that handled the message; the newest is on top." },
  { name: "MIME-Version", doc: "The MIME version of the message, always `1.0`.", values: ["1.0"] },
  {
    name: "Content-Type",
    doc: "The media type of the body or part, with parameters such as `charset` and `boundary`.",
    values: [
      "text/plain; charset=UTF-8",
      "text/html; charset=UTF-8",
      "multipart/mixed; boundary=",
      "multipart/alternative; boundary=",
      "multipart/related; boundary=",
      "multipart/signed; boundary=",
      "message/rfc822",
      "application/pdf",
      "application/octet-stream",
      "image/png",
      "image/jpeg",
    ],
  },
  {
    name: "Content-Transfer-Encoding",
    doc: "How the body is encoded for transport.",
    values: ["7bit", "8bit", "binary", "base64", "quoted-printable"],
  },
  {
    name: "Content-Disposition",
    doc: "Whether a part is shown inline or is an attachment, with an optional `filename`.",
    values: ["inline", "attachment", 'attachment; filename=""'],
  },
  { name: "Content-ID", doc: "Identifier of a part, referenced from HTML as `cid:`." },
  { name: "Content-Description", doc: "A human readable description of a part." },
  { name: "X-Mailer", doc: "The software that composed the message." },
  { name: "User-Agent", doc: "The mail client that composed the message." },
  { name: "X-Priority", doc: "Priority hint, `1` (highest) to `5` (lowest).", values: ["1", "3", "5"] },
  { name: "Importance", doc: "Importance hint.", values: ["high", "normal", "low"] },
  { name: "List-Id", doc: "Identifier of the mailing list the message was sent through." },
  { name: "List-Unsubscribe", doc: "How to unsubscribe, as `<mailto:…>` and/or `<https://…>` (RFC 2369)." },
  { name: "List-Unsubscribe-Post", doc: "`List-Unsubscribe=One-Click` enables one-click unsubscribe (RFC 8058)." },
  { name: "Precedence", doc: "Delivery hint for bulk mail.", values: ["bulk", "list", "junk"] },
  { name: "Auto-Submitted", doc: "Marks automatically generated mail (RFC 3834).", values: ["auto-generated", "auto-replied", "no"] },
  { name: "Authentication-Results", doc: "SPF, DKIM and DMARC verdicts recorded by the receiving server (RFC 8601)." },
  { name: "Received-SPF", doc: "The result of the SPF check." },
  { name: "DKIM-Signature", doc: "A DKIM signature of the message (RFC 6376)." },
  { name: "ARC-Seal", doc: "Authenticated Received Chain seal (RFC 8617)." },
  { name: "X-Spam-Status", doc: "SpamAssassin verdict and score." },
  { name: "X-Original-To", doc: "The original envelope recipient, added by some delivery agents." },
  { name: "Delivered-To", doc: "The recipient the message was delivered to." },
  { name: "Disposition-Notification-To", doc: "Request a read receipt (RFC 8098)." },
];

const HEADER_MAP = new Map(HEADERS.map((h) => [h.name.toLowerCase(), h]));
const knownHeader = (name: string) => HEADER_MAP.has(name.toLowerCase());

// ---------------------------------------------------------------------------------------
// Line classification (shared by the highlighter, the checks and folding)
// ---------------------------------------------------------------------------------------

export type LineKind = "header" | "cont" | "badheader" | "badcont" | "blank" | "boundary" | "body";

export interface EmlState {
  section: "headers" | "body";
  /** Lower-case name of the header being read, null before the first one. */
  header: string | null;
  /** Raw (unfolded) value of the Content-Type / Content-Transfer-Encoding of this part. */
  ct: string;
  cte: string;
  /** Transfer encoding of the current body. */
  encoding: TransferEncoding;
  /** Open multipart boundaries, innermost last. */
  boundaries: string[];
  /** Kind of the current line, set at its start. */
  line: LineKind;
  /** Within a header line: next token is the name, the colon, or the value. */
  phase: "name" | "colon" | "value";
  /** Content-Type value has produced its media type already. */
  typeSeen: boolean;
  /** Current body line is a quoted reply. */
  quote: boolean;
  /** Boundary index hit by the last boundary line, and the boundaries it closed implicitly. */
  hit: number;
  dropFrom: number;
  dropTo: number;
}

export const emlStartState = (): EmlState => ({
  section: "headers",
  header: null,
  ct: "",
  cte: "",
  encoding: "plain",
  boundaries: [],
  line: "blank",
  phase: "name",
  typeSeen: false,
  quote: false,
  hit: -1,
  dropFrom: 0,
  dropTo: 0,
});

/** The headers of a part end: its body starts, possibly opening a nested multipart. */
function finishHeaders(state: EmlState): void {
  state.section = "body";
  state.header = null;
  const boundary = parseBoundary(state.ct);
  if (boundary) {
    state.boundaries.push(boundary);
    state.encoding = "plain";
  } else state.encoding = transferEncoding(state.cte);
}

/** Classify the line starting now and update the MIME state. */
export function lineKind(state: EmlState, text: string): LineKind {
  state.hit = -1;
  state.dropFrom = state.dropTo = 0;
  // A boundary line also ends a part that has no body (`--b` straight after its headers).
  if (text.startsWith("--") && matchBoundary(state, text)) return "boundary";
  if (state.section === "headers") {
    if (!text.trim()) {
      finishHeaders(state);
      return "blank";
    }
    if (/^\s/.test(text)) {
      if (!state.header) return "badcont";
      const rest = text.trim();
      if (state.header === "content-type") state.ct += ` ${rest}`;
      else if (state.header === "content-transfer-encoding") state.cte += ` ${rest}`;
      return "cont";
    }
    const m = /^([^\s:]+):(.*)$/.exec(text);
    if (!m) {
      state.header = null;
      return "badheader";
    }
    state.header = m[1].toLowerCase();
    if (state.header === "content-type") state.ct = m[2].trim();
    else if (state.header === "content-transfer-encoding") state.cte = m[2].trim();
    return "header";
  }
  return "body";
}

function matchBoundary(state: EmlState, text: string): boolean {
  for (let i = state.boundaries.length - 1; i >= 0; i--) {
    const prefix = `--${state.boundaries[i]}`;
    if (!text.startsWith(prefix)) continue;
    const rest = text.slice(prefix.length);
    const closing = /^--\s*$/.test(rest);
    if (!closing && !/^\s*$/.test(rest)) continue;
    state.hit = i;
    state.dropFrom = i + 1;
    state.dropTo = state.boundaries.length;
    if (closing) {
      state.boundaries.length = i;
      state.section = "body";
    } else {
      state.boundaries.length = i + 1;
      state.section = "headers";
    }
    state.header = null;
    state.ct = state.cte = "";
    state.encoding = "plain";
    return true;
  }
  return false;
}

// ---------------------------------------------------------------------------------------
// Highlighting
// ---------------------------------------------------------------------------------------

const URL_RE = /^(?:https?:\/\/|mailto:)[^\s<>"')\]]+/i;
const ADDR_IN_BRACKETS = /^<[^<>\s]*@?[^<>\s]*>/;
const BARE_ADDR = /^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}/;
const ENCODED_AT_START = new RegExp(`^${ENCODED_WORD_SOURCE}`);

function headerValueToken(stream: StringStream, state: EmlState): string | null {
  if (stream.eatSpace()) return null;
  if (stream.match(ENCODED_AT_START)) return "special";
  if (stream.match(ADDR_IN_BRACKETS)) return "url";
  if (stream.match(URL_RE)) return "url";
  if (stream.match(/^"(?:[^"\\]|\\.)*"?/)) return "string";
  if (stream.match(/^\([^)]*\)?/)) return "comment";
  if (stream.match(BARE_ADDR)) return "url";
  const header = state.header;
  if (header === "content-transfer-encoding") {
    if (stream.match(/^[\w-]+/)) return "keyword";
  }
  if (header === "content-type" || header === "content-disposition") {
    if (header === "content-type" && !state.typeSeen && stream.match(/^[\w.+-]+\/[\w.+-]+/)) {
      state.typeSeen = true;
      return "type";
    }
    if (stream.match(/^[\w-]+(?==)/)) return "property";
    if (stream.eat("=")) return "operator";
    if (header === "content-disposition" && !state.typeSeen && stream.match(/^(?:inline|attachment)\b/i)) {
      state.typeSeen = true;
      return "keyword";
    }
  }
  if (stream.match(/^[;,:]/)) return "punct";
  if (stream.match(/^\d+/)) return "number";
  if (!stream.match(/^[^\s<"(;,=:@]+/)) stream.next();
  return null;
}

function bodyToken(stream: StringStream, state: EmlState): string | null {
  if (state.quote) {
    stream.skipToEnd();
    return "comment";
  }
  if (state.encoding === "base64") {
    stream.skipToEnd();
    return "comment";
  }
  if (state.encoding === "qp") {
    if (stream.match(/^=[0-9A-Fa-f]{2}/) || stream.match(/^=\s*$/)) return "special";
    if (!stream.match(/^[^=]+/)) stream.next();
    return null;
  }
  if (stream.eatSpace()) return null;
  if (stream.match(URL_RE)) return "url";
  if (stream.match(ADDR_IN_BRACKETS) && /[@:]/.test(stream.current())) return "url";
  if (stream.pos > stream.start) return null;
  if (stream.match(BARE_ADDR)) return "url";
  while (!stream.eol()) {
    const ch = stream.peek()!;
    if (/[\s<]/.test(ch) || stream.match(/^(?:https?:\/\/|mailto:)/i, false)) break;
    stream.next();
  }
  if (stream.pos === stream.start) stream.next();
  return null;
}

export const emlParser: StreamParser<EmlState> = {
  name: "eml",
  startState: emlStartState,
  copyState: (s) => ({ ...s, boundaries: [...s.boundaries] }),
  blankLine(state) {
    lineKind(state, "");
  },
  token(stream, state) {
    if (stream.sol()) {
      state.line = lineKind(state, stream.string);
      state.phase = "name";
      state.typeSeen = false;
      state.quote = state.line === "body" && /^\s*>/.test(stream.string);
    }
    switch (state.line) {
      case "blank":
        stream.skipToEnd();
        return null;
      case "boundary":
        stream.skipToEnd();
        return "meta";
      case "badheader":
      case "badcont":
        stream.skipToEnd();
        return "invalid";
      case "header":
        if (state.phase === "name") {
          state.phase = "colon";
          stream.match(/^[^\s:]+/);
          return knownHeader(stream.current()) ? "def" : "property";
        }
        if (state.phase === "colon") {
          state.phase = "value";
          stream.eat(":");
          return "punct";
        }
        return headerValueToken(stream, state);
      case "cont":
        return headerValueToken(stream, state);
      default:
        return bodyToken(stream, state);
    }
  },
};

// ---------------------------------------------------------------------------------------
// Structure analysis: problems and fold regions
// ---------------------------------------------------------------------------------------

export interface EmlAnalysis {
  problems: SyntaxProblem[];
  /** Fold regions: first line -> last line (0-based, inclusive). */
  folds: Map<number, number>;
}

export function analyzeEml(text: string): EmlAnalysis {
  const problems: SyntaxProblem[] = [];
  const folds = new Map<number, number>();
  const lines = text.split("\n");
  const state = emlStartState();
  const openAt: { from: number; to: number; boundary: string }[] = [];
  let ctRange = { from: 0, to: 0 };
  let offset = 0;
  let blockStart = -1;
  let lastHeader = -1;
  let headerStart = -1;
  let headerEnd = -1;
  const parts: { line: number; depth: number }[] = [];

  const closeHeader = () => {
    if (headerStart >= 0 && headerEnd > headerStart && !folds.has(headerStart)) folds.set(headerStart, headerEnd);
    headerStart = headerEnd = -1;
  };
  const closeBlock = () => {
    closeHeader();
    if (blockStart >= 0 && lastHeader > blockStart && !folds.has(blockStart)) folds.set(blockStart, lastHeader);
    blockStart = lastHeader = -1;
  };
  const closeParts = (depth: number, endLine: number) => {
    while (parts.length && parts[parts.length - 1].depth >= depth) {
      const p = parts.pop()!;
      if (endLine > p.line && !folds.has(p.line)) folds.set(p.line, endLine);
    }
  };

  for (let i = 0; i < lines.length; i++) {
    const raw = lines[i];
    const line = raw.replace(/\r$/, "");
    const start = offset;
    offset += raw.length + 1;
    const wasHeaders = state.section === "headers";
    const kind = lineKind(state, line);
    switch (kind) {
      case "header":
        closeHeader();
        if (blockStart < 0) blockStart = i;
        headerStart = i;
        headerEnd = i;
        lastHeader = i;
        if (state.header === "content-type") ctRange = { from: start, to: start + line.length };
        break;
      case "cont":
        headerEnd = i;
        lastHeader = i;
        if (state.header === "content-type") ctRange.to = start + line.length;
        break;
      case "badheader":
        closeHeader();
        problems.push({ from: start, to: start + line.length, message: "Malformed header line: expected `Name: value`" });
        break;
      case "badcont":
        problems.push({ from: start, to: start + line.length, message: "Continuation line without a header to continue" });
        break;
      case "blank":
        if (wasHeaders) closeBlock();
        break;
      case "boundary": {
        closeBlock();
        closeParts(state.hit, i - 1);
        for (let j = state.dropFrom; j < state.dropTo; j++) {
          const at = openAt[j];
          if (at) problems.push({ from: at.from, to: at.to, message: `Missing closing boundary \`--${at.boundary}--\``, severity: "warning" });
        }
        openAt.length = state.boundaries.length;
        if (state.section === "headers") parts.push({ line: i, depth: state.hit });
        break;
      }
    }
    while (openAt.length < state.boundaries.length) {
      openAt.push({ ...ctRange, boundary: state.boundaries[openAt.length] });
    }
  }
  closeBlock();
  closeParts(0, lines.length - 1);
  for (const at of openAt) {
    problems.push({ from: at.from, to: at.to, message: `Missing closing boundary \`--${at.boundary}--\``, severity: "warning" });
  }
  return { problems, folds };
}

export const emlProblems = (text: string): SyntaxProblem[] => analyzeEml(text).problems;

const foldCache = new WeakMap<Text, Map<number, number>>();

export function emlFolding(): Extension {
  return foldService.of((state, lineStart) => {
    let folds = foldCache.get(state.doc);
    if (!folds) {
      folds = analyzeEml(state.doc.toString()).folds;
      foldCache.set(state.doc, folds);
    }
    const line = state.doc.lineAt(lineStart);
    const end = folds.get(line.number - 1);
    if (end === undefined || end + 1 > state.doc.lines) return null;
    const to = state.doc.line(end + 1).to;
    return to > line.to ? { from: line.to, to } : null;
  });
}

// ---------------------------------------------------------------------------------------
// Completion and hover
// ---------------------------------------------------------------------------------------

/** MIME state at the start of line `lineNumber` (1-based). */
export function stateAtLine(doc: Text, lineNumber: number): EmlState {
  const state = emlStartState();
  for (let n = 1; n < lineNumber; n++) lineKind(state, doc.line(n).text.replace(/\r$/, ""));
  return state;
}

const headerOption = (h: HeaderDoc): Completion => ({
  label: h.name,
  type: "property",
  info: () => docDom({ title: `${h.name}:`, doc: h.doc }),
  apply: `${h.name}: `,
});

function emlCompletions(ctx: CompletionContext): CompletionResult | null {
  const line = ctx.state.doc.lineAt(ctx.pos);
  const before = line.text.slice(0, ctx.pos - line.from);
  const state = stateAtLine(ctx.state.doc, line.number);
  if (state.section !== "headers") return null;

  const name = /^([A-Za-z][\w-]*):\s*(.*)$/.exec(before);
  if (name) {
    const doc = HEADER_MAP.get(name[1].toLowerCase());
    if (!doc?.values) return null;
    const param = /;\s*([\w-]*)$/.exec(name[2]);
    if (param && /^content-(?:type|disposition)$/i.test(name[1])) {
      const params = /type/i.test(name[1]) ? ["charset=", "boundary=", "name=", "format=flowed"] : ["filename=", "size="];
      return { from: ctx.pos - param[1].length, options: params.map((label) => ({ label, type: "property" })), validFor: /^[\w-]*$/ };
    }
    const typed = /[^\s;]*$/.exec(before)![0];
    if (/;/.test(name[2])) return null;
    return {
      from: ctx.pos - typed.length,
      options: doc.values.map((label) => ({ label, type: "constant" })),
      validFor: /^[\w/.+;=" -]*$/,
    };
  }
  if (/^[A-Za-z-]*$/.test(before)) {
    if (!before && !ctx.explicit) return null;
    return { from: line.from, options: HEADERS.map(headerOption), validFor: /^[A-Za-z-]*$/ };
  }
  return null;
}

/** The encoded word at column `col` of `text`, with its decoded text. */
export function encodedWordAt(text: string, col: number): { from: number; to: number; charset: string; encoding: string; decoded: string } | null {
  for (const m of text.matchAll(new RegExp(ENCODED_WORD_SOURCE, "g"))) {
    const from = m.index!;
    const to = from + m[0].length;
    if (col < from || col > to) continue;
    const decoded = decodeEncodedWord(m[1], m[2], m[3]);
    if (decoded === null) return null;
    return { from, to, charset: m[1], encoding: /^b$/i.test(m[2]) ? "Base64" : "Quoted-printable", decoded };
  }
  return null;
}

const escapeMarkdown = (s: string) => s.replace(/[\\`*_{}[\]()#+!<>|~]/g, "\\$&");

function encodedWordHover(state: EditorState, pos: number) {
  const line = state.doc.lineAt(pos);
  const hit = encodedWordAt(line.text, pos - line.from);
  if (!hit) return null;
  return {
    from: line.from + hit.from,
    to: line.from + hit.to,
    dom: () =>
      docDom({
        title: `Encoded word · ${hit.charset.toUpperCase()} · ${hit.encoding}`,
        doc: escapeMarkdown(hit.decoded) || "(empty)",
      }),
  };
}

export function emlSupport() {
  const lang = streamLanguage(emlParser);
  return [lang, lang.data.of({ autocomplete: emlCompletions }), emlFolding(), hoverDocs(encodedWordHover)];
}

registerLanguage({
  id: "eml",
  label: "E-Mail",
  extensions: ["eml"],
  indent: "    ",
  load: emlSupport,
});

registerEditorExtension(({ path }) =>
  languageIdFor(path) === "eml"
    ? syntaxDiagnostics(path, { source: "E-Mail" }, (s) => emlProblems(s.doc.toString()))
    : null,
);
