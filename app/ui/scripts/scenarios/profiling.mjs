/**
 * Profiling screenshots: an Xdebug snapshot of the Laravel demo in the Profiler (Execution
 * Statistics with the callers and callees of a hot function, and the Call Tree), and on the
 * Node.js demo (`?project=node`) a V8 CPU profile as a flame graph and a V8 heap snapshot's
 * Memory view. The snapshots are the demo files parsed by `forge-profiler` (WebAssembly).
 */

/** @param {import("playwright-core").Page} page */
async function closeBalloons(page) {
  for (const close of await page.locator(".balloon-close").all()) await close.click().catch(() => {});
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

/** Open a (demo) snapshot file in the Profiler. */
async function open(page, path) {
  await page.evaluate(async (p) => {
    const { openSnapshot } = await import("/src/features/profiling/store.ts");
    await openSnapshot(p);
  }, path);
}

const CACHE = "~/.cache/forge/profiler";
const node = (fn) => Object.assign(fn, { query: "?project=node" });

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  // Tools | Analyze Xdebug Profiler Snapshot: the statistics sorted by own time, with the
  // callers and callees of PDOStatement->execute.
  "profiler-xdebug-statistics": async (page) => {
    await closeBalloons(page);
    await open(page, `${CACHE}/1761646500000/cachegrind.out.4242.gz`);
    await page.waitForSelector('[data-testid="profiler-statistics"]');
    await bottomHeight(page, 470);
    await page.locator(".profiling-table tr", { hasText: "php::PDOStatement->execute" }).first().click();
    await page.waitForSelector(".profiling-split");
    await page.waitForTimeout(400);
  },
  "profiler-xdebug-call-tree": async (page) => {
    await closeBalloons(page);
    await open(page, `${CACHE}/1761646500000/cachegrind.out.4242.gz`);
    await page.waitForSelector('[data-testid="profiler-statistics"]');
    await bottomHeight(page, 470);
    await page.click('.prof-tabs button:has-text("Call Tree")');
    await page.waitForSelector(".prof-tree-row");
    await page.waitForTimeout(400);
  },
  "profiler-v8-flame-graph": node(async (page) => {
    await closeBalloons(page);
    await open(page, `${CACHE}/1761646380000/CPU.20261028.101500.4242.0.001.cpuprofile`);
    await page.waitForSelector('[data-testid="profiler-statistics"]');
    await bottomHeight(page, 430);
    await page.click('.prof-tabs button:has-text("Flame Graph")');
    await page.waitForSelector('[data-testid="profiler-flame"]');
    await page.waitForTimeout(400);
  }),
  "profiler-v8-heap-summary": node(async (page) => {
    await closeBalloons(page);
    await open(page, "/tmp/Heap.20261028.101500.4242.heapsnapshot");
    await page.waitForSelector('[data-testid="profiler-memory"]');
    await bottomHeight(page, 470);
    await page.locator(".profiling-table tbody tr").filter({ has: page.locator(".profiling-callable", { hasText: /^Order$/ }) }).first().click();
    await page.waitForTimeout(400);
  }),
};
