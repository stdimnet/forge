import { registerMock } from "@/ipc";
import type { FileEntry } from "@/ipc/workspace";
import { callWasmSync, forgeWasm } from "@/wasm";
import { mockSettingValue } from "./settingsMock";
import { demoFiles, demoProjectName } from "./demoProject";
import { demoConfigFiles } from "./demoConfigFiles";
import { demoLaravelFiles } from "./demoLaravel";
import { createIgnoreMatcher } from "./gitignoreMatcher";
import { excludedFlags } from "./projectStructureMock";
import {
  adoptRemoteProject,
  closeRemoteProject,
  isRemoteDemo,
  remoteWorkspace,
  setRemoteProjectFiles,
} from "./remoteMock";
import { trackedPaths } from "./gitMock";
import { recentPrimaries } from "./projectMock";
import { isSymfonyDemo, symfonyDemoFiles, symfonyDemoName } from "./demoSymfonyProject";
import { webDemo } from "./demoWebProjects";
import { isLuaDemo, luaDemoFiles, luaDemoName } from "./demoLuaProject";
import { isPythonDemo, pythonDemoFiles, pythonDemoName } from "./demoPythonProject";
import { goDemoFiles, goDemoName, isGoDemo } from "./demoGoProject";
import { isJavaDemo, javaDemoFiles, javaDemoName } from "./demoJavaProject";
import { isWordPressDemo, wordpressDemoFiles, wordpressDemoName } from "./demoWordPressProject";
import { drupalDemoFiles, drupalDemoName, isDrupalDemo } from "./demoDrupalProject";

/** The in-memory project (demo files plus whatever the UI saved); read by other mocks. */
// `?project=symfony` opens the Symfony demo project instead of the Laravel one,
// `?project=react|vue|angular` a WebForge demo (demoWebProjects.ts), `?project=lua` the LÖVE game,
// `?project=wordpress` a WordPress plugin, `?project=drupal` a Drupal module, `?project=python`
// the FastAPI service of PyForge, `?project=go` the `ledger` module of GoForge.
// the FastAPI service of PyForge, `?project=java` a Gradle Java service.
export const mockFiles: Record<string, string> = isSymfonyDemo()
  ? { ...symfonyDemoFiles }
  : isWordPressDemo()
    ? { ...wordpressDemoFiles }
    : isDrupalDemo()
      ? { ...drupalDemoFiles }
      : isLuaDemo()
    ? { ...luaDemoFiles }
    : isPythonDemo()
    ? { ...pythonDemoFiles }
    : isGoDemo()
    ? { ...goDemoFiles }
    : isJavaDemo()
    ? { ...javaDemoFiles }
    : webDemo()
      ? { ...webDemo()!.files }
      : { ...demoFiles, ...demoLaravelFiles, ...demoConfigFiles };
const files = mockFiles;
/** Name of the open demo project. */
export const mockProjectName = isSymfonyDemo()
  ? symfonyDemoName
  : isWordPressDemo()
    ? wordpressDemoName
    : isDrupalDemo()
      ? drupalDemoName
      : isLuaDemo()
    ? luaDemoName
    : isPythonDemo()
    ? pythonDemoName
    : isGoDemo()
    ? goDemoName
    : isJavaDemo()
    ? javaDemoName
    : (webDemo()?.name ?? demoProjectName);
const openName = mockProjectName;
// The remote demo host serves the same project as `/var/www/shop`.
setRemoteProjectFiles(Object.keys(files));
const ignores = createIgnoreMatcher((dir) => files[dir ? `${dir}/.gitignore` : ".gitignore"]);

/** Ignored means matched by `.gitignore` and not tracked (a file or a folder with tracked files). */
function isIgnored(path: string, isDir: boolean, tracked: string[]): boolean {
  if (!ignores.isIgnored(path, isDir)) return false;
  return !tracked.some((t) => t === path || t.startsWith(`${path}/`));
}

/** Folders created in the UI that have no file yet (the file map only knows folders by their files). */
const emptyDirs = new Set<string>();

