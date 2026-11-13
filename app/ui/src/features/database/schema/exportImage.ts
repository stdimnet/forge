/**
 * Export an ER diagram as SVG or PNG. The diagram is drawn with theme variables, so the
 * exported copy gets the computed colours and fonts inlined; PNGs are rendered from that
 * SVG at twice the size. The desktop app saves through a save dialog, the browser downloads.
 */
import { isTauri } from "@/ipc";
import { dbSchemaApi } from "@/ipc/dbSchema";
import { balloonsFor } from "@/keymap";
import { t } from "@/i18n";

const showBalloon = balloonsFor("Database");

const STYLE_PROPS = ["fill", "stroke", "stroke-width", "stroke-dasharray", "color", "opacity", "font-family", "font-size", "font-weight", "text-anchor", "dominant-baseline"];

/** A standalone copy of the diagram at 1:1 with its styles inlined. */
export function standaloneSvg(svg: SVGSVGElement, width: number, height: number): string {
  const clone = svg.cloneNode(true) as SVGSVGElement;
  const src = [svg, ...svg.querySelectorAll("*")];
  const dst = [clone, ...clone.querySelectorAll("*")];
  src.forEach((el, i) => {
    const cs = getComputedStyle(el);
    const target = dst[i] as SVGElement;
    for (const p of STYLE_PROPS) {
      const v = cs.getPropertyValue(p);
      if (v) target.style.setProperty(p, v);
    }
    target.removeAttribute("class");
  });
  clone.setAttribute("xmlns", "http://www.w3.org/2000/svg");
  clone.setAttribute("width", String(width));
  clone.setAttribute("height", String(height));
  clone.setAttribute("viewBox", `0 0 ${width} ${height}`);
  clone.removeAttribute("style");
  const view = clone.querySelector("[data-erd-viewport]");
  view?.setAttribute("transform", "");
  const bg = document.createElementNS("http://www.w3.org/2000/svg", "rect");
  bg.setAttribute("width", "100%");
  bg.setAttribute("height", "100%");
  bg.setAttribute("fill", getComputedStyle(svg.parentElement ?? svg).backgroundColor || "#ffffff");
  clone.insertBefore(bg, clone.firstChild);
  return `<?xml version="1.0" encoding="UTF-8"?>\n${new XMLSerializer().serializeToString(clone)}`;
}

function pngOf(svgText: string, width: number, height: number): Promise<string> {
  return new Promise((resolve, reject) => {
    const url = URL.createObjectURL(new Blob([svgText], { type: "image/svg+xml" }));
    const img = new Image();
    img.onload = () => {
      const scale = 2;
      const canvas = document.createElement("canvas");
      canvas.width = Math.ceil(width * scale);
      canvas.height = Math.ceil(height * scale);
      const ctx = canvas.getContext("2d");
      if (!ctx) return reject(new Error("no canvas"));
      ctx.scale(scale, scale);
      ctx.drawImage(img, 0, 0);
      URL.revokeObjectURL(url);
      resolve(canvas.toDataURL("image/png"));
    };
    img.onerror = () => {
      URL.revokeObjectURL(url);
      reject(new Error("The diagram could not be rendered"));
    };
    img.src = url;
  });
}

function download(href: string, name: string): void {
  const a = document.createElement("a");
  a.href = href;
  a.download = name;
  a.click();
}

export async function exportDiagram(svg: SVGSVGElement, size: { width: number; height: number }, base: string, format: "svg" | "png"): Promise<void> {
  const name = `${base}.${format}`;
  try {
    const text = standaloneSvg(svg, size.width, size.height);
    const png = format === "png" ? await pngOf(text, size.width, size.height) : null;
    if (isTauri()) {
      const { save } = await import("@tauri-apps/plugin-dialog");
      const path = await save({ defaultPath: name, filters: [{ name: format.toUpperCase(), extensions: [format] }] });
      if (!path) return;
      await dbSchemaApi.saveExport(path, png ? { base64: png.slice(png.indexOf(",") + 1) } : { text });
      showBalloon(t("database.schema.exportImage.diagramExported"), path);
      return;
    }
    if (png) download(png, name);
    else {
      const url = URL.createObjectURL(new Blob([text], { type: "image/svg+xml" }));
      download(url, name);
      URL.revokeObjectURL(url);
    }
    showBalloon(t("database.schema.exportImage.diagramExported"), name);
  } catch (e) {
    showBalloon(t("database.schema.exportImage.exportFailed"), e instanceof Error ? e.message : String(e));
  }
}
