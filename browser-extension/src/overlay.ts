/**
 * The inspect overlay: a highlight box with a label (framework, component, file:line), a hint
 * bar while inspect mode is on, and toasts. Lives in a closed shadow root so page styles
 * cannot leak in, and never receives pointer events.
 */
import type { Framework } from "./lib/types";

const FRAMEWORK_LABELS: Record<Framework, string> = {
  react: "React",
  vue: "Vue",
  vue2: "Vue 2",
  angular: "Angular",
  svelte: "Svelte",
  attribute: "Source",
};

const CSS = `
:host { all: initial; }
.layer { position: fixed; inset: 0; pointer-events: none; z-index: 2147483647;
  font: 12px/1.4 Inter, -apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif; }
.box { position: fixed; display: none; box-sizing: border-box; border: 2px solid #ff7a1a; border-radius: 3px;
  background: rgba(255, 122, 26, 0.12); box-shadow: 0 0 0 1px rgba(26, 15, 6, 0.35); transition: all 60ms ease-out; }
.label { position: fixed; display: none; align-items: center; gap: 6px; max-width: min(560px, calc(100vw - 16px));
  padding: 4px 8px 4px 4px; border-radius: 6px; background: #1a1a1f; color: #f2f2f5; white-space: nowrap;
  box-shadow: 0 6px 20px rgba(0, 0, 0, 0.35); border: 1px solid #34343c; }
.chip { padding: 1px 6px; border-radius: 4px; background: #ff7a1a; color: #1a0f06; font-weight: 700; font-size: 11px; }
.name { font-weight: 600; }
.path { color: #a6a6b2; overflow: hidden; text-overflow: ellipsis; font: 11.5px/1.4 "JetBrains Mono", "Fira Code", Menlo, Consolas, monospace; }
.hint { position: fixed; left: 50%; bottom: 16px; transform: translateX(-50%); display: none; align-items: center; gap: 8px;
  padding: 7px 12px; border-radius: 8px; background: #1a1a1f; color: #d9d9df; border: 1px solid #34343c;
  box-shadow: 0 8px 28px rgba(0, 0, 0, 0.4); }
.hint b { color: #ff8f3d; font-weight: 600; }
.hint kbd { padding: 0 5px; border-radius: 4px; border: 1px solid #44444c; background: #26262d; color: #f2f2f5;
  font: 11px/1.6 "JetBrains Mono", Menlo, monospace; }
.toast { position: fixed; right: 16px; bottom: 16px; display: none; max-width: 420px; padding: 8px 12px; border-radius: 8px;
  background: #1a1a1f; color: #f2f2f5; border: 1px solid #34343c; border-left: 3px solid #5fbf77;
  box-shadow: 0 8px 28px rgba(0, 0, 0, 0.4); }
.toast.error { border-left-color: #ef5f5f; }
.logo { width: 14px; height: 14px; border-radius: 3px; background: #ff7a1a; display: inline-block; }
`;

export interface LabelInfo {
  framework: Framework;
  name?: string;
  path?: string;
}

export class Overlay {
  private host = document.createElement("forge-companion-overlay");
  private box: HTMLDivElement;
  private label: HTMLDivElement;
  private hint: HTMLDivElement;
  private toastEl: HTMLDivElement;
  private toastTimer: ReturnType<typeof setTimeout> | undefined;

  constructor() {
    const shadow = this.host.attachShadow({ mode: "closed" });
    const style = document.createElement("style");
    style.textContent = CSS;
    const layer = document.createElement("div");
    layer.className = "layer";
    this.box = Object.assign(document.createElement("div"), { className: "box" });
    this.label = Object.assign(document.createElement("div"), { className: "label" });
    this.hint = Object.assign(document.createElement("div"), { className: "hint" });
    this.toastEl = Object.assign(document.createElement("div"), { className: "toast" });
    layer.append(this.box, this.label, this.hint, this.toastEl);
    shadow.append(style, layer);
  }

  private attach(): void {
    if (!this.host.isConnected) document.documentElement.appendChild(this.host);
  }

  /** Whether `node` belongs to the overlay (so it is never inspected itself). */
  owns(node: EventTarget | null): boolean {
    return node === this.host;
  }

  highlight(target: Element, info: LabelInfo): void {
    this.attach();
    const r = target.getBoundingClientRect();
    Object.assign(this.box.style, {
      display: "block",
      left: `${r.left}px`,
      top: `${r.top}px`,
      width: `${Math.max(r.width, 2)}px`,
      height: `${Math.max(r.height, 2)}px`,
    });
    this.label.replaceChildren();
    const chip = Object.assign(document.createElement("span"), { className: "chip", textContent: FRAMEWORK_LABELS[info.framework] });
    this.label.append(chip);
    if (info.name) this.label.append(Object.assign(document.createElement("span"), { className: "name", textContent: info.name }));
    if (info.path) this.label.append(Object.assign(document.createElement("span"), { className: "path", textContent: info.path }));
    this.label.style.display = "flex";
    // Above the box, or inside its top edge when there is no room.
    const height = this.label.offsetHeight || 24;
    const top = r.top - height - 6 >= 0 ? r.top - height - 6 : Math.max(4, r.top + 4);
    const left = Math.min(Math.max(4, r.left), window.innerWidth - this.label.offsetWidth - 4);
    Object.assign(this.label.style, { top: `${top}px`, left: `${left}px` });
  }

  clearHighlight(): void {
    this.box.style.display = "none";
    this.label.style.display = "none";
  }

  showHint(shortcut: string): void {
    this.attach();
    this.hint.replaceChildren();
    this.hint.append(
      Object.assign(document.createElement("span"), { className: "logo" }),
      Object.assign(document.createElement("b"), { textContent: "Forge" }),
      document.createTextNode("Click a component to open its source ·"),
      Object.assign(document.createElement("kbd"), { textContent: "Esc" }),
      document.createTextNode(shortcut ? `or ${shortcut} to stop` : "to stop"),
    );
    this.hint.style.display = "flex";
  }

  hideHint(): void {
    this.hint.style.display = "none";
  }

  toast(text: string, error = false): void {
    this.attach();
    this.toastEl.textContent = text;
    this.toastEl.classList.toggle("error", error);
    this.toastEl.style.display = "block";
    clearTimeout(this.toastTimer);
    this.toastTimer = setTimeout(() => (this.toastEl.style.display = "none"), error ? 4500 : 2500);
  }
}
