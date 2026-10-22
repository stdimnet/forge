/**
 * Analyze External Stack Trace (Rust: `forge_output::stacktrace`): frames of a pasted trace
 * linked to project files, with server and container paths mapped into the project.
 */
import { invoke } from "./index";
import type { ConsoleLine } from "./run";

export type TraceKind = "php" | "javaScript" | "lua" | "unknown";

export interface TraceAnalysis {
  lines: ConsoleLine[];
  kind: TraceKind;
  /** File references found. */
  frames: number;
  /** Of those, the ones that lead to a project file. */
  resolved: number;
}

export const stacktraceApi = {
  analyze: (text: string) => invoke<TraceAnalysis>("stacktrace_analyze", { text }),
};
