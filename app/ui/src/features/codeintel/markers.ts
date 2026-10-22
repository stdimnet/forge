/**
 * PHPStorm's implemented / overridden gutter icons for PHP: ↓ on classes and interfaces
 * with subclasses or implementations and on methods overridden below, ↑ on methods that
 * implement or override one above. Computed in Rust from the PHP index
 * (`code_line_markers`); clicking one opens the target or the Choose Declaration list.
 */
import { CircleArrowDown, CircleArrowUp } from "lucide-solid";
import { registerLineMarkerProvider, type LineMarker } from "@/editor/lineMarkers";
import { codeApi, isLibraryPath, type LineMarkerInfo } from "@/ipc/codeintel";
import { isPluginActive } from "@/state/project";
import { getSetting } from "@/state/settings";
import { chooseTargetAtPoint } from "./targets";

const isPhp = (path: string) => /\.(php|phtml|inc)$/i.test(path) && !path.endsWith(".blade.php") && !isLibraryPath(path);

/** The Choose Declaration title of a marker, e.g. "Choose Implementation of charge (2 found)". */
export function markerChooserTitle(m: LineMarkerInfo): string {
  const subject = m.targets[0]?.name.split("::")[1]?.replace(/\(\)$/, "");
  const what =
    m.kind === "implements" || m.kind === "overrides"
      ? "Choose Super Method"
      : m.kind === "subclassed"
        ? "Choose Subclass"
        : "Choose Implementation";
  return `${what}${subject && m.kind !== "subclassed" ? ` of ${subject}` : ""} (${m.targets.length} found)`;
}

/** Code Vision text of a marker that points down: "2 implementations", "1 inheritor". */
export function codeVisionText(m: LineMarkerInfo): string | undefined {
  const n = m.targets.length;
  const noun = { subclassed: "inheritor", implemented: "implementation", overridden: "override" }[m.kind as string];
  return noun && n ? `${n} ${noun}${n === 1 ? "" : "s"}` : undefined;
}

export function toLineMarker(m: LineMarkerInfo): LineMarker {
  const up = m.kind === "implements" || m.kind === "overrides";
  return {
    vision: codeVisionText(m),
    line: m.line,
    icon: up ? CircleArrowUp : CircleArrowDown,
    title: m.title,
    className: up ? `marker-super marker-${m.kind}` : `marker-impl marker-${m.kind}`,
    onClick: (_view, e) => chooseTargetAtPoint(e.clientX, e.clientY + 8, m.targets, markerChooserTitle(m)),
  };
}

registerLineMarkerProvider({
  appliesTo: isPhp,
  watch: () => [isPluginActive("forge.php"), getSetting<boolean>("editor.inlayHints.implementations", true)],
  markers: async (path, text) => {
    if (!isPluginActive("forge.php")) return [];
    const markers = await codeApi.lineMarkers(path, text).catch(() => [] as LineMarkerInfo[]);
    return markers.map(toLineMarker);
  },
});
