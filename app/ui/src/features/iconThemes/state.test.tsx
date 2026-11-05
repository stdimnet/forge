import { afterEach, describe, expect, it } from "vitest";
import type { JSX } from "solid-js";
import { render as solidRender } from "solid-js/web";
import { setLocalSetting } from "@/state/settings";
import { forgeWasm } from "@/wasm";
import { FileIcon } from "@/shell/FileIcon";
import { setTheme } from "@/state/theme";
import { SymbolIcon } from "@/features/php/SymbolIcon";
import { TreeArrow } from "@/shell/TreeArrow";
import {
  folderColor,
  iconLook,
  iconStyle,
  iconThemes,
  iconVariantIsLight,
  registerIconTheme,
  themedIcon,
  themedIconKey,
  themedSymbol,
  themedUiIcon,
  ASSOCIATIONS_KEY,
  FOLDER_COLORS_KEY,
  THEME_KEY,
  type IconThemeData,
} from "./state";
import { UiIcon } from "./UiIcon";

const svg = (color: string) => `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><rect width="16" height="16" fill="${color}"/></svg>`;

const data: IconThemeData = {
  definition: {
    icons: { file: "file.svg", php: "php.svg", folder: "folder.svg", src: "src.svg" },
    file: "file",
    folder: "folder",
    fileExtensions: { php: "php" },
    folderNames: { src: "src" },
    options: { treeArrows: "triangle", iconSize: 16 },
  },
  svgs: { file: svg("#999"), php: svg("#88f"), folder: svg("#da4"), src: svg("#48d") },
};

function render(ui: () => JSX.Element) {
  const container = document.createElement("div");
  document.body.appendChild(container);
  disposers.push(solidRender(ui, container), () => container.remove());
  return { container };
}
const disposers: (() => void)[] = [];

const settle = async () => {
  await forgeWasm();
  for (let i = 0; i < 5; i++) await new Promise((r) => setTimeout(r, 0));
};

let remove: (() => void) | undefined;
afterEach(async () => {
  disposers.splice(0).forEach((d) => d());
  remove?.();
  setLocalSetting(THEME_KEY, "");
  setLocalSetting(ASSOCIATIONS_KEY, []);
  setLocalSetting("fileIcons.treeArrows", "theme");
  setLocalSetting("fileIcons.monochrome", "theme");
  setLocalSetting("fileIcons.accentColor", "");
  setLocalSetting("fileIcons.tintMode", "layer");
  setLocalSetting(FOLDER_COLORS_KEY, []);
  setTheme("dark");
  await settle();
});

