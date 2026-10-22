/**
 * Hints for language servers that a file would benefit from but that are not installed. The
 * editor shows a banner with an Install button for such files (see `serverHint.ts`).
 *
 * `$schema` in JSON files and `# yaml-language-server: $schema=` modelines need no server:
 * Forge's own JSON Schema support uses them (`features/jsonSchema`).
 */
import type { ServerRow } from "@/ipc/langservers";

export interface ServerHint {
  /** The server that provides the feature (catalog id). */
  serverId: string;
  /** Editor language ids (file types) the hint applies to. */
  languageIds: readonly string[];
  /** Whether the start of the file uses the feature the server provides. */
  applies: (head: string) => boolean;
  title: string;
  detail: string;
}

/** Only the start of a file is inspected: `$schema` and modelines come first. */
export const HINT_HEAD_CHARS = 8192;

/** Hints of the servers Forge suggests; add an entry for a feature only a server provides. */
export const SERVER_HINTS: readonly ServerHint[] = [];

export interface HintInput {
  /** Editor language id of the file, if it has one. */
  languageId: string | undefined;
  /** The start of the file's text. */
  head: string;
  rows: readonly ServerRow[];
  /** Server ids whose hint the user turned off for good. */
  dismissed: readonly string[];
  /** The "suggest installing" setting. */
  enabled: boolean;
}

export interface ActiveHint {
  hint: ServerHint;
  row: ServerRow;
}

/** The hint to show for a file, or null when there is nothing to suggest. */
export function activeHint(
  { languageId, head, rows, dismissed, enabled }: HintInput,
  hints: readonly ServerHint[] = SERVER_HINTS,
): ActiveHint | null {
  if (!enabled || !languageId) return null;
  for (const hint of hints) {
    if (!hint.languageIds.includes(languageId) || dismissed.includes(hint.serverId) || !hint.applies(head)) continue;
    const row = rows.find((r) => r.id === hint.serverId);
    // Only a server that exists, is wanted and can be installed is worth a banner.
    if (row && row.status === "notInstalled" && row.enabled && row.install.length > 0) return { hint, row };
  }
  return null;
}
