/** Renders navigation popups (Search Everywhere, Recent Files) and balloons. */
import { For, Show } from "solid-js";
import { Info, X } from "lucide-solid";
import { balloons, dismissBalloon } from "@/keymap";
import { ContextMenuHost } from "@/features/contextMenu/ContextMenu";
import { DialogHost } from "@/features/contextMenu/dialogs";
import { SearchEverywhere } from "@/features/search/SearchEverywhere";
import { seOpen } from "@/features/search/state";
import { RecentFilesPopup, recentOpen } from "./RecentFiles";

export function PopupHost() {
  return (
    <>
      <Show when={seOpen()}>
        <SearchEverywhere />
      </Show>
      <Show when={recentOpen()}>
        <RecentFilesPopup />
      </Show>
      <ContextMenuHost />
      <DialogHost />
      <div class="balloons">
        <For each={balloons()}>
          {(b) => (
            <div class="balloon" classList={{ interactive: !!b.actions?.length }} role="status">
              <Info size={15} class="balloon-icon" />
              <div class="balloon-body">
                <div class="balloon-text">{b.text}</div>
                <Show when={b.detail}>
                  <div class="balloon-detail">{b.detail}</div>
                </Show>
                <Show when={b.actions?.length}>
                  <div class="balloon-actions">
                    <For each={b.actions}>
                      {(a) => (
                        <button
                          class="balloon-action"
                          onClick={() => {
                            dismissBalloon(b.id);
                            a.run();
                          }}
                        >
                          {a.label}
                        </button>
                      )}
                    </For>
                  </div>
                </Show>
              </div>
              <Show when={b.actions?.length}>
                <button class="balloon-close" title="Close" onClick={() => dismissBalloon(b.id)}>
                  <X size={13} />
                </button>
              </Show>
            </div>
          )}
        </For>
      </div>
    </>
  );
}
