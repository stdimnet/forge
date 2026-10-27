/**
 * CForge v2 screenshots: an AddressSanitizer report in the Run tool window's Sanitizers tab,
 * the Valgrind Memcheck results, a perf flame graph in the Profiler (all on the `shapes`
 * demo, `?project=cpp`), and on the PlatformIO demo (`?project=pio`) the PlatformIO tool
 * window and the Peripherals view of an OpenOCD debug session.
 */

/** @param {import("playwright-core").Page} page */
async function closeBalloons(page) {
  for (const close of await page.locator(".balloon-close").all()) await close.click().catch(() => {});
}

/** @param {import("playwright-core").Page} page */
async function openFile(page, path, line = 1) {
  await page.evaluate(
    async ([p, l]) => {
      const { openFileAt } = await import("/src/editor/views.ts");
      await openFileAt(p, l, 1, { flash: false });
    },
    [path, line],
  );
  await page.waitForSelector(`.editor-tab.active[title="${path}"]`);
  await page.waitForTimeout(400);
}

/** Drag the bottom splitter so the bottom tool window gets `height` pixels. */
async function bottomHeight(page, height) {
  const box = await page.locator(".splitter-h").boundingBox();
  if (!box) return;
  const y = box.y + box.height / 2;
  await page.mouse.move(box.x + 200, y);
  await page.mouse.down();
  await page.mouse.move(box.x + 200, 900 - 26 - height, { steps: 4 });
  await page.mouse.up();
}

/** Run a stored configuration, optionally under Valgrind or perf. */
async function run(page, name, nativeTool) {
  await page.evaluate(
    async ([n, tool]) => {
      const { launch } = await import("/src/features/run/runStore.ts");
      const { findConfiguration } = await import("/src/features/run/configStore.ts");
      await launch(findConfiguration(n), tool ? { extra: { nativeTool: tool } } : {});
    },
    [name, nativeTool],
  );
}

const cpp = (fn) => Object.assign(fn, { query: "?project=cpp" });
const pio = (fn) => Object.assign(fn, { query: "?project=pio", waitFor: ".cm-content, .editor-empty, .welcome" });

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  // `shapes_app --stress` in the ASan profile: the heap-buffer-overflow with its access and
  // allocation stacks, the code preview and the highlighted lines in the editor.
  "cpp-sanitizers": cpp(async (page) => {
    await closeBalloons(page);
    await openFile(page, "src/stats.cpp", 12);
    await run(page, "shapes_app (stress)");
    await page.waitForSelector('[data-run-panel="sanitizers"]', { timeout: 20000 });
    await page.waitForSelector(".native-issue");
    await bottomHeight(page, 380);
    await page.click(".native-frame.native-own >> nth=0");
    await page.waitForSelector(".native-preview");
    await openFile(page, "src/stats.cpp", 12);
    await page.waitForTimeout(600);
    await closeBalloons(page);
  }),
  // Run with Valgrind Memcheck: errors and leaks with their stacks and the leak summary.
  "cpp-valgrind": cpp(async (page) => {
    await closeBalloons(page);
    await openFile(page, "src/geometry/polygon.cpp", 30);
    await run(page, "shapes_app", "valgrind");
    await page.waitForSelector('[data-run-panel="valgrind"]', { timeout: 25000 });
    await page.waitForSelector(".native-issue");
    await bottomHeight(page, 400);
    await page.click('.native-issue[data-native-issue="v3"]');
    await page.waitForTimeout(700);
    await closeBalloons(page);
  }),
  // Profile with perf: the flame graph of shapes_app in the Profiler tool window.
  "cpp-perf-flame-graph": cpp(async (page) => {
    await closeBalloons(page);
    await openFile(page, "src/geometry/polygon.cpp", 17);
    await run(page, "shapes_app", "perf");
    await page.waitForSelector('[data-testid="profiler-flame"]', { timeout: 25000 });
    await bottomHeight(page, 420);
    await page.waitForTimeout(800);
    await closeBalloons(page);
  }),
  // The PlatformIO tool window with its environments and tasks, after a build.
  "platformio-tool-window": pio(async (page) => {
    await closeBalloons(page);
    await openFile(page, "platformio.ini", 17);
    await page.click('[data-tool-window="platformio"]');
    await page.waitForSelector('[data-pio-env="nucleo_f401re"]');
    await page.click('[data-pio-env="esp32dev"]');
    await page.click('[data-pio-env="native"]');
    await page.dblclick('[data-pio-task="nucleo_f401re/build"]');
    await page.waitForFunction(() => document.querySelector(".run-console")?.textContent?.includes("SUCCESS"), null, { timeout: 20000 });
    await bottomHeight(page, 300);
    await page.waitForTimeout(500);
    await closeBalloons(page);
  }),
  // Debugging the OpenOCD Download & Run configuration: GDB stopped in loop(), the
  // Peripherals tab with GPIOA's registers and decoded fields.
  "embedded-peripherals": pio(async (page) => {
    await closeBalloons(page);
    await openFile(page, "src/main.cpp", 15);
    await page.evaluate(async () => {
      const { debugConfiguration } = await import("/src/features/run/runStore.ts");
      const { findConfiguration } = await import("/src/features/run/configStore.ts");
      await debugConfiguration(findConfiguration("blinky (OpenOCD)"));
    });
    await page.waitForSelector('.dbg-tab[data-tab="peripherals"]', { timeout: 25000 });
    await page.click('.dbg-tab[data-tab="peripherals"]');
    await page.waitForSelector('[data-peripheral="GPIOA"]');
    await bottomHeight(page, 420);
    await page.click('[data-peripheral="GPIOA"]');
    await page.waitForSelector('[data-register="MODER"]');
    await page.click('[data-register="MODER"]');
    await page.click('[data-register="ODR"]');
    await page.waitForTimeout(600);
    await closeBalloons(page);
  }),
};
