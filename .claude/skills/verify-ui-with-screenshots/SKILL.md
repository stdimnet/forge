---
name: verify-ui-with-screenshots
description: Visually verify Forge UI changes by capturing Playwright screenshots of the browser-mode app and looking at them. Use after any UI change, before committing UI work, or when asked to show what a feature looks like.
---

# Verify UI with screenshots

Forge runs completely in a browser with mocked IPC, so every feature can be screenshotted
headless.

## Steps

1. Start the dev server in the background on a free port:

   ```sh
   cd app/ui && pnpm dev --port 1430
   ```

2. Add or update a scenario file `app/ui/scripts/scenarios/<feature>.mjs`:

   ```js
   /** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
   export default {
     "routes-tool-window": async (page) => {
       await page.click('[data-tool-window="routes"]');      // stripe button
       await page.waitForSelector(".routes-table tr");
     },
   };
   ```

   - The page starts with the demo project open and `User.php` in the editor
     (`.cm-content` is visible).
   - To start elsewhere, set properties on the function: `fn.query = "?welcome"` and
     `fn.waitFor = ".welcome-project"`.
   - In the project tree, a single click selects and a **double click** opens a folder or file:
     `page.dblclick('.tree-row[data-path="app/Models"]')`.
   - Commands: `page.evaluate(() => window.__forgeRunCommand("settings.open"))`.
   - Keys: `page.keyboard.press("Control+Alt+KeyS")`.

3. Capture the screenshots:

   ```sh
   FORGE_URL=http://localhost:1430 node scripts/screenshots.mjs <scenario> [more…]
   ```

   PNGs are written to `docs/screenshots/<scenario>.png` (1440×900). Without arguments all
   scenarios run. Chromium is found at `/opt/pw-browsers/...`, or set `CHROMIUM_PATH`.

4. **Look at every image** (open the PNG) and compare it with PHPStorm's New UI: layout,
   spacing, wording and states should match, and colors must come from the Forge theme. Check
   the light theme too (`document.documentElement.dataset.theme = "light"`).
5. Commit the PNGs together with the change.

## Troubleshooting

- `Timeout … .cm-content`: the dev server restarted (e.g. Vite re-optimized deps after
  `pnpm add`). Run the scenario again.
- `ERR_CONNECTION_REFUSED`: the dev server is not running on that port.
