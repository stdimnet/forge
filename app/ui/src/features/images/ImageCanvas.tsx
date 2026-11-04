/**
 * The image editor's canvas and toolbar, like PHPStorm's: zoom in/out, actual size, fit to
 * window, grid and transparency chessboard, and the image info ("128x128 PNG (32-bit
 * color) 484 B"). Used for raster images (a view tab) and as the preview of SVG files.
 */
import { createEffect, createSignal, on, onCleanup, onMount, Show } from "solid-js";
import { Grid2x2, Grid3x3, Maximize, ZoomIn, ZoomOut } from "lucide-solid";
import type { ImageInfo } from "@/ipc/viewers";
import { getSetting } from "@/state/settings";
import { shortcutLabel } from "@/keymap";
import { formatImageInfo, nextZoom, fitZoom } from "./zoom";
import { t } from "@/i18n";

export interface ImageControls {
  zoomIn: () => void;
  zoomOut: () => void;
  actualSize: () => void;
  fit: () => void;
  toggleGrid: () => void;
  toggleChessboard: () => void;
}

/** Image editors by file path, for the keymap actions on the active tab. */
export const imageControls = new Map<string, ImageControls>();

/** A toolbar button title with its shortcut, like "Zoom In (Ctrl+=)". */
function titled(title: string, action: string): string {
  const key = shortcutLabel(action);
  return key ? `${title} (${key})` : title;
}

export function ImageCanvas(props: {
  path: string;
  /** Data URL of the image (undefined while loading). */
  src: string | undefined;
  /** Info from Rust; SVG previews measure the rendered image instead. */
  info?: ImageInfo;
  error?: string;
  /** Size in bytes when there is no info (SVG text). */
  size?: number;
}) {
  const [zoom, setZoom] = createSignal(1);
  const [grid, setGrid] = createSignal(getSetting<boolean>("images.showGrid", false) === true);
  const [chessboard, setChessboard] = createSignal(getSetting<boolean>("images.showChessboard", true) !== false);
  const [natural, setNatural] = createSignal<{ w: number; h: number } | null>(null);
  /** The user zoomed: keep the zoom when the image changes (SVG typing). */
  let userZoomed = false;
  let viewport!: HTMLDivElement;

  const width = () => props.info?.width ?? natural()?.w ?? 0;
  const height = () => props.info?.height ?? natural()?.h ?? 0;

  const fit = () => {
    if (!width() || !height()) return;
    setZoom(fitZoom(width(), height(), viewport.clientWidth - 32, viewport.clientHeight - 32));
  };
  /** Initial zoom: 100%, or smaller when the image does not fit (PHPStorm's default). */
  const autoZoom = () => {
    if (userZoomed || !width() || !viewport.clientWidth) return;
    setZoom(Math.min(1, fitZoom(width(), height(), viewport.clientWidth - 32, viewport.clientHeight - 32)));
  };
  createEffect(on([width, height], autoZoom, { defer: true }));

  const controls: ImageControls = {
    zoomIn: () => ((userZoomed = true), setZoom((z) => nextZoom(z, 1))),
    zoomOut: () => ((userZoomed = true), setZoom((z) => nextZoom(z, -1))),
    actualSize: () => ((userZoomed = true), setZoom(1)),
    fit: () => ((userZoomed = true), fit()),
    toggleGrid: () => setGrid((g) => !g),
    toggleChessboard: () => setChessboard((c) => !c),
  };
  onMount(() => {
    imageControls.set(props.path, controls);
    const ro = new ResizeObserver(() => autoZoom());
    ro.observe(viewport);
    onCleanup(() => ro.disconnect());
  });
  onCleanup(() => {
    if (imageControls.get(props.path) === controls) imageControls.delete(props.path);
  });

  const onWheel = (e: WheelEvent) => {
    if (!e.ctrlKey && !e.metaKey) return;
    e.preventDefault();
    if (e.deltaY < 0) controls.zoomIn();
    else controls.zoomOut();
  };

  const info = (): string => {
    if (props.info) return formatImageInfo(props.info);
    const n = natural();
    return n ? formatImageInfo({ format: "SVG", mime: "image/svg+xml", width: n.w, height: n.h, size: props.size ?? 0 }) : "";
  };

  return (
    <div class="img-editor">
      <div class="img-toolbar" role="toolbar" aria-label={t("tools.images.image")}>
        <button class="img-btn" title={titled("Zoom In", "Images.Editor.ZoomIn")} onClick={controls.zoomIn}>
          <ZoomIn size={15} />
        </button>
        <button class="img-btn" title={titled("Zoom Out", "Images.Editor.ZoomOut")} onClick={controls.zoomOut}>
          <ZoomOut size={15} />
        </button>
        <button class="img-btn img-btn-text" title={titled("Actual Size", "Images.Editor.ActualSize")} onClick={controls.actualSize}>
          1:1
        </button>
        <button class="img-btn" title={titled("Fit Zoom to Window", "Images.Editor.FitZoomToWindow")} onClick={controls.fit}>
          <Maximize size={14} />
        </button>
        <span class="img-sep" />
        <button
          class="img-btn"
          classList={{ active: grid() }}
          aria-pressed={grid()}
          title={titled("Show Grid", "Images.Editor.ToggleGrid")}
          onClick={controls.toggleGrid}
        >
          <Grid3x3 size={15} />
        </button>
        <button
          class="img-btn"
          classList={{ active: chessboard() }}
          aria-pressed={chessboard()}
          title={titled("Show Chessboard", "Images.ToggleTransparencyChessboard")}
          onClick={controls.toggleChessboard}
        >
          <Grid2x2 size={15} />
        </button>
        <span class="img-zoom">{Math.round(zoom() * 100)}%</span>
        <span class="img-info">{info()}</span>
      </div>
      <div class="img-viewport" ref={viewport} tabIndex={0} onWheel={onWheel} data-image-path={props.path}>
        <Show when={!props.error} fallback={<div class="img-error">{props.error}</div>}>
          <Show when={props.src}>
            {(src) => (
              <div
                class="img-frame"
                classList={{ chessboard: chessboard() }}
                style={{
                  width: width() ? `${width() * zoom()}px` : undefined,
                  height: height() ? `${height() * zoom()}px` : undefined,
                }}
              >
                <img
                  class="img-image"
                  classList={{ pixelated: zoom() > 1 && props.info?.format !== "SVG" }}
                  src={src()}
                  alt=""
                  draggable={false}
                  onLoad={(e) => {
                    const img = e.currentTarget;
                    setNatural({ w: img.naturalWidth || 300, h: img.naturalHeight || 150 });
                  }}
                />
                <Show when={grid() && zoom() >= 3}>
                  <div class="img-grid" style={{ "background-size": `${zoom()}px ${zoom()}px` }} />
                </Show>
              </div>
            )}
          </Show>
        </Show>
      </div>
    </div>
  );
}
