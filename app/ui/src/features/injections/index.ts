/**
 * Language injections (PHPStorm's IntelliLang): SQL, HTML, CSS, JSON, RegExp, JavaScript and
 * XML inside PHP and JavaScript/TypeScript strings, from `// language=` comments, heredoc
 * labels and built-in rules (`crates/forge-injections`). The editor colors injected ranges
 * with the injected language's parser; Alt+Enter offers Inject language or reference,
 * Un-inject Language and Edit <language> Fragment (from `code_context_actions`); Ctrl+Enter
 * or the gutter's run icon runs injected SQL in a query console. Settings | Editor |
 * Language Injections turns rules on and off.
 *
 * Shortcuts come from the keymap: Ctrl+Enter is PHPStorm's Split Line / Execute, which run
 * the SQL when the caret is in an injected SQL string.
 */
import { render } from "solid-js/web";
import { Play } from "lucide-solid";
import type { EditorView } from "@codemirror/view";
import { registerEditorExtension } from "@/editor/extensions";
import { registerLineMarkerProvider } from "@/editor/lineMarkers";
import { registerSettingsPage } from "@/features/settingsPages";
import { activeView } from "@/features/lsp/navigation";
import { showHint } from "@/features/lsp/hint";
import { sources } from "@/features/database/store";
import { injectionsApi, type Injection } from "@/ipc/injections";
import { registerKeymapActions } from "@/keymap";
import { commands, registerCommand } from "@/state/commands";
import { getSetting, saveSetting } from "@/state/settings";
import { editFragment, fragmentEditor, fragmentHostOf } from "./fragmentEditor";
import { UNINJECTED_KEY, injectionAt, injectionHighlighting, injectionSettings, isInjectionHost, refreshInjections } from "./highlight";
import { InjectionsPage } from "./InjectionsPage";
import { chooseInjectedLanguage, LanguageChooserHost } from "./LanguageChooser";
import { runSqlInConsole } from "./runSql";
import "./injections.css";
import { t } from "@/i18n";

registerEditorExtension(({ path }) => (isInjectionHost(path) ? [injectionHighlighting(path), fragmentEditor()] : null));

// ---------------------------------------------------------------- actions

/** The injection at the caret, analysed again so a fresh edit is included. */
function injectionAtCaret(path: string, view: EditorView): Injection | undefined {
  const pos = view.state.selection.main.head;
  try {
    const list = injectionsApi.findSync({ text: view.state.doc.toString(), path, ...injectionSettings() });
    if (list) return list.find((i) => i.literals.some((l) => l.contentFrom <= pos && pos <= l.contentTo));
  } catch {
    // Fall back to the last analysis below.
  }
  return injectionAt(view.state, pos);
}

const NO_INJECTION = "No injected language at the caret";

async function unInject(): Promise<void> {
  const a = activeView();
  if (!a) return;
  const { path, view } = a;
  const inj = injectionAtCaret(path, view);
  if (!inj) return showHint(view, view.state.selection.main.head, NO_INJECTION);
  if (inj.source === "comment" && inj.comment) {
    // Remove the comment, with its line when it is alone there.
    const { from, to } = inj.comment;
    const line = view.state.doc.lineAt(from);
    const alone = !line.text.slice(0, from - line.from).trim() && !view.state.sliceDoc(to, line.to).trim();
    const end = alone ? Math.min(line.to + 1, view.state.doc.length) : to + (/^ */.exec(view.state.sliceDoc(to, line.to))?.[0].length ?? 0);
    view.dispatch({ changes: { from: alone ? line.from : from, to: end }, userEvent: "delete.uninject" });
    return;
  }
  const entry = `${path}#${inj.key}`;
  const list = getSetting<string[]>(UNINJECTED_KEY, []) ?? [];
  if (!list.includes(entry)) await saveSetting(UNINJECTED_KEY, [...list, entry]);
  view.dispatch({ effects: refreshInjections.of(null) });
}

/** Run the injected SQL at the caret (or of the focused fragment editor); false when there is none. */
function runInjectedSql(): boolean {
  const panel = fragmentHostOf(document.activeElement);
  if (panel) {
    const f = panel.fragment();
    if (f?.language !== "sql") return false;
    void runSqlInConsole(f.text);
    return true;
  }
  const a = activeView();
  if (!a || !a.view.hasFocus) return false;
  const inj = injectionAtCaret(a.path, a.view);
  if (inj?.language !== "sql") return false;
  void runSqlInConsole(inj.text);
  return true;
}

registerKeymapActions([
  { id: "InjectLanguageAction", title: "Inject Language or Reference", group: "Language Injections", context: "editor" },
  { id: "UnInjectLanguageAction", title: "Un-inject Language", group: "Language Injections", context: "editor" },
  { id: "EditInjectedFragment", title: "Edit Injected Fragment", group: "Language Injections", context: "editor" },
]);

registerCommand({
  id: "InjectLanguageAction",
  title: "Inject Language or Reference",
  category: "Code",
  run: () => {
    const a = activeView();
    if (a) void chooseInjectedLanguage(a.view, a.path);
  },
});
registerCommand({ id: "UnInjectLanguageAction", title: "Un-inject Language", category: "Code", run: () => void unInject() });
registerCommand({
  id: "EditInjectedFragment",
  title: "Edit Injected Fragment",
  category: "Code",
  run: () => {
    const a = activeView();
    if (a && !editFragment(a.view, a.path)) showHint(a.view, a.view.state.selection.main.head, NO_INJECTION);
  },
});

// Ctrl+Enter is PHPStorm's Split Line in editors and Execute in consoles; in an injected SQL
// string (or its fragment editor) it runs the SQL in a console.
for (const id of ["EditorSplitLine", "Console.Jdbc.Execute"]) {
  const base = commands().get(id);
  if (!base) continue;
  registerCommand({ ...base, run: () => (runInjectedSql() ? undefined : base.run()) });
}

registerLineMarkerProvider({
  appliesTo: isInjectionHost,
  watch: () => [sources().length, JSON.stringify(injectionSettings())],
  markers: async (path, text) => {
    // Run icons only make sense once there is a data source to run against.
    if (!sources().length) return [];
    const list = await injectionsApi.find({ text, path, ...injectionSettings() }).catch(() => [] as Injection[]);
    const lineOf = (offset: number) => text.slice(0, offset).split("\n").length - 1;
    const seen = new Set<number>();
    return list
      .filter((i) => i.language === "sql" && i.text.trim())
      .filter((i) => {
        const line = lineOf(i.from);
        if (seen.has(line)) return false;
        seen.add(line);
        return true;
      })
      .map((i) => ({
        line: lineOf(i.from),
        icon: Play,
        title: "Run SQL in Console",
        className: "marker-run marker-inject-run",
        onClick: () => void runSqlInConsole(i.text),
      }));
  },
});

registerSettingsPage({
  id: "editor.injections",
  title: t("settings.page.editor.injections"),
  group: "Editor",
  order: 37,
  keywords: ["injection", "inject", "language", "sql", "html", "json", "regexp", "css", "intellilang", "fragment"],
  component: InjectionsPage,
});

if (typeof document !== "undefined") {
  const host = document.createElement("div");
  host.id = "injections-overlay";
  document.body.appendChild(host);
  render(LanguageChooserHost, host);
}
