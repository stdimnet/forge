/**
 * Syntax modes of the web tooling file types (`crates/forge-filetypes`): GraphQL and Prisma
 * schema files as stream parsers, Svelte and Astro components as HTML with their script and
 * style blocks (the Svelte / Astro language servers add the rest). These are per-keystroke
 * tokenizers, so they stay in TypeScript.
 */
import type { StreamParser, StringStream } from "@codemirror/language";
import { html } from "@codemirror/lang-html";
import { registerLanguage } from "@/editor/languages";
import { streamLanguage } from "@/editor/tokens";

// ---- GraphQL ---------------------------------------------------------------------------

const GQL_KEYWORDS = new Set([
  "query",
  "mutation",
  "subscription",
  "fragment",
  "on",
  "type",
  "interface",
  "union",
  "enum",
  "input",
  "scalar",
  "schema",
  "extend",
  "directive",
  "implements",
  "repeatable",
]);
const GQL_ATOMS = new Set(["true", "false", "null"]);

interface GqlState {
  /** Inside a block string `"""`. */
  block: boolean;
  /** After `:` (a type follows in definitions and variables). */
  typeNext: boolean;
}

export const graphqlParser: StreamParser<GqlState> = {
  name: "graphql",
  startState: () => ({ block: false, typeNext: false }),
  token(stream: StringStream, state: GqlState) {
    if (state.block) {
      if (stream.skipTo('"""')) {
        stream.pos += 3;
        state.block = false;
      } else stream.skipToEnd();
      return "string";
    }
    if (stream.eatSpace()) return null;
    if (stream.match("#")) {
      stream.skipToEnd();
      return "comment";
    }
    if (stream.match('"""')) {
      state.block = true;
      return "string";
    }
    if (stream.match(/^"(?:[^"\\]|\\.)*"?/)) return "string";
    if (stream.match(/^\$[A-Za-z_]\w*/)) return "variable";
    if (stream.match(/^@[A-Za-z_]\w*/)) return "meta";
    if (stream.match(/^-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?/)) return "number";
    if (stream.match("...")) return "operator";
    if (stream.match(/^[A-Za-z_]\w*/)) {
      const w = stream.current();
      const typeNext = state.typeNext;
      state.typeNext = false;
      if (GQL_KEYWORDS.has(w)) return "keyword";
      if (GQL_ATOMS.has(w)) return "atom";
      if (typeNext || /^[A-Z]/.test(w)) return "type";
      return stream.match(/^\s*\(/, false) ? "func" : "property";
    }
    const ch = stream.next();
    if (ch === ":") state.typeNext = true;
    if (ch && "{}()[]".includes(ch)) return "bracket";
    if (ch === "!" || ch === "=" || ch === "|" || ch === "&") return "operator";
    return "punct";
  },
  languageData: { commentTokens: { line: "#" }, closeBrackets: { brackets: ["(", "[", "{", '"'] } },
};

registerLanguage({ id: "graphql", load: () => streamLanguage(graphqlParser) });

// ---- Prisma ----------------------------------------------------------------------------

const PRISMA_BLOCKS = new Set(["model", "enum", "datasource", "generator", "type", "view"]);
const PRISMA_SCALARS = new Set(["String", "Int", "BigInt", "Float", "Decimal", "Boolean", "DateTime", "Json", "Bytes", "Unsupported"]);

export const prismaParser: StreamParser<{ inBlock: boolean; fieldStart: boolean }> = {
  name: "prisma",
  startState: () => ({ inBlock: false, fieldStart: true }),
  token(stream, state) {
    if (stream.sol()) state.fieldStart = true;
    if (stream.eatSpace()) return null;
    if (stream.match("//")) {
      stream.skipToEnd();
      return "comment";
    }
    if (stream.match(/^"(?:[^"\\]|\\.)*"?/)) return "string";
    if (stream.match(/^@@?[A-Za-z_][\w.]*/)) return "meta";
    if (stream.match(/^-?\d+(?:\.\d+)?/)) return "number";
    if (stream.match(/^[A-Za-z_]\w*/)) {
      const w = stream.current();
      const first = state.fieldStart;
      state.fieldStart = false;
      if (!state.inBlock && PRISMA_BLOCKS.has(w)) return "keyword";
      if (w === "true" || w === "false" || w === "null") return "atom";
      if (PRISMA_SCALARS.has(w)) return "builtin";
      if (stream.match(/^\s*\(/, false)) return "func";
      if (state.inBlock && first) return "property";
      return /^[A-Z]/.test(w) ? "type" : "name";
    }
    const ch = stream.next();
    if (ch === "{") state.inBlock = true;
    if (ch === "}") state.inBlock = false;
    if (ch && "{}()[]".includes(ch)) return "bracket";
    if (ch === "?" || ch === "=") return "operator";
    return "punct";
  },
  languageData: { commentTokens: { line: "//" }, closeBrackets: { brackets: ["(", "[", "{", '"'] } },
};

registerLanguage({ id: "prisma", load: () => streamLanguage(prismaParser) });

// ---- Svelte and Astro ------------------------------------------------------------------

// HTML with `<script lang="ts">` and `<style>` blocks; Astro's `---` frontmatter is
// highlighted by the Astro language server's semantic tokens when it runs.
registerLanguage({ id: "svelte", load: () => html({ selfClosingTags: true }) });
registerLanguage({ id: "astro", load: () => html({ selfClosingTags: true }) });
