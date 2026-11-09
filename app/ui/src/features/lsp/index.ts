/**
 * Editor intelligence backed by language servers: hover docs, completion, diagnostics,
 * go to declaration, parameter info, reformat, the Problems tool window and the status
 * bar widget. Language specifics live in the servers; this feature is generic.
 */
import { TriangleAlert } from "lucide-solid";
import { registerEditorExtension } from "@/editor/extensions";
import { isLibraryPath } from "@/ipc/codeintel";
import { registerToolWindow } from "@/features/registry";
import { registerStatusItem } from "@/features/statusItems";
import { registerCommand } from "@/state/commands";
import { showToolWindow } from "@/state/layout";
import { lspCompletion } from "./completion";
import { gotoProblem, lspDiagnostics } from "./diagnostics";
import { reformat } from "./format";
import { gotoDeclaration, lspGoto } from "./goto";
import { lspHover } from "./hover";
import { lspInlayHints } from "./inlayHints";
import { LspStatusItem } from "./LspStatusItem";
import { activeView, trackView } from "./navigation";
import { ProblemsView } from "./ProblemsView";
import { lspSignatureHelp, parameterInfo } from "./signature";
import { startLspListeners } from "./store";
import { documentSync } from "./sync";
import "./lsp.css";
import { t } from "@/i18n";

startLspListeners();

registerEditorExtension(({ path }) =>
  // Read-only library files (PHP stubs) are not the servers' documents; navigation in them
  // is answered from the stubs.
  isLibraryPath(path)
    ? [trackView(path), lspGoto(path)]
    : [
        trackView(path),
        documentSync(path),
        lspDiagnostics(path),
        lspHover(path),
        lspCompletion(path),
        lspGoto(path),
        lspSignatureHelp(path),
        lspInlayHints(path),
      ],
);

registerToolWindow({
  id: "problems",
  title: t("toolwindow.problems.title"),
  icon: TriangleAlert,
  anchor: "bottom",
  order: 50,
  shortcut: "Alt+6",
  component: ProblemsView,
});

registerStatusItem({ id: "lsp", align: "right", order: 5, component: LspStatusItem });

const withActive = (fn: (path: string, view: NonNullable<ReturnType<typeof activeView>>["view"]) => unknown) => () => {
  const a = activeView();
  if (a) void fn(a.path, a.view);
};

registerCommand({ id: "GotoDeclaration", title: "Go to Declaration or Usages", category: "Navigate", run: withActive((p, v) => gotoDeclaration(p, v)) });
registerCommand({ id: "ParameterInfo", title: "Parameter Info", category: "View", run: withActive((p, v) => parameterInfo(p, v)) });
registerCommand({ id: "ReformatCode", title: "Reformat Code", category: "Code", run: withActive((p, v) => reformat(p, v)) });
registerCommand({ id: "GotoNextError", title: "Next Highlighted Error", category: "Navigate", run: withActive((_p, v) => gotoProblem(v, 1)) });
registerCommand({ id: "GotoPreviousError", title: "Previous Highlighted Error", category: "Navigate", run: withActive((_p, v) => gotoProblem(v, -1)) });
registerCommand({
  id: "ActivateProblemsViewToolWindow",
  title: "Problems",
  category: "Tool Windows",
  run: () => showToolWindow("problems"),
});
