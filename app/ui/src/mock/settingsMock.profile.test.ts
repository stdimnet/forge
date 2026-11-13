import { beforeAll, describe, expect, it } from "vitest";
import { settingsApi } from "@/ipc/settings";

// `?packs=phpforge` is browser mode's `forge --packs phpforge`; the mock reads it on load.
beforeAll(async () => {
  history.replaceState(null, "", "?packs=phpforge");
  await import("./settingsMock");
});

describe("browser mode started with some packs", () => {
  it("turns the plugins of the other packs off", async () => {
    const profile = await settingsApi.launchProfile();
    expect(profile.all).toBe(false);
    expect(profile.packs).toEqual(["phpforge"]);
    expect(profile.hidden).toContain("forge.lua");
    expect(profile.hidden).toContain("forge.react");
    // The pack itself, the base packs and HTML/CSS/JS for its templates stay.
    for (const id of ["forge.php", "forge.laravel", "forge.git", "forge.yaml", "forge.bash", "forge.web"]) {
      expect(profile.hidden).not.toContain(id);
    }
    const plugins = await settingsApi.plugins();
    expect(plugins.filter((p) => profile.hidden.includes(p.id)).every((p) => !p.enabled)).toBe(true);
    expect(plugins.find((p) => p.id === "forge.laravel")?.enabled).toBe(true);
    const schema = await settingsApi.schema();
    expect(schema.some((s) => s.key === "php.languageLevel")).toBe(true);
  });

  it("keeps them off and out of the stored disabled plugins", async () => {
    await expect(settingsApi.setPluginEnabled("forge.lua", true)).rejects.toThrow(/packs Forge was started with/);
    await settingsApi.setPluginEnabled("forge.xdebug", false);
    expect((await settingsApi.getAll()).user["plugins.disabled"]).toEqual(["forge.xdebug"]);
    await settingsApi.setPluginEnabled("forge.xdebug", true);
    expect((await settingsApi.getAll()).user["plugins.disabled"]).toEqual([]);
  });
});
