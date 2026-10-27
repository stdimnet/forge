/**
 * Browser-mode Django, Flask, FastAPI and Celery for the Python demos (`?project=django`,
 * `?project=python`): the project model, template / URL name navigation, completion,
 * gutter marks, inspections, manage.py commands and run command lines come from the Rust
 * crate `pyforge-frameworks` through WebAssembly; runs of the web configurations play
 * canned server / worker output. Nothing touches a real project.
 */
import { registerMock } from "@/ipc";
import type { CommandLine, RunConfiguration, RunRequest } from "@/ipc/run";
import type { WebProject, WebRunInfo } from "@/ipc/pythonWeb";
import { callWasmSync, forgeWasm } from "@/wasm";
import { isDjangoDemo, isPythonDemo, pythonDemoName } from "./demoPythonProject";
import { registerMockRunHandler, startMockRun, type Step } from "./runMock";
import { mockSettingValue } from "./settingsMock";
import { mockFiles } from "./workspaceMock";

const ROOT = `~/Projects/${pythonDemoName}`;
const PYTHON = `${ROOT}/.venv/bin/python`;
const WEB_TYPES = new Set(["django-server", "django-manage", "flask-server", "fastapi", "celery-worker", "celery-beat"]);
const ANALYSED = /\.(py|html?|txt|jinja2?|j2|djhtml|xml|toml|cfg|ini)$|(^|\/)Pipfile$/;

const EMPTY: WebProject = { frameworks: [], routes: [], models: [], tasks: [], templateRoots: [], templates: [] };

function request(extra: Record<string, unknown> = {}) {
  const files = Object.fromEntries(Object.entries(mockFiles).filter(([p, v]) => typeof v === "string" && ANALYSED.test(p)));
  const setting = (key: string) => String(mockSettingValue(key) ?? "");
  return {
    files,
    options: {
      settingsModule: setting("django.settingsModule"),
      manageScript: setting("django.manageScript"),
      flaskApp: setting("flask.app"),
      fastapiApp: setting("fastapi.app"),
      celeryApp: setting("celery.app"),
    },
    ...extra,
  };
}

async function call<T>(fn: string, extra: Record<string, unknown> = {}): Promise<T> {
  const wasm = await forgeWasm();
  return callWasmSync<T>((wasm as unknown as Record<string, (json: string) => string>)[fn], request(extra));
}

registerMock("python_web_project", async () => (isPythonDemo() ? call<WebProject>("python_web_project") : structuredClone(EMPTY)));
registerMock("python_web_goto", async ({ path, content, offset }) => (isPythonDemo() ? call("python_web_goto", { path, content, offset }) : []));
registerMock("python_web_complete", async ({ path, content, offset }) =>
  isPythonDemo() ? call("python_web_complete", { path, content, offset }) : null,
);
registerMock("python_web_marks", async ({ path, content }) =>
  isPythonDemo() ? call("python_web_marks", { path, content }) : { gutter: [], problems: [] },
);
// A trusted project lists the output of `manage.py help --commands`; the demo's equals the
// static list (Django's commands plus catalog's `import_books`).
registerMock("python_manage_commands", async () =>
  isDjangoDemo()
    ? { commands: await call("python_web_manage_commands"), source: "manage.py" }
    : { commands: [], source: "static", message: "No manage.py found" },
);
registerMock("python_web_run_info", async ({ configuration }) => call<WebRunInfo>("python_web_run_info", { configuration }));

// ---- runs ----------------------------------------------------------------------------------

