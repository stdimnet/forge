/** JSON and JSON with comments: Lezer grammar, folding and syntax errors in the Problems view. */
import { json } from "@codemirror/lang-json";
import { registerLanguage, languageIdFor } from "@/editor/languages";
import { registerEditorExtension } from "@/editor/extensions";
import { lezerProblems, syntaxDiagnostics } from "./shared";

registerLanguage({
  id: "json",
  load: () => json(),
});

/**
 * JSON with comments and trailing commas (`.jsonc`, `.json5`, devcontainer.json, tsconfig):
 * the Lezer grammar is strict JSON, so its errors would be wrong there.
 */
export const isJsonWithComments = (path: string): boolean =>
  /\.(jsonc|json5)$|(^|[\\/])(\.?devcontainer|tsconfig[^\\/]*|jsconfig[^\\/]*)\.json$/.test(path);

registerEditorExtension(({ path }) =>
  languageIdFor(path) === "json" && !isJsonWithComments(path)
    ? syntaxDiagnostics(path, { source: "JSON", serverIds: ["vscode-json-language-server"] }, (s) => lezerProblems(s))
    : null,
);
