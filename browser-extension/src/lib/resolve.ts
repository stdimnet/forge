/**
 * Find the component (and, where the framework exposes it, the exact source location) of a
 * DOM element. Runs in the page's own JavaScript world, where framework internals are
 * visible: React fibers, Vue instances, Angular's `ng` dev API, Svelte metadata, plus
 * `data-*` source attributes added by build plugins.
 */
import type { SourceInfo } from "./types";

export interface Resolved extends SourceInfo {
  /** Element to highlight: the component's root, or the element itself for element-level sources. */
  element: Element;
}

/** Loosely typed framework internals. */
type Any = any;

const MAX_DEPTH = 60;

const basename = (file: string) => file.replace(/[?#].*$/, "").split(/[\\/]/).pop() ?? file;
const stripExt = (name: string) => name.replace(/\.[^.]+$/, "");
const int = (v: unknown): number | undefined => {
  const n = typeof v === "number" ? v : Number.parseInt(String(v ?? ""), 10);
  return Number.isFinite(n) && n > 0 ? n : undefined;
};

/** `path:line:col` (vite-plugin-vue-inspector) or `path:line:col:tag` (code-inspector). */
function splitLocation(value: string): { file: string; line?: number; column?: number } {
  const m = /^(.*?)(?::(\d+))?(?::(\d+))?(?::[A-Za-z][\w-]*)?$/.exec(value);
  if (!m || !m[1]) return { file: value };
  return { file: m[1], line: int(m[2]), column: int(m[3]) };
}

/** `data-source-file`/`-line`, react-dev-inspector, vite-plugin-vue-inspector, code-inspector. */
export function fromAttributes(el: Element): Resolved | null {
  const attr = (n: string) => el.getAttribute(n);
  const file = attr("data-source-file") ?? attr("data-inspector-relative-path");
  if (file) {
    return {
      framework: "attribute",
      element: el,
      file,
      line: int(attr("data-source-line") ?? attr("data-inspector-line")),
      column: int(attr("data-source-column") ?? attr("data-inspector-column")),
      name: attr("data-source-component") ?? attr("data-inspector-component") ?? undefined,
    };
  }
  const loc = attr("data-v-inspector") ?? attr("data-insp-path");
  if (loc) return { framework: "attribute", element: el, ...splitLocation(loc) };
  return null;
}

// ---- React -------------------------------------------------------------------------------

function reactFiber(el: Element): Any {
  for (const key of Object.keys(el)) {
    if (key.startsWith("__reactFiber$") || key.startsWith("__reactInternalInstance$")) return (el as Any)[key];
  }
  return null;
}

export function reactComponentName(type: Any): string | undefined {
  if (!type || typeof type === "string") return undefined;
  if (typeof type === "function") return type.displayName || type.name || undefined;
  // forwardRef / memo / lazy wrappers
  return type.displayName || reactComponentName(type.render) || reactComponentName(type.type) || undefined;
}

/** First user frame of a React 19 `_debugStack`: the file that created the element. */
export function fileFromDebugStack(stack: unknown): string | undefined {
  const text = typeof stack === "string" ? stack : ((stack as Any)?.stack as string | undefined);
  if (!text) return undefined;
  for (const line of text.split("\n").slice(1)) {
    const m = /\(?((?:https?|file|webpack(?:-internal)?):\/\/[^\s)]+?)(?::\d+){1,2}\)?\s*$/.exec(line.trim());
    if (!m) continue;
    const url = m[1];
    if (/\/node_modules\/|react-stack-top-frame|\/react(-dom)?[.-]/.test(url)) continue;
    return url;
  }
  return undefined;
}

function resolveReact(el: Element): Resolved | null {
  const fiber = reactFiber(el);
  if (!fiber) return null;
  // Nearest composite (function/class) fiber owning this element: the component name.
  let owner = fiber._debugOwner ?? fiber.return;
  let name: string | undefined;
  for (let f = owner, i = 0; f && i < MAX_DEPTH; f = f.return, i++) {
    name = reactComponentName(f.type);
    if (name) {
      owner = f;
      break;
    }
  }
  // React ≤ 18 in development: where the JSX element was written.
  for (let f = fiber, i = 0; f && i < MAX_DEPTH; f = f.return, i++) {
    const src = f._debugSource ?? f.memoizedProps?.__source;
    if (src?.fileName) {
      return { framework: "react", element: el, name, file: src.fileName, line: int(src.lineNumber), column: int(src.columnNumber) };
    }
  }
  // React 19 dropped _debugSource: use the owner's file from its debug stack (if any) and
  // let Forge find the component's declaration by name.
  const file = fileFromDebugStack(fiber._debugStack) ?? fileFromDebugStack(owner?._debugStack);
  if (!name && !file) return null;
  return { framework: "react", element: el, name, file };
}

// ---- Vue ---------------------------------------------------------------------------------

function vueName(type: Any, file?: string): string | undefined {
  return type?.name || type?.__name || (file ? stripExt(basename(file)) : undefined);
}

function resolveVue3(el: Element): Resolved | null {
  let inst = (el as Any).__vueParentComponent;
  for (let i = 0; inst && i < MAX_DEPTH; inst = inst.parent, i++) {
    const file: string | undefined = inst.type?.__file;
    const name = vueName(inst.type, file);
    if (!file && !name) continue;
    const root = inst.subTree?.el ?? inst.vnode?.el;
    return { framework: "vue", element: root instanceof Element ? root : el, name, file };
  }
  return null;
}

function resolveVue2(el: Element): Resolved | null {
  let vm = (el as Any).__vue__;
  for (let i = 0; vm && i < MAX_DEPTH; vm = vm.$parent, i++) {
    const opts = vm.$options ?? {};
    const file: string | undefined = opts.__file;
    const name: string | undefined = opts.name || opts._componentTag || (file ? stripExt(basename(file)) : undefined);
    if (!file && !name) continue;
    return { framework: "vue2", element: vm.$el instanceof Element ? vm.$el : el, name, file };
  }
  return null;
}

// ---- Svelte ------------------------------------------------------------------------------

function resolveSvelte(el: Element): Resolved | null {
  const meta = (el as Any).__svelte_meta;
  const loc = meta?.loc;
  if (!loc?.file) return null;
  // Svelte 4 reports 0-based lines; Svelte 5 (which adds `parent`) 1-based. Columns are 0-based.
  const svelte5 = "parent" in meta;
  return {
    framework: "svelte",
    element: el,
    name: stripExt(basename(loc.file)),
    file: loc.file,
    line: typeof loc.line === "number" ? loc.line + (svelte5 ? 0 : 1) : undefined,
    column: typeof loc.column === "number" ? loc.column + 1 : undefined,
  };
}

// ---- Angular -----------------------------------------------------------------------------

/** Constructor names may carry a leading `_` from decorator down-leveling. */
export const angularName = (ctor: Any): string | undefined => (ctor?.name ? String(ctor.name).replace(/^_+/, "") : undefined);

function resolveAngular(el: Element, win: Any): Resolved | null {
  const ng = win?.ng;
  if (typeof ng?.getComponent !== "function") return null;
  let component: Any = null;
  try {
    component = ng.getComponent(el) ?? ng.getOwningComponent?.(el);
  } catch {
    return null;
  }
  const name = angularName(component?.constructor);
  if (!name) return null;
  let host: Element = el;
  try {
    host = ng.getHostElement?.(component) ?? el;
  } catch {
    /* keep the hovered element */
  }
  return { framework: "angular", element: host, name };
}

// ---- Entry point -------------------------------------------------------------------------

/** Resolve `target`, walking up to its ancestors until something is found. */
export function resolveElement(target: Element, win: Any = globalThis): Resolved | null {
  const angular = resolveAngular(target, win);
  if (angular) return angular;
  let el: Element | null = target;
  for (let i = 0; el && i < MAX_DEPTH; el = el.parentElement, i++) {
    const found = fromAttributes(el) ?? resolveReact(el) ?? resolveVue3(el) ?? resolveVue2(el) ?? resolveSvelte(el);
    if (found) return found;
  }
  return null;
}