const tintSvg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><path d="M1 3h14v10H1z" fill="#da4" data-forge-tint/><circle cx="11" cy="10" r="2" fill="#48d"/></svg>`;

const v2: IconThemeData = {
  definition: {
    version: 2,
    icons: {
      file: "file.svg",
      folder: "folder.svg",
      root: "root.svg",
      hollow: "hollow.svg",
      cls: "cls.svg",
      static: "static.svg",
      project: "project.svg",
      run: "run.svg",
      sh: "sh.svg",
      ctl: "ctl.svg",
    },
    iconsLight: { project: "project-light.svg" },
    file: "file",
    folder: "folder",
    rootFolder: "root",
    folderHollow: "hollow",
    fileTypes: { shell: "sh" },
    pathRules: [{ pattern: "Controllers/*.php", icon: "ctl" }],
    symbols: { class: "cls", method: "cls", "marker.static": "static" },
    ui: { "toolwindow.project": "project", "action.run": "run" },
    options: { treeArrows: "plusMinus" },
  },
  svgs: {
    file: svg("#999"),
    folder: tintSvg,
    root: svg("#a52"),
    hollow: svg("#fff"),
    cls: svg("#33c"),
    static: svg("#ec0"),
    project: svg("#7ae"),
    project_light: svg("#26b"),
    run: svg("#4a7"),
    sh: svg("#3a5"),
    ctl: svg("#e73"),
  },
};

describe("icon themes", () => {
  it("draws format 2 sections: root, hollow, path rules, symbols and UI icons", async () => {
    remove = registerIconTheme({ id: "test.v2", label: "V2", load: async () => v2 });
    setLocalSetting(THEME_KEY, "test.v2");
    await settle();
    expect(themedIconKey("shop", "folderOpen", { root: true })).toBe("root");
    expect(themedIconKey("app", "folder", { hollow: true })).toBe("hollow");
    expect(themedIconKey("app", "folder")).toBe("folder");
    expect(themedIconKey("app/Http/Controllers/UserController.php")).toBe("ctl");
    expect(themedSymbol("class")).toEqual({ icon: "cls", markers: [] });
    expect(themedSymbol("method", { isStatic: true })).toEqual({ icon: "cls", markers: ["static"] });
    expect(themedSymbol("trait")).toBeNull();
    expect(themedUiIcon("toolwindow.project")).toBe("project");
    expect(iconLook().arrows).toBe("plusMinus");

    const { container } = render(() => (
      <div>
        <SymbolIcon kind="method" visibility="public" modifiers={{ isStatic: true }} />
        <SymbolIcon kind="trait" />
        <UiIcon id="action.run" size={16}>
          <b class="fallback" />
        </UiIcon>
        <UiIcon id="action.debug" size={16}>
          <b class="fallback" />
        </UiIcon>
        <TreeArrow open />
      </div>
    ));
    expect(container.querySelector('[data-symbol-kind="method"] .themed-symbol-marker')).not.toBeNull();
    expect(container.querySelector(".php-visibility-public")).not.toBeNull();
    expect(container.querySelector(".php-badge-trait")).not.toBeNull();
    expect(container.querySelector('[data-ui-icon="action.run"]')).not.toBeNull();
    expect(container.querySelectorAll(".fallback").length).toBe(1);
    expect(container.querySelector(".tree-plus-minus")).not.toBeNull();
  });

  it("switches to light variants live with the UI theme", async () => {
    remove = registerIconTheme({ id: "test.v2", label: "V2", load: async () => v2 });
    setLocalSetting(THEME_KEY, "test.v2");
    await settle();
    expect(themedUiIcon("toolwindow.project")).toBe("project");
    setTheme("high-contrast-light");
    await settle();
    expect(iconVariantIsLight()).toBe(true);
    expect(themedUiIcon("toolwindow.project")).toBe("project_light");
    setTheme("high-contrast");
    await settle();
    expect(themedUiIcon("toolwindow.project")).toBe("project");
  });

  it("recolors only the tint layer, or the whole icon in mask mode", async () => {
    remove = registerIconTheme({ id: "test.v2", label: "V2", load: async () => v2 });
    setLocalSetting(THEME_KEY, "test.v2");
    setLocalSetting("fileIcons.accentColor", "#ff0000");
    await settle();
    const layered = iconStyle("folder", "folder", 16);
    expect(layered["mask-image"]).toBeUndefined();
    const url = decodeURIComponent(layered["background-image"]);
    expect(url).toContain("fill:#ff0000!important");
    expect(url).toContain("#48d");
    // An icon without a tint layer is still drawn as a mask.
    expect(iconStyle("root", "folder", 16)["mask-image"]).toBeDefined();
    setLocalSetting("fileIcons.tintMode", "mask");
    await settle();
    expect(iconStyle("folder", "folder", 16)["mask-image"]).toBeDefined();
  });

  it("colors folders from the folder colors table", async () => {
    setLocalSetting(FOLDER_COLORS_KEY, [{ pattern: "vendor", color: "#123456" }]);
    await settle();
    expect(folderColor("vendor")).toBe("#123456");
    expect(folderColor("src")).toBeNull();
    const { container } = render(() => <FileIcon name="vendor" path="vendor" dir />);
    expect(container.querySelector<SVGElement>("svg.file-icon")?.getAttribute("style")).toContain("rgb(18, 52, 86)");
  });

  it("falls back to Forge's icons without an active theme", async () => {
    await settle();
    expect(themedIcon("app.php")).toBeNull();
    expect(iconLook().arrows).toBe("chevron");
    const { container } = render(() => <FileIcon name="app.php" />);
    expect(container.querySelector("svg.file-icon")).not.toBeNull();
  });

  it("resolves files and folders through the active theme", async () => {
    remove = registerIconTheme({ id: "test.shapes", label: "Shapes", load: async () => data });
    expect(iconThemes().map((t) => t.id)).toContain("test.shapes");
    setLocalSetting(THEME_KEY, "test.shapes");
    await settle();
    expect(themedIcon("src/User.php")).toContain("data:image/svg+xml");
    expect(decodeURIComponent(themedIcon("src/User.php")!)).toContain("#88f");
    expect(decodeURIComponent(themedIcon("src", "folder")!)).toContain("#48d");
    expect(decodeURIComponent(themedIcon("README")!)).toContain("#999");
    expect(iconLook()).toMatchObject({ arrows: "triangle", size: 16, monochrome: false });

    const { container } = render(() => <FileIcon name="User.php" />);
    const el = container.querySelector<HTMLElement>(".themed-file-icon");
    expect(el?.style.backgroundImage).toContain("data:image/svg+xml");
    expect(el?.style.width).toBe("16px");
  });

  it("applies user associations and look overrides", async () => {
    remove = registerIconTheme({ id: "test.shapes", label: "Shapes", load: async () => data });
    setLocalSetting(THEME_KEY, "test.shapes");
    setLocalSetting(ASSOCIATIONS_KEY, [{ kind: "fileName", value: "Makefile", icon: "src" }]);
    setLocalSetting("fileIcons.treeArrows", "none");
    setLocalSetting("fileIcons.monochrome", "on");
    await settle();
    expect(decodeURIComponent(themedIcon("Makefile")!)).toContain("#48d");
    expect(iconLook()).toMatchObject({ arrows: "none", monochrome: true });
    const { container } = render(() => <FileIcon name="User.php" />);
    // Monochrome icons are drawn as a mask in one color.
    expect(container.querySelector<HTMLElement>(".themed-file-icon")?.getAttribute("style")).toContain("mask-image");
  });

  it("forgets the theme when it is unregistered or fails to load", async () => {
    remove = registerIconTheme({ id: "test.broken", label: "Broken", load: async () => Promise.reject(new Error("bad theme")) });
    setLocalSetting(THEME_KEY, "test.broken");
    await settle();
    expect(themedIcon("a.php")).toBeNull();
  });
});
