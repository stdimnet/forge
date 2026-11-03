/**
 * DotNetForge in the editor: MSBuild files (`.csproj`, `.props`, `.slnx`, …) as XML, Razor
 * as HTML with C# (`web/razor.ts`), `.sln` solutions with their own highlighting; Reformat Code with `dotnet format`
 * (`dotnet.formatter`), Code Cleanup with `dotnet format` on a file, folder or the
 * solution; the solution reloads when a project file is saved.
 */
import type { StreamParser, StringStream } from "@codemirror/language";
import { xml } from "@codemirror/lang-xml";
import { registerLanguage } from "@/editor/languages";
import { streamLanguage } from "@/editor/tokens";
import { registerFormatProvider } from "@/features/lsp/format";
import { setChooserDialog } from "@/features/git/overlays";
import { dotnetApi } from "@/ipc/dotnet";
import { commands, registerCommand } from "@/state/commands";
import { activeTab, onFileSaved } from "@/state/editor";
import { getSetting } from "@/state/settings";
import { dotnetActive, dotnetWorkspace, errorText, refreshSolution, runDotnetCommand, showDotnetBalloon } from "./state";

// ---- Languages ---------------------------------------------------------------------------------

/** `.sln`: sections, GUIDs, strings and configurations. */
export const slnParser: StreamParser<{ value: boolean }> = {
  name: "sln",
  startState: () => ({ value: false }),
  copyState: (s) => ({ ...s }),
  token(stream: StringStream, state) {
    if (stream.sol()) state.value = false;
    if (stream.eatSpace()) return null;
    if (stream.sol() && stream.peek() === "#") {
      stream.skipToEnd();
      return "comment";
    }
    if (stream.match(/^"[^"\n]*"?/)) return "string";
    if (stream.match(/^\{[0-9A-Fa-f-]{8,}\}/)) return "atom";
    if (stream.match(/^(?:EndProjectSection|EndGlobalSection|ProjectSection|GlobalSection|EndProject|EndGlobal|Project|Global)\b/)) return "keyword";
    if (stream.match(/^(?:preSolution|postSolution|preProject|postProject)\b/)) return "builtin";
    if (stream.match(/^Microsoft Visual Studio Solution File|^Format Version|^VisualStudioVersion|^MinimumVisualStudioVersion/)) return "def";
    if (stream.match(/^\d+(?:\.\d+)*/)) return "number";
    if (stream.eat("=")) {
      state.value = true;
      return "operator";
    }
    if (stream.match(/^[(),|]/)) return "punct";
    stream.next();
    return state.value ? "string" : "variable";
  },
};

registerLanguage({ id: "msbuild", load: () => xml() });
registerLanguage({ id: "sln", load: () => streamLanguage(slnParser) });

const isDotnetSource = (path: string) => /\.(cs|csx|fs|fsi|fsx|vb)$/i.test(path);

// ---- Reformat Code -------------------------------------------------------------------------------

registerFormatProvider(async (path, text) => {
  if (!isDotnetSource(path) || !dotnetActive()) return null;
  const formatter = getSetting<string>("dotnet.formatter", "lsp");
  if (formatter === "none") return text;
  if (formatter !== "dotnetFormat") return null;
  try {
    return (await dotnetApi.format(path, text)) ?? null;
  } catch (e) {
    showDotnetBalloon("dotnet format", errorText(e), 6000);
    return text;
  }
});

// ---- Code Cleanup --------------------------------------------------------------------------------

/** Run `dotnet format` on paths (the whole solution without them) and report the fixes. */
export async function cleanup(paths?: string[]): Promise<void> {
  try {
    await commands().get("SaveAll")?.run();
    const { command, reportDir } = await dotnetApi.cleanupCommand(paths);
    const what = paths?.length ? paths.map((p) => p.split("/").pop()).join(", ") : (dotnetWorkspace()?.solution?.split("/").pop() ?? "the solution");
    const code = await runDotnetCommand(command, `Code Cleanup: ${what}`);
    const changes = reportDir ? await dotnetApi.formatReport(reportDir) : [];
    if (code !== 0) {
      showDotnetBalloon("Code Cleanup failed", "See the Run tool window for dotnet format's output.", 8000);
      return;
    }
    const files = new Set(changes.map((c) => c.path));
    showDotnetBalloon(
      "Code Cleanup",
      changes.length ? `${changes.length} fix${changes.length === 1 ? "" : "es"} in ${files.size} file${files.size === 1 ? "" : "s"} (.editorconfig rules).` : "Nothing to clean up: the code follows the .editorconfig rules.",
      6000,
    );
  } catch (e) {
    showDotnetBalloon("Code Cleanup", errorText(e), 8000);
  }
}

// Code | Code Cleanup… in .NET projects: dotnet format on the file, its folder or the
// solution; Forge's own cleanup stays available for other files.
const forgeCleanup = commands().get("CodeCleanup");
registerCommand({
  id: "CodeCleanup",
  title: "Code Cleanup…",
  category: "Code",
  run: () => {
    const path = activeTab()?.path;
    if (!dotnetActive() || (path && !isDotnetSource(path) && forgeCleanup)) {
      void forgeCleanup?.run();
      return;
    }
    const folder = path?.includes("/") ? path.slice(0, path.lastIndexOf("/")) : "";
    setChooserDialog({
      title: "Code Cleanup with dotnet format",
      items: [
        ...(path ? [{ id: "file", label: `File '${path.split("/").pop()}'`, detail: path }] : []),
        ...(folder ? [{ id: "folder", label: `Folder '${folder.split("/").pop()}'`, detail: folder }] : []),
        { id: "solution", label: "Whole solution", detail: dotnetWorkspace()?.solution ?? "" },
      ],
      empty: "",
      okLabel: "Clean Up",
      onOk: (item) => void cleanup(item.id === "file" && path ? [path] : item.id === "folder" ? [folder] : undefined),
    });
  },
});

// Reload the solution when a project, solution or launch settings file is saved.
const RELOAD = /\.(sln|slnx|csproj|fsproj|vbproj|props)$|(^|\/)(launchSettings|global)\.json$/i;
onFileSaved((path) => {
  if (dotnetActive() && RELOAD.test(path)) void refreshSolution(true);
});
