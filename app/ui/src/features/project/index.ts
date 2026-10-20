import { Folder } from "lucide-solid";
import { registerCommand } from "@/state/commands";
import { closeWorkspace, pickAndOpenWorkspace } from "@/state/workspace";
import { registerToolWindow } from "@/features/registry";
import { ProjectView } from "./ProjectView";

registerToolWindow({
  id: "project",
  title: "Project",
  icon: Folder,
  anchor: "left",
  order: 0,
  shortcut: "Alt+1",
  component: ProjectView,
});

registerCommand({ id: "workspace.open", title: "Open…", category: "File", run: pickAndOpenWorkspace });
registerCommand({ id: "workspace.close", title: "Close Project", category: "File", run: closeWorkspace });
