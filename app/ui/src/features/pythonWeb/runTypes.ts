/**
 * The web framework run configuration types: Django Server, Django tests, Flask Server,
 * FastAPI, Celery worker and Celery beat (plus the hidden manage.py task type). Rust builds
 * them as Python runs (`pyforge_frameworks::run`, `pyforge_testing::command::django_tests`),
 * so they use the project's interpreter, run targets and debugpy; empty fields take what
 * the project analysis found (manage.py, settings module, FLASK_APP, the uvicorn app, the
 * Celery app).
 */
import { Cog, Server, TestTubeDiagonal, Timer, Zap, FlaskRound, SquareTerminal } from "lucide-solid";
import type { RunConfiguration } from "@/ipc/run";
import { baseName, registerRunConfigurationType, type RunConfigField } from "@/features/run/types";
import { getSetting } from "@/state/settings";
import { currentInterpreter } from "@/features/python/state";
import { PYTHON_DEBUG_TYPES } from "@/features/python/debug";
import { webProject } from "./state";

const opt = (cfg: RunConfiguration, key: string) => cfg.options?.[key] ?? "";
const defaultInterpreter = () => currentInterpreter()?.interpreter.path || getSetting<string>("python.interpreter", "") || "python3";

for (const t of ["django-server", "django-manage", "django-tests", "flask-server", "fastapi", "celery-worker", "celery-beat"]) PYTHON_DEBUG_TYPES.add(t);

const hostPort = (port: string): RunConfigField[] => [
  { key: "host", label: "Host", placeholder: "127.0.0.1" },
  { key: "port", label: "Port", placeholder: port },
];

const common = {
  color: "var(--success)",
  interpreter: "custom" as const,
  defaultInterpreter,
  debuggable: true,
};

registerRunConfigurationType({
  ...common,
  id: "django-server",
  name: "Django Server",
  description: "Run manage.py runserver on a host and port; Debug runs it under debugpy.",
  icon: Server,
  pluginId: "forge.django",
  argumentsLabel: "Additional options",
  defaults: () => ({ options: { host: "127.0.0.1", port: "8000" } }),
  fields: [
    ...hostPort("8000"),
    { key: "noReload", label: "No reload (--noreload)", kind: "bool", default: "false" },
    { key: "settings", label: "Settings module", placeholder: "DJANGO_SETTINGS_MODULE (empty: from manage.py)" },
    { key: "manage", label: "manage.py", kind: "path", placeholder: "manage.py (empty: the project's)" },
  ],
  suggestName: (cfg) => `Django: ${opt(cfg, "host") || "127.0.0.1"}:${opt(cfg, "port") || "8000"}`,
});

registerRunConfigurationType({
  ...common,
  id: "django-manage",
  name: "manage.py",
  description: "A manage.py task (Tools | Run manage.py Task…).",
  icon: SquareTerminal,
  pluginId: "forge.django",
  hidden: true,
  argumentsLabel: false,
  fields: [{ key: "task", label: "Task", required: true, placeholder: "migrate" }],
  suggestName: (cfg) => (opt(cfg, "task") ? `manage.py ${opt(cfg, "task")}` : undefined),
});

registerRunConfigurationType({
  ...common,
  id: "django-tests",
  name: "Django tests",
  description: "Run manage.py test with Django's test runner, or pytest with pytest-django, in the test runner.",
  icon: TestTubeDiagonal,
  color: "var(--info)",
  pluginId: "forge.django",
  testRunner: true,
  argumentsLabel: "Additional options",
  defaults: () => ({ options: { runner: getSetting<string>("django.testRunner", "django") || "django" } }),
  fields: [
    {
      key: "runner",
      label: "Runner",
      kind: "segmented",
      default: "django",
      options: [
        { value: "django", label: "Django (manage.py test)" },
        { value: "pytest", label: "pytest-django" },
      ],
    },
    { key: "target", label: "Target", placeholder: "catalog.tests catalog.tests.BookViewsTest (empty: all tests)" },
    { key: "settings", label: "Settings module", placeholder: "DJANGO_SETTINGS_MODULE (empty: from manage.py)" },
  ],
  suggestName: (cfg) => (opt(cfg, "target") ? `Django tests: ${opt(cfg, "target")}` : "Django tests"),
});

