/**
 * Captures UI screenshots from the Vite dev server (browser mode with mocked backend).
 * Usage: pnpm dev &  then  pnpm screenshots [scenario...]
 * Scenarios live in scripts/scenarios/*.mjs; each opens the app, performs steps and writes
 * docs/screenshots/<name>.png. Set FORGE_URL / CHROMIUM_PATH to override defaults.
 */
import { chromium } from "playwright-core";
import { existsSync, mkdirSync, readdirSync } from "node:fs";
import { resolve } from "node:path";

const url = process.env.FORGE_URL ?? "http://localhost:1420";
const outDir = resolve(import.meta.dirname, "../../../docs/screenshots");
const executablePath =
  process.env.CHROMIUM_PATH ??
  ["/opt/pw-browsers/chromium-1194/chrome-linux/chrome"].find((p) => existsSync(p));

// Each feature adds its own scenarios file in scripts/scenarios/ (default export: name -> steps).
const scenarios = {};
for (const file of readdirSync(resolve(import.meta.dirname, "scenarios")).sort()) {
  if (file.endsWith(".mjs")) Object.assign(scenarios, (await import(`./scenarios/${file}`)).default);
}

const wanted = process.argv.slice(2);
mkdirSync(outDir, { recursive: true });
const browser = await chromium.launch({ executablePath });
for (const [name, steps] of Object.entries(scenarios)) {
  if (wanted.length && !wanted.includes(name)) continue;
  // Fresh context per scenario so localStorage from one scenario never leaks into the next.
  const context = await browser.newContext({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 1 });
  const page = await context.newPage();
  await page.goto(url);
  await page.waitForSelector(".cm-content");
  await steps(page);
  await page.waitForTimeout(300);
  await page.screenshot({ path: `${outDir}/${name}.png` });
  console.log(`wrote ${name}.png`);
  await context.close();
}
await browser.close();
