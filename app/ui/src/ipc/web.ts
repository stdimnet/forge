/**
 * WebForge IPC (`app/src-tauri/src/commands/web.rs`, logic in `packs/webforge/*`): npm
 * packages and scripts, package manager detection, and component navigation and line
 * markers of the React, Vue and Angular plugins. Offsets are UTF-16, lines 0-based.
 */
import { invoke } from "./index";

export type PackageManager = "npm" | "pnpm" | "yarn" | "bun";
export type WebFramework = "react" | "vue" | "angular";

export interface NpmScript {
  name: string;
  command: string;
  line: number;
  column: number;
}

export type PackageManagerReason =
  | { kind: "setting" }
  | { kind: "field"; path: string; value: string }
  | { kind: "lockFile"; path: string }
  | { kind: "default" };

export interface NpmPackage {
  /** Workspace-relative path of the package.json. */
  path: string;
  dir: string;
  name?: string;
  version?: string;
  packageManager: { manager: PackageManager; version?: string; reason: PackageManagerReason };
  scripts: NpmScript[];
}

export interface WebLocation {
  path: string;
  line: number;
  column: number;
}

export interface WebMember {
  name: string;
  alias?: string;
  location: WebLocation;
  typeText?: string;
  required: boolean;
}

export type WebComponentKind = "function" | "class" | "singleFile" | "script" | "component" | "directive" | "pipe";

export interface WebComponent {
  name: string;
  framework: WebFramework;
  kind: WebComponentKind;
  selector?: string;
  location: WebLocation;
  props: WebMember[];
  events: WebMember[];
  template?: WebLocation;
  styles: WebLocation[];
  defaultExport: boolean;
  usages: number;
}

export type WebGutterKind = "template" | "styles" | "component" | "usages" | "run";

export interface WebGutterMark {
  line: number;
  kind: WebGutterKind;
  title: string;
  targets: WebLocation[];
}

export const npmApi = {
  /** Every package.json with its scripts; `refresh` reads the project again. */
  packages: (refresh = false) => invoke<NpmPackage[]>("npm_packages", { refresh }),
  /** Run icons next to the scripts of a package.json. */
  gutter: (path: string, content: string) => invoke<WebGutterMark[]>("npm_gutter", { path, content }),
};

export const webApi = {
  components: (framework: WebFramework) => invoke<WebComponent[]>("web_components", { framework }),
  goto: (framework: WebFramework, path: string, content: string, offset: number) =>
    invoke<WebLocation[]>("web_goto", { framework, path, content, offset }),
  gutter: (framework: WebFramework, path: string, content: string) =>
    invoke<WebGutterMark[]>("web_gutter", { framework, path, content }),
};
