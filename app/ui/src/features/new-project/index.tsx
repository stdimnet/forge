/**
 * New Project wizard (File | New Project, Welcome screen "New Project"): generators for PHP,
 * Web, Lua and empty projects that run the official CLIs or write offline templates.
 */
import { registerCommand } from "@/state/commands";
import { showBalloon } from "@/keymap";
import { registerDialog } from "@/shell/window/dialogs";
import { openProject } from "@/state/workspace";
import { NewProjectDialog } from "./NewProjectDialog";
import { NEW_PROJECT_DIALOG, openNewProject, prepareNewProject, type NewProjectProps, type NewProjectResult } from "./store";
import "./newProject.css";

export { openNewProject };

registerCommand({ id: "NewProject", title: "New Project…", category: "File", run: () => void openNewProject() });

registerDialog<NewProjectProps, NewProjectResult>({
  id: NEW_PROJECT_DIALOG,
  title: "New Project",
  width: 880,
  height: 640,
  minWidth: 680,
  minHeight: 460,
  component: NewProjectDialog,
  prepare: (props) => prepareNewProject(props?.generator),
  // The project was created: open it (in this window or a new one, as the user prefers).
  onClosed: (result) => {
    if (result?.root) {
      openProject(result.root).catch((e) => showBalloon("Cannot open the new project", String(e)));
    }
  },
});
