/**
 * JavaForge Minecraft plugin IPC (app/src-tauri/src/commands/minecraft.rs; crate
 * packs/javaforge/minecraft): plugin descriptor checks and navigation, and the gutter markers
 * of event handlers and the main class. Browser mode answers from `mock/minecraftMock.ts`
 * (the demo plugin, `?project=minecraft`). "Refresh versions" of the New Project wizard is
 * `minecraft_versions_refresh` (called through the generator option's `refresh`).
 */
import { invoke } from "./index";

export interface DescriptorProblem {
  /** UTF-16 offsets of the `main` value. */
  from: number;
  to: number;
  severity: "error" | "warning";
  message: string;
  code: string;
}

export interface MinecraftTarget {
  path: string;
  /** 0-based. */
  line: number;
  name: string;
}

export interface MinecraftMarker {
  /** 0-based line of the declaration. */
  line: number;
  kind: "listener" | "plugin";
  title: string;
  /** The event class (listeners) or the class name (plugins). */
  name: string;
}

export const minecraftApi = {
  inspect: (path: string, content: string) => invoke<DescriptorProblem[]>("minecraft_inspect", { path, content }),
  goto: (path: string, content: string, pos: number) => invoke<MinecraftTarget[]>("minecraft_goto", { path, content, pos }),
  gutter: (path: string, content: string) => invoke<MinecraftMarker[]>("minecraft_gutter", { path, content }),
};
