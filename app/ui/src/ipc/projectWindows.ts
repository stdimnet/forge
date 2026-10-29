/**
 * Several project windows (`app/src-tauri/src/commands/project_windows.rs`,
 * `forge_window::projects`): every project window is a Forge instance with its own project
 * state; these commands list them, bring one to the front and open a project in a new one.
 */
import { invoke } from "./index";

export interface ProjectWindowInfo {
  id: string;
  /** The open project's name; `null` on the Welcome screen. */
  name: string | null;
  root: string | null;
  /** The window that asked. */
  current: boolean;
}

export const projectWindowsApi = {
  /** The open project windows in opening order. */
  list: () => invoke<ProjectWindowInfo[]>("project_windows_list"),
  focus: (id: string) => invoke<void>("project_window_focus", { id }),
  /** Next (`1`) / Previous (`-1`) Project Window; `null` when this is the only one. */
  cycle: (delta: 1 | -1) => invoke<ProjectWindowInfo | null>("project_window_cycle", { delta }),
  /** Open a local project in a new window (or focus the window that has it open). */
  open: (path: string) => invoke<void>("project_window_open", { path }),
};
