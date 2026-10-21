/**
 * LuaForge in the UI. Everything is computed by the Rust Lua index (`luaforge-lang-lua`,
 * `src/ipc/lua.ts`); this module only plugs it into the contribution points:
 *
 * - Structure tool window (Alt+7) for Lua files,
 * - Search Everywhere: Classes (`---@class`, aliases, modules) and Symbols,
 * - Go to Declaration (Ctrl+B / Ctrl+Click) for locals, globals, fields and `require`,
 * - hover documentation from EmmyLua/LuaLS annotations (a running lua-language-server wins),
 * - gutter ▶ on main chunks and the "Lua Script" run configuration type,
 * - Reformat Code (Ctrl+Alt+L) through StyLua when it is installed,
 * - Settings | Languages & Frameworks | Lua.
 */
import { createEffect, createRoot, on } from "solid-js";
import { isDialogWindow } from "@/shell/window/mode";
import { hoverTooltip, type EditorView } from "@codemirror/view";
import { Moon, Play, Settings2 } from "lucide-solid";
import { luaApi, type LuaRunMarker } from "@/ipc/lua";
import { registerEditorExtension } from "@/editor/extensions";
import { registerLineMarkerProvider } from "@/editor/lineMarkers";
import { openFileAt } from "@/editor/views";
import { shortcutLabel, showBalloon } from "@/keymap";
import { registerSettingsPage } from "@/features/settingsPages";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { registerSearchProvider, type SearchItem } from "@/features/search/providers";
import { registerDeclarationProvider } from "@/features/lsp/goto";
import { chooseDeclarationAt } from "@/features/navigation/declarationChooser";
import { registerFormatProvider } from "@/features/lsp/format";
import { lspClaimsHover } from "@/features/lsp/hover";
import { buildHoverDom } from "@/features/lsp/hoverDoc";
import { registerStructureProvider } from "@/features/php/StructureView";
import { SymbolIcon } from "@/features/php/SymbolIcon";
import { baseName, registerRunConfigurationType } from "@/features/run/types";
import { useTemporary } from "@/features/run/configStore";
import { launch } from "@/features/run/runStore";
import { openMenu } from "@/features/run/PopupMenu";
import { openRunConfigurations } from "@/features/run/dialogState";
import { commands, registerCommand } from "@/state/commands";
import { activeTab } from "@/state/editor";
import { onFsChanged } from "@/state/fsWatch";
import { isPluginActive } from "@/state/project";
import { getSetting } from "@/state/settings";
import { workspace } from "@/state/workspace";
import { hitDetail, isLuaPath, markerConfiguration, SEARCH_KIND, STRUCTURE_KIND, toStructureNode } from "./model";

const active = () => isPluginActive("forge.lua");

// ---- Settings ---------------------------------------------------------------------------

registerSettingsPage({
  id: "lang.lua",
  title: "Lua",
  group: "Languages & Frameworks",
  order: 30,
  keywords: ["lua", "love", "löve", "neovim", "luajit", "lua-language-server", "luals", "stylua", "require", "package.path", "interpreter"],
  component: () => <SchemaPage pageId="lang.lua" />,
});

// ---- Index ------------------------------------------------------------------------------

async function reindex(): Promise<void> {
  try {
    await luaApi.indexWorkspace();
  } catch (e) {
    showBalloon("Lua indexing failed", String(e));
  }
}

registerCommand({ id: "lua.reindex", title: "Reindex Lua Files", category: "Lua", run: () => void reindex() });

if (typeof document !== "undefined" && !isDialogWindow()) {
  createRoot(() => {
    createEffect(
      on(
        () => !!workspace() && active(),
        (on) => {
          if (on) void reindex();
        },
      ),
    );
  });
  onFsChanged((c) => {
    const lua = (paths: string[]) => paths.filter(isLuaPath);
    const changed = lua(c.changed);
    const removed = lua(c.removed);
    if (active() && (changed.length || removed.length)) void luaApi.filesChanged(changed, removed).catch(() => undefined);
  });
}

// ---- Structure --------------------------------------------------------------------------

registerStructureProvider({
  appliesTo: (path) => isLuaPath(path) && active(),
  load: async (path, content) => {
    const s = await luaApi.fileStructure(path, content);
    return s && { path: s.path, nodes: s.nodes.map(toStructureNode) };
  },
  reveal: (path, node) => void openFileAt(path, node.line + 1, (node.column ?? 0) + 1),
});

// ---- Search Everywhere ------------------------------------------------------------------

async function search(kind: "class" | "symbol", query: string, limit: number): Promise<SearchItem[]> {
  if (!query.trim() || !active()) return [];
  const hits = await luaApi.searchSymbols(query, kind, limit);
  return hits.map((hit) => ({
    id: `lua:${hit.path}:${hit.line}:${hit.name}`,
    label: hit.name,
    matches: hit.positions,
    detail: hitDetail(hit),
    hint: hit.path,
    kind: SEARCH_KIND[hit.kind],
    icon: () => <SymbolIcon kind={STRUCTURE_KIND[hit.kind]} />,
    score: hit.score,
    run: () => void openFileAt(hit.path, hit.line + 1, hit.column + 1),
  }));
}

