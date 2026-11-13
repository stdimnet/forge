import { describe, expect, it } from "vitest";
import type { ManageCommands, WebCompletions, WebFileMarks, WebLocation, WebProject, WebRunInfo } from "@/ipc/pythonWeb";

// The Django demo is chosen by the page URL, before the mocks load.
history.replaceState(null, "", "/?project=django");
const { invoke } = await import("@/ipc");
const { pythonDemoFiles } = await import("./demoPythonProject");
await import("./pythonWebMock");

describe("Python web frameworks mock (pyforge-frameworks through WebAssembly)", () => {
  it("reads the Django demo's URLs, models, tasks and templates", async () => {
    const p = await invoke<WebProject>("python_web_project", { refresh: false });
    expect(p.frameworks).toEqual(["django", "celery"]);
    expect(p.django?.settingsModule).toBe("bookshop.settings");
    expect(p.routes.map((r) => [r.path, r.name ?? ""])).toContainEqual(["/books/<slug:slug>/", "catalog:book-detail"]);
    expect(p.models.map((m) => m.name)).toEqual(["Author", "Book", "Review", "Order", "OrderLine"]);
    expect(p.tasks.map((t) => t.name)).toEqual(["bookshop.celery.debug_task", "catalog.tasks.send_review_digest", "catalog.reindex"]);
    expect(p.templates.map((t) => t.name)).toContain("catalog/book_detail.html");
    expect(p.celery?.app).toBe("bookshop");
  });

  it("navigates and completes template and URL names", async () => {
    const path = "catalog/templates/catalog/book_list.html";
    const content = pythonDemoFiles[path];
    const at = content.indexOf("catalog:book-detail") + 3;
    const targets = await invoke<WebLocation[]>("python_web_goto", { path, content, offset: at });
    expect(targets[0].path).toBe("catalog/urls.py");
    const typed = "{% url 'catalog:";
    const done = await invoke<WebCompletions>("python_web_complete", { path, content: typed, offset: typed.length });
    expect(done.items.map((i) => i.label)).toContain("catalog:add-review");
    const marks = await invoke<WebFileMarks>("python_web_marks", { path: "catalog/views.py", content: pythonDemoFiles["catalog/views.py"] });
    expect(marks.problems.map((p) => p.message)).toEqual(["Template file 'catalog/author_detail.html' not found"]);
  });

  it("lists manage.py commands and builds run command lines", async () => {
    const list = await invoke<ManageCommands>("python_manage_commands", { refresh: false });
    expect(list.commands.find((c) => c.name === "import_books")?.app).toBe("catalog");
    const run = await invoke<WebRunInfo>("python_web_run_info", { configuration: { name: "w", type: "celery-worker", options: {} } });
    expect(run.args).toEqual(["-A", "bookshop", "worker", "--loglevel", "INFO"]);
  });
});
