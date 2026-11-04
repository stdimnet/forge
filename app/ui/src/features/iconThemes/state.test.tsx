import { afterEach, describe, expect, it } from "vitest";
import type { JSX } from "solid-js";
import { render as solidRender } from "solid-js/web";
import { setLocalSetting } from "@/state/settings";
import { forgeWasm } from "@/wasm";
import { FileIcon } from "@/shell/FileIcon";
import { iconLook, iconThemes, registerIconTheme, themedIcon, ASSOCIATIONS_KEY, THEME_KEY, type IconThemeData } from "./state";

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
  await settle();
});

describe("icon themes", () => {
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
