/**
 * `.env` files (dotenv, Symfony, Laravel, docker): highlighting of keys, quoted and
 * unquoted values and `${VAR}` interpolation, completion of variables defined in the file,
 * Flex section folding, and problems that make dotenv ignore or reject a line.
 */
import type { CompletionContext } from "@codemirror/autocomplete";
import { registerEditorExtension } from "@/editor/extensions";
import { languageIdFor, registerLanguage } from "@/editor/languages";
import { dotenv, parseEnv } from "@/editor/dotenv";
import { markerFolding, streamLanguage, syntaxDiagnostics } from "./shared";

function completions(ctx: CompletionContext) {
  const m = ctx.matchBefore(/\$\{?\w*/);
  if (!m) return null;
  const braces = m.text.startsWith("${");
  const keys = [...new Set(parseEnv(ctx.state.doc.toString()).entries.map((e) => e.key))];
  return {
    from: m.from + (braces ? 2 : 1),
    options: keys.map((label) => ({
      label,
      type: "variable",
      apply: braces ? `${label}}` : label,
    })),
    validFor: /^\w*$/,
  };
}

export function envSupport() {
  const lang = streamLanguage(dotenv);
  return [lang, lang.data.of({ autocomplete: completions }), markerFolding()];
}

export const ENV_FILE_NAMES = [
  ".env",
  ".env.local",
  ".env.*.local",
  ".env.dist",
  ".env.*.dist",
  ".env.example",
  ".env.sample",
  ".env.template",
  ".env.defaults",
  ".env.dev",
  ".env.development",
  ".env.prod",
  ".env.production",
  ".env.stage",
  ".env.staging",
  ".env.test",
  ".env.testing",
];

registerLanguage({
  id: "dotenv",
  label: "Dotenv",
  extensions: ["env"],
  fileNames: ENV_FILE_NAMES,
  indent: "  ",
  load: envSupport,
});

registerEditorExtension(({ path }) =>
  languageIdFor(path) === "dotenv"
    ? syntaxDiagnostics(path, { source: ".env" }, (s) => parseEnv(s.doc.toString()).problems)
    : null,
);

export { dotenv as envParser, parseEnv };
