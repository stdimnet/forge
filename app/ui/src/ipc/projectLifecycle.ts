/**
 * The project lifecycle of this window (Rust: `forge_core::lifecycle`, `src-tauri/src/lifecycle.rs`).
 * Rust publishes `closing`, `opened` and `ready` on `project://lifecycle`; `beforeClose` is a
 * question (`project_before_close`) whose objections the UI shows before it opens another
 * project.
 */
import { invoke, listen } from "./index";
import type { WorkspaceInfo } from "./workspace";

export type ProjectPhase = "beforeClose" | "closing" | "opened" | "ready";

/** The project a phase is about: the workspace info plus what the lifecycle knows of it. */
export interface LifecycleProject extends WorkspaceInfo {
  /** The repository root containing the project (local projects with git). */
  vcsRoot?: string;
  /** Trusted (not in Safe Mode) when the phase ran. */
  trusted?: boolean;
}

/** Payload of `project://lifecycle`. */
export interface LifecycleEvent {
  phase: ProjectPhase;
  /** Counts the projects opened in this window. */
  generation: number;
  project: LifecycleProject;
}

/** Why a Rust subscriber wants the user asked before the project closes. */
export interface Objection {
  source: string;
  /** e.g. `terminal.running`; UI handlers claim the kinds they render themselves. */
  kind: string;
  message: string;
  items: string[];
}

export const LIFECYCLE_EVENT = "project://lifecycle";

export const projectLifecycleApi = {
  /** The Rust subscribers' objections to closing the open project (closes nothing). */
  beforeClose: () => invoke<Objection[]>("project_before_close"),
  /** File | Close Project in Rust: `closing`, then no project. */
  close: () => invoke<void>("workspace_close"),
  onEvent: (cb: (e: LifecycleEvent) => void) => listen<LifecycleEvent>(LIFECYCLE_EVENT, cb),
};
