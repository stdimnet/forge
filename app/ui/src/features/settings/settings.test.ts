import { describe, expect, it } from "vitest";
import { createRoot } from "solid-js";
import { settingsApi } from "@/ipc/settings";
import "@/mock/settingsMock";
import { cssVars, fontStack, rootFlags } from "./apply";
import { createSettingsSearch, nodesById, segmentsOf } from "./search";

// Tree building, filtering and highlighting are tested in Rust
// (`forge_plugin_api::settings_model`); these check the round trip the dialog relies on.
describe("settings search", () => {
  const pages = [
    { id: "editor.code-style", title: "Code Style", group: "Editor" },
    { id: "editor.code-style.php", title: "PHP", group: "Editor", parent: "editor.code-style" },
    { id: "plugins", title: "Plugins", group: "Plugins", keywords: ["marketplace"] },
  ];

  it("returns the tree, matches and breadcrumbs", async () => {
    const res = await settingsApi.search({ query: "php", pages, highlight: ["PHP"] });
    expect(res.tree.map((n) => n.id)).toEqual(["group:Editor", "plugins"]);
    expect(res.matches).toEqual(["editor.code-style.php"]);
    expect(res.firstMatch).toBe("editor.code-style.php");
    expect(res.breadcrumbs["editor.code-style.php"]).toEqual(["group:Editor", "editor.code-style", "editor.code-style.php"]);
    expect(nodesById(res.tree).get("editor.code-style")?.pages).toEqual(["editor.code-style", "editor.code-style.php"]);
    expect(res.highlights[0]).toEqual([{ text: "PHP", hit: true }]);
  });

  it("validates values against the schema", async () => {
    expect(await settingsApi.validate("editor.font.size", 14)).toBeNull();
    expect(await settingsApi.validate("editor.font.size", "huge")).toMatch(/expected a value of type number/);
  });

  it("keeps the latest answer and maps highlights by text", async () => {
    const result = createRoot(() => createSettingsSearch(() => ({ query: "market", pages, highlight: ["Marketplace"] })));
    await expect.poll(() => result().ready).toBe(true);
    expect(result().response.matches).toEqual(["plugins"]);
    expect(segmentsOf(result(), "Marketplace")[0]).toEqual({ text: "Market", hit: true });
    expect(segmentsOf(result(), "unknown")).toEqual([{ text: "unknown", hit: false }]);
  });
});

describe("applying settings", () => {
  const values: Record<string, unknown> = {
    "editor.font.family": "Fira Code",
    "editor.font.size": 15,
    "editor.font.lineHeight": 1.4,
    "editor.font.ligatures": false,
    "appearance.uiFont.family": "system-ui",
    "appearance.uiFont.size": 14,
    "appearance.compactMode": true,
    "appearance.showToolWindowBars": false,
    "appearance.smoothScrolling": true,
    "appearance.showToolWindowNumbers": false,
  };
  const get = <T,>(key: string) => values[key] as T;

  it("maps fonts and sizes to CSS variables", () => {
    const vars = cssVars(get);
    expect(vars["--fs-editor"]).toBe("15px");
    expect(vars["--font-mono"].startsWith('"Fira Code", "JetBrains Mono"')).toBe(true);
    expect(vars["--editor-ligatures"]).toBe("none");
    expect(vars["--fs-small"]).toBe("13px");
    expect(rootFlags(get)).toMatchObject({ compact: true, hideStripes: true, smoothLists: true, toolWindowNumbers: false });
  });

  it("does not repeat the chosen font in the fallback stack", () => {
    expect(fontStack("Menlo", "Menlo, monospace")).toBe('"Menlo", monospace');
    expect(fontStack("", "monospace")).toBe("monospace");
  });
});

describe("the Settings dialog window", () => {
  it("can be kept ready in the background, which the user can turn off (#244)", async () => {
    // The hidden window costs memory (see `prewarm` in shell/window/dialogs.ts).
    const spec = (await settingsApi.schema()).find((s) => s.key === "system.preloadSettings");
    expect(spec).toMatchObject({ type: "bool", default: true, group: "system" });
  });
});
