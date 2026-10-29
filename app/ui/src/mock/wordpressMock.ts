/**
 * Browser-mode WordPress and CMS backends: the demo project's files (`?project=wordpress`,
 * `?project=drupal`) are indexed by the Rust crates `phpforge-fw-wordpress` and
 * `phpforge-fw-cms` through WebAssembly, so hooks, completion, navigation, gutter marks,
 * usages, Yoda hints and Drupal hooks are answered exactly like in the desktop app.
 */
import { registerMock } from "@/ipc";
import { callWasm, forgeWasm } from "@/wasm";
import { mockFiles } from "./workspaceMock";
import { isWordPressDemo, wordpressDemoInfo, wordpressDemoName } from "./demoWordPressProject";
import { drupalDemoInfo, drupalDemoName, isDrupalDemo } from "./demoDrupalProject";
import { setSetting } from "./settingsMock";

async function wordpress(query: unknown): Promise<unknown> {
  const wasm = await forgeWasm();
  return callWasm(wasm.wordpress_query, { files: mockFiles, query });
}

async function cms(query: unknown): Promise<unknown> {
  const wasm = await forgeWasm();
  return callWasm(wasm.cms_query, { files: mockFiles, query });
}

const editor = (args: Record<string, unknown>) => ({ path: String(args.path), content: String(args.content), offset: Number(args.offset ?? 0) });

registerMock("wordpress_detect", () => wordpress("detect"));
registerMock("wordpress_refresh", () => wordpress("refresh"));
registerMock("wordpress_hooks", () => wordpress("hooks"));
registerMock("wordpress_complete", (a) => wordpress({ complete: editor(a) }));
registerMock("wordpress_goto", (a) => wordpress({ goto: editor(a) }));
registerMock("wordpress_usages", (a) => wordpress({ usages: editor(a) }));
registerMock("wordpress_gutter", (a) => wordpress({ gutter: { path: String(a.path), content: String(a.content) } }));
registerMock("wordpress_inspect", (a) => wordpress({ inspect: { path: String(a.path), content: String(a.content) } }));

registerMock("cms_detect", () => cms("detect"));
registerMock("cms_refresh", () => cms("refresh"));
registerMock("cms_complete", (a) => cms({ complete: editor(a) }));
registerMock("cms_goto", (a) => cms({ goto: editor(a) }));
registerMock("cms_gutter", (a) => cms({ gutter: { path: String(a.path), content: String(a.content) } }));

const demo = isWordPressDemo() ? { info: wordpressDemoInfo, name: wordpressDemoName, id: "wordpress" } : isDrupalDemo() ? { info: drupalDemoInfo, name: drupalDemoName, id: "drupal" } : null;
if (demo) {
  registerMock("project_detect", () => structuredClone(demo.info));
  // Like the other demos, announce the framework only with `?announce`.
  setSetting(
    "project.announcedFrameworks",
    new URLSearchParams(location.search).has("announce") ? null : [`~/Projects/${demo.name}#${demo.id}`],
    "user",
  );
}