registerRunConfigurationType({
  ...common,
  id: "flask-server",
  name: "Flask Server",
  description: "Run flask run with FLASK_APP on a host and port; Debug runs it under debugpy.",
  icon: FlaskRound,
  pluginId: "forge.flask",
  argumentsLabel: "Additional options",
  defaults: () => ({ options: { host: "127.0.0.1", port: "5000", debugMode: "true" } }),
  fields: [
    { key: "app", label: "Target (FLASK_APP)", placeholder: "app (empty: the module that creates Flask(...))" },
    ...hostPort("5000"),
    { key: "debugMode", label: "FLASK_DEBUG", kind: "bool", default: "true" },
    { key: "noReload", label: "No reload (--no-reload)", kind: "bool", default: "false" },
  ],
  suggestName: (cfg) => `Flask (${opt(cfg, "app") || baseName(webProject()?.flask?.location?.path) || "app"})`,
});

registerRunConfigurationType({
  ...common,
  id: "fastapi",
  name: "FastAPI",
  description: "Run the application with uvicorn (--reload) on a host and port; Debug runs it under debugpy.",
  icon: Zap,
  pluginId: "forge.fastapi",
  argumentsLabel: "Uvicorn options",
  defaults: () => ({ options: { host: "127.0.0.1", port: "8000", reload: "true" } }),
  fields: [
    { key: "app", label: "Application", placeholder: "shop_api.main:app (empty: the module that creates FastAPI())" },
    { key: "appDir", label: "Application directory (--app-dir)", kind: "path", directory: true, placeholder: "src" },
    ...hostPort("8000"),
    { key: "reload", label: "Reload on changes (--reload)", kind: "bool", default: "true" },
  ],
  suggestName: (cfg) => `FastAPI: ${opt(cfg, "app") || webProject()?.fastapi?.app || "app"}`,
});

const celeryFields: RunConfigField[] = [
  { key: "app", label: "Application (-A)", placeholder: "proj (empty: the module that creates Celery())" },
  {
    key: "loglevel",
    label: "Log level",
    kind: "enum",
    default: "INFO",
    options: ["DEBUG", "INFO", "WARNING", "ERROR", "CRITICAL"].map((l) => ({ value: l, label: l })),
  },
];

registerRunConfigurationType({
  ...common,
  id: "celery-worker",
  name: "Celery worker",
  description: "Run celery -A <app> worker; Debug runs it under debugpy (use the solo pool to stop in tasks).",
  icon: Cog,
  color: "var(--syn-type)",
  pluginId: "forge.celery",
  argumentsLabel: "Additional options",
  defaults: () => ({ options: { loglevel: "INFO" } }),
  fields: [
    ...celeryFields,
    { key: "concurrency", label: "Concurrency", placeholder: "number of CPUs" },
    {
      key: "pool",
      label: "Pool",
      kind: "enum",
      default: "",
      options: [
        { value: "", label: "Default (prefork)" },
        { value: "solo", label: "solo" },
        { value: "threads", label: "threads" },
        { value: "gevent", label: "gevent" },
        { value: "eventlet", label: "eventlet" },
      ],
    },
  ],
  suggestName: (cfg) => `Celery worker (${opt(cfg, "app") || webProject()?.celery?.app || "app"})`,
});

registerRunConfigurationType({
  ...common,
  id: "celery-beat",
  name: "Celery beat",
  description: "Run the celery -A <app> beat scheduler.",
  icon: Timer,
  color: "var(--syn-type)",
  pluginId: "forge.celery",
  argumentsLabel: "Additional options",
  defaults: () => ({ options: { loglevel: "INFO" } }),
  fields: celeryFields,
  suggestName: (cfg) => `Celery beat (${opt(cfg, "app") || webProject()?.celery?.app || "app"})`,
});
