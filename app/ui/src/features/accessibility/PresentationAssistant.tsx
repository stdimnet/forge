/** The Presentation Assistant popup: action name, shortcut and the alternate keymap's shortcut. */
import { Show } from "solid-js";
import { getSetting } from "@/state/settings";
import { POPUP_FONT, presentation, type PopupPosition, type PopupSize } from "./presentation";

export function PresentationAssistant() {
  const position = () => getSetting<PopupPosition>("appearance.presentationAssistant.position", "bottom-center");
  const size = () => getSetting<PopupSize>("appearance.presentationAssistant.size", "medium");
  return (
    <Show when={presentation()}>
      {(p) => (
        <div
          class={`presentation-assistant pa-${position()}`}
          style={{ "--pa-font": `${POPUP_FONT[size()] ?? POPUP_FONT.medium}px` }}
          data-presentation-assistant
          aria-hidden="true"
        >
          <span class="pa-title">{p().title}</span>
          <Show when={p().shortcuts.length}>
            <span class="pa-shortcut">{p().shortcuts.join(", ")}</span>
          </Show>
          <Show when={p().alternate}>
            {(alt) => (
              <span class="pa-alternate">
                {alt().shortcut} <span class="pa-keymap">({alt().keymap})</span>
              </span>
            )}
          </Show>
        </div>
      )}
    </Show>
  );
}
