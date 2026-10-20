import { describe, expect, it } from "vitest";
import "./settingsMock";
import { listen } from "@/ipc";
import { SETTINGS_CHANGED_EVENT, settingsApi, type SettingsChange } from "@/ipc/settings";
import { getSetting, loadSettings, saveSetting, scopeFor, settingsState } from "@/state/settings";

describe("browser-mode settings backend", () => {
  it("serves the built-in plugins and their schema", async () => {
    const plugins = await settingsApi.plugins();
    expect(plugins.find((p) => p.id === "forge.core")?.required).toBe(true);
    const schema = await settingsApi.schema();
    expect(schema.find((s) => s.key === "editor.font.size")?.type).toBe("number");
    expect(schema.find((s) => s.key === "php.languageLevel")?.project).toBe(true);
  });

  it("layers project over user over defaults and emits changes", async () => {
    const seen: SettingsChange[] = [];
    const stop = await listen<SettingsChange>(SETTINGS_CHANGED_EVENT, (c) => seen.push(c));
    await settingsApi.set("editor.tabSizeTest", 2, "user");
    await settingsApi.set("editor.tabSizeTest", 8, "project");
    expect((await settingsApi.getAll()).values["editor.tabSizeTest"]).toBe(8);
    await settingsApi.set("editor.tabSizeTest", null, "project");
    expect((await settingsApi.getAll()).values["editor.tabSizeTest"]).toBe(2);
    expect(seen.map((c) => [c.value, c.scope])).toEqual([
      [2, "user"],
      [8, "project"],
      [2, "user"],
    ]);
    stop();
  });

  it("rejects values of the wrong type", async () => {
    await expect(settingsApi.set("editor.font.size", "huge", "user")).rejects.toThrow();
  });

  it("stores lists of tables and rejects duplicate pairs", async () => {
    const smarty = { template: "smarty", language: "sql", left: "[[", right: "]]" };
    await settingsApi.set("templates.delimiters", [smarty], "user");
    expect((await settingsApi.getAll()).values["templates.delimiters"]).toEqual([smarty]);
    await expect(settingsApi.set("templates.delimiters", [smarty, { ...smarty, left: "<<" }], "user")).rejects.toThrow(/duplicate/);
    await expect(settingsApi.set("templates.delimiters", [{ template: 1 } as never], "user")).rejects.toThrow();
    await settingsApi.set("templates.delimiters", null, "user");
  });

  it("cascades plugin enablement over dependencies", async () => {
    expect(await settingsApi.setPluginEnabled("forge.composer", false)).toEqual([
      "forge.composer",
      "forge.laravel",
      "forge.symfony",
    ]);
    expect(await settingsApi.setPluginEnabled("forge.laravel", true)).toEqual(["forge.composer", "forge.laravel"]);
    await expect(settingsApi.setPluginEnabled("forge.core", false)).rejects.toThrow();
    await settingsApi.setPluginEnabled("forge.symfony", true);
  });
});

describe("settings state", () => {
  it("loads values, falls back to defaults and picks the scope from the schema", async () => {
    await loadSettings();
    expect(settingsState.loaded).toBe(true);
    expect(getSetting("editor.font.family")).toBe("JetBrains Mono");
    expect(scopeFor("php.languageLevel")).toBe("project");
    expect(scopeFor("editor.font.size")).toBe("user");

    await saveSetting("editor.font.size", 16);
    expect(getSetting("editor.font.size")).toBe(16);
    expect(settingsState.user["editor.font.size"]).toBe(16);
    await saveSetting("editor.font.size", null);
    expect(getSetting("editor.font.size")).toBe(13.5);
  });
});
