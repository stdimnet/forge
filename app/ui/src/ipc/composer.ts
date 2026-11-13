/**
 * Composer (Rust: `packs/phpforge/composer`, `commands/composer.rs`): outdated markers,
 * completion and hover in composer.json, Packagist search and versions for the Manage
 * Composer Dependencies dialog and the composer.phar download (`composer audit` runs with
 * the vulnerable dependency check, `ipc/vulnerabilities.ts`). Offsets are UTF-16
 * (CodeMirror positions).
 */
import { invoke } from "./index";
import type { SchemaCompletion, SchemaHover } from "./jsonSchema";

export interface Requirement {
  name: string;
  constraint: string;
  dev: boolean;
}

export interface InstalledPackage {
  name: string;
  version: string;
  description?: string;
  homepage?: string;
  source?: string;
  dev: boolean;
  abandoned?: string;
}

export interface ComposerProject {
  /** Path of composer.json (Settings | PHP | Composer). */
  composerJson: string;
  exists: boolean;
  hasLock: boolean;
  requirements: Requirement[];
  installed: InstalledPackage[];
}

/** 0-based line and UTF-16 column. */
export interface Position {
  line: number;
  character: number;
}

export interface OutdatedMarker {
  name: string;
  constraint: string;
  installed: string;
  latest: string;
  /** `semver-safe-update` (the constraint allows it) or `update-possible`. */
  status: "semver-safe-update" | "update-possible" | string;
  dev: boolean;
  /** The constraint that allows `latest`. */
  suggested: string;
  /** UTF-16 range of the constraint string and its 0-based line. */
  from: number;
  to: number;
  line: number;
  start: Position;
  end: Position;
  /** The outdated package inspection's message. */
  message: string;
}

export interface SearchHit {
  name: string;
  description: string;
  downloads: number;
  favers: number;
  repository?: string;
  abandoned: boolean;
}

export interface PackageDetails {
  name: string;
  description: string;
  homepage?: string;
  repository?: string;
  abandoned?: string;
  /** Newest first. */
  versions: string[];
  /** What `composer require` writes for the latest version (`^12.3`). */
  suggested?: string | null;
  url: string;
}

export const composerApi = {
  project: () => invoke<ComposerProject>("composer_project"),
  markers: (path: string, text: string) => invoke<OutdatedMarker[]>("composer_markers", { path, text }),
  complete: (path: string, text: string, offset: number) =>
    invoke<SchemaCompletion | null>("composer_complete", { path, text, offset }),
  hover: (path: string, text: string, offset: number) => invoke<SchemaHover | null>("composer_hover", { path, text, offset }),
  search: (query: string) => invoke<SearchHit[]>("composer_search", { query }),
  package: (name: string) => invoke<PackageDetails>("composer_package", { name }),
  downloadPhar: () => invoke<string>("composer_download_phar"),
};
