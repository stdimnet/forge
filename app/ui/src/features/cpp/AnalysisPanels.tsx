/**
 * The Sanitizers and Valgrind tabs of the Run tool window. Sanitizer reports come from the
 * output of C/C++ runs and tests (`{ type: "sanitizer" }`), Valgrind Memcheck's errors and
 * leak summary from "Run with Valgrind Memcheck" (`{ type: "valgrind" }`); both are parsed
 * and mapped onto the project in Rust (`cforge-analyzers`).
 */
import { Droplets, ShieldAlert, TriangleAlert, Zap, CircleAlert, Brain, Bug } from "lucide-solid";
import type { SanitizerReport, ValgrindError, ValgrindReport, Leaked } from "@/ipc/cppTools";
import type { RunEvent } from "@/ipc/run";
import { IssueTree, type Issue } from "./IssueTree";
import { t } from "@/i18n";

const SANITIZER_ICONS = {
  address: { icon: ShieldAlert, color: "var(--danger)" },
  leak: { icon: Droplets, color: "var(--warning)" },
  thread: { icon: Zap, color: "var(--warning)" },
  memory: { icon: Brain, color: "var(--danger)" },
  undefined: { icon: TriangleAlert, color: "var(--warning)" },
} as const;

export function sanitizerIssues(reports: SanitizerReport[]): Issue[] {
  return reports.map((r, i) => ({
    id: `s${i}`,
    group: r.tool,
    ...SANITIZER_ICONS[r.sanitizer],
    title: r.kind,
    message: r.message,
    details: r.details,
    summary: r.summary,
    location: r.location,
    stacks: r.stacks,
  }));
}

export const acceptSanitizer = (e: RunEvent, current: unknown) =>
  e.type === "sanitizer" && e.reports.length ? [...((current as SanitizerReport[] | undefined) ?? []), ...e.reports] : undefined;

export function SanitizerPanel(props: { data: unknown }) {
  const reports = () => (props.data as SanitizerReport[]) ?? [];
  const tools = () => [...new Set(reports().map((r) => r.tool))].join(", ");
  return (
    <IssueTree
      issues={sanitizerIssues(reports())}
      empty="No sanitizer reports."
      header={
        <span>
          <b>{reports().length}</b> {t("langs.cpp.problemsReportedBy", { count: reports().length, tools: tools() })}
        </span>
      }
    />
  );
}

const bytes = (l: Leaked) => `${l.bytes.toLocaleString("en-US")} bytes in ${l.blocks} block${l.blocks === 1 ? "" : "s"}`;

function valgrindIssue(e: ValgrindError, i: number): Issue {
  const leak = e.kind.startsWith("Leak_");
  const kind = leak ? e.kind.replace(/^Leak_/, "").replace(/([a-z])([A-Z])/g, "$1 $2") : e.kind.replace(/([a-z])([A-Z])/g, "$1 $2");
  return {
    id: `v${i}`,
    group: leak ? "Memory Leaks" : "Errors",
    icon: leak ? Droplets : e.kind.startsWith("Uninit") ? CircleAlert : Bug,
    color: leak ? "var(--warning)" : "var(--danger)",
    title: leak ? kind : e.what,
    message: e.what,
    badge: leak && e.leaked ? `${e.leaked.bytes} B` : e.count > 1 ? `×${e.count}` : undefined,
    location: e.location,
    stacks: e.stacks,
  };
}

export const acceptValgrind = (e: RunEvent) => (e.type === "valgrind" && e.report ? e.report : undefined);

export function ValgrindPanel(props: { data: unknown }) {
  const report = () => props.data as ValgrindReport;
  const s = () => report().leakSummary;
  return (
    <IssueTree
      issues={report().errors.map(valgrindIssue)}
      empty={`Memcheck found no errors in ${report().command || "the program"}.`}
      header={
        <span class="native-leaks">
          <span>
            <b>{report().errorCount}</b> {report().errorCount === 1 ? "error" : "errors"}
          </span>
          <span title={t("langs.cpp.definitelyLost2")}>{t("langs.cpp.definitelyLost", { value: bytes(s().definitelyLost) })}</span>
          <span title={t("langs.cpp.indirectlyLost2")}>{t("langs.cpp.indirectlyLost", { value: bytes(s().indirectlyLost) })}</span>
          <span title={t("langs.cpp.possiblyLost2")}>{t("langs.cpp.possiblyLost", { value: bytes(s().possiblyLost) })}</span>
          <span title={t("langs.cpp.stillReachable2")}>{t("langs.cpp.stillReachable", { value: bytes(s().stillReachable) })}</span>
        </span>
      }
    />
  );
}
