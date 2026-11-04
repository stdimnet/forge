/**
 * DotNetForge for web and cloud projects (Rider's ASP.NET Core, EF Core, Aspire and Docker
 * support), all computed in Rust (`src/ipc/dotnetWeb.ts`):
 *
 * - Razor and Blazor in the editor (`razor.ts`): C# inside `.razor` / `.cshtml`, Go to
 *   Declaration on component tags, parameter completion;
 * - ASP.NET Core endpoints in the Endpoints tool window with Generate HTTP Request
 *   (`endpoints.tsx`);
 * - the EF Core tool window and its `dotnet ef` dialogs (`efcore.tsx`);
 * - .NET Aspire AppHosts in Services and the ".NET Aspire" configuration (`aspire.tsx`);
 * - Add Docker Support and the ".NET Docker" configuration (`docker.tsx`);
 * - Hot Reload of `dotnet watch` runs with Apply Changes, Ctrl+F10 (`hotReload.tsx`);
 * - Manage User Secrets (`secrets.tsx`).
 */
import { Database } from "lucide-solid";
import { registerEndpointsView, showEndpointsView } from "@/features/endpoints/registry";
import { registerToolWindow } from "@/features/registry";
import { action, group, registerMainMenuEntries } from "@/features/mainMenu/model";
import { registerKeymapActions } from "@/keymap";
import { registerCommand } from "@/state/commands";
import { showToolWindow } from "@/state/layout";
import { dotnetActive, showDotnetBalloon } from "../state";
import { AspNetEndpointsActions, AspNetEndpointsView, aspnetActive } from "./endpoints";
import { EfCoreToolWindow, efActive, listMigrations, openAddMigration, openRemoveMigration, openScaffold, openUpdateDatabase } from "./efcore";
import { openDashboard } from "./aspire";
import { openAddDockerSupport } from "./docker";
import { applyChanges } from "./hotReload";
import { manageUserSecretsAction } from "./secrets";
import "./razor";
import { t } from "@/i18n";

registerEndpointsView({ id: "aspnet", title: t("langs.dotnet.aspNetCore"), order: 22, when: aspnetActive, component: AspNetEndpointsView, actions: AspNetEndpointsActions });

registerToolWindow({
  id: "efcore",
  title: t("toolwindow.efcore.title"),
  icon: Database,
  anchor: "bottom",
  order: 49,
  component: EfCoreToolWindow,
  when: efActive,
});

const needsDotnet = (run: () => void) => () => {
  if (!dotnetActive()) {
    showDotnetBalloon("No .NET solution", "Open a folder with a .sln, .slnx or project file, or enable the .NET plugin.", 5000);
    return;
  }
  run();
};

registerKeymapActions([
  { id: "dotnet.showEndpoints", title: "ASP.NET Core Endpoints", group: "Plugins/.NET" },
  { id: "dotnet.ef.addMigration", title: "EF Core: Add Migration…", group: "Plugins/.NET" },
  { id: "dotnet.ef.removeMigration", title: "EF Core: Remove Last Migration…", group: "Plugins/.NET" },
  { id: "dotnet.ef.updateDatabase", title: "EF Core: Update Database…", group: "Plugins/.NET" },
  { id: "dotnet.ef.listMigrations", title: "EF Core: Refresh Migrations", group: "Plugins/.NET" },
  { id: "dotnet.ef.scaffold", title: "EF Core: Scaffold DbContext…", group: "Plugins/.NET" },
  { id: "dotnet.addDockerSupport", title: "Add Docker Support…", group: "Plugins/.NET" },
  { id: "dotnet.manageUserSecrets", title: "Manage User Secrets", group: "Plugins/.NET" },
  { id: "dotnet.aspire.openDashboard", title: "Open Aspire Dashboard", group: "Plugins/.NET" },
  // IntelliJ's "Update Running Application"; for .NET it applies Hot Reload changes.
  { id: "UpdateRunningApplication", title: "Apply Changes (Hot Reload)", group: "Main Menu/Run", shortcuts: { $default: ["Ctrl+F10"], macOS: ["Cmd+F10"] } },
]);

registerCommand({ id: "dotnet.showEndpoints", title: "ASP.NET Core Endpoints", category: ".NET", run: needsDotnet(() => showEndpointsView("aspnet")) });
registerCommand({
  id: "dotnet.ef.addMigration",
  title: "EF Core: Add Migration…",
  category: ".NET",
  run: needsDotnet(() => {
    showToolWindow("efcore");
    openAddMigration();
  }),
});
registerCommand({ id: "dotnet.ef.removeMigration", title: "EF Core: Remove Last Migration…", category: ".NET", run: needsDotnet(openRemoveMigration) });
registerCommand({ id: "dotnet.ef.updateDatabase", title: "EF Core: Update Database…", category: ".NET", run: needsDotnet(openUpdateDatabase) });
registerCommand({
  id: "dotnet.ef.listMigrations",
  title: "EF Core: Refresh Migrations",
  category: ".NET",
  run: needsDotnet(() => {
    showToolWindow("efcore");
    void listMigrations();
  }),
});
registerCommand({ id: "dotnet.ef.scaffold", title: "EF Core: Scaffold DbContext…", category: ".NET", run: needsDotnet(openScaffold) });
registerCommand({ id: "dotnet.addDockerSupport", title: "Add Docker Support…", category: ".NET", run: needsDotnet(() => openAddDockerSupport()) });
registerCommand({ id: "dotnet.manageUserSecrets", title: "Manage User Secrets", category: ".NET", run: needsDotnet(manageUserSecretsAction) });
registerCommand({ id: "dotnet.aspire.openDashboard", title: "Open Aspire Dashboard", category: ".NET", run: () => openDashboard() });
registerCommand({ id: "UpdateRunningApplication", title: "Apply Changes (Hot Reload)", category: "Run", run: () => void applyChanges() });

registerMainMenuEntries(
  "_Tools",
  [
    group("Entity Framework _Core", [
      action("dotnet.ef.addMigration", "_Add Migration…"),
      action("dotnet.ef.removeMigration", "_Remove Last Migration…"),
      action("dotnet.ef.updateDatabase", "_Update Database…"),
      action("dotnet.ef.listMigrations", "Refresh _Migrations"),
      action("dotnet.ef.scaffold", "_Scaffold DbContext…"),
    ]),
    action("dotnet.addDockerSupport", "Add _Docker Support…"),
    action("dotnet.manageUserSecrets", "Manage User _Secrets"),
  ],
  { when: dotnetActive },
);
registerMainMenuEntries("_Run", [action("UpdateRunningApplication", "Apply Changes (Hot Reload)")], { when: dotnetActive });
