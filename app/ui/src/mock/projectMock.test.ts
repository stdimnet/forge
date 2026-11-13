import { describe, expect, it } from "vitest";
import { invoke, listen } from "@/ipc";
import { CREATE_EVENT, type CreateEvent, type Generator, type ProjectInfo } from "@/ipc/project";
import { mockPlan } from "./projectMock";

describe("project mock", () => {
  it("detects the demo Laravel project", async () => {
    const info = await invoke<ProjectInfo>("project_detect");
    expect(info.primary?.id).toBe("laravel");
    expect(info.packs).toContain("phpforge");
  });

  it("lists the generators of the Rust snapshot", async () => {
    const gens = await invoke<Generator[]>("project_generators_list");
    expect(gens.map((g) => g.id)).toContain("love-game");
  });

  it("previews the same commands as forge_project::plan", () => {
    const plan = mockPlan({
      generator: "laravel",
      location: "/projects",
      name: "demo",
      options: { starterKit: "vue", git: true },
    });
    expect(plan.preview).toEqual(["composer create-project laravel/vue-starter-kit demo --no-interaction", "git init"]);
    const love = mockPlan({ generator: "love-game", location: "~/P", name: "Pixel Dungeon", options: {} });
    expect(love.preview).toEqual(["Create Pixel Dungeon/ from the built-in template"]);
    expect(love.files).toContain("conf.lua");
    const nvim = mockPlan({ generator: "neovim-plugin", location: "~/P", name: "my-plugin.nvim", options: {} });
    expect(nvim.files).toContain("lua/my_plugin_nvim/init.lua");
    expect(mockPlan({ generator: "empty", location: "", name: "x", options: {} }).error).toBe("Choose a location");
  });

  it("simulates creation without touching files", async () => {
    const events: CreateEvent[] = [];
    const off = await listen<CreateEvent>(CREATE_EVENT, (e) => events.push(e));
    const root = await invoke<string>("project_create", {
      generator: "empty",
      location: "~/Projects",
      name: "scratch",
      options: { git: true },
    });
    off();
    expect(root).toBe("~/Projects/scratch");
    expect(events[0]).toMatchObject({ type: "step", index: 0, total: 2 });
    expect(events.at(-1)).toEqual({ type: "done", root, name: "scratch" });
  });
});
