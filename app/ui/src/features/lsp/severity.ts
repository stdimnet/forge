import { CircleAlert, Info, SpellCheck, TriangleAlert, WholeWord } from "lucide-solid";
import type { Severity } from "@/ipc/lsp";
import { t } from "@/i18n";

export const severityIcon = {
  error: CircleAlert,
  warning: TriangleAlert,
  weak: TriangleAlert,
  info: Info,
  typo: SpellCheck,
  grammar: WholeWord,
} as const;

export const severityLabel: Record<Severity, string> = {
  error: "Error",
  warning: "Warning",
  weak: "Weak Warning",
  info: "Info",
  typo: "Typo",
  grammar: "Grammar Error",
};

/** "1 error", "3 weak warnings", "2 problems" (`problem`: any severity). */
export function severityCount(severity: Severity | "problem", count: number): string {
  switch (severity) {
    case "error":
      return t("tools.lsp.errorCount", { count });
    case "warning":
      return t("tools.lsp.warningCount", { count });
    case "weak":
      return t("tools.lsp.weakWarningCount", { count });
    case "info":
      return t("tools.lsp.infoCount", { count });
    case "typo":
      return t("tools.lsp.typoCount", { count });
    case "grammar":
      return t("tools.lsp.grammarErrorCount", { count });
    default:
      return t("tools.lsp.problemCount", { count });
  }
}
