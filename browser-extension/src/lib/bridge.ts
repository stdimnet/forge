/**
 * Synchronous bridge between the content script (isolated world, no access to framework
 * internals) and the page script (page world). Both share the DOM: the content script marks
 * the element, dispatches an event, and the page script answers through attributes before
 * `dispatchEvent` returns. No tokens or Forge data ever reach the page.
 */
export const RESOLVE_EVENT = "forge-companion:resolve";
export const PROBE_ATTR = "data-forge-companion-probe";
export const TARGET_ATTR = "data-forge-companion-target";
export const RESULT_ATTR = "data-forge-companion-result";
