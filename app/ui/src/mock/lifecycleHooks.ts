/**
 * What the mocked Rust lifecycle subscribers do (`lifecycleMock.ts` runs them): objections for
 * `project_before_close` and work before `closing` is sent. A module without side effects, so
 * other mocks (the fake terminals) can register early without wrapping `workspace_open`.
 */
import type { LifecycleProject, Objection } from "@/ipc/projectLifecycle";

export const objectionSources: (() => Objection[])[] = [];
export const closingHooks: ((project: LifecycleProject) => void)[] = [];

/** Objections of a mocked Rust subscriber (the fake terminals' running commands). */
export function registerMockObjections(source: () => Objection[]): void {
  objectionSources.push(source);
}

/** Runs before the closing event, like a Rust subscriber's `closing` (the fake shells end). */
export function onMockProjectClosing(hook: (project: LifecycleProject) => void): void {
  closingHooks.push(hook);
}

let openRoot: string | null = null;

/** The root of the project the mocked lifecycle has open (the fake shells start there). */
export const mockOpenRoot = (): string | null => openRoot;

export function setMockOpenRoot(root: string | null): void {
  openRoot = root;
}
