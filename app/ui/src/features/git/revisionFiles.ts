/**
 * Files as they were in a revision (`gitrev://<sha>/<path>`): Annotate Revision opens one in
 * a read-only editor tab with the blame of that revision.
 */
import { EditorState, type Extension } from "@codemirror/state";
import { showPanel, type Panel } from "@codemirror/view";
import { History } from "lucide-solid";
import { registerEditorExtension } from "@/editor/extensions";
import { registerFileSystem } from "@/state/editor";
import { h, iconNode } from "@/features/lsp/util";
import { gitApi } from "./api";

export const REVISION_PREFIX = "gitrev://";

/** Editor path of `path` as it was in `sha`. */
export const revisionPath = (sha: string, path: string) => `${REVISION_PREFIX}${sha}/${path}`;

/** `{ sha, path }` of a revision path, `null` for other paths. */
export function parseRevisionPath(path: string): { sha: string; path: string } | null {
  if (!path.startsWith(REVISION_PREFIX)) return null;
  const rest = path.slice(REVISION_PREFIX.length);
  const slash = rest.indexOf("/");
  return slash > 0 ? { sha: rest.slice(0, slash), path: rest.slice(slash + 1) } : null;
}

registerFileSystem({
  prefix: REVISION_PREFIX,
  async read(path) {
    const rev = parseRevisionPath(path);
    if (!rev) throw new Error(`not a revision path: ${path}`);
    const text = await gitApi.show(rev.sha, rev.path);
    if (text === null) throw new Error(`${rev.path} does not exist in ${rev.sha.slice(0, 8)}`);
    return text;
  },
  write: async () => {
    throw new Error("Files of past revisions are read-only");
  },
  breadcrumbs: (path) => {
    const rev = parseRevisionPath(path);
    return rev ? [`Revision ${rev.sha.slice(0, 8)}`, ...rev.path.split("/")] : [path];
  },
});

function revisionEditor(sha: string, path: string): Extension {
  const banner = showPanel.of((): Panel => {
    const dom = h(
      "div",
      "library-banner",
      iconNode(History, 13, "library-banner-icon"),
      h("strong", undefined, `Revision ${sha.slice(0, 8)}`),
      h("span", "library-banner-text", `${path} · read-only`),
    );
    return { dom, top: true };
  });
  return [EditorState.readOnly.of(true), banner];
}

registerEditorExtension(({ path }) => {
  const rev = parseRevisionPath(path);
  return rev ? revisionEditor(rev.sha, rev.path) : null;
});
