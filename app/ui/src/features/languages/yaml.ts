/** YAML: Lezer grammar with folding and indentation, plus syntax errors in the Problems view. */
import { yaml } from "@codemirror/lang-yaml";
import { registerLanguage, languageIdFor } from "@/editor/languages";
import { registerEditorExtension } from "@/editor/extensions";
import { lezerProblems, syntaxDiagnostics } from "./shared";

registerLanguage({
  id: "yaml",
  load: () => yaml(),
});

/** Language ids whose files are YAML underneath (they stack their own features on top). */
export const YAML_BASED = new Set(["yaml", "docker-compose", "symfony-services"]);

/**
 * A plain block-mapping key with braces, such as OpenAPI's `/users/{id}:`. It is valid YAML
 * (`{` only starts a flow mapping at the start of a scalar), but the Lezer grammar marks it.
 */
const PLAIN_BRACE_KEY = /^\s*(?:-\s+)?[^\s#'"{[][^#]*\{[^}\n]*\}[^#]*:(?:\s|$)/;

registerEditorExtension(({ path }) =>
  YAML_BASED.has(languageIdFor(path) ?? "")
    ? syntaxDiagnostics(path, { source: "YAML", serverIds: ["yaml-language-server", "docker-compose-langserver"] }, (s) =>
        lezerProblems(s).filter((p) => !PLAIN_BRACE_KEY.test(s.doc.lineAt(p.from).text)),
      )
    : null,
);
