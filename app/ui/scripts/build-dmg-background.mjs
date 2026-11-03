/**
 * Renders app/src-tauri/dmg/background.html to the DMG window background:
 * background.png (660×400), background@2x.png and background.tiff (both, so Finder picks the
 * sharp one on Retina displays). Run after changing the HTML; the results are committed.
 */
import { execFileSync } from "node:child_process";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { chromium } from "playwright-core";

const dir = resolve(import.meta.dirname, "../../src-tauri/dmg");
const browser = await chromium.launch({ channel: "chrome" }).catch(() => chromium.launch());
for (const [scale, name] of [[1, "background.png"], [2, "background@2x.png"]]) {
  const page = await browser.newPage({ viewport: { width: 660, height: 400 }, deviceScaleFactor: scale });
  await page.goto(pathToFileURL(resolve(dir, "background.html")).href);
  await page.evaluate(() => document.fonts.ready);
  await page.screenshot({ path: resolve(dir, name) });
  await page.close();
}
await browser.close();
// macOS only: Finder reads the two representations from one TIFF.
execFileSync("tiffutil", ["-cathidpicheck", resolve(dir, "background.png"), resolve(dir, "background@2x.png"), "-out", resolve(dir, "background.tiff")]);
console.log("wrote", dir);
