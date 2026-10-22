/**
 * What the Find scope tabs send: a directory, an explicit list of files (open or changed),
 * or a scope of the Scopes page (evaluated in Rust, `forge-scopes`).
 */
import { editor } from "@/state/editor";
import { status } from "@/features/git/store";
import type { ScopeFilter } from "@/ipc/scopes";
import { scopeFilter } from "@/features/scopes/model";
import type { FindScope } from "./state";

export interface ScopeRequest {
  directory: string;
  paths?: string[];
  scope?: ScopeFilter;
}

export interface ScopeSources {
  openFiles: () => string[];
  changedFiles: () => string[];
  filter: (name: string) => ScopeFilter | undefined;
}

const liveSources: ScopeSources = {
  openFiles: () => editor.tabs.map((t) => t.path),
  changedFiles: () => (status()?.entries ?? []).map((e) => e.path),
  filter: (name) => scopeFilter(name),
};

export function scopeRequest(
  o: { scope: FindScope; directory: string; scopeName?: string },
  sources: ScopeSources = liveSources,
): ScopeRequest {
  switch (o.scope) {
    case "directory":
      return { directory: o.directory, paths: undefined };
    case "openFiles":
      return { directory: "", paths: sources.openFiles() };
    case "changedFiles":
      return { directory: "", paths: [...new Set(sources.changedFiles())] };
    case "scope":
      return { directory: "", paths: undefined, scope: sources.filter(o.scopeName ?? "Project Files") };
    default:
      return { directory: "", paths: undefined };
  }
}
