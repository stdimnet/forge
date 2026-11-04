/**
 * Settings | Build, Execution, Deployment | Trusted Locations: the folders whose projects
 * open without "Trust and Open Project?", and the projects trusted one by one. Changes are
 * stored right away in the user config dir (`forge-trust`), not in the settings files, so a
 * project's own settings can never trust it.
 */
import { createResource, createSignal, For, Show } from "solid-js";
import { FolderOpen, Minus, Plus } from "lucide-solid";
import { isTauri } from "@/ipc";
import { trustApi, type TrustedEntry } from "@/ipc/trust";
import { t } from "@/i18n";

export function TrustedLocationsPage() {
  const [entries, { mutate }] = createResource(() => trustApi.locations().catch(() => [] as TrustedEntry[]), {
    initialValue: [],
  });
  const [selected, setSelected] = createSignal<string | null>(null);
  const [adding, setAdding] = createSignal(false);
  const [path, setPath] = createSignal("");
  const [error, setError] = createSignal<string | null>(null);

  const run = async (op: () => Promise<TrustedEntry[]>) => {
    try {
      mutate(await op());
      setError(null);
      return true;
    } catch (e) {
      setError(String(e).replace(/^Error: /, ""));
      return false;
    }
  };
  const add = async () => {
    if (!path().trim()) return;
    if (await run(() => trustApi.addLocation(path()))) {
      setAdding(false);
      setPath("");
    }
  };
  const remove = async () => {
    const key = selected();
    if (key && (await run(() => trustApi.removeLocation(key)))) setSelected(null);
  };
  const browse = async () => {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const dir = await open({ directory: true, multiple: false, title: t("tools.trust.addTrustedLocation") });
    if (typeof dir === "string") setPath(dir);
  };

  return (
    <div class="set-page trust-page" data-testid="trusted-locations">
      <p class="set-desc trust-page-desc">
        {t("tools.trust.projectsInTheseFoldersAnd")}
      </p>
      <div class="set-list set-table-box trust-list">
        <div class="set-list-toolbar">
          <button
            class="icon-btn"
            title={t("common.add")}
            data-testid="trust-add"
            onClick={() => {
              setAdding(true);
              setError(null);
            }}
          >
            <Plus size={14} />
          </button>
          <button class="icon-btn" title={t("common.remove")} disabled={selected() === null} onClick={() => void remove()}>
            <Minus size={14} />
          </button>
        </div>
        <table class="set-table">
          <thead>
            <tr>
              <th>{t("common.path")}</th>
              <th>{t("tools.trust.trusted")}</th>
            </tr>
          </thead>
          <tbody>
            <For each={entries()}>
              {(e) => (
                <tr classList={{ selected: selected() === e.key }} data-entry={e.key} onClick={() => setSelected(e.key)}>
                  <td class="set-table-mono">{e.display}</td>
                  <td class="trust-kind">{e.kind === "location" ? t("tools.trust.projectsInThisFolder") : t("tools.trust.thisProject")}</td>
                </tr>
              )}
            </For>
          </tbody>
        </table>
        <Show when={!entries().length}>
          <div class="set-list-empty">{t("tools.trust.noTrustedLocations")}</div>
        </Show>
      </div>
      <Show when={adding()}>
        <form
          class="trust-add-form"
          onSubmit={(e) => {
            e.preventDefault();
            void add();
          }}
        >
          <input
            class="set-input set-path trust-add-input"
            placeholder={t("tools.trust.projects")}
            value={path()}
            onInput={(e) => setPath(e.currentTarget.value)}
            spellcheck={false}
            autofocus
            data-testid="trust-add-path"
          />
          <Show when={isTauri()}>
            <button type="button" class="icon-btn" title={t("common.browseEllipsis")} onClick={() => void browse()}>
              <FolderOpen size={14} />
            </button>
          </Show>
          <button type="submit" class="btn-primary" disabled={!path().trim()}>
            {t("common.add")}
          </button>
          <button type="button" class="btn-secondary" onClick={() => (setAdding(false), setPath(""), setError(null))}>
            {t("common.cancel")}
          </button>
        </form>
      </Show>
      <Show when={error()}>
        <div class="trust-error">{error()}</div>
      </Show>
    </div>
  );
}
