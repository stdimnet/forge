import { Folder } from "lucide-solid";
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
