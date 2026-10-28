/**
 * Manage User Secrets, like Rider's: a project's `secrets.json` lives in the user profile
 * (`~/.microsoft/usersecrets/<UserSecretsId>/`, `%APPDATA%\Microsoft\UserSecrets\…` on
 * Windows), never in the project, so it is never committed. The action opens it in an
 * editor tab (`usersecrets://<id>/secrets.json`, read and written by Rust); a project
 * without a `UserSecretsId` gets one first, like `dotnet user-secrets init`.
 */
import { KeyRound } from "lucide-solid";
import { dotnetWebApi } from "@/ipc/dotnetWeb";
import { askConfirm } from "@/features/contextMenu/dialogs";
import { setChooserDialog } from "@/features/git/overlays";
import { activeTab, openFile, registerFileSystem } from "@/state/editor";
import { dotnetWorkspace, errorText, refreshSolution, runnableProjects, showDotnetBalloon } from "../state";

const PREFIX = "usersecrets://";
const idOf = (path: string) => path.slice(PREFIX.length).split("/")[0] ?? "";

registerFileSystem({
  prefix: PREFIX,
  read: (path) => dotnetWebApi.readUserSecrets(idOf(path)),
  write: (path, content) => dotnetWebApi.writeUserSecrets(idOf(path), content),
  icon: (p) => <KeyRound size={p.size ?? 14} />,
  breadcrumbs: (path) => ["User Secrets", idOf(path), "secrets.json"],
});

/** Open (and if needed create) the user secrets of a project. */
export async function manageUserSecrets(project: string): Promise<void> {
  const name = dotnetWorkspace()?.projects.find((p) => p.path === project)?.name ?? project;
  try {
    let s = await dotnetWebApi.userSecrets(project);
    if (!s.id) {
      const ok = await askConfirm({
        title: "Manage User Secrets",
        message: `${name} has no UserSecretsId. Add one to ${project.split("/").pop()} and create its secrets.json in your user profile (outside the project)?`,
        confirm: "Initialize",
      });
      if (!ok) return;
      s = await dotnetWebApi.initUserSecrets(project);
      await refreshSolution(true);
    }
    await openFile(`${PREFIX}${s.id}/secrets.json`);
    showDotnetBalloon(`User secrets of ${name}`, s.path ?? "", 5000);
  } catch (e) {
    showDotnetBalloon("Cannot open the user secrets", errorText(e), 8000);
  }
}

/** The action: the project of the current file, the only application, or a chooser. */
export function manageUserSecretsAction(): void {
  const projects = (dotnetWorkspace()?.projects ?? []).filter((p) => !p.isTestProject);
  const file = activeTab()?.path;
  const ofFile = file && projects.filter((p) => file.startsWith(`${p.path.slice(0, p.path.lastIndexOf("/") + 1)}`)).sort((a, b) => b.path.length - a.path.length)[0];
  if (ofFile) return void manageUserSecrets(ofFile.path);
  const apps = runnableProjects();
  if (apps.length === 1) return void manageUserSecrets(apps[0]!.path);
  setChooserDialog({
    title: "Manage User Secrets",
    items: projects.map((p) => ({ id: p.path, label: p.name, detail: p.properties.UserSecretsId ? `UserSecretsId ${p.properties.UserSecretsId}` : "No UserSecretsId yet" })),
    empty: "No projects",
    okLabel: "Open",
    onOk: (item) => void manageUserSecrets(item.id),
  });
}
