/**
 * Draft edits of Settings | Editor | File Types: the `fileTypes.associations` entries (patterns
 * added to or removed from a type) and the `fileTypes.custom` definitions. Rust resolves what
 * the entries mean (`forge_filetypes::user`); these helpers only keep the lists tidy.
 */
import type { SettingObject } from "@/ipc/settings";
import type { FileTypeRow } from "@/ipc/filetypes";

const same = (a: string, b: string) => a.toLowerCase() === b.toLowerCase();

/** Add `pattern` to `row`: it is taken away from any other type (PHPStorm's "reassign"). */
export function addPattern(entries: SettingObject[], row: FileTypeRow, pattern: string): SettingObject[] {
  const rest = entries.filter((e) => !same(e.pattern ?? "", pattern));
  // A bundled pattern that was removed comes back by dropping the removal.
  if (row.removed.some((p) => same(p, pattern))) return rest;
  return [...rest, { fileType: row.id, pattern }];
}

/** Remove `pattern` from `row`. */
export function removePattern(entries: SettingObject[], row: FileTypeRow, pattern: string): SettingObject[] {
  const added = row.added.some((p) => same(p, pattern));
  const rest = entries.filter((e) => !(e.fileType === row.id && same(e.pattern ?? "", pattern)));
  return added ? rest : [...rest, { fileType: row.id, pattern, action: "remove" }];
}

/** The type that has `pattern` now, other than `except`. */
export function ownerOf(rows: FileTypeRow[], pattern: string, except?: string): FileTypeRow | undefined {
  return rows.find((r) => r.id !== except && r.patterns.some((p) => same(p, pattern)));
}

/** A new id for a custom file type; it stays when the type is renamed. */
export function newCustomId(existing: string[]): string {
  let n = existing.length + 1;
  while (existing.includes(`custom-${n}`)) n++;
  return `custom-${n}`;
}

/** Drop a custom type and its patterns. */
export function removeCustomType(
  custom: SettingObject[],
  entries: SettingObject[],
  id: string,
): { custom: SettingObject[]; entries: SettingObject[] } {
  return {
    custom: custom.filter((c) => c.id !== id),
    entries: entries.filter((e) => e.fileType !== id),
  };
}

/** The `;`-separated ignored names as a list, and back. */
export const splitIgnored = (value: string): string[] =>
  value
    .split(";")
    .map((s) => s.trim())
    .filter(Boolean);
export const joinIgnored = (items: string[]): string => items.join(";");
