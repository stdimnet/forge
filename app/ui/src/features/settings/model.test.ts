import { describe, expect, it } from "vitest";
import type { SettingsPage } from "@/features/settingsPages";
import { breadcrumb, buildTree, filterTree, flatten, groupNodeId, highlight, matchesQuery } from "./model";
import { cssVars, fontStack, rootFlags } from "./apply";

const component = () => null;
const page = (p: Omit<SettingsPage, "component">): SettingsPage => ({ ...p, component });

const pages: SettingsPage[] = [
  page({ id: "tools.terminal", title: "Terminal", group: "Tools" }),
  page({ id: "editor.code-style.php", title: "PHP", group: "Editor", parent: "editor.code-style" }),
  page({ id: "editor.code-style", title: "Code Style", group: "Editor" }),
  page({ id: "plugins", title: "Plugins", group: "Plugins", keywords: ["marketplace"] }),
  page({ id: "appearance", title: "Appearance", group: "Appearance & Behavior" }),
  page({ id: "custom", title: "Custom", group: "Zeta" }),
];

describe("settings tree", () => {
  const tree = buildTree(pages);

  it("orders groups like PHPStorm and keeps title==group pages top-level", () => {
    expect(tree.map((n) => n.title)).toEqual(["Appearance & Behavior", "Editor", "Plugins", "Tools", "Zeta"]);
    expect(tree[2].id).toBe("plugins");
    expect(tree[0].id).toBe(groupNodeId("Appearance & Behavior"));
  });

  it("nests pages under their parent page", () => {
    expect(breadcrumb(tree, "editor.code-style.php").map((n) => n.title)).toEqual(["Editor", "Code Style", "PHP"]);
    expect(flatten(tree)).toHaveLength(pages.length + 4);
  });

  it("filters by title, keywords and extra page text, keeping ancestors", () => {
    const extra = (id: string) => (id === "editor.code-style.php" ? "Braces placement" : "");
    expect(flatten(filterTree(tree, "braces", extra)).map((n) => n.id)).toEqual([
      groupNodeId("Editor"),
      "editor.code-style",
      "editor.code-style.php",
    ]);
    expect(filterTree(tree, "marketplace", extra).map((n) => n.id)).toEqual(["plugins"]);
    expect(filterTree(tree, "", extra)).toBe(tree);
  });
});

describe("search helpers", () => {
  it("matches all words case-insensitively", () => {
    expect(matchesQuery("Show tool window bars", "TOOL bars")).toBe(true);
    expect(matchesQuery("Show tool window bars", "tool font")).toBe(false);
  });

  it("splits text into highlighted segments", () => {
    expect(highlight("Editor Font", "font")).toEqual([
      { text: "Editor ", hit: false },
      { text: "Font", hit: true },
    ]);
    expect(highlight("abc", "")).toEqual([{ text: "abc", hit: false }]);
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
  };
  const get = <T,>(key: string) => values[key] as T;

  it("maps fonts and sizes to CSS variables", () => {
    const vars = cssVars(get);
    expect(vars["--fs-editor"]).toBe("15px");
    expect(vars["--font-mono"].startsWith('"Fira Code", "JetBrains Mono"')).toBe(true);
    expect(vars["--editor-ligatures"]).toBe("none");
    expect(vars["--fs-small"]).toBe("13px");
    expect(rootFlags(get)).toMatchObject({ compact: true, hideStripes: true });
  });

  it("does not repeat the chosen font in the fallback stack", () => {
    expect(fontStack("Menlo", "Menlo, monospace")).toBe('"Menlo", monospace');
    expect(fontStack("", "monospace")).toBe("monospace");
  });
});
