import { describe, expect, it } from "vitest";
import type { Generator } from "@/ipc/project";
import generatorsJson from "@/mock/projectGenerators.json";
import {
  applyCreateEvent,
  carryOptions,
  defaultOptions,
  groupGenerators,
  initialProgress,
  isOptionVisible,
  withChoices,
  joinPath,
  MAX_LINES,
  nameAfterSwitch,
  previewLines,
  progressPercent,
  tildify,
  validateName,
} from "./wizard";

const gens = generatorsJson as unknown as Generator[];
const gen = (id: string) => gens.find((g) => g.id === id)!;

describe("generator list", () => {
  it("groups by pack in PHP, Web, Java, Lua, Ruby, .NET, Other order", () => {
    const groups = groupGenerators(gens);
    expect(groups.map((g) => g.group)).toEqual(["PHP", "Web", "Java", "Lua", "Ruby", ".NET", "Other"]);
    expect(groups[0].items.map((g) => g.id)).toEqual([
      "empty-php",
      "laravel",
      "symfony",
      "composer-package",
      "wordpress-plugin",
    ]);
    expect(groups[2].items.map((g) => g.name)).toEqual(["Minecraft Plugin"]);
    expect(groups[3].items.map((g) => g.name)).toEqual(["Lua Script", "LÖVE Game", "Neovim Plugin"]);
  });

  it("puts unknown groups last", () => {
    const extra = { ...gen("empty"), id: "x", group: "Rust" };
    expect(groupGenerators([extra, ...gens]).at(-1)?.group).toBe("Rust");
  });
});

describe("options", () => {
  it("starts from the generator defaults", () => {
    expect(defaultOptions(gen("laravel"))).toEqual({ starterKit: "none", git: false });
  });

  it("keeps shared choices when switching generators", () => {
    const prev = { phpVersion: "8.1", git: true, sample: false, starterKit: "vue" };
    expect(carryOptions(gen("composer-package"), prev)).toEqual({
      vendor: "acme",
      phpVersion: "8.1",
      sample: false,
      git: true,
    });
    // An enum value the next generator does not offer falls back to its default.
    expect(carryOptions(gen("lua-script"), { luaVersion: "6.0" }).luaVersion).toBe("5.4");
  });
});

describe("conditional options", () => {
  const mc = gen("minecraft-plugin");
  const option = (id: string) => mc.options.find((o) => o.id === id)!;

  it("shows options only for the platforms and builds they apply to", () => {
    const values = defaultOptions(mc);
    expect(isOptionVisible(option("mcVersion"), values, mc)).toBe(true);
    expect(isOptionVisible(option("descriptor"), values, mc)).toBe(true);
    expect(isOptionVisible(option("runPaper"), values, mc)).toBe(true);
    const velocity = { ...values, platform: "velocity" };
    expect(isOptionVisible(option("mcVersion"), velocity, mc)).toBe(false);
    expect(isOptionVisible(option("descriptor"), velocity, mc)).toBe(false);
    const maven = { ...values, buildSystem: "maven" };
    expect(isOptionVisible(option("language"), maven, mc)).toBe(false);
    expect(isOptionVisible(option("runPaper"), maven, mc)).toBe(false);
    expect(isOptionVisible(option("groupId"), maven, mc)).toBe(true);
  });

  it("replaces refreshed choices", () => {
    const next = withChoices(gens, "minecraft-plugin", "mcVersion", [{ value: "1.21.11", label: "1.21.11 (Java 21)" }]);
    expect(next.find((g) => g.id === "minecraft-plugin")!.options.find((o) => o.id === "mcVersion")!.choices).toHaveLength(1);
    expect(option("mcVersion").refresh).toBe("minecraft_versions_refresh");
    expect(next.find((g) => g.id === "laravel")).toBe(gen("laravel"));
  });
});

describe("name and location", () => {
  it("replaces an untouched default name only", () => {
    expect(nameAfterSwitch("laravel-app", gen("laravel"), gen("symfony"))).toBe("symfony-app");
    expect(nameAfterSwitch("", gen("laravel"), gen("vue"))).toBe("vue-app");
    expect(nameAfterSwitch("shop", gen("laravel"), gen("symfony"))).toBe("shop");
  });

  it("validates names like the backend", () => {
    expect(validateName("my-app")).toBeNull();
    expect(validateName("  ")).toBe("Enter a project name");
    expect(validateName("..")).toBe("Invalid project name");
    expect(validateName("a/b")).toBe("The name must not contain '/'");
    expect(validateName("a:b")).toBe("The name must not contain ':'");
  });

  it("joins paths with the location's separator", () => {
    expect(joinPath("~/Projects", "app")).toBe("~/Projects/app");
    expect(joinPath("/home/me/", "app")).toBe("/home/me/app");
    expect(joinPath("C:\\Users\\me\\Projects", "app")).toBe("C:\\Users\\me\\Projects\\app");
    expect(tildify("/home/me/Projects/app", "/home/me/")).toBe("~/Projects/app");
    expect(tildify("/srv/app", "/home/me")).toBe("/srv/app");
  });
});

describe("preview", () => {
  it("starts with cd and lists template files", () => {
    const lines = previewLines(
      {
        target: "~/Projects/game",
        preview: ["Create game/ from the built-in template", "git init"],
        files: ["main.lua", "conf.lua"],
        missingTools: [],
      },
      "~/Projects",
    );
    expect(lines).toEqual([
      { kind: "cd", text: "cd ~/Projects" },
      { kind: "template", text: "Create game/ from the built-in template" },
      { kind: "file", text: "main.lua" },
      { kind: "file", text: "conf.lua" },
      { kind: "command", text: "git init" },
    ]);
    expect(previewLines(null, "x")).toEqual([]);
  });
});

describe("creation progress", () => {
  it("follows steps, output and completion", () => {
    let p = initialProgress();
    expect(progressPercent(p)).toBe(4);
    p = applyCreateEvent(p, { type: "step", index: 0, total: 2, label: "composer create-project" });
    expect(p.lines.at(-1)).toEqual({ text: "$ composer create-project", stderr: false });
    expect(progressPercent(p)).toBe(25);
    p = applyCreateEvent(p, { type: "output", line: "Installing", stderr: true });
    p = applyCreateEvent(p, { type: "output", line: "   ", stderr: false });
    expect(p.lines).toHaveLength(2);
    p = applyCreateEvent(p, { type: "step", index: 1, total: 2, label: "git init" });
    expect(progressPercent(p)).toBe(75);
    p = applyCreateEvent(p, { type: "done", root: "/p/app", name: "app" });
    expect(p.phase).toBe("done");
    expect(p.root).toBe("/p/app");
    expect(progressPercent(p)).toBe(100);
  });

  it("records failures and caps the log", () => {
    let p = initialProgress();
    for (let i = 0; i < MAX_LINES + 50; i++) p = applyCreateEvent(p, { type: "output", line: `l${i}`, stderr: false });
    expect(p.lines).toHaveLength(MAX_LINES);
    expect(p.lines[0].text).toBe("l50");
    p = applyCreateEvent(p, { type: "failed", error: "composer exited with code 1" });
    expect(p.phase).toBe("failed");
    expect(p.error).toBe("composer exited with code 1");
  });
});
