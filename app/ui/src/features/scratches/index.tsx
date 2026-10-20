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
import { scratchesApi } from "@/ipc/scratches";
import { registerProjectViewSection } from "@/features/project/ProjectView";
import { registerRunConfigurationType } from "@/features/run/types";
import { NewScratchPopupHost, openNewScratchPopup } from "./NewScratchPopup";
import { AttachPopupHost } from "./attach";
import { ScratchIcon, ScratchesNode } from "./ScratchesNode";
import { createBuffer, isScratchPath, refreshScratches, runScratch, scratchContent, scratchName } from "./store";
import "./scratches.css";

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
  fields: [{ key: "scratch", label: "Scratch", required: true }],
});

registerRunConfigurationType({
  id: "http-request",
  name: "HTTP Request",
  description: "Send a request of an .http scratch.",
  icon: Globe,
  color: "var(--info)",
  hidden: true,
  argumentsLabel: false,
  fields: [{ key: "scratch", label: "Scratch", required: true }],
  start: async (cfg, request) => {
    const name = cfg.options?.scratch ?? "";
    const content = await scratchContent(name);
    return scratchesApi.httpRun(request.runId!, content, Number(cfg.options?.line ?? 0));
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
      onClick: () => void runScratch(name, m.line),
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
      return name ? runScratch(name) : runClass.run();
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
