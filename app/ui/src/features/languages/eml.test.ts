import { describe, expect, it } from "vitest";
import { languageIdFor, languageLabel } from "@/editor/languages";
import {
  analyzeEml,
  decodeEncodedWord,
  decodeEncodedWords,
  emlParser,
  emlProblems,
  encodedWordAt,
  parseBoundary,
  stateAtLine,
  transferEncoding,
} from "./eml";
import "./eml";
import { stateOf, styleOf, tokenize } from "./testkit";

const MAIL = [
  "From: Ann <ann@example.org>",
  "Subject: =?UTF-8?B?SGVsbG8gV8O2cmxk?=",
  "Content-Type: multipart/mixed;",
  '  boundary="outer"',
  "",
  "preamble",
  "--outer",
  "Content-Type: text/plain; charset=utf-8",
  "",
  "Hi https://example.org/x",
  "> quoted",
  "--outer",
  "Content-Type: application/pdf",
  "Content-Transfer-Encoding: base64",
  "",
  "JVBERi0xLjQK",
  "--outer--",
  "epilogue",
].join("\n");

describe("encoded words", () => {
  it("decodes B and Q words in several charsets", () => {
    expect(decodeEncodedWord("UTF-8", "B", "SGVsbG8gV8O2cmxk")).toBe("Hello Wörld");
    expect(decodeEncodedWord("iso-8859-1", "Q", "Gr=FC=DFe_aus_K=F6ln")).toBe("Grüße aus Köln");
    expect(decodeEncodedWord("utf-8", "b", "SGk")).toBe("Hi");
  });
  it("never throws on bad input", () => {
    expect(decodeEncodedWord("utf-8", "B", "@@@")).toBeNull();
    expect(decodeEncodedWord("no-such-charset", "Q", "abc")).toBe("abc");
  });
  it("joins adjacent words and keeps the rest", () => {
    expect(decodeEncodedWords("=?UTF-8?Q?Hel?= =?UTF-8?Q?lo?= plain =?bad")).toBe("Hello plain =?bad");
  });
  it("finds the word under a column", () => {
    const line = "Subject: =?UTF-8?Q?caf=C3=A9?= done";
    const hit = encodedWordAt(line, 14)!;
    expect(hit.decoded).toBe("café");
    expect(line.slice(hit.from, hit.to)).toBe("=?UTF-8?Q?caf=C3=A9?=");
    expect(encodedWordAt(line, 2)).toBeNull();
  });
});

describe("MIME helpers", () => {
  it("parses boundaries", () => {
    expect(parseBoundary('multipart/mixed; boundary="a b"')).toBe("a b");
    expect(parseBoundary("multipart/alternative; charset=x; boundary=abc123;")).toBe("abc123");
    expect(parseBoundary("text/plain; boundary=x")).toBeNull();
  });
  it("reads transfer encodings", () => {
    expect(transferEncoding(" Base64 ")).toBe("base64");
    expect(transferEncoding("quoted-printable")).toBe("qp");
    expect(transferEncoding("7bit")).toBe("plain");
  });
});

describe("tokenizer", () => {
  const lines = tokenize(emlParser, MAIL);
  it("styles header names, values and encoded words", () => {
    expect(styleOf(lines[0], "From")).toBe("def");
    expect(styleOf(lines[0], "<ann@example.org>")).toBe("url");
    expect(styleOf(lines[1], "=?UTF-8?B?SGVsbG8gV8O2cmxk?=")).toBe("special");
    expect(styleOf(lines[2], "multipart/mixed")).toBe("type");
    expect(styleOf(lines[3], '"outer"')).toBe("string");
  });
  it("marks boundaries and follows them", () => {
    expect(lines[6]).toEqual([["--outer", "meta"]]);
    expect(styleOf(lines[7], "Content-Type")).toBe("def");
    expect(styleOf(lines[7], "text/plain")).toBe("type");
    expect(lines[16]).toEqual([["--outer--", "meta"]]);
  });
  it("styles body text", () => {
    expect(styleOf(lines[9], "https://example.org/x")).toBe("url");
    expect(lines[10]).toEqual([["> quoted", "comment"]]);
  });
  it("dims base64 parts and resets after the closing boundary", () => {
    expect(styleOf(lines[13], "base64")).toBe("keyword");
    expect(lines[15]).toEqual([["JVBERi0xLjQK", "comment"]]);
    expect(lines[17].every(([, s]) => s === null)).toBe(true);
  });
  it("handles quoted-printable bodies and bad headers", () => {
    const l = tokenize(emlParser, "X-A: b\nContent-Transfer-Encoding: quoted-printable\n\ncaf=C3=A9 soft=\nnot a header\n");
    expect(styleOf(l[3], "=C3")).toBe("special");
    const bad = tokenize(emlParser, "no colon here\n  orphan\nSubject: x");
    expect(bad[0][0][1]).toBe("invalid");
    expect(styleOf(bad[2], "Subject")).toBe("def");
  });
  it("copies state without sharing boundaries", () => {
    const s = emlParser.startState!(2);
    s.boundaries.push("x");
    const c = emlParser.copyState!(s);
    c.boundaries.push("y");
    expect(s.boundaries).toEqual(["x"]);
  });
});

describe("analysis", () => {
  it("accepts a well-formed message", () => {
    expect(emlProblems(MAIL)).toEqual([]);
  });
  it("reports malformed headers and orphan continuations", () => {
    const p = emlProblems("From: a\nthis is wrong\n  orphan?\n\nbody");
    expect(p.map((x) => x.message)).toEqual([
      "Malformed header line: expected `Name: value`",
      "Continuation line without a header to continue",
    ]);
    expect(p[0].from).toBe(8);
  });
  it("reports a missing closing boundary at the Content-Type header", () => {
    const text = MAIL.replace("--outer--\nepilogue", "tail");
    const p = emlProblems(text);
    expect(p).toHaveLength(1);
    expect(p[0].message).toContain("--outer--");
    expect(text.slice(p[0].from, p[0].to)).toBe('Content-Type: multipart/mixed;\n  boundary="outer"');
  });
  it("reports inner boundaries closed implicitly", () => {
    const text = [
      "Content-Type: multipart/mixed; boundary=o",
      "",
      "--o",
      "Content-Type: multipart/alternative; boundary=i",
      "",
      "--i",
      "",
      "x",
      "--o--",
    ].join("\n");
    const p = emlProblems(text);
    expect(p.map((x) => x.message)).toEqual(["Missing closing boundary `--i--`"]);
  });
  it("accepts a boundary right after part headers", () => {
    const text = "Content-Type: multipart/mixed; boundary=o\n\n--o\nContent-Type: text/plain\n--o--\n";
    expect(emlProblems(text)).toEqual([]);
  });
  it("computes fold regions for headers and parts", () => {
    const { folds } = analyzeEml(MAIL);
    expect(folds.get(2)).toBe(3); // folded Content-Type header
    expect(folds.get(6)).toBe(10); // first part
    expect(folds.get(11)).toBe(15); // second part
    expect(folds.get(0)).toBe(3); // header block
  });
  it("tracks the section at a line", () => {
    const doc = stateOf(MAIL).doc;
    expect(stateAtLine(doc, 2).section).toBe("headers");
    expect(stateAtLine(doc, 8).section).toBe("headers"); // part headers after `--outer`
    expect(stateAtLine(doc, 10).section).toBe("body");
  });
});

describe("registration", () => {
  it("detects .eml files", () => {
    expect(languageIdFor("mail/msg.eml")).toBe("eml");
    expect(languageLabel("a.EML")).toBe("E-Mail");
  });
});
