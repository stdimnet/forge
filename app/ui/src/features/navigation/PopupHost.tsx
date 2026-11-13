/** Renders navigation popups (Search Everywhere, Recent Files) and balloons, each in its layer. */
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
import { t } from "@/i18n";
import { Layer } from "@/shell/layers";

export function PopupHost() {
  return (
    <>
      <Show when={seOpen()}>
        <Layer kind="popup">
          <SearchEverywhere />
        </Layer>
      </Show>
      <Show when={recentOpen()}>
        <Layer kind="popup">
          <RecentFilesPopup />
        </Layer>
      </Show>
      <Show when={fileStructureSession()} keyed>
        {(s) => (
          <Layer kind="popup">
            <FileStructurePopup session={s} />
          </Layer>
        )}
      </Show>
      <FloatingNavBar />
      <Show when={chooser()} keyed>
        {(c) => (
          <Layer kind="popup">
            <ChooseDeclarationPopup chooser={c} />
          </Layer>
        )}
      </Show>
      <ContextMenuHost />
      <DialogHost />
      <Layer kind="notification">
        {/* A live region from startup, so screen readers read every balloon that appears. */}
        <div class="balloons" aria-live="polite" aria-relevant="additions" aria-label={t("a11y.notifications")} role="region">
          <For each={balloons()}>
            {(b) => (
              <div
                class="balloon"
                classList={{ interactive: !!b.actions?.length || b.sticky }}
                role={b.type === "error" ? "alert" : undefined}
                data-balloon-type={b.type ?? "info"}
              >
                <Switch fallback={<Info size={15} class="balloon-icon" aria-label={t("a11y.balloon.info")} />}>
                  <Match when={b.type === "warning"}>
                    <TriangleAlert size={15} class="balloon-icon warning" aria-label={t("a11y.balloon.warning")} />
                  </Match>
                  <Match when={b.type === "error"}>
                    <CircleAlert size={15} class="balloon-icon error" aria-label={t("a11y.balloon.error")} />
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
                  <button class="balloon-close" title={t("common.close")} aria-label={t("common.close")} onClick={() => dismissBalloon(b.id)}>
                    <X size={13} />
                  </button>
                </Show>
              </div>
            )}
          </For>
        </div>
      </Layer>
    </>
  );
}
