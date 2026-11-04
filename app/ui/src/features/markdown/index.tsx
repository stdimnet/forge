/**
 * Markdown: `.md` files open in an editor with preview (Editor Only / Editor and Preview /
 * Preview Only), like PHPStorm's bundled Markdown plugin. The preview renders the AST from
 * Rust (`forge-markdown`). Settings | Languages & Frameworks | Markdown.
 */
import { registerFileEditor } from "@/shell/editorViews";
import { registerSettingsPage } from "@/features/settingsPages";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { getSetting } from "@/state/settings";
import { TextEditorWithPreview, type PreviewLayout } from "@/editor/TextEditorWithPreview";
import { GUEST_PREFIX } from "@/features/code-with-me/docs";
import { MarkdownPreview } from "./MarkdownPreview";
import "./markdown.css";
import { t } from "@/i18n";

/** Markdown files of the project, and of the host's project in a Code With Me guest window. */
export const isMarkdownPath = (path: string) =>
  /\.(md|markdown)$/i.test(path) && !path.replace(GUEST_PREFIX, "").includes("://");

registerFileEditor({
  id: "markdown",
  accepts: isMarkdownPath,
  component: (props) => (
    <TextEditorWithPreview
      tab={props.tab}
      group={props.group}
      defaultLayout={() => getSetting<PreviewLayout>("markdown.defaultLayout", "split")}
      preview={(ctx) => <MarkdownPreview tab={props.tab} editorView={ctx.editorView} />}
    />
  ),
});

registerSettingsPage({
  id: "lang.markdown",
  title: t("settings.page.lang.markdown"),
  group: "Languages & Frameworks",
  order: 40,
  keywords: ["preview", "mermaid", "front matter", "sync scroll", "layout"],
  component: () => <SchemaPage pageId="lang.markdown" />,
});
