/** Status bar item for the project's search index: file count, and a spinner while it builds. */
import { createSignal, onCleanup, onMount, Show } from "solid-js";
import { LoaderCircle } from "lucide-solid";
import { searchApi, type SearchStatus } from "@/ipc/search";
import { indexTooltip } from "./indexTooltip";
import { t } from "@/i18n";

const BUSY_POLL_MS = 1000;
const IDLE_POLL_MS = 30000;

export function IndexStatus() {
  const [status, setStatus] = createSignal<SearchStatus | null>(null);
  let timer: ReturnType<typeof setTimeout> | undefined;
  let stopped = false;

  const poll = async () => {
    const next = await searchApi.status().catch(() => null);
    if (stopped) return;
    setStatus(next);
    timer = setTimeout(poll, next?.indexing ? BUSY_POLL_MS : IDLE_POLL_MS);
  };
  onMount(poll);
  onCleanup(() => {
    stopped = true;
    clearTimeout(timer);
  });

  return (
    <Show when={status()?.enabled ? status() : null}>
      {(s) => (
        <span class="status-item" data-testid="index-status" title={indexTooltip(s())}>
          <Show when={s().indexing}>
            <LoaderCircle size={13} class="lsp-spin" />
          </Show>
          {s().indexing ? t("index.indexing") : t("index.files", { count: s().files })}
        </span>
      )}
    </Show>
  );
}
