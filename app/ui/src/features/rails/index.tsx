/**
 * Ruby on Rails (RubyMine's Rails support). Everything is computed in Rust
 * (`src/ipc/rails.ts`, crate `packs/rubyforge/fw-rails`); this module plugs it into the
 * contribution points:
 *
 * - the Rails view of the Endpoints tool window (routes with Generate HTTP Request, models),
 * - Tools | Run Rails Generator… (Ctrl+Alt+G) and Destroy Generated Code…,
 * - the Rails console and database console in a Terminal tab, the "Rails" run configuration
 *   (`bin/rails server`, debugged with rdbg),
 * - data sources from config/database.yml (a balloon on open and an action),
 * - navigation, completion, gutter icons, inspections and translation folds (`editor.ts`),
 * - Settings | Languages & Frameworks | Ruby | Rails.
 *
 * Go to Related (Ctrl+Alt+Home) for Rails files is computed by `forge_codeintel::related`.
 */
import { createEffect, createRoot, on } from "solid-js";
import { render } from "solid-js/web";
import { TrainFront } from "lucide-solid";
import { dbApi } from "@/ipc/db";
import { railsApi } from "@/ipc/rails";
import { isDialogWindow } from "@/shell/window/mode";
import { registerKeymapActions } from "@/keymap";
import { registerEndpointsView, showEndpointsView } from "@/features/endpoints/registry";
import { registerSettingsPage } from "@/features/settingsPages";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { action, registerMainMenuEntries } from "@/features/mainMenu/model";
import { registerRunConfigurationType } from "@/features/run/types";
import { openTerminalWith } from "@/features/terminal";
import { loadSources, revealSource, setSelectedNode } from "@/features/database/store";
import { registerCommand } from "@/state/commands";
import { onFileSaved } from "@/state/editor";
import { showToolWindow } from "@/state/layout";
import { getSetting } from "@/state/settings";
import { workspace } from "@/state/workspace";
import { projectInfo } from "@/state/project";
import { GeneratorPopup, openGeneratorPopup } from "./GeneratorPopup";
import { RailsEndpointsActions, RailsEndpointsView, setRailsEndpointsTab } from "./RailsEndpointsView";
import { wrapGotoDeclaration } from "./editor";
import { errorText, loadRailsProject, PLUGIN, railsActive, railsProject, showRailsBalloon } from "./state";
import "./rails.css";

wrapGotoDeclaration();

// ---- Endpoints, settings ---------------------------------------------------------------------

registerEndpointsView({ id: "rails", title: "Rails", order: 15, when: railsActive, component: RailsEndpointsView, actions: RailsEndpointsActions });

registerSettingsPage({
  id: "lang.rails",
  title: "Rails",
  group: "Languages & Frameworks",
  parent: "lang.ruby",
  order: 10,
  keywords: ["rails", "routes", "generators", "console", "dbconsole", "database.yml", "i18n", "locales", "translations", "stimulus", "turbo", "hotwire"],
  component: () => <SchemaPage pageId="lang.rails" />,
});

// ---- Run configuration ----------------------------------------------------------------------------

const opt = (cfg: { options?: Record<string, string> }, key: string) => cfg.options?.[key] ?? "";

registerRunConfigurationType({
  id: "rails",
  name: "Rails",
  description: "Run bin/rails server on a host and port in an environment; debug with rdbg.",
  icon: TrainFront,
  color: "var(--danger)",
  pluginId: PLUGIN,
  argumentsLabel: "Server arguments",
  debuggable: true,
  defaults: () => ({ options: { environment: "development", port: "3000" } }),
  fields: [
    {
      key: "environment",
      label: "Environment",
      kind: "enum",
      default: "development",
      options: [
        { value: "development", label: "development" },
        { value: "test", label: "test" },
        { value: "production", label: "production" },
      ],
    },
    { key: "host", label: "IP address", placeholder: "localhost (0.0.0.0 for other machines)" },
    { key: "port", label: "Port", placeholder: "3000" },
    { key: "interpreterOptions", label: "Ruby arguments", placeholder: "-W0" },
  ],
  suggestName: (cfg) => `${railsProject()?.appName?.toLowerCase() ?? "rails"}: ${opt(cfg, "environment") || "development"}`,
  validate: (cfg) => (opt(cfg, "port") && !/^\d{1,5}$/.test(opt(cfg, "port")) ? "The port must be a number" : null),
});

// ---- Actions --------------------------------------------------------------------------------------

/** The Rails console or database console in a Terminal tab. */
export async function openRailsConsole(kind: "console" | "dbconsole"): Promise<void> {
  try {
    const cmd = await railsApi.consoleCommand(kind);
    openTerminalWith({ title: kind === "console" ? "Rails Console" : "Rails DB Console", program: cmd.program, args: cmd.args, env: Object.entries(cmd.env ?? {}) });
  } catch (e) {
    showRailsBalloon(`Cannot start the Rails ${kind === "console" ? "console" : "database console"}`, errorText(e), 8000);
  }
}

