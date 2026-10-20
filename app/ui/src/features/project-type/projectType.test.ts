import { describe, expect, it } from "vitest";
import type { ProjectInfo } from "@/ipc/project";
import { frameworkLabel, pluginAppliesTo, usesProjectType } from "@/state/project";
import { announcement } from "./announce";

const laravel: ProjectInfo = {
  languages: ["php", "javascript"],
  frameworks: [
    { id: "laravel", name: "Laravel", version: "12.4.1", language: "php" },
    { id: "vite", name: "Vite", version: "6.2.0", language: "javascript" },
  ],
  packs: ["phpforge", "webforge"],
  tools: ["composer", "git"],
  primary: { id: "laravel", name: "Laravel", version: "12.4.1", language: "php" },
};

const plugins = [
  { id: "forge.php", name: "PHP", enabled: true, projectTypes: ["php"] },
  { id: "forge.laravel", name: "Laravel", enabled: true, projectTypes: ["laravel"] },
  { id: "forge.symfony", name: "Symfony", enabled: true, projectTypes: ["symfony"] },
  { id: "forge.git", name: "Git", enabled: true },
];

describe("project types", () => {
  it("matches languages, frameworks, packs and tools", () => {
    for (const id of ["php", "laravel", "webforge", "composer"]) expect(usesProjectType(id, laravel)).toBe(true);
    expect(usesProjectType("symfony", laravel)).toBe(false);
    expect(usesProjectType("php", null)).toBe(false);
  });

  it("activates plugins only for matching projects", () => {
    expect(pluginAppliesTo(undefined, laravel)).toBe(true);
    expect(pluginAppliesTo([], null)).toBe(true);
    expect(pluginAppliesTo(["laravel"], laravel)).toBe(true);
    expect(pluginAppliesTo(["symfony", "drupal"], laravel)).toBe(false);
    expect(pluginAppliesTo(["laravel"], null)).toBe(false);
  });

  it("labels frameworks with the major version and languages with major.minor", () => {
    expect(frameworkLabel(laravel.primary)).toBe("Laravel 12");
    expect(frameworkLabel({ id: "php", name: "PHP", version: "8.3", language: "php" })).toBe("PHP 8.3");
    expect(frameworkLabel({ id: "sv", name: "Svelte", version: "0.6.1", language: "javascript" })).toBe("Svelte 0.6");
    expect(frameworkLabel({ id: "neovim", name: "Neovim", language: "lua" })).toBe("Neovim");
    expect(frameworkLabel(null)).toBe("");
  });
});

describe("detection balloon", () => {
  it("announces a detected framework once per project", () => {
    const a = announcement(laravel, "/p/shop", plugins, []);
    expect(a).toEqual({
      key: "/p/shop#laravel",
      title: "Laravel project detected",
      detail: "Laravel support enabled.",
      settingsPages: ["lang.php.laravel", "lang.laravel", "plugins"],
    });
    expect(announcement(laravel, "/p/shop", plugins, ["/p/shop#laravel"])).toBeNull();
    expect(announcement(laravel, "/p/other", plugins, ["/p/shop#laravel"])?.key).toBe("/p/other#laravel");
  });

  it("stays quiet for plain languages and disabled plugins", () => {
    const php: ProjectInfo = { ...laravel, frameworks: [], primary: { id: "php", name: "PHP", language: "php" } };
    expect(announcement(php, "/p", plugins, [])).toBeNull();
    const disabled = plugins.map((p) => (p.id === "forge.laravel" ? { ...p, enabled: false } : p));
    expect(announcement(laravel, "/p", disabled, [])).toBeNull();
  });
});
