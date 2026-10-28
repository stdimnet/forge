import { describe, expect, it } from "vitest";
import type { CommandLine } from "@/ipc/run";
import type { RailsCompletions, RailsFileMarks, RailsGenerators, RailsLocation, RailsProject } from "@/ipc/rails";

// The Rails demo is chosen by the page URL, before the mocks load.
history.replaceState(null, "", "/?project=rails");
const { invoke } = await import("@/ipc");
const { railsDemoFiles } = await import("./demoRailsProject");
await import("./dbMock");
await import("./railsMock");

const offset = (text: string, needle: string, plus = 0) => text.indexOf(needle) + plus;

describe("Rails mock (rubyforge-fw-rails through WebAssembly)", () => {
  it("reads the blog demo's routes, models, locales, Stimulus controllers and databases", async () => {
    const p = await invoke<RailsProject>("rails_project", { refresh: false });
    expect(p.rails).toBe(true);
    expect(p.version).toBe("7.2.1");
    expect(p.routes.map((r) => r.name).filter(Boolean)).toContain("publish_article");
    expect(p.routes.find((r) => r.name === "admin_users")?.controller).toBe("admin/users");
    expect(p.models.map((m) => m.name)).toEqual(["Article", "Comment", "User"]);
    expect(p.models[0].columns.map((c) => c.name)).toContain("slug");
    expect(p.locales).toEqual(["en", "de"]);
    expect(p.stimulus.map((s) => s.identifier)).toEqual(["clipboard", "hello"]);
    expect(p.databases.map((d) => [d.environment, d.database, d.hasPassword])).toEqual([
      ["development", "blog_development", true],
      ["test", "blog_test", true],
      ["production", "blog_production", true],
    ]);
  });

  it("navigates, completes and marks views", async () => {
    const path = "app/views/articles/show.html.erb";
    const content = railsDemoFiles[path];
    const targets = await invoke<RailsLocation[]>("rails_goto", { path, content, offset: offset(content, "edit_article_path", 2) });
    expect(targets[0]).toMatchObject({ path: "config/routes.rb", line: 5 });
    const typed = "<%= t(\".";
    const done = await invoke<RailsCompletions>("rails_complete", { path, content: typed, offset: typed.length });
    expect(done.items.map((i) => i.label)).toContain(".copy_link");
    const marks = await invoke<RailsFileMarks>("rails_marks", { path, content });
    expect(marks.gutter[0].tooltip).toBe("Go to ArticlesController#show");
    expect(marks.folds.map((f) => f.text)).toContain("Copy link");
  });

  it("lists generators and builds generator and console command lines", async () => {
    const g = await invoke<RailsGenerators>("rails_generators", { refresh: false });
    expect(g.source).toBe("rails");
    expect(g.generators.map((x) => x.name)).toContain("active_record:multi_db");
    const gen = await invoke<CommandLine>("rails_generate_command", { generator: "model", arguments: "Tag name:string", options: ["--pretend"], destroy: false });
    expect(gen.args).toEqual(["bin/rails", "generate", "model", "Tag", "name:string", "--pretend"]);
    const c = await invoke<CommandLine>("rails_console_command", { kind: "console" });
    expect(c.args).toEqual(["bin/rails", "console"]);
  });

  it("adds the databases of database.yml as data sources once", async () => {
    const added = await invoke<{ name: string; hasPassword: boolean }[]>("rails_add_data_sources", { environments: ["development", "test"] });
    expect(added.map((d) => d.name)).toEqual(["blog_development@localhost (development)", "blog_test@localhost (test)"]);
    expect(added[0].hasPassword).toBe(true);
    expect(await invoke("rails_add_data_sources", { environments: ["development"] })).toEqual([]);
  });
});
