/**
 * Scratches and Consoles (PHPStorm): New Scratch File (Ctrl+Alt+Shift+Insert), New Scratch
 * Buffer, the "Scratches and Consoles" node of the Project view, scratch editor tabs at
 * `scratch://<name>`, gutter ▶ icons and runs: PHP with the project's autoloader, Node for
 * JavaScript/TypeScript, HTTP requests of `.http` files and SQL in a data source's console.
 */
import { render } from "solid-js/web";
import { Globe, NotebookPen, Play } from "lucide-solid";
import { registerKeymapActions } from "@/keymap";
import { registerCommand, commands } from "@/state/commands";
import { activeTab, registerFileSystem } from "@/state/editor";
import { registerLineMarkerProvider } from "@/editor/lineMarkers";
import { registerEditorExtension } from "@/editor/extensions";
import { scratchesApi } from "@/ipc/scratches";
import { registerSettingsPage } from "@/features/settingsPages";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { registerProjectViewSection } from "@/features/project/ProjectView";
import { registerRunConfigurationType } from "@/features/run/types";
import { NewScratchPopupHost, openNewScratchPopup } from "./NewScratchPopup";
import { AttachPopupHost } from "./attach";
import { ScratchIcon, ScratchesNode } from "./ScratchesNode";
import {
  createBuffer,
  fileContent,
  httpEnvironment,
  isHttpPath,
  isScratchPath,
  refreshScratches,
  runHttpFile,
  runScratch,
  scratchContent,
  scratchName,
  scratchPath,
} from "./store";
import { httpToolbar } from "./httpToolbar";
import { openMenu } from "@/features/run/PopupMenu";
import { shortcutLabel } from "@/keymap";
import { Bug } from "lucide-solid";

/** Gutter ▶ of a request: Run, or Debug with the Xdebug session cookie (PHP's debugger). */
function openHttpRunMenu(e: MouseEvent, name: string, run: (debug: boolean) => Promise<void>): void {
  openMenu({
    x: e.clientX,
    y: e.clientY,
    items: [
      { label: t("common.run", { name }), icon: Play, iconColor: "var(--success)", shortcut: shortcutLabel("RunClass"), run: () => void run(false) },
      { label: t("tools.scratches.debugWithXdebug", { name }), icon: Bug, iconColor: "var(--info)", shortcut: shortcutLabel("DebugClass"), run: () => void run(true) },
    ],
  });
}
import "./httpLanguage";
import "./scratches.css";
import { t } from "@/i18n";

registerFileSystem({
  prefix: "scratch://",
  read: (path) => scratchesApi.read(scratchName(path)!),
  write: (path, content) => scratchesApi.write(scratchName(path)!, content),
  icon: (p) => <ScratchIcon name={p.name} size={p.size} />,
  breadcrumbs: (path) => ["Scratches and Consoles", "Scratches", scratchName(path)!],
});

registerProjectViewSection({ id: "scratches", order: 100, component: ScratchesNode });

// Runs started from scratches only (gutter, Ctrl+Shift+F10); not in the configurations dialog.
registerRunConfigurationType({
  id: "scratch",
  name: "Scratch",
  description: "Run a scratch file.",
  icon: NotebookPen,
  color: "var(--accent)",
  hidden: true,
  interpreter: "php",
  argumentsLabel: false,
  fields: [{ key: "scratch", label: t("tools.scratches.scratch"), required: true }],
});

registerRunConfigurationType({
  id: "http-request",
  name: "HTTP Request",
  description: "Send a request of an .http file.",
  icon: Globe,
  color: "var(--info)",
  hidden: true,
  debuggable: true,
  argumentsLabel: false,
  // `scratch` (a scratch's name) or `file` (a project path), and the request's `line`.
  fields: [
    { key: "scratch", label: t("tools.scratches.scratch") },
    { key: "file", label: t("common.file") },
  ],
  start: async (cfg, request) => {
    const file = cfg.options?.file;
    const scratch = cfg.options?.scratch ?? "";
    const content = file ? await fileContent(file) : await scratchContent(scratch);
    const source = file ?? scratchPath(scratch);
    // A Debug run (gutter menu, Shift+F9) carries the Xdebug environment: add the cookie.
    const debug = !!request.env?.XDEBUG_SESSION;
    return scratchesApi.httpRun(request.runId!, content, Number(cfg.options?.line ?? 0), httpEnvironment(), source, debug);
  },
});

// Toolbar with the environment selector above every `.http` editor.
registerEditorExtension(({ path }) => (isHttpPath(path) ? httpToolbar() : null));

registerSettingsPage({
  id: "tools.http-client",
  title: t("settings.page.tools.http-client"),
  group: "Tools",
  keywords: ["http", "rest", "environment", "request"],
  component: () => <SchemaPage pageId="tools.http-client" />,
});

// Project `.http` files get the same ▶ icons as `.http` scratches.
registerLineMarkerProvider({
  appliesTo: (path) => isHttpPath(path) && !isScratchPath(path),
  markers: async (path, text) => {
    const markers = await scratchesApi.markers(path.split("/").pop() ?? path, text).catch(() => []);
    return markers
      .filter((m) => m.kind === "http")
      .map((m) => ({
        line: m.line,
        icon: Play,
        title: m.title,
        className: "marker-run",
        onClick: (_view, e) => openHttpRunMenu(e, m.runName, (debug) => runHttpFile(path, m.line, debug)),
      }));
  },
});

registerLineMarkerProvider({
  appliesTo: isScratchPath,
  markers: async (path, text) => {
    const name = scratchName(path)!;
    const markers = await scratchesApi.markers(name, text).catch(() => []);
    return markers.map((m) => ({
      line: m.line,
      icon: Play,
      title: m.title,
      className: "marker-run",
      onClick: (_view, e) =>
        m.kind === "http" ? openHttpRunMenu(e, m.runName, (debug) => runScratch(name, m.line, debug)) : void runScratch(name, m.line),
    }));
  },
});

registerKeymapActions([{ id: "NewScratchBuffer", title: "New Scratch Buffer", group: "Main Menu/File" }]);
registerCommand({ id: "NewScratchFile", title: "New Scratch File…", category: "File", run: () => void openNewScratchPopup() });
registerCommand({ id: "NewScratchBuffer", title: "New Scratch Buffer", category: "File", run: () => void createBuffer() });

// Ctrl+Shift+F10 in a scratch runs it.
const runClass = commands().get("RunClass");
if (runClass) {
  registerCommand({
    ...runClass,
    run: () => {
      const tab = activeTab();
      const name = tab ? scratchName(tab.path) : null;
      if (name) return runScratch(name);
      if (tab && isHttpPath(tab.path)) return runHttpFile(tab.path);
      return runClass.run();
    },
  });
}

if (typeof document !== "undefined") {
  const host = document.createElement("div");
  host.id = "scratches-overlay";
  document.body.appendChild(host);
  render(
    () => (
      <>
        <NewScratchPopupHost />
        <AttachPopupHost />
      </>
    ),
    host,
  );
  void refreshScratches();
  void scratchesApi.onChanged(() => void refreshScratches());
}
