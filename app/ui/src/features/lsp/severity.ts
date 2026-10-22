import { CircleAlert, Info, SpellCheck, TriangleAlert, WholeWord } from "lucide-solid";
import type { Severity } from "@/ipc/lsp";

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

export const pluralize = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;
