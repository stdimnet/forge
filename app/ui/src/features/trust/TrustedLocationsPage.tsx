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
    const dir = await open({ directory: true, multiple: false, title: "Add Trusted Location" });
    if (typeof dir === "string") setPath(dir);
  };

  return (
    <div class="set-page trust-page" data-testid="trusted-locations">
      <p class="set-desc trust-page-desc">
        Projects in these folders and their subfolders open in normal mode. Other projects ask first and can be previewed
        in Safe Mode, where none of their scripts, run configurations, tests or tools run. Projects created by Forge are
        trusted automatically.
      </p>
      <div class="set-list set-table-box trust-list">
        <div class="set-list-toolbar">
          <button
            class="icon-btn"
            title="Add"
            data-testid="trust-add"
            onClick={() => {
              setAdding(true);
              setError(null);
            }}
          >
            <Plus size={14} />
          </button>
          <button class="icon-btn" title="Remove" disabled={selected() === null} onClick={() => void remove()}>
            <Minus size={14} />
          </button>
        </div>
        <table class="set-table">
          <thead>
            <tr>
              <th>Path</th>
              <th>Trusted</th>
            </tr>
          </thead>
          <tbody>
            <For each={entries()}>
              {(e) => (
                <tr classList={{ selected: selected() === e.key }} data-entry={e.key} onClick={() => setSelected(e.key)}>
                  <td class="set-table-mono">{e.display}</td>
                  <td class="trust-kind">{e.kind === "location" ? "Projects in this folder" : "This project"}</td>
                </tr>
              )}
            </For>
          </tbody>
        </table>
        <Show when={!entries().length}>
          <div class="set-list-empty">No trusted locations</div>
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
            placeholder="~/Projects"
            value={path()}
            onInput={(e) => setPath(e.currentTarget.value)}
            spellcheck={false}
            autofocus
            data-testid="trust-add-path"
          />
          <Show when={isTauri()}>
            <button type="button" class="icon-btn" title="Browse…" onClick={() => void browse()}>
              <FolderOpen size={14} />
            </button>
          </Show>
          <button type="submit" class="btn-primary" disabled={!path().trim()}>
            Add
          </button>
          <button type="button" class="btn-secondary" onClick={() => (setAdding(false), setPath(""), setError(null))}>
            Cancel
          </button>
        </form>
      </Show>
      <Show when={error()}>
        <div class="trust-error">{error()}</div>
      </Show>
    </div>
  );
}
