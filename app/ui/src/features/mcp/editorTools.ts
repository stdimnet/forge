/**
 * The MCP server's editor tools: Rust (`commands/mcp.rs`) forwards calls that need the
 * editor (open tabs, the selection, undoable edits, problems, run configurations) as
 * `mcp://tool-request` and gets the answer through `mcp_tool_result`. Policy, approvals and
 * text changes are decided in Rust; these handlers only read and drive the editor.
 */
import { mcpApi, type McpToolRequest } from "@/ipc/mcp";
import { workspaceApi } from "@/ipc/workspace";
import { activeTab, editor, isCustomTab, isDirty, openFile, saveFile } from "@/state/editor";
import { activeView, openFileAt, visibleView, viewFor } from "@/editor/views";
import { diagnostics } from "@/features/lsp/store";
import { reformat, replaceChangedPart } from "@/features/lsp/format";
import { allConfigurations, findConfiguration, isTemporary } from "@/features/run/configStore";
import { failedTests, launch, sessionData, sessions } from "@/features/run/runStore";
import { lineText } from "@/features/run/tests";

type Args = Record<string, unknown>;
type Handler = (args: Args) => Promise<unknown>;

const str = (v: unknown) => (typeof v === "string" ? v : "");
const num = (v: unknown) => (typeof v === "number" && Number.isFinite(v) ? v : undefined);
const fileTabs = () => editor.tabs.filter((t) => !t.view && !isCustomTab(t.path));

/** The tail of a run's console that is returned to the client. */
const MAX_CONSOLE_CHARS = 20_000;

async function viewOf(path: string, create = false) {
  if (!editor.tabs.some((t) => t.path === path)) {
    await openFile(path);
    if (!editor.tabs.some((t) => t.path === path)) {
      if (!create) throw new Error(`${path}: file not found`);
      await workspaceApi.writeFile(path, "");
      await openFile(path);
    }
  } else {
    await openFile(path);
  }
  const view = await visibleView(path);
  if (!view) throw new Error(`${path} cannot be opened in the editor`);
  return view;
}

const handlers: Record<string, Handler> = {
  get_open_editors: async () => ({
    editors: fileTabs().map((t) => ({ path: t.path, modified: isDirty(t), active: t.path === editor.active })),
  }),

  get_current_file: async (args) => {
    const tab = activeTab();
    if (!tab || tab.view || isCustomTab(tab.path)) return { path: null, message: "No file is open in the active editor." };
    const view = activeView();
    const out: Record<string, unknown> = { path: tab.path, modified: isDirty(tab), caret: { line: tab.cursor.line, column: tab.cursor.col } };
    if (view) {
      const doc = view.state.doc;
      const { from, to, head } = view.state.selection.main;
      const at = (pos: number) => {
        const l = doc.lineAt(pos);
        return { line: l.number, column: pos - l.from + 1 };
      };
      out.caret = at(head);
      out.lineCount = doc.lines;
      if (from !== to) out.selection = { start: at(from), end: at(to), text: view.state.sliceDoc(from, to) };
      if (args.includeText === true) out.text = doc.toString();
    } else if (args.includeText === true) {
      out.text = tab.content;
    }
    return out;
  },

  open_file_in_editor: async (args) => {
    const path = str(args.path);
    const ok = await openFileAt(path, num(args.line), num(args.column));
    if (!ok) throw new Error(`${path} cannot be opened`);
    return `Opened ${path} in Forge's editor.`;
  },

  get_problems: async (args) => {
    const only = str(args.path);
    const files = Object.entries(diagnostics)
      .filter(([path, list]) => (only ? path === only : list.length > 0))
      .map(([path, list]) => ({
        path,
        problems: list.map((d) => ({
          line: d.range.start.line + 1,
          column: d.range.start.character + 1,
          severity: d.severityName ?? d.severity,
          message: d.message,
          source: d.source ?? undefined,
          inspection: d.inspectionName ?? undefined,
        })),
      }));
    if (only && !files.length) files.push({ path: only, problems: [] });
    return { files };
  },

  list_run_configurations: async () => ({
    configurations: allConfigurations().map((c) => ({ name: c.name, type: c.type, temporary: isTemporary(c) })),
  }),

  /** Internal: the text of a file as the editor has it (unsaved changes included). */
  buffer_text: async (args) => {
    const path = str(args.path);
    const view = viewFor(path);
    if (view) return { text: view.state.doc.toString(), open: true };
    const tab = editor.tabs.find((t) => t.path === path);
    if (tab) return { text: tab.content, open: true };
    return { text: await workspaceApi.readFile(path), open: false };
  },

  /** Internal: replace a file's text in the editor as one undo step, then save it. */
  apply_text: async (args) => {
    const path = str(args.path);
    const text = str(args.text);
    const view = await viewOf(path, args.create === true);
    replaceChangedPart(view, text, "input.mcp", true);
    await saveFile(path, "plain");
    return { ok: true };
  },

  reformat_file: async (args) => {
    const path = str(args.path);
    const view = await viewOf(path);
    const before = view.state.doc.toString();
    await reformat(path, view);
    await saveFile(path, "plain");
    return before === view.state.doc.toString() ? `${path} was already formatted.` : `Reformatted ${path} and saved it.`;
  },

  run_configuration: async (args) => {
    const name = str(args.name);
    const cfg = findConfiguration(name);
    if (!cfg) throw new Error(`No run configuration '${name}'. Use list_run_configurations.`);
    const handle = await launch(cfg);
    if (!handle) throw new Error(`'${name}' did not start (see the Run tool window).`);
    const timeout = (num(args.timeoutSeconds) ?? 300) * 1000;
    const code = await Promise.race([handle.exited, new Promise<"timeout">((r) => setTimeout(() => r("timeout"), timeout))]);
    const session = sessions().find((s) => s.runId === handle.started.runId);
    const data = session ? sessionData(session.id) : undefined;
    const consoleText = (data?.lines ?? []).map(lineText).join("\n");
    const tests = data?.tests;
    return {
      name,
      commandLine: handle.started.commandLine,
      finished: code !== "timeout",
      exitCode: code === "timeout" ? null : code,
      tests: tests
        ? { summary: tests.summary.text, counts: tests.summary.counts, failed: failedTests(tests.root).map((t) => ({ name: t.name, path: t.path })) }
        : undefined,
      output: consoleText.length > MAX_CONSOLE_CHARS ? "…" + consoleText.slice(-MAX_CONSOLE_CHARS) : consoleText,
    };
  },
};

/** Answer one forwarded tool call. */
export async function handleToolRequest(req: McpToolRequest): Promise<void> {
  const handler = handlers[req.tool];
  try {
    if (!handler) throw new Error(`${req.tool} is not available in this window`);
    await mcpApi.toolResult(req.id, { result: await handler(req.arguments ?? {}) });
  } catch (e) {
    await mcpApi.toolResult(req.id, { error: String((e as Error)?.message ?? e) }).catch(() => undefined);
  }
}

export const editorToolNames = Object.keys(handlers);
