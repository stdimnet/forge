/**
 * Running an External Tool: collect the macro context (the file of the focused editor or
 * the Project view, caret, selection, clipboard, `$Prompt$` answers), let Rust expand the
 * command (`external_tool_command`) and run it in the Run tool window with the tool's
 * output filters. "Synchronize files after execution" refreshes the Project view.
 */
import { runApi, type RunConfiguration } from "@/ipc/run";
import { showToolWindow } from "@/state/layout";
import { runToolsApi, type ExternalTool, type MacroContext } from "@/ipc/runTools";
import { balloonsFor } from "@/keymap";
import { activeView } from "@/features/lsp/navigation";
import { askName } from "@/features/contextMenu/dialogs";
import { launch, registerRunConfigurationType, type RunHandle } from "@/features/run";
import { refreshDirs } from "@/features/project/ProjectView";
import { Wrench } from "lucide-solid";
import { findTool, toolRef } from "./model";

const showBalloon = balloonsFor("External Tools");

/** Type of the temporary configurations tool runs use (hidden from the dialog). */
export const TOOL_TYPE = "external-tool";

const parse = <T>(text: string | undefined, fallback: T): T => {
  try {
    return text ? (JSON.parse(text) as T) : fallback;
  } catch {
    return fallback;
  }
};

registerRunConfigurationType({
  id: TOOL_TYPE,
  name: "External Tool",
  icon: Wrench,
  color: "var(--fg-muted)",
  fields: [],
  hidden: true,
  buildCommand: (cfg) => ({
    program: cfg.options?.program ?? "",
    args: parse<string[]>(cfg.options?.args, []),
    cwd: cfg.options?.cwd ?? "",
    env: {},
  }),
  outputFilters: (cfg) => parse<string[]>(cfg.options?.outputFilters, []),
});

/** The macro context of the focused editor, or of a file picked in the Project view. */
export function currentContext(path?: string): MacroContext {
  const a = activeView();
  if (path !== undefined && path !== a?.path) return { filePath: path || undefined };
  if (!a) return {};
  const state = a.view.state;
  const sel = state.selection.main;
  const pos = (offset: number) => {
    const line = state.doc.lineAt(offset);
    return { line: line.number, column: offset - line.from + 1 };
  };
  const head = pos(sel.head);
  const from = pos(sel.from);
  const to = pos(sel.to);
  return {
    filePath: a.path,
    line: head.line,
    column: head.column,
    selectedText: sel.empty ? undefined : state.sliceDoc(sel.from, sel.to),
    selectionStartLine: from.line,
    selectionStartColumn: from.column,
    selectionEndLine: to.line,
    selectionEndColumn: to.column,
  };
}

const uses = (tool: ExternalTool, macro: string) =>
  [tool.program, tool.arguments, tool.workingDirectory].some((t) => t.includes(`$${macro}$`));

const promptCount = (tool: ExternalTool) =>
  [tool.program, tool.arguments, tool.workingDirectory].reduce((n, t) => n + (t.match(/\$Prompt\$/g)?.length ?? 0), 0);

/**
 * Run a tool. `path` is the Project view file it was started on (else the focused editor's).
 * Resolves with the run, or null when it did not start (cancelled prompt, no program).
 */
export async function runExternalTool(tool: ExternalTool, path?: string): Promise<RunHandle | null> {
  const context = currentContext(path);
  if (uses(tool, "ClipboardContent")) context.clipboard = await navigator.clipboard?.readText().catch(() => "");
  const prompts: string[] = [];
  for (let i = promptCount(tool); i > 0; i--) {
    const answer = await askName({ title: "Enter Parameters", label: "Enter parameters:", initial: "", confirm: "OK" });
    if (answer === null) return null;
    prompts.push(answer);
  }
  context.prompts = prompts;
  let cmd;
  try {
    cmd = await runToolsApi.command(tool, context);
  } catch (err) {
    showBalloon(`Cannot run '${tool.name}'`, String(err));
    return null;
  }
  const missing = (cmd.missing ?? []).filter((m) => m !== "$Prompt$");
  if (missing.length) showBalloon(`'${tool.name}': no value for ${missing.join(", ")}`, "The macros were replaced with empty text.");
  const cfg: RunConfiguration = {
    name: tool.name,
    type: TOOL_TYPE,
    options: {
      tool: toolRef(tool),
      program: cmd.program,
      args: JSON.stringify(cmd.args),
      cwd: cmd.cwd,
      outputFilters: JSON.stringify(cmd.outputFilters),
    },
  };
  // "Make console active on message in stdout / stderr": show the console with the first
  // such message instead of right away.
  const activateLater = tool.openConsole && (tool.activateOnStdout || tool.activateOnStderr);
  const handle = await launch(cfg, { background: !tool.openConsole || activateLater });
  if (handle && activateLater) {
    const stop = await runApi.onEvent(handle.started.runId, (e) => {
      if (e.type !== "output" || !(e.stream === "stderr" ? tool.activateOnStderr : tool.activateOnStdout)) return;
      showToolWindow("run");
      stop();
    });
    void handle.exited.then(() => stop());
  }
  if (handle && tool.synchronize) {
    void handle.exited.then(() => {
      const dir = context.filePath?.includes("/") ? context.filePath.slice(0, context.filePath.lastIndexOf("/")) : "";
      refreshDirs(dir ? ["", dir] : [""]);
    });
  }
  return handle;
}

/** Run a tool by its `Group/Name` reference and wait for it ("Run External tool" tasks). */
export async function runToolAndWait(ref: string, path?: string): Promise<boolean> {
  const tool = findTool(ref);
  if (!tool) {
    showBalloon(`External tool '${ref}' not found`, "Check Settings | Tools | External Tools.");
    return false;
  }
  const handle = await runExternalTool(tool, path);
  return !!handle && (await handle.exited) === 0;
}
