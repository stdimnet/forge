/**
 * Per-project state Forge keeps outside the project (`forge_core::project_state`,
 * `app/src-tauri/src/commands/project_state.rs`): the workbench layout (Forge's state dir)
 * and the AI chat history (user data dir). Both are keyed by the open project's root.
 */
import { invoke } from "./index";

export type LayoutAnchor = "left" | "right" | "bottom";

export interface ProjectLayout {
  /** Visible tool window per anchor; `null` = that side is hidden. */
  active: Partial<Record<LayoutAnchor, string | null>>;
  /** Size of each anchor's area in pixels. */
  sizes: Partial<Record<LayoutAnchor, number>>;
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
  listChats: () => invoke<ChatSummary[]>("agent_history_list"),
  loadChat: (id: string) => invoke<ChatRecord>("agent_history_load", { id }),
  /** Save a chat; an empty conversation removes it. */
  saveChat: (record: ChatRecord) => invoke<ChatSummary | null>("agent_history_save", { record }),
  deleteChat: (id: string) => invoke<void>("agent_history_delete", { id }),
};
