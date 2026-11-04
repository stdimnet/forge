import { Folder } from "lucide-solid";
import { registerCommand } from "@/state/commands";
import { closeWorkspace, pickAndOpenWorkspace } from "@/state/workspace";
import { registerToolWindow } from "@/features/registry";
import { affectedDirs, onFsChanged, startReloadingChangedTabs } from "@/state/fsWatch";
import { ProjectActions, ProjectView, refreshDirs } from "./ProjectView";
import { newEntryFromSelection } from "./fileMenu";
import { t } from "@/i18n";

// Files changed outside the editor: refresh the tree and reload unmodified open tabs.
onFsChanged((c) => refreshDirs(affectedDirs(c)));
startReloadingChangedTabs();

registerToolWindow({
  id: "project",
  title: t("toolwindow.project.title"),
  icon: Folder,
  anchor: "left",
  order: 0,
  shortcut: "Alt+1",
  component: ProjectView,
  actions: ProjectActions,
});

registerCommand({ id: "workspace.open", title: "Open…", category: "File", run: pickAndOpenWorkspace });
registerCommand({ id: "workspace.close", title: "Close Project", category: "File", run: closeWorkspace });
registerCommand({ id: "NewFile", title: "File", category: "File", run: () => newEntryFromSelection("file") });
registerCommand({ id: "NewDir", title: "Directory", category: "File", run: () => newEntryFromSelection("dir") });
