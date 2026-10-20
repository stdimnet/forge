import { registerMock } from "@/ipc";
import type { FileEntry } from "@/ipc/workspace";
import { demoFiles, demoProjectName } from "./demoProject";
import { demoConfigFiles } from "./demoConfigFiles";
import { recentPrimaries } from "./projectMock";
import { isSymfonyDemo, symfonyDemoFiles, symfonyDemoName } from "./demoSymfonyProject";

// `?project=symfony` opens the Symfony demo project instead of the Laravel one.
const files: Record<string, string> = isSymfonyDemo() ? { ...symfonyDemoFiles } : { ...demoFiles, ...demoConfigFiles };
const openName = isSymfonyDemo() ? symfonyDemoName : demoProjectName;

function listDir(dir: string): FileEntry[] {
  const prefix = dir ? `${dir}/` : "";
  const seen = new Map<string, FileEntry>();
  for (const path of Object.keys(files)) {
    if (!path.startsWith(prefix)) continue;
    const rest = path.slice(prefix.length);
    const [head, ...tail] = rest.split("/");
    const full = prefix + head;
    if (!seen.has(full)) seen.set(full, { name: head, path: full, kind: tail.length ? "dir" : "file" });
  }
  return [...seen.values()].sort(
    (a, b) =>
      Number(a.kind !== "dir") - Number(b.kind !== "dir") ||
      a.name.toLowerCase().localeCompare(b.name.toLowerCase()),
  );
}

registerMock("workspace_current", () => null);
registerMock("workspace_recent", () =>
  [
    { name: demoProjectName, root: `~/Projects/${demoProjectName}` },
    { name: "symfony-api", root: "~/Projects/symfony-api" },
    { name: "storefront-ui", root: "~/Projects/storefront-ui" },
    { name: "pixel-dungeon", root: "~/Games/pixel-dungeon" },
    { name: "dotfiles.nvim", root: "~/.config/nvim" },
    { name: "legacy-crm", root: "~/Work/legacy-crm" },
  ].map((p) => ({ ...p, primary: recentPrimaries[p.name] })),
);
registerMock("workspace_open", () => ({ name: openName, root: `~/Projects/${openName}` }));
registerMock("fs_list_dir", ({ path }) => listDir(String(path ?? "")));
registerMock("fs_read_file", ({ path }) => {
  const content = files[String(path)];
  if (content === undefined) throw new Error(`not found: ${path}`);
  return content;
});
registerMock("fs_write_file", ({ path, contents }) => {
  files[String(path)] = String(contents);
});
