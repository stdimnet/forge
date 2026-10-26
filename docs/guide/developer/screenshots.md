---
title: Screenshots and scenarios
description: Capture Playwright screenshots of the browser-mode UI with scenario files, review them in both themes and commit them with UI changes.
order: 120
tags: [screenshots, playwright, ui, testing]
---

# Screenshots and scenarios

Every UI change needs screenshots, and you must look at them. Recipe:
[`verify-ui-with-screenshots`](../../../.claude/skills/verify-ui-with-screenshots/SKILL.md).

## How it works

Forge runs entirely in a browser with mocked IPC, so every feature can be captured headless.
`app/ui/scripts/screenshots.mjs` opens the dev server in Chromium (Playwright), runs a scenario
and writes `docs/screenshots/<scenario>.png` at 1440×900.

## Steps

1. Start the dev server on a free port:

   ```sh
   cd app/ui && pnpm dev --port 1430
   ```

2. Add or update a scenario in `app/ui/scripts/scenarios/<feature>.mjs`:

   ```js
   /** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
   export default {
     "routes-tool-window": async (page) => {
       await page.click('[data-tool-window="routes"]');
       await page.waitForSelector(".routes-table tr");
     },
   };
   ```

   - The page starts with the demo project open and `User.php` in the editor (`.cm-content` is
     visible).
   - Start elsewhere with properties on the function: `fn.query = "?welcome"`,
     `fn.waitFor = ".welcome-project"`.
   - In the Project tree a double click opens folders and files:
     `page.dblclick('.tree-row[data-path="app/Models"]')`.
   - Run commands: `page.evaluate(() => window.__forgeRunCommand("settings.open"))`.
   - Press keys: `page.keyboard.press("Control+Alt+KeyS")`.
   - Light theme: `document.documentElement.dataset.theme = "light"`.

3. Capture:

   ```sh
   FORGE_URL=http://localhost:1430 node scripts/screenshots.mjs <scenario> [more…]
   ```

   Without arguments all scenarios run. Chromium is found under `/opt/pw-browsers/…` or set
   `CHROMIUM_PATH`.

4. **Open every image** and compare it with PHPStorm's New UI: layout, spacing, wording and
   states; colors must come from the Forge theme. Check the light theme too.
5. Commit the PNGs with the change.

## Existing scenario files

`ai-chat`, `ai-providers`, `browser-companion`, `context-menu`, `database`, `debugger`,
`dialog-windows`, `docker`, `editor-lsp`, `file-menu`, `forges`, `gemini`, `integration-polish`,
`javaforge`, `language-servers`, `laravel`, `luaforge`, `navigation`, `php`, `project-tree`, `project-wizard`,
`remote`, `scratches`, `settings`, `symfony`, `syntax`, `terminal-run`, `vcs`, `webforge`,
`welcome`, `workbench`.

The guide links these screenshots relatively (`../../screenshots/<name>.png`); when you rename a
scenario, update the pages that use it.

## Troubleshooting

- `Timeout … .cm-content`: the dev server restarted (e.g. Vite re-optimized dependencies after
  `pnpm add`). Run the scenario again.
- `ERR_CONNECTION_REFUSED`: the dev server is not running on that port.
