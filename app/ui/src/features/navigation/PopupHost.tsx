/** Renders navigation popups (Search Everywhere, Recent Files) and balloons. */
import { For, Show } from "solid-js";
import { Info } from "lucide-solid";
import { balloons } from "@/keymap";
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
      <div class="balloons">
        <For each={balloons()}>
          {(b) => (
            <div class="balloon" role="status">
              <Info size={15} class="balloon-icon" />
              <div>
                <div class="balloon-text">{b.text}</div>
                <Show when={b.detail}>
                  <div class="balloon-detail">{b.detail}</div>
                </Show>
              </div>
            </div>
          )}
        </For>
      </div>
    </>
  );
}
