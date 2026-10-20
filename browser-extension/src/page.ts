/**
 * Runs in the page's JavaScript world (manifest `"world": "MAIN"`), where React fibers, Vue
 * instances, `window.ng` and Svelte metadata are visible. It only answers resolve requests
 * from the content script; it never talks to Forge.
 */
import { PROBE_ATTR, RESOLVE_EVENT, RESULT_ATTR, TARGET_ATTR } from "./lib/bridge";
import { resolveElement } from "./lib/resolve";

document.addEventListener(RESOLVE_EVENT, () => {
  const el = document.querySelector(`[${PROBE_ATTR}]`);
  if (!el) return;
  try {
    const resolved = resolveElement(el, window);
    if (!resolved) return;
    const { element, ...info } = resolved;
    element.setAttribute(TARGET_ATTR, "");
    document.documentElement.setAttribute(RESULT_ATTR, JSON.stringify(info));
  } catch {
    /* framework internals changed shape: behave as if nothing was found */
  }
});