function listDir(dir: string): FileEntry[] {
  const tracked = trackedPaths();
  const prefix = dir ? `${dir}/` : "";
  const seen = new Map<string, FileEntry>();
  for (const d of emptyDirs) {
    if (!d.startsWith(prefix)) continue;
    const head = d.slice(prefix.length).split("/")[0];
    const full = prefix + head;
    seen.set(full, { name: head, path: full, kind: "dir", ignored: isIgnored(full, true, tracked) });
  }
  for (const path of Object.keys(files)) {
    if (!path.startsWith(prefix)) continue;
    const rest = path.slice(prefix.length);
    const [head, ...tail] = rest.split("/");
    const full = prefix + head;
    if (seen.has(full)) continue;
    const kind = tail.length ? "dir" : "file";
    seen.set(full, { name: head, path: full, kind, ignored: isIgnored(full, kind === "dir", tracked) });
  }
  const listed = [...seen.values()];
  const flags = excludedFlags(listed.map((e) => [e.path, e.kind === "dir"]));
  listed.forEach((e, i) => flags[i] && (e.excluded = true));
  return listed.sort(
    (a, b) =>
      Number(a.kind !== "dir") - Number(b.kind !== "dir") ||
      a.name.toLowerCase().localeCompare(b.name.toLowerCase()),
  );
}

registerMock("workspace_current", () => null);
registerMock("workspace_recent", () =>
  [
    { name: demoProjectName, root: `~/Projects/${demoProjectName}` },
    { ...remoteWorkspace(), primaryOf: demoProjectName },
    { name: "symfony-api", root: "~/Projects/symfony-api" },
    { name: "storefront-ui", root: "~/Projects/storefront-ui" },
    { name: "pixel-dungeon", root: "~/Games/pixel-dungeon" },
    { name: "shop-api", root: "~/Projects/shop-api" },
    { name: "dotfiles.nvim", root: "~/.config/nvim" },
    { name: "legacy-crm", root: "~/Work/legacy-crm" },
  ].map(({ primaryOf, ...p }: { name: string; root: string; primaryOf?: string }) => ({
    ...p,
    primary: recentPrimaries[primaryOf ?? p.name],
  })),
);
registerMock("workspace_open", () => {
  if (isRemoteDemo()) return adoptRemoteProject(remoteWorkspace());
  closeRemoteProject();
  return { name: openName, root: `~/Projects/${openName}` };
});
/** Settings | Editor | File Types | Ignored Files and Folders, matched by Rust. */
async function withoutIgnored(entries: FileEntry[]): Promise<FileEntry[]> {
  const patterns = mockSettingValue("fileTypes.ignoredFiles");
  if (typeof patterns !== "string" || !entries.length) return entries;
  const hidden = callWasmSync<boolean[]>((await forgeWasm()).filetypes_ignored, { names: entries.map((e) => e.name), patterns });
  return entries.filter((_, i) => !hidden[i]);
}

registerMock("fs_list_dir", ({ path }) => withoutIgnored(listDir(String(path ?? ""))));
registerMock("fs_read_file", ({ path }) => {
  const content = files[String(path)];
  if (content === undefined) throw new Error(`not found: ${path}`);
  return content;
});
registerMock("fs_write_file", ({ path, contents }) => {
  files[String(path)] = String(contents);
});

const exists = (path: string) =>
  path in files || [...emptyDirs, ...Object.keys(files)].some((p) => p === path || p.startsWith(`${path}/`));

registerMock("fs_create_file", ({ path }) => {
  const p = String(path);
  if (exists(p)) throw new Error(`${p} already exists`);
  files[p] = "";
});
registerMock("fs_create_dir", ({ path }) => {
  const p = String(path);
  if (exists(p)) throw new Error(`${p} already exists`);
  emptyDirs.add(p);
});
registerMock("fs_rename", ({ from, to }) => {
  const src = String(from);
  const dst = String(to);
  if (!exists(src)) throw new Error(`not found: ${src}`);
  if (exists(dst)) throw new Error(`${dst} already exists`);
  if (dst.startsWith(`${src}/`)) throw new Error(`cannot move ${src} into itself`);
  const move = (path: string) => (path === src ? dst : `${dst}${path.slice(src.length)}`);
  const under = (path: string) => path === src || path.startsWith(`${src}/`);
  for (const f of Object.keys(files).filter(under)) {
    files[move(f)] = files[f];
    delete files[f];
  }
  for (const d of [...emptyDirs].filter(under)) {
    emptyDirs.delete(d);
    emptyDirs.add(move(d));
  }
});
registerMock("fs_delete", ({ path }) => {
  const p = String(path);
  if (!exists(p)) throw new Error(`not found: ${p}`);
  for (const f of Object.keys(files)) if (f === p || f.startsWith(`${p}/`)) delete files[f];
  for (const d of [...emptyDirs]) if (d === p || d.startsWith(`${p}/`)) emptyDirs.delete(d);
});
