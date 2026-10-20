/**
 * Built-in Search Everywhere providers: project files (backend `fs_find_files`) and
 * actions (Find Action). Classes, symbols and text come from language packs.
 */
import { FileIcon } from "@/shell/FileIcon";
import { openFile } from "@/state/editor";
import { workspace } from "@/state/workspace";
import { searchApi } from "@/ipc/search";
import { allActions, isActionAvailable, runAction, shortcutLabel } from "@/keymap";
import { recentFiles } from "@/features/navigation/history";
import { fuzzyMatch } from "./fuzzy";
import { registerSearchProvider, type SearchItem } from "./providers";

const dirOf = (path: string) => path.split("/").slice(0, -1).join("/");

function fileItem(path: string, score: number, matches?: number[]): SearchItem {
  const name = path.split("/").pop() ?? path;
  return {
    id: path,
    label: name,
    matches,
    detail: dirOf(path),
    hint: workspace()?.name,
    kind: "file",
    icon: () => <FileIcon name={name} size={16} />,
    score,
    run: () => openFile(path),
  };
}

registerSearchProvider({
  id: "core.files",
  tab: "files",
  title: "Files",
  order: 0,
  async search(query, { limit }) {
    if (!query.trim()) return recentFiles().slice(0, limit).map((p, i) => fileItem(p, 1000 - i));
    const hits = await searchApi.findFiles(query, limit);
    return hits.map((h) => fileItem(h.path, h.score, h.matchedPath ? undefined : h.positions));
  },
});

const groupLabel = (group: string) => group.split("/").slice(1).join(" | ") || group;

registerSearchProvider({
  id: "core.actions",
  tab: "actions",
  title: "Actions",
  order: 0,
  search(query, { limit }) {
    if (!query.trim()) return [];
    const out: SearchItem[] = [];
    for (const a of allActions()) {
      if (a.native) continue;
      const m = fuzzyMatch(query, a.title.replace(/…$/, ""), { strict: true });
      const byId = m ? null : fuzzyMatch(query, a.id, { strict: true });
      if (!m && !byId) continue;
      const available = isActionAvailable(a.id);
      out.push({
        id: a.id,
        label: a.title,
        matches: m?.positions,
        detail: groupLabel(a.group),
        hint: shortcutLabel(a.id),
        kind: "action",
        // Implemented actions first, then by match quality.
        score: (m?.score ?? (byId!.score - 10)) + (available ? 5 : 0),
        dimmed: !available,
        run: () => runAction(a.id),
      });
    }
    return out.sort((a, b) => b.score - a.score).slice(0, limit);
  },
});
