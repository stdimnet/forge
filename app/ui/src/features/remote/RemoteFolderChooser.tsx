/**
 * Choose a folder on the connected host (Gateway's "Project directory" chooser): a path
 * field, Home / Up / Refresh / hidden files, and the folder listing from the Forge agent.
 * Double click enters a folder, a single click selects it.
 */
import { createEffect, createMemo, createResource, createSignal, For, Show } from "solid-js";
import { ArrowUp, Eye, EyeOff, File, Folder, FolderCode, House, LoaderCircle, RefreshCw, X } from "lucide-solid";
import { remoteApi, type BrowseEntry } from "@/ipc/remote";
import { folderChooser, type FolderChooserRequest } from "./store";
import { t } from "@/i18n";

export function RemoteFolderChooser() {
  return <Show when={folderChooser()}>{(req) => <Chooser request={req()} />}</Show>;
}

function Chooser(props: { request: FolderChooserRequest }) {
  const [dir, setDir] = createSignal(props.request.start || "~");
  const [input, setInput] = createSignal(props.request.start || "~");
  const [selected, setSelected] = createSignal<string | null>(null);
  const [hidden, setHidden] = createSignal(false);
  const browse = (d: string) => (props.request.browse ? props.request.browse(d) : remoteApi.browse(props.request.connectionId, d));
  const [listing, { refetch }] = createResource(dir, browse);

  // The path field follows navigation; selection shows the selected folder.
  createEffect(() => {
    const l = listing();
    if (l) {
      setInput(selected() ?? l.path);
    }
  });

  const entries = createMemo(() =>
    (listing()?.entries ?? []).filter((e) => hidden() || !e.name.startsWith(".")),
  );

  const go = (path: string) => {
    setSelected(null);
    setDir(path);
  };

  const choose = () => props.request.pick((selected() ?? listing()?.path ?? input()).trim() || null);

  const rowIcon = (e: BrowseEntry) => (e.project ? FolderCode : e.dir ? Folder : File);

  return (
    <div class="ssh-overlay ssh-overlay-top" onMouseDown={(e) => e.target === e.currentTarget && props.request.pick(null)}>
      <div
        class="ssh-dialog ssh-chooser"
        role="dialog"
        aria-label={t("tools.remote.chooseProjectDirectory")}
        data-testid="remote-chooser"
        onKeyDown={(e) => e.key === "Escape" && props.request.pick(null)}
      >
        <div class="ssh-titlebar">
          <span>{t("tools.remote.chooseProjectDirectoryOn", { label: props.request.label })}</span>
          <button class="icon-btn" title={t("common.close")} onClick={() => props.request.pick(null)}>
            <X size={15} />
          </button>
        </div>
        <div class="ssh-chooser-toolbar">
          <button class="icon-btn" title={t("tools.remote.homeDirectory")} onClick={() => go("~")}>
            <House size={15} />
          </button>
          <button
            class="icon-btn"
            title={t("tools.remote.parentDirectory")}
            disabled={!listing()?.parent}
            onClick={() => listing()?.parent && go(listing()!.parent!)}
          >
            <ArrowUp size={15} />
          </button>
          <button class="icon-btn" title={t("common.refresh")} onClick={() => void refetch()}>
            <RefreshCw size={14} />
          </button>
          <button
            class="icon-btn"
            classList={{ active: hidden() }}
            title={hidden() ? t("tools.remote.hideHiddenFiles") : t("tools.remote.showHiddenFiles")}
            onClick={() => setHidden((v) => !v)}
          >
            {hidden() ? <Eye size={15} /> : <EyeOff size={15} />}
          </button>
          <input
            class="ssh-input mono ssh-chooser-path"
            value={input()}
            spellcheck={false}
            onInput={(e) => setInput(e.currentTarget.value)}
            onKeyDown={(e) => e.key === "Enter" && go(input())}
            data-testid="remote-chooser-path"
          />
        </div>
        <div class="ssh-chooser-list" role="listbox">
          <Show when={listing.loading}>
            <div class="ssh-chooser-empty muted">
              <LoaderCircle size={14} class="ssh-spin" /> {t("common.loadingEllipsis")}
            </div>
          </Show>
          <Show when={listing.error}>
            <div class="ssh-chooser-empty ssh-error-text">{String((listing.error as Error)?.message ?? listing.error)}</div>
          </Show>
          <Show when={!listing.loading && !listing.error}>
            <For each={entries()} fallback={<div class="ssh-chooser-empty muted">{t("tools.remote.emptyFolder")}</div>}>
              {(e) => (
                <div
                  class="ssh-chooser-row"
                  classList={{ selected: selected() === e.path, file: !e.dir }}
                  role="option"
                  aria-selected={selected() === e.path}
                  data-path={e.path}
                  onClick={() => e.dir && setSelected(e.path)}
                  onDblClick={() => e.dir && go(e.path)}
                >
                  {(() => {
                    const Icon = rowIcon(e);
                    return <Icon size={15} class={e.project ? "ssh-project-icon" : e.dir ? "ssh-folder-icon" : "muted"} />;
                  })()}
                  <span class="ssh-chooser-name">{e.name}</span>
                  <Show when={e.project}>
                    <span class="ssh-chooser-tag">project</span>
                  </Show>
                </div>
              )}
            </For>
          </Show>
        </div>
        <div class="ssh-footer">
          <span class="ssh-progress muted ssh-small">{t("tools.remote.doubleClickAFolderTo")}</span>
          <button class="btn-secondary" onClick={() => props.request.pick(null)}>
            {t("common.cancel")}
          </button>
          <button class="btn-primary" disabled={!listing()} onClick={choose} data-testid="remote-chooser-ok">
            OK
          </button>
        </div>
      </div>
    </div>
  );
}
