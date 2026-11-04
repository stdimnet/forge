/**
 * Deployment (PHPStorm's Tools | Deployment): servers in Settings | Build, Execution,
 * Deployment | Deployment, upload / download / sync / compare actions in the Tools menu and
 * the Project view, automatic upload on save and the File Transfer tool window. Transfers
 * run in Rust (`forge-deploy`, `deploy_*` commands).
 */
import { createEffect, createRoot, createSignal, on } from "solid-js";
import { render } from "solid-js/web";
import { ArrowUpDown, Columns2 } from "lucide-solid";
import { registerToolWindow } from "@/features/registry";
import { registerSettingsPage } from "@/features/settingsPages";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { openSettings } from "@/features/settings/dialogState";
import { registerCommand } from "@/state/commands";
import { onFileSaved, openCustomTab } from "@/state/editor";
import { workspace } from "@/state/workspace";
import { isPluginActive } from "@/state/project";
import { actionById, showBalloon } from "@/keymap";
import { isDialogWindow } from "@/shell/window/mode";
import { item, registerContextMenuProvider, separator, submenu } from "@/features/contextMenu/state";
import { registerMainMenuToggle } from "@/features/mainMenu/model";
import { DirDiffView, type DirDiffSource } from "@/features/compare/DirDiffView";
import type { FileEntry } from "@/ipc/workspace";
import { deploymentApi, type ServerView } from "@/ipc/deployment";
import { DeploymentPage } from "./DeploymentPage";
import { FileTransferView } from "./FileTransferView";
import { ChooseServerHost } from "./ChooseServerPopup";
import {
  autoUploadAlways,
  chooseServer,
  compareWithDeployed,
  config,
  defaultServer,
  deleteOnServer,
  downloadFrom,
  downloadFromDefault,
  listenTransfers,
  log,
  openDeploymentOptions,
  openDeploymentSettings,
  refreshConfig,
  savedForAutoUpload,
  setSettingsOpener,
  setSyncOpener,
  syncFolder,
  syncWith,
  targetPaths,
  toggleAutoUpload,
  transfer,
  uploadTo,
  uploadToDefault,
} from "./store";
import "./deployment.css";
import { t } from "@/i18n";

const PLUGIN = "forge.deployment";
const active = () => isPluginActive(PLUGIN);

setSettingsOpener((page) => openSettings(page));

registerSettingsPage({
  id: "build.deployment",
  title: t("settings.page.build.deployment"),
  group: "Build, Execution, Deployment",
  order: 5,
  keywords: ["deployment", "sftp", "ftp", "ftps", "server", "upload", "mappings", "web server", "excluded paths", "root path"],
  component: DeploymentPage,
});

registerSettingsPage({
  id: "build.deployment.options",
  title: t("settings.page.build.deployment.options"),
  group: "Build, Execution, Deployment",
  parent: "build.deployment",
  order: 0,
  keywords: ["upload on save", "automatic upload", "overwrite", "timestamps", "exclude items by name"],
  component: () => <SchemaPage pageId="build.deployment.options" />,
});

registerToolWindow({
  id: "fileTransfer",
  title: t("toolwindow.fileTransfer.title"),
  icon: ArrowUpDown,
  anchor: "bottom",
  order: 60,
  component: FileTransferView,
  when: () => active() && (!!config()?.servers.length || log.operations.length > 0),
});

/** Sync with Deployed: the folder diff with Upload / Download per row, in an editor tab. */
function openSyncTab(server: ServerView, path: string): void {
  const name = path.split("/").pop() || workspace()?.name || "project";
  const [remoteRoot, setRemoteRoot] = createSignal("");
  const source: DirDiffSource = {
    load: async () => {
      const r = await deploymentApi.syncCompare(server.id, path);
      setRemoteRoot(r.remote);
      return r.compare;
    },
    leftTitle: () => `Local: ${path || "/"}`,
    rightTitle: () => `${server.name}: ${remoteRoot() || "…"}`,
    openDiff: (e) => compareWithDeployed(path ? `${path}/${e.path}` : e.path, server),
    copy: async (e, toRight) => {
      const full = path ? `${path}/${e.path}` : e.path;
      await transfer(server, toRight ? "upload" : "download", [full]);
    },
    copyTitles: { toRight: `Upload to ${server.name}`, toLeft: `Download from ${server.name}` },
  };
  openCustomTab({
    id: `forge:deploy-sync:${server.id}\u0000${path}`,
    title: `${name} ↔ ${server.name}`,
    tooltip: t("tools.deployment.syncWith", { value: path || "the project", name: server.name }),
    icon: Columns2,
    component: () => <DirDiffView left={path} right={server.rootPath} source={source} />,
  });
}
setSyncOpener(openSyncTab);

