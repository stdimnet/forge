/**
 * TEMPORARY — drop this file (and its import in ./index.ts) once the real Settings dialog
 * renders pages from `registerSettingsPage`. It only makes the AI Providers page reachable
 * via the `ai.settings` command (and `window.__forgeOpenAiSettings` for screenshots).
 */
import { onCleanup } from "solid-js";
import { render } from "solid-js/web";
import { X } from "lucide-solid";
import { AiProvidersPage } from "./AiProvidersPage";

let dispose: (() => void) | undefined;

export function openTempAiSettings(): void {
  if (dispose) return;
  const host = document.createElement("div");
  document.body.appendChild(host);
  const close = () => {
    dispose?.();
    dispose = undefined;
    host.remove();
  };
  dispose = render(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && !document.querySelector(".ai-picker") && close();
    document.addEventListener("keydown", onKey);
    onCleanup(() => document.removeEventListener("keydown", onKey));
    return (
      <div
        style={{ position: "fixed", inset: "0", "z-index": "100", display: "grid", "place-items": "center", background: "rgba(0,0,0,0.35)" }}
      >
        <div
          style={{
            width: "min(1040px, 94vw)",
            "max-height": "90vh",
            overflow: "auto",
            background: "var(--bg-panel)",
            border: "1px solid var(--border-strong)",
            "border-radius": "10px",
            "box-shadow": "var(--shadow)",
          }}
        >
          <div style={{ display: "flex", "align-items": "center", padding: "10px 12px 10px 20px", "border-bottom": "1px solid var(--border)" }}>
            <span class="muted">Tools</span>
            <span class="muted" style={{ margin: "0 6px" }}>›</span>
            <span style={{ color: "var(--fg-strong)", "font-weight": 600 }}>AI Providers</span>
            <span style={{ flex: 1 }} />
            <button class="icon-btn" title="Close" onClick={close}>
              <X size={16} />
            </button>
          </div>
          <div style={{ padding: "14px 20px 20px" }}>
            <AiProvidersPage />
          </div>
        </div>
      </div>
    );
  }, host);
}

(window as unknown as { __forgeOpenAiSettings?: () => void }).__forgeOpenAiSettings = openTempAiSettings;
