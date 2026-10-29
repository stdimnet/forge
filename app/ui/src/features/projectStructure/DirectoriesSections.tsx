/**
 * Settings | Directories: Resource Folders, Test Resource Folders, Excluded Folders and the
 * "Exclude files" patterns, like PHPStorm. Edits go to the dialog's draft of the project
 * settings `projectStructure.folders` / `projectStructure.excludePatterns`; the lists are
 * computed in Rust (`forge_scopes::structure::describe`).
 */
import { For, Show } from "solid-js";
import { X } from "lucide-solid";
import { draftValue, setDraft } from "@/features/settings/dialogState";
import type { SettingValue } from "@/ipc/settings";
import type { FolderKind, FolderMark, StructureConfig } from "@/ipc/projectStructure";
import { describeFolders, FOLDERS_KEY, markedFolders, PATTERNS_KEY } from "./model";

export type StructureKind = "resource" | "testResource" | "excluded";

const draftConfig = (): StructureConfig => ({
  folders: (draftValue<SettingValue>(FOLDERS_KEY) ?? []) as unknown as FolderMark[],
  excludePatterns: String(draftValue<string>(PATTERNS_KEY) ?? ""),
});

const setFolders = (folders: FolderMark[]) => setDraft(FOLDERS_KEY, folders as unknown as SettingValue);

/** Add a folder of a kind from the page's "Add Folder" row. */
export function addStructureFolder(path: string, kind: StructureKind): void {
  setFolders(markedFolders(path, kind, draftConfig()));
}

const SECTIONS: { kind: FolderKind; title: string; remove: string }[] = [
  { kind: "resource", title: "Resource Folders", remove: "Unmark as Resource Root" },
  { kind: "testResource", title: "Test Resource Folders", remove: "Unmark as Test Resources Root" },
  { kind: "excluded", title: "Excluded Folders", remove: "Cancel Exclusion" },
  { kind: "included", title: "Cancelled Default Exclusions", remove: "Exclude Again" },
];

export function ProjectStructureSections() {
  const rows = () => describeFolders(draftConfig());
  const of = (k: FolderKind) => rows().filter((r) => r.kind === k);
  const remove = (path: string) => setFolders(draftConfig().folders.filter((f) => f.path !== path));
  return (
    <>
      <For each={SECTIONS}>
        {(section) => (
          <Show when={section.kind !== "included" || of("included").length > 0}>
            <section>
              <h3 class={`php-dirs-title structure-dirs-${section.kind}`}>{section.title}</h3>
              <div class="php-list php-dirs-list" data-kind={section.kind}>
                <For each={of(section.kind)} fallback={<div class="php-hint">Nothing to show</div>}>
                  {(row) => (
                    <div class="php-list-row php-dirs-row" data-path={row.path}>
                      <span class="php-dirs-path">{row.path}</span>
                      <span class="php-dirs-actions">
                        <button class="icon-btn" title={section.remove} aria-label={section.remove} onClick={() => remove(row.path)}>
                          <X size={13} />
                        </button>
                      </span>
                    </div>
                  )}
                </For>
              </div>
            </section>
          </Show>
        )}
      </For>
      <section>
        <h3>Exclude Files</h3>
        <div class="structure-exclude-files">
          <input
            type="text"
            placeholder="*.min.js;tmp"
            spellcheck={false}
            value={draftConfig().excludePatterns}
            onChange={(e) => setDraft(PATTERNS_KEY, e.currentTarget.value)}
          />
        </div>
        <p class="php-hint">
          Files and folders whose name matches one of these patterns are excluded. Separate patterns with a semicolon; use * and ?
          as wildcards. node_modules is excluded by default; vendor stays in the project as a library.
        </p>
      </section>
    </>
  );
}