registerSearchProvider({ id: "lua.classes", tab: "classes", title: "Classes", order: 1, search: (q, o) => search("class", q, o.limit) });
registerSearchProvider({ id: "lua.symbols", tab: "symbols", title: "Symbols", order: 1, search: (q, o) => search("symbol", q, o.limit) });

// ---- Go to Declaration ------------------------------------------------------------------

registerDeclarationProvider(async (path, view, pos) => {
  if (!isLuaPath(path) || !active()) return false;
  const targets = await luaApi.goto(path, pos, view.state.doc.toString());
  if (!targets.length) return false;
  // Several declarations (a global assigned in several files): PHPStorm's chooser.
  chooseDeclarationAt(view, pos, targets, {
    open: (t) => void openFileAt(t.path, t.line + 1, (t.column ?? 0) + 1, { flash: t.path !== path }),
  });
  return true;
});

// ---- Hover ------------------------------------------------------------------------------

function luaHover(path: string) {
  return hoverTooltip(
    async (view: EditorView, pos: number) => {
      if (!active()) return null;
      // A running lua-language-server answers instead (one tooltip only).
      if (await lspClaimsHover(path, view, pos)) return null;
      const hover = await luaApi.hover(path, pos, view.state.doc.toString()).catch(() => null);
      if (!hover) return null;
      return {
        pos: hover.start,
        end: hover.end,
        above: false,
        create: () => ({ dom: buildHoverDom(hover.doc, [], hover.language) }),
      };
    },
    { hoverTime: 450 },
  );
}

registerEditorExtension(({ path }) => (isLuaPath(path) ? luaHover(path) : null));

// ---- Run --------------------------------------------------------------------------------

registerRunConfigurationType({
  id: "lua-script",
  name: "Lua Script",
  description: "Run a Lua file, or a LÖVE game directory with love.",
  icon: Moon,
  color: "var(--info)",
  pluginId: "forge.lua",
  interpreter: "custom",
  defaultInterpreter: () => getSetting<string>("lua.interpreter", "lua") || "lua",
  argumentsLabel: "Script arguments",
  fields: [{ key: "file", label: "File", kind: "path", required: true, placeholder: "main.lua (or . for a LÖVE game)" }],
  suggestName: (cfg) => {
    const file = cfg.options?.file;
    return file === "." ? workspace()?.name : baseName(file)?.replace(/\.lua$/, "");
  },
});

function runMarker(marker: LuaRunMarker): void {
  void launch(useTemporary(markerConfiguration(marker)));
}

registerLineMarkerProvider({
  appliesTo: (path) => path.endsWith(".lua"),
  watch: active,
  markers: async (path, text) => {
    if (!active()) return [];
    const markers = await luaApi.runMarkers(path, text).catch(() => [] as LuaRunMarker[]);
    return markers.map((m) => ({
      line: m.line,
      icon: Play,
      title: `Run '${m.name}'`,
      className: "marker-run",
      onClick: (_view, e) =>
        openMenu({
          x: e.clientX,
          y: e.clientY + 6,
          items: [
            {
              label: `Run '${m.name}'`,
              icon: Play,
              iconColor: "var(--success)",
              shortcut: shortcutLabel("RunClass"),
              run: () => runMarker(m),
            },
            { separator: true },
            {
              label: "Modify Run Configuration…",
              icon: Settings2,
              run: () => openRunConfigurations(useTemporary(markerConfiguration(m)).name),
            },
          ],
        }),
    }));
  },
});

// Ctrl+Shift+F10 in a Lua main chunk runs it.
const runClass = commands().get("RunClass");
if (runClass) {
  registerCommand({
    ...runClass,
    run: async () => {
      const tab = activeTab();
      if (tab && tab.path.endsWith(".lua") && active()) {
        const [marker] = await luaApi.runMarkers(tab.path, tab.content).catch(() => [] as LuaRunMarker[]);
        if (marker) return runMarker(marker);
      }
      return runClass.run();
    },
  });
}

// ---- Reformat Code (StyLua) ---------------------------------------------------------------

registerFormatProvider(async (path, view) => {
  if (!path.endsWith(".lua") || !active()) return false;
  const text = view.state.doc.toString();
  let formatted: string | null;
  try {
    formatted = await luaApi.format(path, text);
  } catch (e) {
    showBalloon("StyLua failed", String(e));
    return true;
  }
  if (formatted === null) return false;
  if (formatted !== text) {
    view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: formatted }, userEvent: "format", scrollIntoView: false });
  }
  return true;
});
