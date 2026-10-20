/** Python: Lezer grammar with scope-aware completion, plus syntax errors in the Problems view. */
import { python } from "@codemirror/lang-python";
import { registerLanguage } from "@/editor/languages";
import { registerEditorExtension } from "@/editor/extensions";
import { languageIdFor } from "@/editor/languages";
import { lezerProblems, syntaxDiagnostics } from "./shared";

registerLanguage({
  id: "python",
  load: () => python(),
});

registerEditorExtension(({ path }) =>
  languageIdFor(path) === "python"
    ? syntaxDiagnostics(path, { source: "Python", serverIds: ["pyright", "basedpyright"] }, (s) => lezerProblems(s))
    : null,
);
