/**
 * Minecraft plugins in the editor: the `main` class of plugin descriptors is checked
 * (unknown or invalid classes are errors) and Ctrl+B / Ctrl+Click on it opens the class;
 * Java / Kotlin sources get gutter icons for event handlers and ▶ "Run Paper Server" on the
 * plugin's main class (with run-paper). Everything is computed in Rust.
 */
import type { Extension } from "@codemirror/state";
import { ViewPlugin, type EditorView, type ViewUpdate } from "@codemirror/view";
import { Play, Radio, Settings2 } from "lucide-solid";
import { minecraftApi, type MinecraftMarker } from "@/ipc/minecraft";
import type { EditorDiagnostic } from "@/ipc/lsp";
import { registerEditorExtension } from "@/editor/extensions";
import { registerLineMarkerProvider, type LineMarker } from "@/editor/lineMarkers";
import { registerDeclarationProvider } from "@/features/lsp/goto";
import { publishDiagnostics } from "@/features/lsp/store";
import { toLsp } from "@/features/lsp/util";
import { chooseDeclarationAt } from "@/features/navigation/declarationChooser";
import { revealSymbol } from "@/features/php/editorViews";
import { openMenu } from "@/features/run/PopupMenu";
import { openRunConfigurations } from "@/features/run/dialogState";
import { launch } from "@/features/run/runStore";
import { getSetting } from "@/state/settings";
import { minecraftActive, RUN_SERVER, runServerConfiguration } from "./store";

const SOURCE = "minecraft";

/** `plugin.yml`, `paper-plugin.yml`, `bungee.yml` or `velocity-plugin.json` in `src/main/resources`. */
export const isDescriptor = (path: string) =>
  /(^|[/\\])src[/\\]main[/\\]resources[/\\](plugin\.yml|paper-plugin\.yml|bungee\.yml|velocity-plugin\.json)$/.test(path);
const isSource = (path: string) => /\.(java|kt)$/.test(path);

// ---- Descriptors --------------------------------------------------------------------------

function diagnostics(path: string): Extension {
  return ViewPlugin.define((view) => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    let seq = 0;
    const run = async () => {
      timer = undefined;
      const id = ++seq;
      if (!minecraftActive()) {
        publishDiagnostics(path, [], SOURCE);
        return;
      }
      const text = view.state.doc.toString();
      const problems = await minecraftApi.inspect(path, text).catch(() => null);
      if (id !== seq || !problems || view.state.doc.toString() !== text) return;
      const doc = view.state.doc;
      const diags: EditorDiagnostic[] = problems.map((p) => ({
        range: { start: toLsp(doc, p.from), end: toLsp(doc, Math.max(p.to, p.from)) },
        severity: p.severity,
        message: p.message,
        source: "Minecraft",
        code: p.code,
      }));
      publishDiagnostics(path, diags, SOURCE);
    };
    const schedule = (delay: number) => {
      clearTimeout(timer);
      timer = setTimeout(() => void run(), delay);
    };
    schedule(0);
    return {
      update(u: ViewUpdate) {
        if (u.docChanged) schedule(400);
      },
      destroy() {
        clearTimeout(timer);
        publishDiagnostics(path, [], SOURCE);
      },
    };
  });
}

registerEditorExtension(({ path }): Extension | null => (isDescriptor(path) ? diagnostics(path) : null));

registerDeclarationProvider(async (path, view, pos) => {
  if (!minecraftActive() || !isDescriptor(path)) return false;
  const targets = await minecraftApi.goto(path, view.state.doc.toString(), pos).catch(() => []);
  if (!targets.length) return false;
  chooseDeclarationAt(view, pos, targets, { title: "Main Class", open: (t) => void revealSymbol(t.path, { line: t.line, name: t.name }) });
  return true;
});

// ---- Gutter -------------------------------------------------------------------------------

function runMenu(e: MouseEvent): void {
  const cfg = () => runServerConfiguration();
  openMenu({
    x: e.clientX,
    y: e.clientY + 6,
    items: [
      { label: `Run '${RUN_SERVER}'`, icon: Play, iconColor: "var(--success)", run: () => void launch(cfg()) },
      { separator: true },
      { label: "Modify Run Configuration…", icon: Settings2, run: () => openRunConfigurations(cfg().name) },
    ],
  });
}

const toMarker = (m: MinecraftMarker): LineMarker =>
  m.kind === "plugin"
    ? { line: m.line, icon: Play, title: `Run '${RUN_SERVER}'`, className: "marker-run", onClick: (_view: EditorView, e: MouseEvent) => runMenu(e) }
    : { line: m.line, icon: Radio, title: m.title, className: "marker-minecraft-listener" };

registerLineMarkerProvider({
  appliesTo: isSource,
  watch: () => [minecraftActive(), getSetting<boolean>("minecraft.gutterIcons", true)],
  markers: async (path, text) => {
    if (!minecraftActive() || !getSetting<boolean>("minecraft.gutterIcons", true)) return [];
    const marks = await minecraftApi.gutter(path, text).catch(() => [] as MinecraftMarker[]);
    return marks.map(toMarker);
  },
});