const tc = (name: string, attrs: Record<string, string | number>) =>
  `##teamcity[${name} ${Object.entries(attrs)
    .map(([k, v]) => `${k}='${String(v).replace(/[|'[\]]/g, (c) => `|${c}`).replace(/\n/g, "|n")}'`)
    .join(" ")}]\n`;

const time = () => new Date().toTimeString().slice(0, 8);

function serverSteps(cfg: RunConfiguration, info: WebRunInfo, debug: boolean): Step[] {
  const port = info.port ?? 8000;
  const host = info.url ? new URL(info.url).host : `127.0.0.1:${port}`;
  switch (cfg.type) {
    case "django-server":
      return [
        [200, `Watching for file changes with StatReloader\n`],
        [150, `Performing system checks...\n\nSystem check identified no issues (0 silenced).\n`],
        [120, `${new Date().toDateString().slice(4)} - ${time()}\nDjango version 5.1.2, using settings '${info.env.DJANGO_SETTINGS_MODULE ?? "bookshop.settings"}'\n`],
        [60, `Starting development server at http://${host}/\nQuit the server with CONTROL-C.\n\n`],
        [900, `[${new Date().toUTCString().slice(5, 25)}] "GET / HTTP/1.1" 200 4127\n`],
        [140, `[${new Date().toUTCString().slice(5, 25)}] "GET /books/ HTTP/1.1" 200 6380\n`],
      ];
    case "flask-server":
      return [
        [200, ` * Serving Flask app '${info.env.FLASK_APP ?? "app"}'\n * Debug mode: ${debug || info.args.includes("--debug") ? "on" : "off"}\n`],
        [100, ` * Running on http://${host}\nPress CTRL+C to quit\n`],
      ];
    case "fastapi":
      return [
        [150, `INFO:     Will watch for changes in these directories: ['${ROOT}/src']\n`, "stderr"],
        [120, `INFO:     Uvicorn running on http://${host} (Press CTRL+C to quit)\n`, "stderr"],
        [80, `INFO:     Started reloader process [48311] using WatchFiles\n`, "stderr"],
        [250, `INFO:     Started server process [48315]\nINFO:     Waiting for application startup.\n`, "stderr"],
        [80, `INFO:     Application startup complete.\n`, "stderr"],
        [900, `INFO:     127.0.0.1:52144 - "GET /docs HTTP/1.1" 200 OK\nINFO:     127.0.0.1:52144 - "GET /openapi.json HTTP/1.1" 200 OK\n`, "stderr"],
      ];
    case "celery-worker":
      return [
        [300, ` \n -------------- celery@workstation v5.4.0 (opalescent)\n--- ***** ----- \n-- ******* ---- Linux-6.8.0-x86_64-with-glibc2.39 ${new Date().toISOString().slice(0, 19).replace("T", " ")}\n- *** --- * --- \n- ** ---------- [config]\n- ** ---------- .> app:         ${info.args[1]}:0x7f3b2c1d5a90\n- ** ---------- .> transport:   redis://localhost:6379/0\n- ** ---------- .> results:     disabled://\n- *** --- * --- .> concurrency: ${info.args.includes("--concurrency") ? info.args[info.args.indexOf("--concurrency") + 1] : "8"} (prefork)\n-- ******* ---- .> task events: OFF\n--- ***** ----- \n -------------- [queues]\n                .> celery           exchange=celery(direct) key=celery\n\n`],
        [120, `[tasks]\n  . bookshop.celery.debug_task\n  . catalog.reindex\n  . catalog.tasks.send_review_digest\n\n`],
        [400, `[${new Date().toISOString().slice(0, 19).replace("T", " ")},214: INFO/MainProcess] Connected to redis://localhost:6379/0\n`, "stderr"],
        [200, `[${new Date().toISOString().slice(0, 19).replace("T", " ")},380: INFO/MainProcess] celery@workstation ready.\n`, "stderr"],
        [1200, `[${new Date().toISOString().slice(0, 19).replace("T", " ")},902: INFO/MainProcess] Task catalog.tasks.send_review_digest[5c1f0a7e-3b2d-4c55-9a41-0e6b1f2d7c33] received\n`, "stderr"],
        [180, `[${new Date().toISOString().slice(0, 19).replace("T", " ")},087: INFO/ForkPoolWorker-1] Task catalog.tasks.send_review_digest[5c1f0a7e-3b2d-4c55-9a41-0e6b1f2d7c33] succeeded in 0.18s: 4\n`, "stderr"],
      ];
    case "celery-beat":
      return [
        [300, `celery beat v5.4.0 (opalescent) is starting.\n__    -    ... __   -        _\nLocalTime -> ${new Date().toISOString().slice(0, 19).replace("T", " ")}\nConfiguration ->\n    . broker -> redis://localhost:6379/0\n    . loader -> celery.loaders.app.AppLoader\n    . scheduler -> celery.beat.PersistentScheduler\n`],
        [200, `[${new Date().toISOString().slice(0, 19).replace("T", " ")},551: INFO/MainProcess] beat: Starting...\n`, "stderr"],
      ];
    default: {
      const task = info.args[0] ?? "";
      const out: Record<string, string> = {
        migrate: "Operations to perform:\n  Apply all migrations: admin, auth, catalog, contenttypes, orders, sessions\nRunning migrations:\n  No migrations to apply.\n",
        makemigrations: "No changes detected\n",
        check: "System check identified no issues (0 silenced).\n",
        showmigrations: "catalog\n [X] 0001_initial\n [X] 0002_review\norders\n [X] 0001_initial\n",
      };
      return [[300, out[task] ?? `manage.py ${info.args.join(" ")}\n`], [100, { code: 0 }]];
    }
  }
}

