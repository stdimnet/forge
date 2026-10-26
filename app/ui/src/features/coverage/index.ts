/**
 * Coverage (PHPStorm: Run with Coverage, the Coverage tool window, Show Coverage Data):
 * Run with Coverage runs of any test framework that declares coverage end with a suite
 * (`coverage` run event); the Coverage tool window shows its directories and files with line
 * and branch coverage, the editor gutter marks covered / partly covered / uncovered lines
 * with their hits, and the Project view shows "x% lines covered". Reports (LCOV, Cobertura,
 * Clover, JaCoCo, Go profiles) are read in Rust (`forge-testing`).
 */
import { ShieldCheck } from "lucide-solid";
import { registerToolWindow } from "@/features/registry";
import { registerEditorExtension } from "@/editor/extensions";
import { registerCommand } from "@/state/commands";
import { registerKeymapActions } from "@/keymap/store";
import { registerMainMenuEntries } from "@/features/mainMenu";
import { action as menuAction } from "@/features/mainMenu/model";
import { registerRowDecorator } from "@/features/project/ProjectView";
import { item, registerContextMenuProvider } from "@/features/contextMenu/state";
import type { FileEntry } from "@/ipc/workspace";
import { onCoverageSuite } from "@/features/run";
import { CoverageToolWindow, showSuitesMenu } from "./CoverageToolWindow";
import { coverageGutter } from "./gutter";
import { activeSuite, projectHint, setCoverageVisible, showSuite, suites, visible } from "./store";
import { importCoverageFile, importReport } from "./actions";
import "./coverage.css";

registerToolWindow({
  id: "coverage",
  title: "Coverage",
  icon: ShieldCheck,
  anchor: "right",
  order: 40,
  component: CoverageToolWindow,
  when: () => suites().length > 0,
});

registerKeymapActions([
  { id: "Coverage", title: "Run with Coverage", group: "Main Menu/Run" },
  { id: "RunCoverage", title: "Run Context Configuration with Coverage", group: "Main Menu/Run" },
  {
    id: "SwitchCoverage",
    title: "Show Coverage Data…",
    group: "Main Menu/Run",
    shortcuts: { $default: ["Ctrl+Alt+F6"], macOS: ["Cmd+Alt+F6"] },
  },
  { id: "HideCoverageInfo", title: "Hide Coverage", group: "Main Menu/Run" },
  { id: "ImportCoverage", title: "Import External Coverage Report…", group: "Main Menu/Run" },
]);

registerMainMenuEntries(
  "Run",
  [menuAction("Coverage", "Run with Co_verage"), menuAction("SwitchCoverage", "Show Coverage _Data..."), menuAction("HideCoverageInfo", "_Hide Coverage")],
  { after: "Debug" },
);

registerCommand({
  id: "SwitchCoverage",
  title: "Show Coverage Data…",
  category: "Run",
  run: () => showSuitesMenu(window.innerWidth / 2 - 160, window.innerHeight / 3),
});
registerCommand({
  id: "HideCoverageInfo",
  title: "Hide Coverage",
  category: "Run",
  run: () => setCoverageVisible(!visible()),
});
registerCommand({ id: "ImportCoverage", title: "Import External Coverage Report…", category: "Run", run: () => importReport() });

registerEditorExtension(({ path }) => coverageGutter(path));

// "62% lines covered" next to files and folders while coverage is shown.
registerRowDecorator((path) => {
  const hint = activeSuite() ? projectHint(path) : undefined;
  return hint ? { class: "coverage-tree-row", hint } : undefined;
});

const REPORT = /(^|\/)(lcov\.info|[^/]*\.lcov|clover\.xml|cobertura[^/]*\.xml|coverage[^/]*\.xml|jacoco[^/]*\.xml|cover(age)?\.out)$/i;

registerContextMenuProvider<FileEntry | null>("project", (entry) =>
  entry && entry.kind !== "dir" && REPORT.test(entry.path)
    ? [item("Show Coverage Data from This Report", () => void importCoverageFile(entry.path), "ImportCoverage")]
    : [],
);

// A finished Run with Coverage run shows its suite.
onCoverageSuite((suite) => void showSuite(suite.id));
