/**
 * Built-in Search Everywhere providers: project files (backend `fs_find_files`) and
 * actions (Find Action). Classes, symbols and text come from language packs.
 */
import { FileIcon } from "@/shell/FileIcon";
import { activeTab, openFile } from "@/state/editor";
import { openFileAt } from "@/editor/views";
import { workspace } from "@/state/workspace";
import { isTauri } from "@/ipc";
import { searchApi } from "@/ipc/search";
import { allActions, isActionAvailable, runAction, shortcutLabel } from "@/keymap";
import { recentFiles } from "@/features/navigation/history";
import { recencyBoost } from "./recencyBoost";
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
    const hits = await searchApi.findFiles(query, limit * 3);
    const recent = recentFiles();
    const active = activeTab()?.path;
    return hits
      .map((h) => fileItem(h.path, h.score + recencyBoost(h.path, recent, active), h.matchedPath ? undefined : h.positions))
      .sort((a, b) => (b.score ?? 0) - (a.score ?? 0))
      .slice(0, limit);
  },
});

const SYMBOL_KINDS = new Set<string>(["class", "interface", "trait", "enum", "function", "method", "property", "constant"]);

/** Languages whose pack answers Classes and Symbols from an index of its own. */
const PACK_INDEXED = isTauri() ? ["php", "phtml", "lua"] : ["lua"];

function symbolProvider(id: string, tab: "classes" | "symbols", title: string, typesOnly: boolean) {
  registerSearchProvider({
    id,
    tab,
    title,
    order: 2,
    async search(query, { limit }) {
      if (!query.trim()) return [];
      const hits = await searchApi.findSymbols(query, typesOnly, limit, PACK_INDEXED).catch(() => []);
      return hits.map(
        (h): SearchItem => ({
          id: `${h.path}:${h.line}:${h.name}`,
          label: h.name,
          matches: h.matched,
          detail: h.container ?? undefined,
          hint: `${h.path.split("/").pop()} ${h.line}`,
          kind: SYMBOL_KINDS.has(h.kind) ? (h.kind as SearchItem["kind"]) : "function",
          score: h.score,
          run: () => void openFileAt(h.path, h.line, 1),
        }),
      );
    },
  });
}

symbolProvider("core.classes", "classes", "Classes", true);
symbolProvider("core.symbols", "symbols", "Symbols", false);

const groupLabel = (group: string) => group.split("/").slice(1).join(" | ") || group;

registerSearchProvider({
  id: "core.actions",
  tab: "actions",
  title: "Actions",
  order: 0,
  search(query) {
    if (!query.trim()) return [];
    const actions = allActions();
    // A command that implements a PHPStorm action (an alias like `git.commit` for
    // `CheckinProject`) is listed once, under the PHPStorm action.
    const aliasTargets = new Set(actions.flatMap((a) => a.aliases ?? []));
    return actions
      .filter((a) => !a.native && !aliasTargets.has(a.id))
      .map((a): SearchItem => {
        const available = isActionAvailable(a.id);
        return {
          id: a.id,
          label: a.title,
          // Matched by title, else (ranked lower) by id, the way PHPStorm does.
          match: { strict: true, altTexts: [a.id] },
          detail: groupLabel(a.group),
          hint: shortcutLabel(a.id),
          kind: "action",
          // Available actions first, then by match quality.
          score: available ? 5 : 0,
          dimmed: !available,
          run: () => runAction(a.id),
        };
      });
  },
});
