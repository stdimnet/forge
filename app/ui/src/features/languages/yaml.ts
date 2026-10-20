/** YAML: Lezer grammar with folding and indentation, plus syntax errors in the Problems view. */
import { yaml } from "@codemirror/lang-yaml";
import { registerLanguage, languageIdFor } from "@/editor/languages";
import { registerEditorExtension } from "@/editor/extensions";
import { lezerProblems, syntaxDiagnostics } from "./shared";

registerLanguage({
  id: "yaml",
  label: "YAML",
  extensions: ["yml", "yaml", "neon"],
  fileNames: [".clang-format", ".clang-tidy", ".yamllint", ".gemrc", "CITATION.cff", ".sops.yaml"],
  indent: "  ",
  load: () => yaml(),
});

/** Language ids whose files are YAML underneath (they stack their own features on top). */
export const YAML_BASED = new Set(["yaml", "docker-compose", "symfony-services"]);

registerEditorExtension(({ path }) =>
  YAML_BASED.has(languageIdFor(path) ?? "")
    ? syntaxDiagnostics(path, { source: "YAML", serverIds: ["yaml-language-server", "docker-compose-langserver"] }, (s) =>
        lezerProblems(s),
      )
    : null,
);
