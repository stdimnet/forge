/** What the Find scope tabs send: a directory, or an explicit list of files (open or changed). */
import { editor } from "@/state/editor";
import { status } from "@/features/git/store";
import type { FindScope } from "./state";

export interface ScopeRequest {
  directory: string;
  paths?: string[];
}

export interface ScopeSources {
  openFiles: () => string[];
  changedFiles: () => string[];
}

const liveSources: ScopeSources = {
  openFiles: () => editor.tabs.map((t) => t.path),
  changedFiles: () => (status()?.entries ?? []).map((e) => e.path),
};

export function scopeRequest(
  o: { scope: FindScope; directory: string },
  sources: ScopeSources = liveSources,
): ScopeRequest {
  switch (o.scope) {
    case "directory":
      return { directory: o.directory, paths: undefined };
    case "openFiles":
      return { directory: "", paths: sources.openFiles() };
    case "changedFiles":
      return { directory: "", paths: [...new Set(sources.changedFiles())] };
    default:
      return { directory: "", paths: undefined };
  }
}
