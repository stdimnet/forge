import { createSignal } from "solid-js";
import { workspaceApi, type WorkspaceInfo } from "@/ipc/workspace";

const [workspace, setWorkspace] = createSignal<WorkspaceInfo | null>(null);

export { workspace };

export async function openWorkspace(path: string): Promise<void> {
  setWorkspace(await workspaceApi.open(path));
}
