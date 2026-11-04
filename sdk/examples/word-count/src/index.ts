/**
 * Word Count: an example Forge plugin. It shows the number of words of the active editor in
 * the status bar, a details table in a tool window, and a command (Ctrl+Alt+Shift+W) that
 * shows a message. It only needs `"editor": ["read"]`.
 */
import type { ActiveEditor, PluginContext } from "@forge/plugin-api";

const STATUS = "example.word-count.status";
const TOOL_WINDOW = "example.word-count.details";

export interface Counts {
  words: number;
  characters: number;
  lines: number;
  minutes: number;
}

/** Count words (runs of letters, digits, `'` and `-`), characters and lines. */
export function count(text: string, wordsPerMinute = 230): Counts {
  const words = text.match(/[\p{L}\p{N}][\p{L}\p{N}'’-]*/gu)?.length ?? 0;
  return {
    words,
    characters: [...text].length,
    lines: text === "" ? 0 : text.split("\n").length,
    minutes: Math.max(1, Math.round(words / Math.max(1, wordsPerMinute))),
  };
}

const escape = (s: string) => s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);

async function current(editor: ActiveEditor | null): Promise<{ counts: Counts; scope: string; file: string } | null> {
  if (!editor) return null;
  const onlySelection = (await forge.settings.get<boolean>("example.word-count.countSelection")) ?? true;
  const wpm = (await forge.settings.get<number>("example.word-count.wordsPerMinute")) ?? 230;
  const { from, to } = editor.selection;
  const selected = onlySelection && to > from;
  const text = selected ? editor.text.slice(from, to) : editor.text;
  return { counts: count(text, wpm), scope: selected ? "selection" : "file", file: editor.path.split("/").pop() ?? editor.path };
}

async function render(editor: ActiveEditor | null): Promise<void> {
  const state = await current(editor);
  if (!state) {
    await forge.statusBar.update(STATUS, { text: "" });
    await forge.toolWindows.setHtml(TOOL_WINDOW, `<p style="color:var(--fg-muted)">Open a file to count its words.</p>`);
    return;
  }
  const { counts, scope, file } = state;
  await forge.statusBar.update(STATUS, {
    text: `${counts.words.toLocaleString()} words`,
    tooltip: `Word Count (${scope}): ${counts.characters.toLocaleString()} characters, ${counts.lines} lines`,
  });
  const row = (label: string, value: string) => `<tr><td style="color:var(--fg-muted);padding:3px 16px 3px 0">${label}</td><td>${value}</td></tr>`;
  await forge.toolWindows.setHtml(
    TOOL_WINDOW,
    `<h3 style="margin:0 0 8px;font-size:13px">${escape(file)} <span style="color:var(--fg-muted);font-weight:normal">(${scope})</span></h3>
     <table style="border-collapse:collapse">
       ${row("Words", counts.words.toLocaleString())}
       ${row("Characters", counts.characters.toLocaleString())}
       ${row("Lines", counts.lines.toLocaleString())}
       ${row("Reading time", `about ${counts.minutes} min`)}
     </table>`,
  );
}

export function activate(context: PluginContext): void {
  context.subscriptions.push(
    forge.commands.register("example.word-count.show", async () => {
      const state = await current(await forge.editor.getActive());
      await forge.window.showMessage(
        state
          ? `${state.file}: ${state.counts.words} words, ${state.counts.characters} characters (${state.scope})`
          : "Open a file to count its words.",
      );
    }),
    forge.editor.onDidChange((editor) => void render(editor)),
    forge.settings.onDidChange(() => void forge.editor.getActive().then(render)),
  );
  void forge.editor.getActive().then(render);
}
