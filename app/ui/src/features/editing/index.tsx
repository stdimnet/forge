/**
 * Editor actions computed in Rust (`forge-editing`): postfix completion with its settings
 * page (Editor | General | Postfix Completion), Complete Current Statement
 * (`EditorCompleteStatement`) and Cyclic Expand Word (`HippieCompletion`,
 * `HippieBackwardCompletion`). Shortcuts come from the PHPStorm keymap.
 */
import { Scissors } from "lucide-solid";
import { registerCommand } from "@/state/commands";
import { editor } from "@/state/editor";
import { registerEditorExtension } from "@/editor/extensions";
import { languageIdFor } from "@/editor/languages";
import { registerSettingsPage } from "@/features/settingsPages";
import { registerObjectSettingEditor, SchemaPage } from "@/features/settings/SchemaPage";
import { registerCompletionKind } from "@/features/lsp/completion";
import { activeView } from "@/features/navigation/editorCommands";
import { postfixCompletion } from "./postfix";
import { completeStatement } from "./statement";
import { expandWord } from "./hippie";
import { PostfixTemplatesEditor } from "./PostfixTemplatesEditor";
import "./editing.css";

registerEditorExtension(({ path }) => postfixCompletion(languageIdFor(path)));
registerCompletionKind("postfix", Scissors, "keyword");

registerSettingsPage({
  id: "editor.postfix",
  title: "Postfix Completion",
  group: "Editor",
  parent: "editor.general",
  order: 10,
  keywords: ["postfix templates", "expand", ".if", ".foreach", ".var"],
  component: () => <SchemaPage pageId="editor.postfix" />,
});
registerObjectSettingEditor("editor.postfix.disabled", PostfixTemplatesEditor);

registerCommand({
  id: "EditorCompleteStatement",
  title: "Complete Current Statement",
  category: "Code",
  run: async () => {
    const view = activeView();
    if (!view) return;
    view.focus();
    await completeStatement(view, editor.active ? languageIdFor(editor.active) : undefined);
  },
});

/** Texts of the other open files, searched after the current one. */
const otherOpenFiles = () =>
  editor.tabs.filter((t) => !t.view && t.path !== editor.active).map((t) => t.content);

for (const [id, title, backward] of [
  ["HippieCompletion", "Cyclic Expand Word", false],
  ["HippieBackwardCompletion", "Cyclic Expand Word (Backward)", true],
] as const) {
  registerCommand({
    id,
    title,
    category: "Code",
    run: async () => {
      const view = activeView();
      if (!view) return;
      view.focus();
      await expandWord(view, {
        backward,
        language: editor.active ? languageIdFor(editor.active) : undefined,
        others: otherOpenFiles,
      });
    },
  });
}
