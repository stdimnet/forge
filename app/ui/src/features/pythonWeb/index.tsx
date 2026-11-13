/**
 * PyForge web frameworks (PyCharm Professional's Django, Flask, FastAPI and Celery
 * support). Everything is computed in Rust (`src/ipc/pythonWeb.ts`, crate
 * `packs/pyforge/frameworks`); this module plugs it into the contribution points:
 *
 * - the Endpoints tool window (Django URLs, Flask routes, FastAPI endpoints, Celery tasks),
 * - the Django Structure tool window (settings, apps, models and fields, template folders),
 * - Tools | Run manage.py Task… (Ctrl+Alt+R) with command completion,
 * - template / URL name navigation, completion, gutter icons and inspections (`editor.ts`),
 * - the run configuration types (`runTypes.ts`),
 * - Settings | Languages & Frameworks | Django, Flask, FastAPI and Celery.
 */
import { createEffect, createRoot, on } from "solid-js";
import { render } from "solid-js/web";
import { Boxes } from "lucide-solid";
import { isDialogWindow } from "@/shell/window/mode";
import { registerKeymapActions, showBalloon } from "@/keymap";
import { registerToolWindow } from "@/features/registry";
import { registerEndpointsView, showEndpointsView } from "@/features/endpoints/registry";
import { registerSettingsPage } from "@/features/settingsPages";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { action, registerMainMenuEntries } from "@/features/mainMenu/model";
import { registerCommand } from "@/state/commands";
import { toggleToolWindow } from "@/state/layout";
import { workspace } from "@/state/workspace";
import { projectInfo } from "@/state/project";
import { DjangoStructureActions, DjangoStructureView } from "./DjangoStructureView";
import { EndpointsActions, EndpointsView, setEndpointsTab } from "./EndpointsView";
import { ManageTaskPopup, openManageTask } from "./ManageTaskPopup";
import { wrapGotoDeclaration } from "./editor";
import { anyWebActive, djangoActive, loadWebProject, webActive } from "./state";
import "./runTypes";
import "./pythonWeb.css";
import { t } from "@/i18n";

wrapGotoDeclaration();

// ---- Tool windows --------------------------------------------------------------------------

// The shared Endpoints tool window (features/endpoints) shows this view for Python projects.
registerEndpointsView({ id: "python", title: "Python", order: 10, when: anyWebActive, component: EndpointsView, actions: EndpointsActions });

registerToolWindow({
  id: "djangoStructure",
  title: t("toolwindow.djangoStructure.title"),
  icon: Boxes,
  anchor: "left",
  order: 40,
  component: DjangoStructureView,
  actions: DjangoStructureActions,
  when: djangoActive,
});

// ---- Settings ------------------------------------------------------------------------------

const page = (id: string, title: string, order: number, keywords: string[]) =>
  registerSettingsPage({
    id,
    title,
    group: "Languages & Frameworks",
    parent: "lang.python",
    order,
    keywords,
    component: () => <SchemaPage pageId={id} />,
  });

page("lang.django", "Django", 10, ["django", "manage.py", "settings", "DJANGO_SETTINGS_MODULE", "templates", "tests", "pytest-django"]);
page("lang.flask", "Flask", 11, ["flask", "FLASK_APP", "blueprint", "jinja2"]);
page("lang.fastapi", "FastAPI", 12, ["fastapi", "uvicorn", "APIRouter", "endpoints"]);
page("lang.celery", "Celery", 13, ["celery", "worker", "beat", "tasks"]);

// ---- Actions -------------------------------------------------------------------------------

registerKeymapActions([
  {
    id: "Django.RunManageTask",
    title: "Run manage.py Task…",
    group: "Plugins/Django",
    shortcuts: { $default: ["Ctrl+Alt+R"], macOS: ["Alt+R"] },
  },
  { id: "ActivateEndpointsToolWindow", title: "Endpoints", group: "Tool Windows" },
  { id: "ActivateDjangoStructureToolWindow", title: "Django Structure", group: "Tool Windows" },
  { id: "Celery.ShowTasks", title: "Celery Tasks", group: "Plugins/Celery" },
]);

registerCommand({
  id: "Django.RunManageTask",
  title: "Run manage.py Task…",
  category: "Tools",
  run: () => {
    if (!djangoActive()) return void showBalloon(t("langs.pythonWeb.runManagePyTask"), t("langs.pythonWeb.theProjectHasNoManage"));
    openManageTask();
  },
});
registerCommand({ id: "ActivateEndpointsToolWindow", title: "Endpoints", category: "View", run: () => toggleToolWindow("endpoints") });
registerCommand({
  id: "ActivateDjangoStructureToolWindow",
  title: "Django Structure",
  category: "View",
  run: () => toggleToolWindow("djangoStructure"),
});
registerCommand({
  id: "Celery.ShowTasks",
  title: "Celery Tasks",
  category: "Tools",
  run: () => {
    setEndpointsTab("tasks");
    showEndpointsView("python");
  },
});

registerMainMenuEntries("Tools", [action("Django.RunManageTask", "Run manage.py Tas_k...")], { when: djangoActive });

// ---- Startup ---------------------------------------------------------------------------

if (typeof document !== "undefined" && !isDialogWindow()) {
  createRoot(() => {
    createEffect(
      on(
        () => [workspace()?.root, anyWebActive(), webActive(), JSON.stringify(projectInfo()?.frameworks ?? [])] as const,
        ([root, on]) => {
          if (root && on) void loadWebProject();
        },
      ),
    );
  });
  const host = document.createElement("div");
  document.body.append(host);
  render(() => <ManageTaskPopup />, host);
}
