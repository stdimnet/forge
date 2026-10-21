/**
 * Syntax colors for fenced code in the Markdown preview: the editor's Lezer grammars for the
 * common languages, the chat highlighter (PHP and a light tokenizer) for the rest.
 */
import { classHighlighter, highlightCode } from "@lezer/highlight";
import { javascriptLanguage, typescriptLanguage, jsxLanguage, tsxLanguage } from "@codemirror/lang-javascript";
import { cssLanguage } from "@codemirror/lang-css";
import { htmlLanguage } from "@codemirror/lang-html";
import { jsonLanguage } from "@codemirror/lang-json";
import { pythonLanguage } from "@codemirror/lang-python";
import { rustLanguage } from "@codemirror/lang-rust";
import { yamlLanguage } from "@codemirror/lang-yaml";
import { xmlLanguage } from "@codemirror/lang-xml";
import { highlight as chatHighlight, type HighlightedLines } from "@/features/ai-chat/highlight";

type Parser = typeof javascriptLanguage.parser;

const PARSERS: Record<string, () => Parser> = {
  js: () => javascriptLanguage.parser,
  javascript: () => javascriptLanguage.parser,
  mjs: () => javascriptLanguage.parser,
  jsx: () => jsxLanguage.parser,
  ts: () => typescriptLanguage.parser,
  typescript: () => typescriptLanguage.parser,
  tsx: () => tsxLanguage.parser,
  css: () => cssLanguage.parser,
  html: () => htmlLanguage.parser,
  vue: () => htmlLanguage.parser,
  json: () => jsonLanguage.parser,
  jsonc: () => jsonLanguage.parser,
  python: () => pythonLanguage.parser,
  py: () => pythonLanguage.parser,
  rust: () => rustLanguage.parser,
  rs: () => rustLanguage.parser,
  yaml: () => yamlLanguage.parser,
  yml: () => yamlLanguage.parser,
  xml: () => xmlLanguage.parser,
  svg: () => xmlLanguage.parser,
};

export type { HighlightedLines };

/** Code split into lines of `{ text, cls }` spans with `tok-*` classes. */
export function highlightFence(code: string, lang: string): HighlightedLines {
  const parser = PARSERS[lang.toLowerCase()];
  if (!parser) return chatHighlight(code, lang);
  try {
    const lines: HighlightedLines = [[]];
    highlightCode(
      code,
      parser().parse(code),
      classHighlighter,
      (text, cls) => lines[lines.length - 1].push({ text, cls }),
      () => lines.push([]),
    );
    return lines;
  } catch {
    return chatHighlight(code, lang);
  }
}
