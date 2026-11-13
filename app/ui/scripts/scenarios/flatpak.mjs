/** Settings | Tools | Flatpak: the sandbox status line and "Run tools on the host (Flatpak)". */

/** Open the Flatpak settings page; `inFlatpak` fakes the sandbox in the browser mock. */
async function openPage(page, inFlatpak) {
  await page.evaluate(async (inFlatpak) => {
    const { registerMock } = await import("/src/ipc/index.ts");
    registerMock("flatpak_status", () => ({
      inFlatpak,
      flatpakId: inFlatpak ? "dev.forge.ide" : null,
      enabled: true,
      active: inFlatpak,
      hostOsVisible: inFlatpak,
    }));
  }, inFlatpak);
  await page.keyboard.press("Control+Alt+KeyS");
  await page.waitForSelector(".set-tree-row");
  await page.keyboard.type("flatpak");
  await page.waitForTimeout(300);
  await page.click('.set-tree-row:has-text("Flatpak")');
  await page.waitForSelector('[data-testid="flatpak-status"]');
  await page.waitForTimeout(300);
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "flatpak-settings": (page) => openPage(page, true),
  "flatpak-settings-native": (page) => openPage(page, false),
};
