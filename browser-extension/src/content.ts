/**
 * Content script (isolated world): inspect mode, Alt+Click, the overlay and the context
 * menu target. Framework lookups go through the page script (see lib/bridge.ts); opening
 * goes through the background worker, which holds the Forge token.
 */
import { PROBE_ATTR, RESOLVE_EVENT, RESULT_ATTR, TARGET_ATTR } from "./lib/bridge";
import { displayPath, toOpenTarget } from "./lib/paths";
import type { BackgroundRequest, ContentConfig, ContentMessage, OpenResult, SourceInfo } from "./lib/types";
import { Overlay } from "./overlay";
import { applyCss, plan, type LivePoll } from "./lib/liveEdit";

interface Probe {
  info: SourceInfo;
  target: Element;
}

const overlay = new Overlay();
let config: ContentConfig = { altClick: true, roots: [], mappings: [], inspecting: false };
let inspecting = false;
let shortcut = "Alt+Shift+F";
let lastProbe: { el: Element; probe: Probe | null } | null = null;
let contextTarget: Element | null = null;
let frame = 0;

const send = <T>(msg: BackgroundRequest) => chrome.runtime.sendMessage<BackgroundRequest, T>(msg);

/** Ask the page script what `el` is (synchronously, through DOM attributes). */
function probe(el: Element): Probe | null {
  if (lastProbe?.el === el) return lastProbe.probe;
  const root = document.documentElement;
  root.removeAttribute(RESULT_ATTR);
  el.setAttribute(PROBE_ATTR, "");
  try {
    document.dispatchEvent(new CustomEvent(RESOLVE_EVENT));
  } finally {
    el.removeAttribute(PROBE_ATTR);
  }
  const raw = root.getAttribute(RESULT_ATTR);
  root.removeAttribute(RESULT_ATTR);
  const target = document.querySelector(`[${TARGET_ATTR}]`);
  target?.removeAttribute(TARGET_ATTR);
  let result: Probe | null = null;
  try {
    if (raw) result = { info: JSON.parse(raw) as SourceInfo, target: target ?? el };
  } catch {
    result = null;
  }
  lastProbe = { el, probe: result };
  return result;
}

function labelFor(info: SourceInfo) {
  let path: string | undefined;
  if (info.file) {
    const target = toOpenTarget(info.file, { roots: config.roots, mappings: config.mappings });
    path = displayPath(target.file, config.roots) + (info.line ? `:${info.line}` : "");
  } else if (info.name) {
    path = "find declaration in project";
  }
  return { framework: info.framework, name: info.name, path };
}

function show(el: EventTarget | null): void {
  if (!(el instanceof Element) || overlay.owns(el)) return;
  const found = probe(el);
  if (found) overlay.highlight(found.target, labelFor(found.info));
  else overlay.clearHighlight();
}

async function open(info: SourceInfo): Promise<void> {
  try {
    const result = await send<OpenResult>({ type: "open", source: info });
    if (!result.ok) overlay.toast(result.error ?? "Could not open the file in Forge", true);
    else if (result.status === "needsConfirmation") overlay.toast(`Forge asks before opening ${result.label}`);
    else overlay.toast(`Opened ${result.label} in Forge`);
  } catch {
    overlay.toast("The Forge Companion extension was updated. Reload the page.", true);
  }
}

function setInspecting(on: boolean): void {
  inspecting = on;
  lastProbe = null;
  if (on) overlay.showHint(shortcut);
  else {
    overlay.hideHint();
    overlay.clearHighlight();
  }
}

// ---- Events --------------------------------------------------------------------------

const isAltClick = (e: MouseEvent) => e.altKey && !e.ctrlKey && !e.metaKey && !e.shiftKey;

document.addEventListener(
  "mousemove",
  (e) => {
    const active = inspecting || (config.altClick && e.altKey);
    if (!active) {
      if (!inspecting && frame === 0) overlay.clearHighlight();
      return;
    }
    cancelAnimationFrame(frame);
    const target = e.target;
    frame = requestAnimationFrame(() => {
      frame = 0;
      show(target);
    });
  },
  { capture: true, passive: true },
);

