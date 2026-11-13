/**
 * WebForge editor support: run icons next to package.json scripts, Go to Declaration for
 * React/Vue/Angular components, props, events, selectors and pipes (several targets, such as
 * the usages of a component, open a chooser), and Angular's component <-> template/styles
 * line markers. Everything is computed in Rust (`packs/webforge/*`).
 */
import type { EditorView } from "@codemirror/view";
import { Bug, Component as ComponentIcon, FileCode2, Paintbrush, Play } from "lucide-solid";
import { webApi, npmApi, type WebFramework, type WebGutterMark } from "@/ipc/web";
import { registerLineMarkerProvider, type LineMarker } from "@/editor/lineMarkers";
import { registerDeclarationProvider } from "@/features/lsp/goto";
import { getSetting } from "@/state/settings";
import { frameworkActive, npmActive } from "./store";
import { packageOrDefault, runScript } from "./npm";
import { chooseTarget, showMenu } from "./popup";

const isPackageJson = (p: string) => p === "package.json" || p.endsWith("/package.json");

/** Files each framework navigates in. */
const APPLIES: Record<WebFramework, (path: string) => boolean> = {
  react: (p) => /\.(jsx|tsx|js|ts|mjs|mts)$/.test(p) && !p.endsWith(".d.ts"),
  vue: (p) => p.endsWith(".vue"),
  angular: (p) => /\.(ts|html)$/.test(p) && !p.endsWith(".d.ts"),
};

const FRAMEWORKS: WebFramework[] = ["react", "vue", "angular"];

// ---- package.json ----------------------------------------------------------------------

registerLineMarkerProvider({
  appliesTo: isPackageJson,
  watch: npmActive,
  markers: async (path, text): Promise<LineMarker[]> => {
    if (!npmActive() || !getSetting<boolean>("web.npm.gutterIcons", true)) return [];
    const marks = await npmApi.gutter(path, text).catch(() => []);
    return marks.map((m) => {
      const script = m.title.replace(/^Run '(.*)'$/, "$1");
      return {
        line: m.line,
        icon: Play,
        title: `Run '${script}'`,
        className: "marker-npm-run",
        onClick: (_view: EditorView, e: MouseEvent) => {
          const pkg = packageOrDefault(path);
          showMenu(
            [
              { label: `Run '${script}'`, icon: Play({ size: 14 }), run: () => void runScript(pkg, script) },
              { label: `Debug '${script}'`, icon: Bug({ size: 14 }), run: () => void runScript(pkg, script, true) },
            ],
            e.clientX,
            e.clientY + 8,
          );
        },
      };
    });
  },
});

// ---- Components ------------------------------------------------------------------------

registerDeclarationProvider(async (path, view, pos) => {
  if (!getSetting<boolean>("web.navigation.components", true)) return false;
  for (const fw of FRAMEWORKS) {
    if (!APPLIES[fw](path) || !frameworkActive(fw)) continue;
    const targets = await webApi.goto(fw, path, view.state.doc.toString(), pos).catch(() => []);
    if (!targets.length) continue;
    const word = view.state.wordAt(pos);
    const name = word ? view.state.sliceDoc(word.from, word.to) : "";
    const before = view.state.sliceDoc(Math.max(0, (word?.from ?? pos) - 300), word?.from ?? pos);
    // Inside a tag (name or attribute) the targets are declarations; on a component's
    // own name or selector they are its usages.
    const inTag = before.lastIndexOf("<") > before.lastIndexOf(">");
    const coords = view.coordsAtPos(pos);
    const title = !inTag && name ? `Usages of ${name}` : "Choose Declaration";
    chooseTarget(targets, title, coords?.left ?? 200, (coords?.bottom ?? 200) + 4);
    return true;
  }
  return false;
});

// ---- Angular line markers --------------------------------------------------------------

const MARKER_ICON: Record<WebGutterMark["kind"], LineMarker["icon"]> = {
  template: FileCode2,
  styles: Paintbrush,
  component: ComponentIcon,
  usages: ComponentIcon,
  run: Play,
};

registerLineMarkerProvider({
  appliesTo: (p) => /\.(ts|html|css|scss|sass|less)$/.test(p),
  watch: () => frameworkActive("angular"),
  markers: async (path, text): Promise<LineMarker[]> => {
    if (!frameworkActive("angular") || !getSetting<boolean>("angular.gutterIcons", true)) return [];
    const marks = await webApi.gutter("angular", path, text).catch(() => []);
    return marks.map((m) => ({
      line: m.line,
      icon: MARKER_ICON[m.kind],
      title: m.title,
      className: `marker-ng-${m.kind}`,
      onClick: (_view: EditorView, e: MouseEvent) =>
        chooseTarget(m.targets, m.kind === "styles" ? "Choose Stylesheet" : "Choose Declaration", e.clientX, e.clientY + 8),
    }));
  },
});
