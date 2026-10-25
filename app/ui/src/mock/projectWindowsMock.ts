/**
 * Browser-mode project windows. The page is the only project window; Open Project › New
 * Window opens the demo project in a new browser tab (`?project=…` picks a demo), which runs
 * its own in-memory backend like a second Forge instance.
 */
import { registerMock } from "@/ipc";
import type { ProjectWindowInfo } from "@/ipc/projectWindows";
import { mockProjectName } from "./workspaceMock";

/** Demo projects by the name the recent projects show. */
const DEMOS: Record<string, string> = {
  "symfony-api": "symfony",
  "storefront-ui": "react",
  "pixel-dungeon": "lua",
};

/** URLs opened with New Window (for tests). */
export const openedProjectWindows: string[] = [];

registerMock("project_windows_list", (): ProjectWindowInfo[] => [
  { id: "browser", name: mockProjectName, root: `~/Projects/${mockProjectName}`, current: true },
]);
registerMock("project_window_focus", () => undefined);
registerMock("project_window_cycle", () => null);
registerMock("project_window_open", ({ path }) => {
  const name = String(path).split("/").pop() ?? "";
  const demo = DEMOS[name];
  const url = `${location.pathname}${demo ? `?project=${demo}` : ""}`;
  openedProjectWindows.push(url);
  if (typeof window !== "undefined" && typeof window.open === "function") window.open(url, "_blank");
});