// While inspecting, the page must not react to presses (buttons, links, drag starts).
for (const type of ["mousedown", "mouseup", "pointerdown", "pointerup", "dblclick"] as const) {
  document.addEventListener(
    type,
    (e) => {
      if (inspecting) {
        e.preventDefault();
        e.stopImmediatePropagation();
      }
    },
    true,
  );
}

document.addEventListener(
  "click",
  (e) => {
    const alt = !inspecting && config.altClick && isAltClick(e);
    if (!inspecting && !alt) return;
    if (!(e.target instanceof Element) || overlay.owns(e.target)) return;
    const found = probe(e.target);
    if (!found && alt) return; // Nothing to open: leave Alt+Click to the page.
    e.preventDefault();
    e.stopImmediatePropagation();
    if (!found) {
      overlay.toast("No component found here", true);
      return;
    }
    void open(found.info);
    if (inspecting) void send({ type: "setInspect", enabled: false });
  },
  true,
);

document.addEventListener(
  "keydown",
  (e) => {
    if (inspecting && e.key === "Escape") {
      e.preventDefault();
      e.stopImmediatePropagation();
      void send({ type: "setInspect", enabled: false });
    }
  },
  true,
);

document.addEventListener("keyup", (e) => {
  if (e.key === "Alt" && !inspecting) overlay.clearHighlight();
});

document.addEventListener("contextmenu", (e) => (contextTarget = e.target instanceof Element ? e.target : null), {
  capture: true,
  passive: true,
});

window.addEventListener("scroll", () => (inspecting ? overlay.clearHighlight() : undefined), { passive: true, capture: true });

chrome.runtime.onMessage.addListener((msg: ContentMessage) => {
  if (msg.type === "inspect") setInspecting(msg.enabled);
  else if (msg.type === "toast") overlay.toast(msg.text, msg.error);
  else if (msg.type === "openContextTarget") {
    const found = contextTarget ? probe(contextTarget) : null;
    if (found) void open(found.info);
    else overlay.toast("No component found under the cursor", true);
  }
});

chrome.storage.onChanged.addListener((changes, area) => {
  if (area !== "local") return;
  if (changes.altClick) config.altClick = changes.altClick.newValue !== false;
  if (changes.mappings) config.mappings = changes.mappings.newValue ?? [];
});

void send<ContentConfig>({ type: "contentConfig" })
  .then((c) => {
    config = c;
    if (c.shortcut !== undefined) shortcut = c.shortcut;
    if (c.inspecting) setInspecting(true);
  })
  .catch(() => {
    /* extension context gone (updated); the page keeps working without us */
  });

// ---- Live Edit ------------------------------------------------------------------------
// The top frame polls Forge through the background worker (which holds the token); style
// sheets are swapped in place, other changed files reload the page if it loaded them.

const LIVE_INTERVAL_MS = 1000;
const LIVE_IDLE_MS = 5000;

async function pollLiveEdit(since: number): Promise<void> {
  let next = since;
  let delay = LIVE_IDLE_MS;
  try {
    const poll = await send<LivePoll>({ type: "liveEditPoll", since });
    if (poll?.enabled) {
      delay = LIVE_INTERVAL_MS;
      const resources = performance.getEntriesByType("resource").map((r) => r.name);
      for (const e of since ? poll.events : []) {
        const what = plan(document, e, resources);
        if (what === "css") applyCss(document, e);
        else if (what === "reload") {
          location.reload();
          return;
        }
      }
      next = poll.seq;
    }
  } catch {
    /* extension context gone */
    return;
  }
  setTimeout(() => void pollLiveEdit(next), delay);
}

if (window.top === window) setTimeout(() => void pollLiveEdit(0), LIVE_INTERVAL_MS);
