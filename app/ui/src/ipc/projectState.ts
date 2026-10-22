/**
 * Per-project state Forge keeps outside the project (`forge_core::project_state`,
 * `app/src-tauri/src/commands/project_state.rs`): the workbench layout (Forge's state dir)
 * and the AI chat history (user data dir). Both are keyed by the open project's root.
 */
import { invoke } from "./index";

export type LayoutAnchor = "left" | "right" | "bottom";
/**
 * Where a tool window sits (`forge_core::project_state::SLOTS`): the primary and the split
 * group of each anchor, e.g. `leftSplit` = Left Bottom, `bottomSplit` = Bottom Right.
 */
export type LayoutSlot = "left" | "leftSplit" | "right" | "rightSplit" | "bottom" | "bottomSplit";

export interface ToolWindowPlacement {
  slot: LayoutSlot;
  /** Position inside the stripe group (lower first). */
  order: number;
  /** Removed from the sidebar: no stripe button until the window is opened another way. */
  removed: boolean;
}

export interface ProjectLayout {
  /** Visible tool window per slot; `null` = nothing open there. */
  active: Partial<Record<LayoutSlot, string | null>>;
  /** Size of each anchor's area in pixels. */
  sizes: Partial<Record<LayoutAnchor, number>>;
  /** Share of the first tool window when an area shows two (per mille). */
  splits?: Partial<Record<LayoutAnchor, number>>;
  /** Tool windows the user moved, reordered or removed from the sidebar. */
  toolWindows?: Record<string, ToolWindowPlacement>;
  /** Selected tab inside tool windows with tabs (tool window id → tab id). */
  tabs: Record<string, string>;
  /** Open editor tabs (workspace-relative paths) in order. */
  openFiles: string[];
  activeFile: string | null;
}

export interface ChatSummary {
  id: string;
  agentId: string;
  /** Milliseconds since the Unix epoch of the last change. */
  at: number;
  title: string;
  turns: number;
}

export interface ChatRecord {
  id: string;
  agentId: string;
  at: number;
  title: string;
  /** The chat panel's conversation model, stored as it is. */
  conversation: unknown;
}

export const projectStateApi = {
  loadLayout: () => invoke<ProjectLayout | null>("layout_load"),
  saveLayout: (layout: ProjectLayout) => invoke<void>("layout_save", { layout }),
  /** The layout stored with Window | Store Current Layout as Default (tool windows only). */
  loadDefaultLayout: () => invoke<ProjectLayout | null>("layout_default_load"),
  saveDefaultLayout: (layout: ProjectLayout) => invoke<void>("layout_default_save", { layout }),
  listChats: () => invoke<ChatSummary[]>("agent_history_list"),
  loadChat: (id: string) => invoke<ChatRecord>("agent_history_load", { id }),
  /** Save a chat; an empty conversation removes it. */
  saveChat: (record: ChatRecord) => invoke<ChatSummary | null>("agent_history_save", { record }),
  deleteChat: (id: string) => invoke<void>("agent_history_delete", { id }),
};
