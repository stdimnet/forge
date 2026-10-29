/**
 * Schema tools of the database support: ER diagrams (Diagrams | Show Diagram,
 * Ctrl+Alt+Shift+U), Compare Structure with migration scripts (Ctrl+D in the Database tool
 * window) and Explain Plan / Explain Plan (Raw) in consoles. Views are editor tabs; actions
 * go through the keymap.
 */
import { GitCompareArrows, Network } from "lucide-solid";
import { registerEditorView } from "@/shell/editorViews";
import { registerCommand } from "@/state/commands";
import type { EditorTab } from "@/state/editor";
import { editor } from "@/state/editor";
import { registerKeymapActions, balloonsFor } from "@/keymap";
import { registerCompareHandler } from "@/features/compare/state";
import { activeConsole } from "../controllers";
import { compareOfSelection, diagramOfSelection } from "../DatabaseView";
import { consoleSourceId, parseTablePath, sourceById, tabSourceId } from "../store";
import { CompareView } from "./CompareView";
import { DiagramView } from "./DiagramView";
import { COMPARE_VIEW, DIAGRAM_VIEW, openCompare, openDiagram, parseDiagramPath } from "./model";
import "./schema.css";

const showBalloon = balloonsFor("Database");

registerEditorView({
  id: DIAGRAM_VIEW,
  component: DiagramView,
  icon: (p) => <Network size={p.size} class="db-tab-icon table" />,
  breadcrumbs: (tab: EditorTab) => {
    const loc = parseDiagramPath(tab.path);
    return loc ? [sourceById(loc.id)?.name ?? loc.id, loc.schema, ...(loc.table ? [loc.table] : []), "Diagram"] : [tab.name];
  },
});

registerEditorView({
  id: COMPARE_VIEW,
  component: CompareView,
  icon: (p) => <GitCompareArrows size={p.size} class="db-tab-icon console" />,
  breadcrumbs: () => ["Database", "Compare Structure"],
});

registerKeymapActions([
  // PHPStorm: Diagrams | Show Diagram…
  { id: "ShowUmlDiagram", title: "Show Diagram…", group: "Diagrams", shortcuts: { $default: ["Ctrl+Alt+Shift+U"], macOS: ["Cmd+Alt+Shift+U"] } },
  { id: "Database.CompareStructure", title: "Compare Structure…", group: "Database" },
  { id: "Console.Jdbc.ExplainPlan", title: "Explain Plan", group: "Database", context: "editor" },
  { id: "Console.Jdbc.ExplainPlan.Raw", title: "Explain Plan (Raw)", group: "Database", context: "editor" },
]);

const inDbTree = () => !!document.activeElement?.closest(".db-tree");

/** Show Diagram outside the tree: the schema of the active console, table or diagram tab. */
function diagramOfActiveTab(): boolean {
  const path = editor.active;
  if (!path) return false;
  const table = parseTablePath(path);
  if (table) {
    openDiagram(table.id, table.schema, table.table);
    return true;
  }
  const id = consoleSourceId(path) ?? tabSourceId(path);
  const ds = id ? sourceById(id) : undefined;
  if (!ds || ds.driver === "redis" || ds.driver === "mongodb") return false;
  openDiagram(ds.id);
  return true;
}

registerCommand({
  id: "ShowUmlDiagram",
  title: "Show Diagram…",
  category: "Diagrams",
  run: () => {
    if ((inDbTree() && diagramOfSelection()) || diagramOfActiveTab() || diagramOfSelection()) return;
    showBalloon("Show Diagram", "Select a schema or table in the Database tool window, or open a console of a data source.");
  },
});

registerCommand({ id: "Database.CompareStructure", title: "Compare Structure…", category: "Database", run: () => compareOfSelection() });
registerCommand({ id: "database.compareStructure", title: "Compare Structure…", category: "Database", run: () => openCompare() });

const explain = (raw: boolean) => {
  const c = activeConsole();
  if (c?.explain) return c.explain(raw);
  showBalloon("Explain Plan", "Put the caret on a statement in a SQL console.");
};
registerCommand({ id: "Console.Jdbc.ExplainPlan", title: "Explain Plan", category: "Database", run: () => explain(false) });
registerCommand({ id: "Console.Jdbc.ExplainPlan.Raw", title: "Explain Plan (Raw)", category: "Database", run: () => explain(true) });

// Ctrl+D in the Database tool window: Compare Structure (DataGrip).
registerCompareHandler(".db-tree", compareOfSelection);
