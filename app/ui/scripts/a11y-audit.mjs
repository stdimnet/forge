/**
 * Accessibility audit: runs axe-core (https://github.com/dequelabs/axe-core) in Chromium against
 * browser mode on the main screens and a few dialogs, for every theme listed in THEMES.
 *
 * Usage: pnpm dev &  then  pnpm a11y:audit [--themes=dark,high-contrast] [--screens=commit,git-log] [--json=out.json]
 * Exit code 1 when a serious or critical violation is found (use --report to only print).
 * FORGE_URL / CHROMIUM_PATH override the defaults like in scripts/screenshots.mjs.
 */
import { chromium } from "playwright-core";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const axeSource = readFileSync(require.resolve("axe-core/axe.min.js"), "utf8");

const url = process.env.FORGE_URL ?? "http://localhost:1420";
const executablePath =
  process.env.CHROMIUM_PATH ?? ["/opt/pw-browsers/chromium-1194/chrome-linux/chrome"].find((p) => existsSync(p));
const arg = (name) => process.argv.find((a) => a.startsWith(`--${name}=`))?.split("=")[1];
const reportOnly = process.argv.includes("--report");
const themes = (arg("themes") ?? "dark").split(",");
/** `--screens=git-log,commit` audits only those screens. */
const only = arg("screens")?.split(",");

const run = (page, id) => page.evaluate((cmd) => window.__forgeRunCommand(cmd), id);

/** Show the Project tool window (it may be open already). */
async function projectView(page) {
  if (!(await page.$(".project-tree"))) await page.keyboard.press("Alt+1");
  await page.waitForSelector(".project-tree .tree-row[data-path]");
}

/**
 * Screens to audit: name → steps that bring the screen up (from the started workbench).
 * Each returns the CSS selector axe should check (the whole document by default).
 * @type {Record<string, (page: import("playwright-core").Page) => Promise<string | undefined>>}
 */
const SCREENS = {
  workbench: async () => undefined,
  "project-view": async (page) => {
    await projectView(page);
    await page.click('.project-tree .tree-row[data-path="app"]');
    return undefined;
  },
  "main-menu": async (page) => {
    await page.click("[data-main-menu-button]");
    await page.waitForSelector(".context-menu.main-menu-popup");
    return undefined;
  },
  "search-everywhere": async (page) => {
    await page.keyboard.press("Control+Shift+A");
    await page.waitForSelector(".se-popup");
    await page.keyboard.type("settings");
    await page.waitForTimeout(300);
    return ".se-popup";
  },
  settings: async (page) => {
    await run(page, "settings.open");
    await page.waitForSelector(".set-tree-row");
    await page.waitForTimeout(300);
    return ".set-dialog";
  },
  "settings-appearance": async (page) => {
    await run(page, "settings.open");
    await page.waitForSelector(".set-tree-row");
    await page.click('.set-tree-row[data-node="appearance"]');
    await page.waitForTimeout(300);
    return ".set-dialog";
  },
  commit: async (page) => {
    await page.keyboard.press("Control+k");
    await page.waitForSelector(".commit-row");
    await page.waitForTimeout(200);
    return undefined;
  },
  "git-branches": async (page) => {
    await run(page, "git.branches");
    await page.waitForSelector(".branches-popup .branch-folder");
    await page.keyboard.press("ArrowDown");
    return ".branches-popup";
  },
  "git-log": async (page) => {
    await run(page, "git.log");
    await page.waitForSelector(".log-row");
    await page.click(".log-branches .tree-row[aria-expanded=false]");
    return ".log-branches";
  },
  rename: async (page) => {
    await projectView(page);
    await page.click('.project-tree .tree-row[data-path="composer.json"]');
    await page.keyboard.press("Shift+F6");
    await page.waitForSelector("[role=dialog] input");
    return "[role=dialog]";
  },
  "presentation-assistant": async (page) => {
    await page.evaluate(async () => {
      const { saveSetting } = await import("/src/state/settings.ts");
      await saveSetting("appearance.presentationAssistant.enabled", true);
    });
    await page.keyboard.press("Control+Shift+F6");
    await page.waitForSelector("[data-presentation-assistant]");
    return undefined;
  },
  welcome: async () => undefined,
};

async function audit(page, include) {
  await page.addScriptTag({ content: axeSource });
  return page.evaluate(async (include) => {
    // Rules about the whole page apply to the main window only.
    const context = include ? { include: [include] } : document;
    const res = await window.axe.run(context, {
      resultTypes: ["violations"],
      rules: { region: { enabled: false } },
    });
    return res.violations.map((v) => ({
      id: v.id,
      impact: v.impact,
      help: v.help,
      nodes: v.nodes.length,
      targets: v.nodes.slice(0, 3).map((n) => n.target.join(" ")),
    }));
  }, include);
}

const browser = await chromium.launch({ executablePath });
const results = [];
for (const theme of themes) {
  for (const [name, steps] of Object.entries(SCREENS)) {
    if (only && !only.includes(name)) continue;
    const context = await browser.newContext({ viewport: { width: 1440, height: 900 } });
    const page = await context.newPage();
    // The theme setting (browser mode keeps the user settings in localStorage).
    await page.addInitScript((theme) => {
      localStorage.setItem("forge.theme", theme);
      const key = "forge.mock.settings.user";
      const user = JSON.parse(localStorage.getItem(key) ?? "{}");
      localStorage.setItem(key, JSON.stringify({ ...user, "appearance.theme": theme }));
    }, theme);
    await page.goto(url + (name === "welcome" ? "?welcome" : ""));
    await page.waitForSelector(name === "welcome" ? ".welcome" : ".cm-content", { timeout: 90000 });
    await page.waitForTimeout(400);
    let violations;
    try {
      const include = await steps(page);
      violations = await audit(page, include);
    } catch (e) {
      console.error(`${theme}/${name}: ${e.message}`);
      violations = [{ id: "scenario-failed", impact: "critical", help: e.message, nodes: 1, targets: [] }];
    }
    results.push({ theme, screen: name, violations });
    const serious = violations.filter((v) => v.impact === "serious" || v.impact === "critical");
    console.log(`${theme}/${name}: ${violations.length} rules violated, ${serious.length} serious or critical`);
    for (const v of violations) console.log(`  [${v.impact}] ${v.id} (${v.nodes}): ${v.help}  ${v.targets.join(" | ")}`);
    await context.close();
  }
}
await browser.close();

const json = arg("json");
if (json) writeFileSync(json, JSON.stringify(results, null, 2));
const total = (impacts) =>
  results.reduce((n, r) => n + r.violations.filter((v) => impacts.includes(v.impact)).reduce((m, v) => m + v.nodes, 0), 0);
console.log(
  `\nnodes: ${total(["critical"])} critical, ${total(["serious"])} serious, ${total(["moderate"])} moderate, ${total(["minor"])} minor`,
);
if (!reportOnly && total(["critical", "serious"]) > 0) process.exit(1);
