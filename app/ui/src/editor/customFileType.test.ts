import { describe, expect, it } from "vitest";
import type { CustomSyntax } from "@/ipc/filetypes";
import { styleOf, tokenize } from "@/features/languages/testkit";
import { customParser } from "./customFileType";

const base: CustomSyntax = {
  lineComment: "#",
  lineCommentAtStart: false,
  blockCommentStart: "/*",
  blockCommentEnd: "*/",
  hexPrefix: "0x",
  numberPostfixes: "lL",
  keywords: [["if", "then", "else"], ["print"], ["int"], ["#define"]],
  ignoreCase: true,
  pairedBraces: true,
  pairedBrackets: true,
  pairedParens: true,
  stringEscapes: true,
};

describe("custom file type highlighting", () => {
  it("colors keywords, numbers, strings and comments", () => {
    const [line] = tokenize(customParser(base), 'IF x then print "a\\"b" 0xFFL 12.5 # done');
    expect(styleOf(line, "IF")).toBe("keyword");
    expect(styleOf(line, "then")).toBe("keyword");
    expect(styleOf(line, "print")).toBe("builtin");
    expect(styleOf(line, "x")).toBeNull();
    expect(styleOf(line, '"a\\"b"')).toBe("string");
    expect(styleOf(line, "0xFFL")).toBe("number");
    expect(styleOf(line, "12.5")).toBe("number");
    expect(styleOf(line, "# done")).toBe("comment");
  });

  it("keeps block comments across lines and finds special keywords", () => {
    const lines = tokenize(customParser(base), "int a /* one\ntwo */ #define B");
    expect(styleOf(lines[0], "int")).toBe("type");
    expect(styleOf(lines[0], "/* one")).toBe("comment");
    expect(styleOf(lines[1], "two */")).toBe("comment");
    // `#define` is a keyword before it is a line comment.
    expect(styleOf(lines[1], "#define")).toBe("atom");
    expect(styleOf(lines[1], "B")).toBeNull();
  });

  it("respects case and line-start comments", () => {
    const strict = { ...base, ignoreCase: false, lineComment: "--", lineCommentAtStart: true, keywords: [["if"], [], [], []] };
    const [line] = tokenize(customParser(strict), "IF if a -- b");
    expect(styleOf(line, "IF")).toBeNull();
    expect(styleOf(line, "if")).toBe("keyword");
    expect(line.some(([, s]) => s === "comment")).toBe(false);
    const [start] = tokenize(customParser(strict), "  -- note");
    expect(styleOf(start, "-- note")).toBe("comment");
  });
});
