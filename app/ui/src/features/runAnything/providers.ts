/**
 * Commands other features offer in Run Anything (Artisan, Symfony console, the Command Line
 * Tool Support): `registerRunAnythingProvider`. The popup asks each active provider once
 * when it opens; ranking happens in Rust with the other suggestions.
 */
import type { ProviderCommands } from "@/ipc/runTools";

export interface RunAnythingProvider {
  id: string;
  /** Group title, e.g. "Artisan commands". */
  title: string;
  /** Offer the commands only while this is true (e.g. the project uses Laravel). */
  when?: () => boolean;
  /** Full command lines (`php artisan migrate`) with descriptions. */
  commands: () => Promise<{ command: string; description?: string }[]>;
}

const providers = new Map<string, RunAnythingProvider>();

/** Add (or replace, by id) a provider of Run Anything commands. */
export function registerRunAnythingProvider(p: RunAnythingProvider): void {
  providers.set(p.id, p);
}

/** Commands of the active providers; a provider that fails is left out. */
export async function collectProviderCommands(): Promise<ProviderCommands[]> {
  const active = [...providers.values()].filter((p) => {
    try {
      return !p.when || p.when();
    } catch {
      return false;
    }
  });
  const lists = await Promise.all(
    active.map(async (p) => {
      try {
        return { id: p.id, title: p.title, commands: await p.commands() };
      } catch (err) {
        console.warn(`Run Anything: ${p.id} commands not available`, err);
        return null;
      }
    }),
  );
  return lists.filter((l): l is ProviderCommands => !!l && l.commands.length > 0);
}