function command(id: string, run: () => void | Promise<void>): void {
  registerCommand({
    id,
    title: actionById(id)?.title ?? id,
    category: "Deployment",
    run: () => {
      if (active()) return run();
      showBalloon(t("tools.deployment.deploymentIsDisabled"), t("tools.deployment.enableTheDeploymentPluginIn"));
    },
  });
}

command("PublishGroup.Upload", () => uploadToDefault());
command("PublishGroup.UploadTo", () => uploadTo());
command("PublishGroup.Download", () => downloadFromDefault());
command("PublishGroup.DownloadFrom", () => downloadFrom());
/** The file Compare with Deployed Version works on, or a hint when a folder is selected. */
function compareTarget(): string | null {
  const [path] = targetPaths();
  const row = document.querySelector<HTMLElement>(`.project-tree .tree-row[data-path="${CSS.escape(path)}"]`);
  if (!path || row?.dataset.dir !== undefined) {
    showBalloon(t("tools.deployment.selectAFileToCompare"), t("tools.deployment.foldersAreComparedWithSync"));
    return null;
  }
  return path;
}

command("PublishGroup.CompareLocalVsRemote", () => {
  const path = compareTarget();
  if (path) return compareWithDeployed(path);
});
command("PublishGroup.CompareLocalVsRemoteWith", async () => {
  const path = compareTarget();
  if (!path) return;
  const s = await chooseServer("Compare with Deployed Version on");
  if (s) await compareWithDeployed(path, s);
});
command("PublishGroup.SyncLocalVsRemote", () => syncWith(null, syncFolder()));
command("PublishGroup.SyncLocalVsRemoteWith", async () => {
  const s = await chooseServer("Sync with Deployed to");
  if (s) await syncWith(s, syncFolder());
});
command("PublishGroup.DeleteOnServer", () => deleteOnServer());
command("PublishGroup.Configuration", () => openDeploymentSettings());
command("PublishGroup.Options", () => openDeploymentOptions());
command("PublishGroup.ToggleAutoUpload", () => toggleAutoUpload());
// PublishGroup.BrowseServers opens the Remote Host tool window (features/remoteHost).
registerMainMenuToggle("PublishGroup.ToggleAutoUpload", autoUploadAlways);

// Project view: Deployment submenu, like PHPStorm.
registerContextMenuProvider<FileEntry | null>("project", (entry) => {
  if (!workspace() || !active() || !config()?.servers.length) return [];
  const paths = () => {
    const selected = targetPaths();
    return entry && !selected.includes(entry.path) ? [entry.path] : selected;
  };
  const isFile = entry?.kind === "file";
  const d = defaultServer();
  const folder = entry ? (entry.kind === "dir" ? entry.path : entry.path.split("/").slice(0, -1).join("/")) : "";
  return [
    submenu("Deployment", [
      item(d ? `Upload to ${d.name}` : "Upload to Default Server", () => void uploadToDefault(paths()), "PublishGroup.Upload", { disabled: !d }),
      item("Upload to…", () => void uploadTo(paths()), "PublishGroup.UploadTo"),
      item(d ? `Download from ${d.name}` : "Download from Default Server", () => void downloadFromDefault(paths()), "PublishGroup.Download", { disabled: !d }),
      item("Download from…", () => void downloadFrom(paths()), "PublishGroup.DownloadFrom"),
      separator,
      item(d ? `Compare with Deployed Version on ${d.name}` : "Compare with Deployed Version", () => void compareWithDeployed(entry!.path), "PublishGroup.CompareLocalVsRemote", {
        disabled: !isFile || !d,
      }),
      item(d ? `Sync with Deployed to ${d.name}` : "Sync with Deployed to Default Server", () => void syncWith(null, folder), "PublishGroup.SyncLocalVsRemote", { disabled: !d }),
      item("Sync with Deployed to…", async () => {
        const s = await chooseServer("Sync with Deployed to");
        if (s) await syncWith(s, folder);
      }, "PublishGroup.SyncLocalVsRemoteWith"),
      separator,
      item(d ? `Delete on ${d.name}` : "Delete on Default Server", () => void deleteOnServer(paths()), "PublishGroup.DeleteOnServer", { disabled: !d }),
    ]),
  ];
});

// "Upload changed files automatically to the default server".
onFileSaved((path, _content, trigger) => {
  if (active()) savedForAutoUpload(path, trigger === "explicit");
});

if (typeof window !== "undefined" && !isDialogWindow()) {
  listenTransfers();
  createRoot(() => createEffect(on(() => workspace()?.root, (root) => void (root ? refreshConfig() : undefined))));
  const host = document.createElement("div");
  host.id = "deployment-overlays";
  document.body.append(host);
  render(() => <ChooseServerHost />, host);
}
