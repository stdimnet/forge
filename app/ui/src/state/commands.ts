/**
 * Command registry: every user action has an id (like PHPStorm action ids) so it can be
 * bound to keys, shown in Find Action and invoked from menus or plugins.
 */
import { createSignal } from "solid-js";

export interface Command {
  id: string;
  title: string;
  category?: string;
  run: () => void | Promise<void>;
}

const [commands, setCommands] = createSignal<Map<string, Command>>(new Map());

export function registerCommand(cmd: Command): void {
  setCommands((prev) => new Map(prev).set(cmd.id, cmd));
}

/** Remove a command (e.g. a deleted External Tool's action). */
export function unregisterCommand(id: string): void {
  setCommands((prev) => {
    if (!prev.has(id)) return prev;
    const next = new Map(prev);
    next.delete(id);
    return next;
  });
}

export { commands };

/** A command that is about to run, for observers like the Presentation Assistant. */
export interface CommandInvocation {
  command: Command;
  /** The PHPStorm action id it was invoked as (keymap, menus, Find Action), when known. */
  actionId?: string;
}

const listeners = new Set<(inv: CommandInvocation) => void>();

/** Observe every command run; returns a function that stops observing. */
export function onCommandInvoked(listener: (inv: CommandInvocation) => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export async function runCommand(id: string, actionId?: string): Promise<void> {
  const cmd = commands().get(id);
  if (!cmd) {
    console.warn(`command not registered: ${id}`);
    return;
  }
  for (const l of listeners) {
    try {
      l({ command: cmd, actionId });
    } catch (e) {
      console.warn("command observer failed", e);
    }
  }
  await cmd.run();
}

// Lets the screenshot scenarios and devtools run any command in browser mode.
if (import.meta.env.DEV && typeof window !== "undefined") {
  (window as unknown as { __forgeRunCommand?: (id: string) => Promise<void> }).__forgeRunCommand = runCommand;
}
