import { CircleAlert, Info, TriangleAlert } from "lucide-solid";
import type { Severity } from "@/ipc/lsp";

export const severityIcon = { error: CircleAlert, warning: TriangleAlert, weak: TriangleAlert, info: Info } as const;

export const severityLabel: Record<Severity, string> = {
  error: "Error",
  warning: "Warning",
  weak: "Weak Warning",
  info: "Info",
};

export const pluralize = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;
