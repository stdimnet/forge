/**
 * The dev container icon in the gutter of `devcontainer.json` (`.devcontainer/…`,
 * `.devcontainer.json`): "Create Dev Container and Mount Sources…". The line comes from Rust
 * (`devcontainer_gutter`).
 */
import { Box, Hammer } from "lucide-solid";
import { registerLineMarkerProvider } from "@/editor/lineMarkers";
import { openMenu } from "@/features/run/PopupMenu";
import { devcontainerApi } from "@/ipc/devenv";
import { workspace } from "@/state/workspace";
import { createDevContainer } from "./store";

const CONFIG = /(^|[\\/])(\.devcontainer[\\/]([^\\/]+[\\/])?devcontainer\.json|\.devcontainer\.json)$/;

/** An editor path as an absolute local path. */
export function absolutePath(root: string, path: string): string {
  if (/^([\\/]|[A-Za-z]:|~)/.test(path)) return path;
  const sep = root.includes("\\") && !root.includes("/") ? "\\" : "/";
  return `${root.replace(/[\\/]+$/, "")}${sep}${path.replace(/[\\/]/g, sep)}`;
}

registerLineMarkerProvider({
  appliesTo: (path) => CONFIG.test(path),
  watch: () => workspace()?.root,
  markers: async (path, text) => {
    const line = await devcontainerApi.gutter(path, text);
    const ws = workspace();
    if (line == null || !ws || ws.remote) return [];
    const configPath = absolutePath(ws.root, path);
    return [
      {
        line,
        icon: Box,
        title: "Create Dev Container and Mount Sources…",
        className: "marker-run",
        onClick: (_view, e) =>
          openMenu({
            x: e.clientX,
            y: e.clientY + 8,
            items: [
              {
                label: "Create Dev Container and Mount Sources…",
                icon: Box,
                run: () => createDevContainer({ folder: ws.root, configPath }),
              },
              {
                label: "Rebuild Dev Container",
                icon: Hammer,
                run: () => createDevContainer({ folder: ws.root, configPath, rebuild: true }),
              },
            ],
          }),
      },
    ];
  },
});
