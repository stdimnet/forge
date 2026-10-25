import { describe, expect, it } from "vitest";
import { callWasm, forgeWasm } from "@/wasm";
import type { HookCompletionList, HookGutterMark, HookSummary, WordPressInfo, WpLocation, YodaProblem } from "@/ipc/wordpress";
import type { UsagesAnswer } from "@/ipc/codeintel";
import type { CmsInfo, DrupalCompletionList, DrupalGutterMark } from "@/ipc/cms";
import { wordpressDemoFiles } from "./demoWordPressProject";
import { drupalDemoFiles } from "./demoDrupalProject";

async function wp<T>(query: unknown): Promise<T> {
  const wasm = await forgeWasm();
  return callWasm<T>(wasm.wordpress_query, { files: wordpressDemoFiles, query });
}

async function cms<T>(query: unknown): Promise<T> {
  const wasm = await forgeWasm();
  return callWasm<T>(wasm.cms_query, { files: drupalDemoFiles, query });
}

const CLASS = "includes/class-acme-events.php";
const FUNCTIONS = "includes/functions.php";

describe("WordPress demo through WebAssembly", () => {
  it("detects the plugin, WP-CLI and the stubs", async () => {
    const info = await wp<WordPressInfo>("detect");
    expect(info.kind).toBe("plugin");
    expect(info.header?.name).toBe("Acme Events");
    expect(info.wpCli).toEqual({ path: "vendor/bin/wp", source: "project" });
    expect(info.stubs).toBe("vendor/php-stubs/wordpress-stubs/wordpress-stubs.php");
  });

  it("indexes hooks with registrations and invocations", async () => {
    const hooks = await wp<HookSummary[]>("hooks");
    const registered = hooks.find((h) => h.name === "acme_events_registered")!;
    expect([registered.kind, registered.registrations, registered.invocations]).toEqual(["action", 1, 1]);
    expect(hooks.find((h) => h.name === "acme_events_label_{$date}")?.dynamic).toBe(true);
  });

  it("completes, navigates and marks hooks", async () => {
    const content = wordpressDemoFiles[FUNCTIONS] + "add_action( 'acme_";
    const list = await wp<HookCompletionList>({ complete: { path: FUNCTIONS, content, offset: content.length } });
    expect(list.items[0].label).toMatch(/^acme_events_/);
    const src = wordpressDemoFiles[CLASS];
    const at = src.indexOf("acme_events_label_");
    const targets = await wp<WpLocation[]>({ goto: { path: CLASS, content: src, offset: at + 3 } });
    expect(targets).toEqual([{ path: FUNCTIONS, line: 7, column: 13 }]);
    const marks = await wp<HookGutterMark[]>({ gutter: { path: CLASS, content: src } });
    expect(marks.some((m) => m.kind === "invocation" && m.title === "1 callback for 'acme_events_registered'")).toBe(true);
    const usages = await wp<UsagesAnswer>({ usages: { path: CLASS, content: src, offset: src.indexOf("acme_events_registered") } });
    expect(usages.byType.total).toBe(2);
  });

  it("reports Yoda conditions", async () => {
    const problems = await wp<YodaProblem[]>({ inspect: { path: FUNCTIONS, content: wordpressDemoFiles[FUNCTIONS] } });
    expect(problems.map((p) => p.replacement)).toEqual(["'auto-draft' == $post->post_status"]);
  });
});

describe("Drupal demo through WebAssembly", () => {
  it("detects the module and completes hook implementations", async () => {
    const info = await cms<CmsInfo[]>("detect");
    expect(info[0]).toMatchObject({ id: "drupal", kind: "extension", extension: "acme_tools" });
    const content = "<?php\n\nfunction acme_tools_form";
    const list = await cms<DrupalCompletionList>({ complete: { path: "acme_tools.module", content, offset: content.length } });
    expect(list.items.find((i) => i.hook === "hook_form_alter")?.label).toBe("acme_tools_form_alter");
    const marks = await cms<DrupalGutterMark[]>({ gutter: { path: "acme_tools.module", content: drupalDemoFiles["acme_tools.module"] } });
    expect(marks.map((m) => m.docs.hook)).toEqual(["hook_help", "hook_form_FORM_ID_alter", "hook_theme"]);
  });
});
