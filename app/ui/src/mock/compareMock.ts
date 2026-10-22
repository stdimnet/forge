/**
 * Browser-mode Compare Directories: both folders are read from the workspace mock and the
 * folder diff is the same Rust code as in the desktop app (`forge-diff`, WebAssembly).
 * Copies go to the in-memory workspace mock only.
 */
import { invoke, registerMock } from "@/ipc";
import { callWasm, forgeWasm } from "@/wasm";
import type { FileEntry } from "@/ipc/workspace";
import type { DirCompare } from "@/features/compare/api";

interface Item {
  path: string;
  dir: boolean;
  text: string;
}

const SKIPPED = new Set([".git", ".hg", ".svn"]);

async function listTree(root: string, rel = ""): Promise<Item[]> {
  const entries = await invoke<FileEntry[]>("fs_list_dir", { path: rel ? `${root}/${rel}`.replace(/^\//, "") : root });
  const out: Item[] = [];
  for (const e of entries) {
    const path = rel ? `${rel}/${e.name}` : e.name;
    if (e.kind === "dir") {
      if (SKIPPED.has(e.name)) continue;
      out.push({ path, dir: true, text: "" });
      out.push(...(await listTree(root, path)));
    } else {
      out.push({ path, dir: false, text: await invoke<string>("fs_read_file", { path: e.path }).catch(() => "") });
    }
  }
  return out;
}

registerMock("compare_dirs", async ({ left, right }) => {
  const w = await forgeWasm();
  const [l, r] = await Promise.all([listTree(String(left)), listTree(String(right))]);
  return callWasm<DirCompare>(w.compare_dir_listings, { left: l, right: r });
});

async function copy(from: string, to: string, dir: boolean): Promise<void> {
  if (!dir) {
    const text = await invoke<string>("fs_read_file", { path: from });
    return invoke<void>("fs_write_file", { path: to, contents: text });
  }
  if (to.startsWith(`${from}/`)) throw new Error(`cannot copy ${from} into ${to}`);
  await invoke<void>("fs_create_dir", { path: to }).catch(() => undefined);
  for (const e of await invoke<FileEntry[]>("fs_list_dir", { path: from })) {
    if (e.kind === "dir" && SKIPPED.has(e.name)) continue;
    await copy(e.path, `${to}/${e.name}`, e.kind === "dir");
  }
}

registerMock("compare_copy", ({ from, to, dir }) => copy(String(from), String(to), !!dir));

// Screenshot scenarios create a second folder to compare (in the in-memory demo project).
if (typeof window !== "undefined") {
  (window as unknown as { __forgeMockWriteFile?: (path: string, contents: string) => Promise<void> }).__forgeMockWriteFile = (path, contents) =>
    invoke<void>("fs_write_file", { path, contents });
}
