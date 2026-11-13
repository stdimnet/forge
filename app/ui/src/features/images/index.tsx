/**
 * Images: PNG, JPEG, GIF, WebP, ICO and BMP open in the image editor (a view tab, the bytes
 * come through the backend so remote projects work); SVG opens as text with an image
 * preview next to it, like PHPStorm. Settings | Editor | Image.
 */
import { createResource, createSignal, onCleanup } from "solid-js";
import { registerEditorView, registerFileEditor } from "@/shell/editorViews";
import { registerBinaryFileView, activeTab, editor, type EditorTab } from "@/state/editor";
import { registerCommand } from "@/state/commands";
import { registerKeymapActions } from "@/keymap/store";
import { registerSettingsPage } from "@/features/settingsPages";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { onFsChanged } from "@/state/fsWatch";
import { getSetting } from "@/state/settings";
import { viewersApi } from "@/ipc/viewers";
import { TextEditorWithPreview, type PreviewLayout } from "@/editor/TextEditorWithPreview";
import { ImageCanvas, imageControls, type ImageControls } from "./ImageCanvas";
import "./images.css";
import { t } from "@/i18n";

export const IMAGE_VIEW = "image";

const RASTER = /\.(png|jpe?g|gif|webp|ico|bmp)$/i;
export const isRasterImage = (path: string) => RASTER.test(path) && !path.includes("://");
export const isSvg = (path: string) => /\.svg$/i.test(path) && !path.includes("://");

registerBinaryFileView({ view: IMAGE_VIEW, accepts: isRasterImage });

/** A raster image of the project, reloaded when the file changes on disk. */
function ImageEditor(props: { tab: EditorTab }) {
  const [version, setVersion] = createSignal(0);
  const [image] = createResource(
    () => [props.tab.path, version()] as const,
    async ([path]) => {
      try {
        const img = await viewersApi.imageRead(path);
        return { src: `data:${img.info.mime};base64,${img.base64}`, info: img.info, error: undefined };
      } catch (e) {
        return { src: undefined, info: undefined, error: e instanceof Error ? e.message : String(e) };
      }
    },
  );
  onCleanup(onFsChanged((c) => c.changed.includes(props.tab.path) && setVersion((v) => v + 1)));
  return <ImageCanvas path={props.tab.path} src={image()?.src} info={image()?.info} error={image()?.error} />;
}

registerEditorView({
  id: IMAGE_VIEW,
  component: ImageEditor,
  breadcrumbs: (tab) => tab.path.split("/"),
});

/** SVG text as an image (live while typing). */
function svgDataUrl(text: string): string {
  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(text)}`;
}

registerFileEditor({
  id: "svg",
  accepts: isSvg,
  component: (props) => (
    <TextEditorWithPreview
      tab={props.tab}
      group={props.group}
      defaultLayout={() => getSetting<PreviewLayout>("images.svgDefaultLayout", "split")}
      preview={() => (
        <ImageCanvas path={props.tab.path} src={svgDataUrl(props.tab.content)} size={new TextEncoder().encode(props.tab.content).length} />
      )}
    />
  ),
});

// ---- Actions (PHPStorm's image editor ids and default shortcuts) -------------------------

const ACTIONS: { id: string; title: string; run: (c: ImageControls) => void; win?: string[]; mac?: string[] }[] = [
  { id: "Images.Editor.ZoomIn", title: "Zoom In", run: (c) => c.zoomIn(), win: ["Ctrl+=", "Ctrl+NumPad+"], mac: ["Cmd+=", "Cmd+NumPad+"] },
  { id: "Images.Editor.ZoomOut", title: "Zoom Out", run: (c) => c.zoomOut(), win: ["Ctrl+-", "Ctrl+NumPad-"], mac: ["Cmd+-", "Cmd+NumPad-"] },
  { id: "Images.Editor.ActualSize", title: "Actual Size", run: (c) => c.actualSize(), win: ["Ctrl+/", "Ctrl+NumPad/"], mac: ["Cmd+/", "Cmd+NumPad/"] },
  { id: "Images.Editor.FitZoomToWindow", title: "Fit Zoom to Window", run: (c) => c.fit() },
  { id: "Images.Editor.ToggleGrid", title: "Show Grid", run: (c) => c.toggleGrid() },
  { id: "Images.ToggleTransparencyChessboard", title: "Show Chessboard", run: (c) => c.toggleChessboard() },
];

registerKeymapActions(
  ACTIONS.map((a) => ({
    id: a.id,
    title: a.title,
    group: "Images",
    shortcuts: a.win ? { $default: a.win, macOS: a.mac } : undefined,
  })),
);

/** The image editor of the active tab (a raster image, or an SVG with its preview). */
const activeImage = (): ImageControls | undefined => {
  const path = activeTab()?.path ?? editor.active;
  return path ? imageControls.get(path) : undefined;
};

for (const a of ACTIONS) {
  registerCommand({
    id: a.id,
    title: a.title,
    category: "Images",
    run: () => {
      const c = activeImage();
      if (c) a.run(c);
    },
  });
}

registerSettingsPage({
  id: "editor.image",
  title: t("settings.page.editor.image"),
  group: "Editor",
  order: 60,
  keywords: ["grid", "chessboard", "transparency", "svg", "zoom"],
  component: () => <SchemaPage pageId="editor.image" />,
});
