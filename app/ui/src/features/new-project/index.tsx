/**
 * New Project wizard (File | New Project, Welcome screen "New Project"): generators for PHP,
 * Web, Lua and empty projects that run the official CLIs or write offline templates.
 */
import { render } from "solid-js/web";
import { registerCommand } from "@/state/commands";
import { NewProjectDialog } from "./NewProjectDialog";
import { openNewProject } from "./store";
import "./newProject.css";

export { openNewProject };

registerCommand({ id: "NewProject", title: "New Project…", category: "File", run: () => openNewProject() });

if (typeof document !== "undefined") {
  const host = document.createElement("div");
  host.id = "new-project-root";
  document.body.appendChild(host);
  render(() => <NewProjectDialog />, host);
}
