/**
 * Remote Host (PHPStorm's Tools | Deployment | Browse Remote Host): the files of the
 * project's deployment servers in a tool window, with Edit Remote File tabs
 * (`remote-host://<server>/<path>`) that upload back on save.
 */
import { Server } from "lucide-solid";
import { registerToolWindow } from "@/features/registry";
import { registerCommand } from "@/state/commands";
import { onTabClosed, registerFileSystem } from "@/state/editor";
import { isPluginActive } from "@/state/project";
import { showToolWindow } from "@/state/layout";
import { registerKeymapActions, showBalloon } from "@/keymap";
import { remoteHostApi } from "@/ipc/remoteHost";
import { chooseServer, config, defaultServer } from "@/features/deployment/store";
import { RemoteHostView } from "./RemoteHostView";
import { EDIT_PREFIX, parseEditPath, saveRemoteEdit, selectServer } from "./store";
import "./remoteHost.css";
import { t } from "@/i18n";

const PLUGIN = "forge.deployment";
const active = () => isPluginActive(PLUGIN);

registerToolWindow({
  id: "remoteHost",
  title: t("toolwindow.remoteHost.title"),
  icon: Server,
  anchor: "right",
  order: 40,
  component: RemoteHostView,
  when: () => active() && !!config()?.servers.length,
});

registerKeymapActions([
  {
    id: "ActivateRemoteHostToolWindow",
    title: "Remote Host",
    group: "Main Menu/View/Tool Windows",
    shortcuts: { $default: [], macOS: [] },
  },
]);
registerCommand({ id: "ActivateRemoteHostToolWindow", title: "Remote Host", category: "View", run: () => showToolWindow("remoteHost") });

// Tools | Deployment | Browse Remote Host: the default server, else the one the user picks.
registerCommand({
  id: "PublishGroup.BrowseServers",
  title: "Browse Remote Host",
  category: "Deployment",
  run: async () => {
    if (!active()) return void showBalloon("Deployment is disabled", "Enable the Deployment plugin in Settings | Plugins.");
    const s = defaultServer() ?? (await chooseServer("Browse Remote Host"));
    if (!s) return;
    selectServer(s.id);
    showToolWindow("remoteHost");
  },
});

const serverName = (id: string) => config()?.servers.find((s) => s.id === id)?.name ?? id;

// Edit Remote File: a temporary copy (Rust keeps it in the cache folder), uploaded on save.
registerFileSystem({
  prefix: EDIT_PREFIX,
  read: async (path) => {
    const target = parseEditPath(path);
    if (!target) throw new Error(`${path} is not a remote file`);
    return (await remoteHostApi.edit(target.server, target.path)).text;
  },
  write: (path, content) => saveRemoteEdit(path, content),
  breadcrumbs: (path) => {
    const target = parseEditPath(path);
    return target ? [serverName(target.server), ...target.path.split("/")] : [path];
  },
});

onTabClosed((tab) => {
  const target = parseEditPath(tab.path);
  if (target) void remoteHostApi.closeEdit(target.server, target.path).catch(() => undefined);
});
