/**
 * Builds the extension into dist/chrome and dist/firefox (Manifest V3).
 * Usage: node scripts/build.mjs [--watch]
 */
import * as esbuild from "esbuild";
import { copyFileSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const icons = resolve(root, "../app/src-tauri/icons");
const { version, description } = JSON.parse(readFileSync(resolve(root, "package.json"), "utf8"));
const watch = process.argv.includes("--watch");

/** Pages the content scripts run on: dev servers can live on any host. */
const PAGES = ["http://*/*", "https://*/*"];

function manifest(browser) {
  const m = {
    manifest_version: 3,
    name: "Forge Companion",
    short_name: "Forge",
    version,
    description,
    icons: { 32: "icons/icon-32.png", 128: "icons/icon-128.png" },
    action: { default_title: "Forge Companion", default_popup: "popup.html", default_icon: { 32: "icons/icon-32.png" } },
    permissions: ["storage", "contextMenus"],
    // Only Forge's local server; nothing else is ever contacted.
    host_permissions: ["http://127.0.0.1/*"],
    commands: {
      "toggle-inspect": {
        suggested_key: { default: "Alt+Shift+F" },
        description: "Toggle inspect mode (click a component to open it in Forge)",
      },
    },
    content_scripts: [
      { matches: PAGES, js: ["page.js"], world: "MAIN", run_at: "document_start", all_frames: true },
      { matches: PAGES, js: ["content.js"], run_at: "document_idle", all_frames: true },
    ],
  };
  if (browser === "chrome") {
    m.background = { service_worker: "background.js" };
    m.minimum_chrome_version = "111";
  } else {
    m.background = { scripts: ["background.js"] };
    m.browser_specific_settings = { gecko: { id: "companion@forge-ide.dev", strict_min_version: "128.0" } };
  }
  return m;
}

function writeStatic(out, browser) {
  mkdirSync(resolve(out, "icons"), { recursive: true });
  writeFileSync(resolve(out, "manifest.json"), `${JSON.stringify(manifest(browser), null, 2)}\n`);
  copyFileSync(resolve(root, "src/popup/popup.html"), resolve(out, "popup.html"));
  copyFileSync(resolve(root, "src/popup/popup.css"), resolve(out, "popup.css"));
  copyFileSync(resolve(icons, "32x32.png"), resolve(out, "icons/icon-32.png"));
  copyFileSync(resolve(icons, "128x128.png"), resolve(out, "icons/icon-128.png"));
}

const browsers = ["chrome", "firefox"];
const contexts = [];
for (const browser of browsers) {
  const out = resolve(root, "dist", browser);
  rmSync(out, { recursive: true, force: true });
  mkdirSync(dirname(out), { recursive: true });
  writeStatic(out, browser);
  const ctx = await esbuild.context({
    entryPoints: {
      background: resolve(root, "src/background.ts"),
      content: resolve(root, "src/content.ts"),
      page: resolve(root, "src/page.ts"),
      popup: resolve(root, "src/popup/popup.ts"),
    },
    outdir: out,
    bundle: true,
    format: "iife",
    target: ["chrome111", "firefox128"],
    legalComments: "none",
    logLevel: "warning",
  });
  contexts.push(ctx);
}

if (watch) {
  for (const ctx of contexts) await ctx.watch();
  console.log("watching src/ … (static files are copied once; restart after editing popup.html/css)");
} else {
  for (const ctx of contexts) {
    await ctx.rebuild();
    await ctx.dispose();
  }
  console.log(`built ${browsers.map((b) => `dist/${b}`).join(" and ")}`);
}
