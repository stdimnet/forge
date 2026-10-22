/**
 * Browser-mode WebForge backend: npm packages, package.json run icons and React/Vue/Angular
 * navigation computed by the Rust `packs/webforge/*` crates (WebAssembly) over the in-memory
 * project, exactly like the desktop commands. `?project=react|vue|angular` opens a demo.
 */
import { registerMock } from "@/ipc";
import { callWasm, forgeWasm } from "@/wasm";
import { mockFiles } from "./workspaceMock";
import { mockSettingValue, setSetting } from "./settingsMock";
import { webDemo } from "./demoWebProjects";

/** Script, template and manifest files the Rust index reads (like `fs::wants_content`). */
const CONTENT = /\.(m?[jt]sx?|cjs|vue|html?)$|(^|\/)(package|angular|jsconfig|tsconfig[\w.]*)\.json$/;

function project() {
  const files = Object.entries(mockFiles)
    .filter(([path]) => CONTENT.test(path) && !path.includes("node_modules/"))
    .map(([path, content]) => ({ path, content }));
  return { files, paths: Object.keys(mockFiles) };
}

const wasm = () => forgeWasm();

/** The demo's `package.json` files with their scripts (also used by Run Anything). */
export const mockNpmPackages = async () =>
  callWasm<{ dir: string; packageManager: { manager: string }; scripts: { name: string }[] }[]>((await wasm()).web_npm_packages, {
    ...project(),
    packageManager: String(mockSettingValue("web.packageManager") ?? "auto"),
  });

registerMock("npm_packages", mockNpmPackages);
registerMock("npm_gutter", async ({ path, content }) => callWasm((await wasm()).web_npm_gutter, { path, content }));
registerMock("web_components", async ({ framework }) => callWasm((await wasm()).web_components, { framework, ...project() }));
registerMock("web_goto", async ({ framework, path, content, offset }) =>
  callWasm((await wasm()).web_goto, { framework, ...project(), path, content, offset }),
);
registerMock("web_gutter", async ({ framework, path, content }) =>
  callWasm((await wasm()).web_gutter, { framework, ...project(), path, content }),
);

const demo = webDemo();
if (demo) {
  registerMock("project_detect", () => structuredClone(demo.info));
  // Like the other demos, announce the frameworks only with `?announce`.
  setSetting(
    "project.announcedFrameworks",
    new URLSearchParams(location.search).has("announce") ? null : demo.info.frameworks.map((f) => `~/Projects/${demo.name}#${f.id}`),
    "user",
  );
}
