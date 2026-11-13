import { Show } from "solid-js";
import { Dynamic } from "solid-js/web";
import { EllipsisVertical, Minus } from "lucide-solid";
import { findToolWindow } from "@/features/registry";
import { hideToolWindow, layout, type ToolWindowSlot } from "@/state/layout";
import { openToolWindowOptions } from "./toolWindowMenu";
import { t } from "@/i18n";

export function ToolWindowPanel(props: { slot: ToolWindowSlot }) {
  const tw = () => findToolWindow(layout.active[props.slot]);
  return (
    <Show when={tw()} keyed>
      {(w) => (
        <section
          class={`tool-window tool-window-${props.slot}`}
          data-tool-window-panel={w.id}
          aria-labelledby={`tool-window-title-${w.id}`}
        >
          <header class="tool-window-header">
            <span class="tool-window-title" id={`tool-window-title-${w.id}`}>
              {w.title}
            </span>
            <div class="tool-window-header-content">{w.headerContent?.()}</div>
            <div class="tool-window-actions">
              {w.actions?.()}
              <button
                class="icon-btn"
                title={t("toolwindow.options")}
                aria-label={t("toolwindow.options")}
                aria-haspopup="menu"
                data-tool-window-options
                onClick={(e) => {
                  const r = e.currentTarget.getBoundingClientRect();
                  openToolWindowOptions(w.id, r.left, r.bottom + 2);
                }}
              >
                <EllipsisVertical size={15} />
              </button>
              <button
                class="icon-btn"
                title={t("toolwindow.hide")}
                aria-label={t("toolwindow.hide")}
                aria-keyshortcuts="Shift+Escape"
                onClick={() => hideToolWindow(w.id)}
              >
                <Minus size={15} />
              </button>
            </div>
          </header>
          <div class="tool-window-body" tabIndex={-1}>
            <Dynamic component={w.component} />
          </div>
        </section>
      )}
    </Show>
  );
}