/** Add the databases of database.yml to the Database tool window and show the first. */
export async function addRailsDataSources(environments: string[] = []): Promise<void> {
  try {
    const added = await railsApi.addDataSources(environments);
    await loadSources();
    if (!added.length) {
      showRailsBalloon("No new data sources", "The databases of config/database.yml are already in the Database tool window.", 5000);
      return;
    }
    showToolWindow("database");
    setSelectedNode(`ds:${added[0].id}`);
    revealSource(added[0].id);
    showRailsBalloon(`Added ${added.length} data source${added.length === 1 ? "" : "s"}`, added.map((d) => d.name).join(", "), 6000);
  } catch (e) {
    showRailsBalloon("Cannot add the data sources", errorText(e), 8000);
  }
}

registerKeymapActions([
  { id: "rails.generate", title: "Run Rails Generator…", group: "Plugins/Rails", shortcuts: { $default: ["Ctrl+Alt+G"], macOS: ["Cmd+Alt+G"] } },
  { id: "rails.destroy", title: "Destroy Generated Code…", group: "Plugins/Rails" },
  { id: "rails.console", title: "Run Rails Console", group: "Plugins/Rails" },
  { id: "rails.dbconsole", title: "Run Rails Database Console", group: "Plugins/Rails" },
  { id: "rails.addDataSources", title: "Add Data Sources from database.yml", group: "Plugins/Rails" },
  { id: "rails.reload", title: "Reload Rails Project", group: "Plugins/Rails" },
  { id: "rails.showRoutes", title: "Show Rails Routes", group: "Plugins/Rails" },
]);

const requireRails = (title: string, fn: () => void) => () => {
  if (!railsActive()) return void showRailsBalloon(title, "The project is not a Rails application, or the Rails plugin is disabled.", 5000);
  fn();
};

registerCommand({ id: "rails.generate", title: "Run Rails Generator…", category: "Rails", run: requireRails("Run Rails Generator", () => openGeneratorPopup()) });
registerCommand({
  id: "rails.destroy",
  title: "Destroy Generated Code…",
  category: "Rails",
  run: requireRails("Destroy Generated Code", () => openGeneratorPopup({ destroy: true })),
});
registerCommand({ id: "rails.console", title: "Run Rails Console", category: "Rails", run: requireRails("Rails Console", () => void openRailsConsole("console")) });
registerCommand({
  id: "rails.dbconsole",
  title: "Run Rails Database Console",
  category: "Rails",
  run: requireRails("Rails Database Console", () => void openRailsConsole("dbconsole")),
});
registerCommand({
  id: "rails.addDataSources",
  title: "Add Data Sources from database.yml",
  category: "Rails",
  run: requireRails("Add Data Sources", () => void addRailsDataSources()),
});
registerCommand({ id: "rails.reload", title: "Reload Rails Project", category: "Rails", run: () => void loadRailsProject(true) });
registerCommand({
  id: "rails.showRoutes",
  title: "Show Rails Routes",
  category: "Rails",
  run: () => {
    setRailsEndpointsTab("routes");
    showEndpointsView("rails");
  },
});

registerMainMenuEntries(
  "Tools",
  [
    action("rails.generate", "Run Rails _Generator..."),
    action("rails.console", "Run Rails C_onsole"),
    action("rails.dbconsole", "Run Rails _Database Console"),
    action("rails.addDataSources", "Add Data Sources from database.y_ml"),
  ],
  { when: railsActive },
);

// ---- Startup ----------------------------------------------------------------------------------

const announced = new Set<string>();

/** Offer the databases of database.yml that the Database tool window does not have yet. */
async function offerDataSources(): Promise<void> {
  const p = railsProject();
  const root = workspace()?.root ?? "";
  if (!p || announced.has(root) || getSetting<boolean>("rails.database.suggest", true) === false) return;
  const usable = p.databases.filter((d) => d.driver && !d.environment.startsWith("production"));
  if (!usable.length) return;
  const known = await dbApi.list().catch(() => []);
  const missing = usable.filter((d) => !known.some((k) => k.driver === d.driver && (d.driver === "sqlite" ? k.file === d.file : k.database === d.database)));
  if (!missing.length) return;
  announced.add(root);
  showRailsBalloon(
    "Databases found in config/database.yml",
    missing.map((d) => `${d.environment}: ${d.driver === "sqlite" ? d.file : `${d.database}@${d.host || "localhost"}`}`).join(", "),
    15000,
    [{ label: "Add data sources", run: () => void addRailsDataSources(missing.map((d) => d.environment)) }],
  );
}

const RELOAD = /^(config\/(routes|locales|database|application)|config\/routes\/|app\/(models|controllers|mailers|javascript\/controllers)\/|db\/(schema\.rb|migrate\/)|app\/views\/)/;

if (typeof document !== "undefined" && !isDialogWindow()) {
  createRoot(() => {
    createEffect(
      on(
        () => [workspace()?.root, railsActive(), JSON.stringify(projectInfo()?.frameworks ?? [])] as const,
        ([root, active]) => {
          if (root && active) void loadRailsProject().then(() => void offerDataSources());
        },
      ),
    );
  });
  onFileSaved((path) => {
    if (railsActive() && RELOAD.test(path)) void loadRailsProject(true);
  });
  const host = document.createElement("div");
  document.body.append(host);
  render(() => <GeneratorPopup />, host);
}
