/**
 * The project lifecycle in the UI: `onProject(phase, handler)` lets every feature with
 * per-project state follow the window's project, in the same four phases as Rust
 * (`forge_core::lifecycle`):
 *
 * | Phase | Handler gets | Purpose |
 * | --- | --- | --- |
 * | `beforeClose` | the old project, the Rust objections | return `false` to keep the project (one "Cancel" stops the switch) |
 * | `closing` | the old project | stop and save; `workspace()` is still the old project |
 * | `opened` | the new project | load and start; may return a cleanup that runs at its `closing` |
 * | `ready` | the new project | after indexing |
 *
 * Handlers run in a fixed order (`order`, then registration) with error isolation: a handler
 * that throws or rejects is logged and the others still run. Handlers that return a promise
 * are awaited before the next one runs; when every handler is synchronous, a phase completes
 * synchronously. `state/workspace.ts` drives the phases from the window's project changes and
 * the Rust event stream.
 */
import type { LifecycleProject, Objection, ProjectPhase } from "@/ipc/projectLifecycle";

export type { LifecycleProject, Objection, ProjectPhase } from "@/ipc/projectLifecycle";

/** What a `beforeClose` handler sees besides the project. */
export interface BeforeCloseContext {
  /** Every objection of the Rust subscribers. */
  readonly objections: readonly Objection[];
  /** Take the objections of `kind`: this handler asks about them, the generic question does not. */
  claim(kind: string): Objection[];
}

type MaybePromise<T> = T | Promise<T>;
type Cleanup = () => void;

export interface ProjectHandlers {
  beforeClose: (project: LifecycleProject, ctx: BeforeCloseContext) => MaybePromise<boolean | void>;
  closing: (project: LifecycleProject) => MaybePromise<void>;
  opened: (project: LifecycleProject) => MaybePromise<void | Cleanup>;
  ready: (project: LifecycleProject) => MaybePromise<void>;
}

export interface ProjectHandlerOptions {
  /** Lower runs first (default 0); equal orders run in registration order. */
  order?: number;
  /** Shown when the handler fails. */
  name?: string;
}

interface Entry {
  id: number;
  phase: ProjectPhase;
  order: number;
  name: string;
  handler: (...args: never[]) => unknown;
}

let entries: Entry[] = [];
let nextId = 0;
/** Cleanups returned by `opened` handlers, run when that project closes. */
let cleanups: { name: string; run: Cleanup }[] = [];

/**
 * Follow the window's project in `phase`. Returns a function that unsubscribes the handler.
 *
 * ```ts
 * onProject("closing", () => setResults([]));
 * onProject("opened", (p) => { const timer = setInterval(poll, 5000); return () => clearInterval(timer); });
 * onProject("beforeClose", async (p, ctx) => ctx.claim("terminal.running").length === 0 || confirm());
 * ```
 */
export function onProject<P extends ProjectPhase>(phase: P, handler: ProjectHandlers[P], options: ProjectHandlerOptions = {}): () => void {
  const entry: Entry = {
    id: ++nextId,
    phase,
    order: options.order ?? 0,
    name: options.name ?? handler.name ?? "",
    handler: handler as Entry["handler"],
  };
  // Stable: after every handler with the same or a lower order.
  const at = entries.findIndex((e) => e.order > entry.order);
  entries = at < 0 ? [...entries, entry] : [...entries.slice(0, at), entry, ...entries.slice(at)];
  return () => {
    entries = entries.filter((e) => e.id !== entry.id);
  };
}

const handlersOf = (phase: ProjectPhase) => entries.filter((e) => e.phase === phase);

const isPromise = (v: unknown): v is Promise<unknown> => !!v && typeof (v as Promise<unknown>).then === "function";

function failed(phase: ProjectPhase, name: string, error: unknown): void {
  console.warn(`project ${phase} handler${name ? ` "${name}"` : ""} failed`, error);
}

/**
 * Call `steps` one after another, isolated from each other; `done` gets each step's result.
 * Synchronous as long as no step returns a promise.
 */
function runInOrder(steps: { name: string; call: () => unknown }[], phase: ProjectPhase, done: (value: unknown, name: string) => void, from = 0): void | Promise<void> {
  for (let i = from; i < steps.length; i++) {
    const step = steps[i];
    let result: unknown;
    try {
      result = step.call();
    } catch (e) {
      failed(phase, step.name, e);
      continue;
    }
    if (isPromise(result)) {
      return result.then(
        (value) => {
          done(value, step.name);
          return runInOrder(steps, phase, done, i + 1);
        },
        (e) => {
          failed(phase, step.name, e);
          return runInOrder(steps, phase, done, i + 1);
        },
      );
    }
    done(result, step.name);
  }
}

/**
 * Run the `closing`, `opened` or `ready` handlers for `project`. `closing` first runs the
 * cleanups that `opened` handlers returned, last one first.
 */
export function runProjectPhase(phase: Exclude<ProjectPhase, "beforeClose">, project: LifecycleProject): void | Promise<void> {
  const steps: { name: string; call: () => unknown }[] = [];
  if (phase === "closing") {
    const pending = cleanups.reverse();
    cleanups = [];
    steps.push(...pending.map((c) => ({ name: c.name, call: c.run })));
  }
  for (const e of handlersOf(phase)) steps.push({ name: e.name, call: () => (e.handler as (p: LifecycleProject) => unknown)(project) });
  return runInOrder(steps, phase, (value, name) => {
    if (phase === "opened" && typeof value === "function") cleanups.push({ name, run: value as Cleanup });
  });
}

/** Asks about objections no handler claimed (a generic "Close the project anyway?"). */
export type ObjectionPrompt = (project: LifecycleProject, objections: Objection[]) => Promise<boolean>;

let objectionPrompt: ObjectionPrompt = async () => true;

/** The dialog for unclaimed objections (registered by the project feature). */
export function setObjectionPrompt(prompt: ObjectionPrompt): void {
  objectionPrompt = prompt;
}

/**
 * `beforeClose`: may `project` close? Runs the handlers in order until one returns `false`
 * (the user chose Cancel); a handler that throws does not veto. Objections of the Rust
 * subscribers that no handler claimed are asked about last.
 */
export async function confirmProjectClose(project: LifecycleProject, objections: readonly Objection[] = []): Promise<boolean> {
  let open = [...objections];
  const ctx: BeforeCloseContext = {
    objections,
    claim: (kind) => {
      const mine = open.filter((o) => o.kind === kind);
      open = open.filter((o) => o.kind !== kind);
      return mine;
    },
  };
  for (const e of handlersOf("beforeClose")) {
    try {
      const answer = await (e.handler as ProjectHandlers["beforeClose"])(project, ctx);
      if (answer === false) return false;
    } catch (err) {
      failed("beforeClose", e.name, err);
    }
  }
  if (!open.length) return true;
  try {
    return await objectionPrompt(project, open);
  } catch (err) {
    failed("beforeClose", "objections", err);
    return true;
  }
}
