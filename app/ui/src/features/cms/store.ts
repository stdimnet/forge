/** The Drupal / Joomla / Shopware detection of the open project (`cms_detect`). */
import { createEffect, createRoot, createSignal, on } from "solid-js";
import { cmsApi, CMS_PLUGIN_ID, type CmsInfo } from "@/ipc/cms";
import { isPluginActive, usesFramework } from "@/state/project";
import { workspace } from "@/state/workspace";

/** The CMS plugin applies to the open project. */
export const cmsActive = (): boolean => isPluginActive(CMS_PLUGIN_ID);
export const drupalActive = (): boolean => cmsActive() && usesFramework("drupal");
export const shopwareActive = (): boolean => cmsActive() && usesFramework("shopware");

const [cms, setCms] = createSignal<CmsInfo[]>([]);
export { cms };
export const cmsInfo = (id: CmsInfo["id"]) => cms().find((c) => c.id === id);

export async function loadCms(force = false): Promise<void> {
  try {
    setCms(force ? await cmsApi.refresh() : await cmsApi.detect());
  } catch (e) {
    console.warn("CMS detection failed", e);
  }
}

if (typeof window !== "undefined") {
  createRoot(() =>
    createEffect(
      on(
        () => [workspace()?.root, cmsActive()] as const,
        ([root, active]) => {
          if (root && active) void loadCms();
          else setCms([]);
        },
      ),
    ),
  );
}
