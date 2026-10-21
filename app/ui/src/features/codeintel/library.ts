/**
 * Read-only library files: PHP's own declarations in the PHP stubs
 * (`library://php-stubs/Core/Core_c.php`, PHPStorm's "PHP Runtime") and files outside the
 * project a language server pointed to (`library://file/<absolute path>`). They open in a
 * normal editor tab that cannot be edited, with a banner saying where the file comes from
 * and breadcrumbs like `PHP Runtime › Core › Core_c.php › Throwable`.
 */
import { createSignal } from "solid-js";
import { EditorState, type Extension } from "@codemirror/state";
import { showPanel, type Panel } from "@codemirror/view";
import { Lock } from "lucide-solid";
import { registerEditorExtension } from "@/editor/extensions";
import { registerFileSystem } from "@/state/editor";
import { codeApi, isLibraryPath, LIBRARY_FILE_PREFIX, STUBS_PREFIX, type LibraryFile } from "@/ipc/codeintel";
import { h, iconNode } from "@/features/lsp/util";

/** What the banner says about each opened library file. */
const meta = new Map<string, Pick<LibraryFile, "library" | "detail">>();

/** The symbol navigation last opened in each library file (the breadcrumbs' last part). */
const [symbols, setSymbols] = createSignal<Record<string, string>>({});

export function noteLibrarySymbol(path: string, name: string | undefined): void {
  if (name && isLibraryPath(path)) setSymbols((s) => ({ ...s, [path]: name.replace(/\(\)$/, "") }));
}

/** Breadcrumbs of a library path. */
export function libraryCrumbs(path: string): string[] {
  const symbol = symbols()[path];
  let parts: string[];
  if (path.startsWith(STUBS_PREFIX)) {
    parts = ["PHP Runtime", ...path.slice(STUBS_PREFIX.length).split("/")];
  } else if (path.startsWith(LIBRARY_FILE_PREFIX)) {
    const segs = path.slice(LIBRARY_FILE_PREFIX.length).split("/").filter(Boolean);
    parts = ["External Libraries", ...segs.slice(-2)];
  } else {
    parts = [path];
  }
  return symbol ? [...parts, symbol] : parts;
}

registerFileSystem({
  prefix: "library://",
  async read(path) {
    const file = await codeApi.readLibrary(path);
    meta.set(path, { library: file.library, detail: file.detail });
    return file.text;
  },
  write: async () => {
    throw new Error("Library files are read-only");
  },
  breadcrumbs: libraryCrumbs,
});

/** The read-only state and the banner above the editor. */
function libraryEditor(path: string): Extension {
  const banner = showPanel.of((): Panel => {
    const info = meta.get(path);
    const dom = h(
      "div",
      "library-banner",
      iconNode(Lock, 13, "library-banner-icon"),
      h("strong", undefined, "Read-only library file"),
      h("span", "library-banner-text", info ? `${info.library}${info.detail ? ` · ${info.detail}` : ""}` : ""),
    );
    dom.title = "Files of libraries and PHP stubs cannot be edited";
    return { dom, top: true };
  });
  return [EditorState.readOnly.of(true), banner];
}

registerEditorExtension(({ path }) => (isLibraryPath(path) ? libraryEditor(path) : null));
