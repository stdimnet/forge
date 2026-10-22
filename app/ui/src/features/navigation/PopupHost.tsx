/** Renders navigation popups (Search Everywhere, Recent Files) and balloons. */
import { For, Match, Show, Switch } from "solid-js";
import { CircleAlert, Info, TriangleAlert, X } from "lucide-solid";
import { balloons, dismissBalloon } from "@/keymap";
import { ContextMenuHost } from "@/features/contextMenu/ContextMenu";
import { DialogHost } from "@/features/contextMenu/dialogs";
import { SearchEverywhere } from "@/features/search/SearchEverywhere";
import { seOpen } from "@/features/search/state";
import { RecentFilesPopup, recentOpen } from "./RecentFiles";
import { ChooseDeclarationPopup } from "./ChooseDeclarationPopup";
import { chooser } from "./declarationChooser";
import { FileStructurePopup, fileStructureSession } from "@/features/fileStructure/FileStructurePopup";
import { FloatingNavBar } from "@/features/navBar";

export function PopupHost() {
  return (
    <>
      <Show when={seOpen()}>
        <SearchEverywhere />
      </Show>
      <Show when={recentOpen()}>
        <RecentFilesPopup />
      </Show>
      <Show when={fileStructureSession()} keyed>
        {(s) => <FileStructurePopup session={s} />}
      </Show>
      <FloatingNavBar />
      <Show when={chooser()} keyed>
        {(c) => <ChooseDeclarationPopup chooser={c} />}
      </Show>
      <ContextMenuHost />
      <DialogHost />
      <div class="balloons">
        <For each={balloons()}>
          {(b) => (
            <div class="balloon" classList={{ interactive: !!b.actions?.length || b.sticky }} role="status">
              <Switch fallback={<Info size={15} class="balloon-icon" />}>
                <Match when={b.type === "warning"}>
                  <TriangleAlert size={15} class="balloon-icon warning" />
                </Match>
                <Match when={b.type === "error"}>
                  <CircleAlert size={15} class="balloon-icon error" />
                </Match>
              </Switch>
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
              <Show when={b.actions?.length || b.sticky}>
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