function djangoTestSteps(): Step[] {
  const cases = [
    { name: "test_detail", line: 26, ms: 41 },
    { name: "test_list", line: 22, ms: 37 },
  ];
  const steps: Step[] = [
    [200, "Found 2 test(s).\nCreating test database for alias 'default'...\nSystem check identified no issues (0 silenced).\n", "stderr"],
    [60, tc("testCount", { count: cases.length })],
    [10, tc("testSuiteStarted", { name: "catalog.tests", locationHint: "python://catalog/tests.py" })],
    [5, tc("testSuiteStarted", { name: "BookViewsTest", locationHint: "python://catalog/tests.py:10::BookViewsTest" })],
  ];
  for (const c of cases) {
    steps.push([80, tc("testStarted", { name: c.name, locationHint: `python://catalog/tests.py:${c.line}::${c.name}` })]);
    steps.push([c.ms, tc("testFinished", { name: c.name, duration: c.ms })]);
  }
  steps.push([5, tc("testSuiteFinished", { name: "BookViewsTest" })], [5, tc("testSuiteFinished", { name: "catalog.tests" })]);
  steps.push([20, "Destroying test database for alias 'default'...\n", "stderr"], [10, { code: 0 }]);
  return steps;
}

async function webRun(r: RunRequest): Promise<{ cmd: CommandLine; steps: Step[]; test: boolean } | null> {
  const cfg = r.configuration;
  if (!cfg || !isPythonDemo()) return null;
  const head = r.interpreterArgs ?? [];
  const debug = head.includes("debugpy");
  if (cfg.type === "django-tests") {
    const o = cfg.options ?? {};
    const labels = (o.target ?? "").split(/\s+/).filter(Boolean);
    const args =
      o.runner === "pytest"
        ? [...head, "-m", "pytest", ...labels, "--junitxml", ".forge/pytest.xml"]
        : [...head, "-c", "<forge_django_tests.py>", "--manage", "manage.py", ...labels];
    return {
      cmd: { program: cfg.interpreter || PYTHON, args, cwd: cfg.workingDirectory || "", env: { DJANGO_SETTINGS_MODULE: o.settings || "bookshop.settings", PYTHONUNBUFFERED: "1" } },
      steps: djangoTestSteps(),
      test: true,
    };
  }
  if (!WEB_TYPES.has(cfg.type)) return null;
  const info = await call<WebRunInfo>("python_web_run_info", { configuration: cfg });
  const target = info.target === "module" ? ["-m", info.script] : [info.script];
  const extra = (cfg.arguments ?? "").split(/\s+/).filter(Boolean);
  const cmd: CommandLine = {
    program: cfg.interpreter || PYTHON,
    args: [...(cfg.interpreterOptions ?? "").split(/\s+/).filter(Boolean), ...head, ...target, ...info.args, ...extra],
    cwd: cfg.workingDirectory || info.workingDirectory || "",
    env: { PYTHONUNBUFFERED: "1", ...info.env, ...(cfg.env ?? {}) },
  };
  return { cmd, steps: serverSteps(cfg, info, debug), test: false };
}

registerMockRunHandler({
  commandLine: async (r) => (await webRun(r).catch(() => null))?.cmd ?? null,
  start: async (r, runId) => {
    const run = await webRun(r);
    if (!run) return null;
    return startMockRun(runId, run.cmd, run.steps, {
      testRunner: run.test || !!r.testRunner,
      testReport: run.test ? "teamCity" : undefined,
      configuration: run.test ? r.configuration : undefined,
    });
  },
});
