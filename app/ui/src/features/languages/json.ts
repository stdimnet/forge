/** JSON and JSON with comments: Lezer grammar, folding and syntax errors in the Problems view. */
import { json } from "@codemirror/lang-json";
import { registerLanguage, languageIdFor } from "@/editor/languages";
import { registerEditorExtension } from "@/editor/extensions";
import { lezerProblems, syntaxDiagnostics } from "./shared";

registerLanguage({
  id: "json",
  load: () => json(),
});

registerEditorExtension(({ path }) =>
  languageIdFor(path) === "json"
    ? syntaxDiagnostics(path, { source: "JSON", serverIds: ["vscode-json-language-server"] }, (s) => lezerProblems(s))
    : null,
);
