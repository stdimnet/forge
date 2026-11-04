/**
 * "Attach Data Source" popup for SQL scratches: the chosen data source is stored with the
 * scratch (Rust) and the scratch reopens as a query console of it.
 */
import { createSignal, For, onMount, Show } from "solid-js";
import { Database, Plus } from "lucide-solid";
import { openDataSources, sources } from "@/features/database/store";
import { runScratch, setScratchDataSource } from "./store";
import { t } from "@/i18n";

const [pending, setPending] = createSignal<string | null>(null);

/** Ask which data source the SQL scratch `name` runs against. */
export const requestAttach = (name: string) => setPending(name);

export function AttachPopupHost() {
  return (
    <Show when={pending()} keyed>
      {(name) => <AttachPopup name={name} />}
    </Show>
  );
}

function AttachPopup(props: { name: string }) {
  let el!: HTMLDivElement;
  const [index, setIndex] = createSignal(0);
  onMount(() => el.focus());
  const close = () => setPending(null);
  const choose = async (id: string | undefined) => {
    close();
    if (!id) return openDataSources({ newDriver: "mysql" });
    await setScratchDataSource(props.name, id);
    await runScratch(props.name);
  };
  const onKeyDown = (e: KeyboardEvent) => {
    const count = sources().length + 1;
    if (e.key === "Escape") close();
    else if (e.key === "ArrowDown") setIndex((i) => (i + 1) % count);
    else if (e.key === "ArrowUp") setIndex((i) => (i - 1 + count) % count);
    else if (e.key === "Enter") void choose(sources()[index()]?.id);
    else return;
    e.preventDefault();
    e.stopPropagation();
  };
  return (
    <div class="popup-backdrop" onMouseDown={(e) => e.target === e.currentTarget && close()}>
      <div ref={el} class="popup scratch-popup" tabIndex={-1} role="listbox" aria-label={t("tools.scratches.attachDataSource")} onKeyDown={onKeyDown}>
        <div class="scratch-popup-title">{t("tools.scratches.attachDataSourceTo", { name: props.name })}</div>
        <div class="scratch-popup-list">
          <For each={sources()}>
            {(ds, i) => (
              <div
                class="scratch-popup-item"
                classList={{ selected: index() === i() }}
                onMouseEnter={() => setIndex(i())}
                onClick={() => void choose(ds.id)}
              >
                <Database size={14} class="scratch-ds-icon" />
                <span class="scratch-popup-name">{ds.name}</span>
                <span class="scratch-popup-ext muted">{ds.summary}</span>
              </div>
            )}
          </For>
          <div
            class="scratch-popup-item"
            classList={{ selected: index() === sources().length }}
            onMouseEnter={() => setIndex(sources().length)}
            onClick={() => void choose(undefined)}
          >
            <Plus size={14} class="muted" />
            <span class="scratch-popup-name">{t("tools.scratches.newDataSourceEllipsis")}</span>
          </div>
        </div>
      </div>
    </div>
  );
}
