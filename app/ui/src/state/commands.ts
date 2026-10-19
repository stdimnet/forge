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

export { commands };

export async function runCommand(id: string): Promise<void> {
  const cmd = commands().get(id);
  if (!cmd) {
    console.warn(`command not registered: ${id}`);
    return;
  }
  await cmd.run();
}
