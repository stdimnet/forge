/**
 * Minimal Settings dialog listing the pages registered with `registerSettingsPage`.
 * Only used while no feature provides the real Settings dialog (`settings.open`).
 */
import { For, Show, createMemo, createSignal } from "solid-js";
import { Dynamic } from "solid-js/web";
import { ChevronRight, Search, X } from "lucide-solid";
import { settingsPages, type SettingsPage } from "@/features/settingsPages";

const [settingsOpen, setSettingsOpen] = createSignal<string | null>(null);
export { settingsOpen };

export function openFallbackSettings(pageId?: string): void {
  setSettingsOpen(pageId ?? settingsPages()[0]?.id ?? "");
}

export function SettingsFallback() {
  const [filter, setFilter] = createSignal("");
  const pages = createMemo(() => {
    const q = filter().trim().toLowerCase();
    return settingsPages().filter(
      (p) => !q || [p.title, p.group, ...(p.keywords ?? [])].some((w) => w.toLowerCase().includes(q)),
    );
  });
  const groups = createMemo(() => {
    const out = new Map<string, SettingsPage[]>();
    for (const p of pages()) out.set(p.group, [...(out.get(p.group) ?? []), p]);
    return [...out.entries()];
  });
  const current = () => settingsPages().find((p) => p.id === settingsOpen());
  const close = () => setSettingsOpen(null);

  return (
    <div class="popup-backdrop modal" onMouseDown={(e) => e.target === e.currentTarget && close()}>
      <div class="dialog settings-dialog" role="dialog" aria-label="Settings" onKeyDown={(e) => e.key === "Escape" && close()}>
        <div class="dialog-title settings-titlebar">
          <span>Settings</span>
          <button class="icon-btn" title="Close" onClick={close}>
            <X size={14} />
          </button>
        </div>
        <div class="settings-body">
          <aside class="settings-nav">
            <div class="search-field">
              <Search size={14} class="muted" />
              <input placeholder="Search settings" value={filter()} onInput={(e) => setFilter(e.currentTarget.value)} />
            </div>
            <div class="settings-tree">
              <For each={groups()}>
                {([group, list]) => (
                  <>
                    <Show when={!(list.length === 1 && list[0].title === group)}>
                      <div class="settings-group">
                        <ChevronRight size={12} class="muted" /> {group}
                      </div>
                    </Show>
                    <For each={list}>
                      {(p) => (
                        <div
                          class="settings-item"
                          classList={{ selected: p.id === settingsOpen(), nested: !(list.length === 1 && p.title === group) }}
                          onClick={() => setSettingsOpen(p.id)}
                        >
                          {p.title}
                        </div>
                      )}
                    </For>
                  </>
                )}
              </For>
            </div>
          </aside>
          <section class="settings-content">
            <Show when={current()} fallback={<div class="se-empty">No settings pages registered yet.</div>}>
              {(p) => (
                <>
                  <div class="settings-crumb">
                    <Show when={p().group !== p().title}>
                      <span class="muted">{p().group}</span>
                      <ChevronRight size={12} class="muted" />
                    </Show>
                    <span>{p().title}</span>
                  </div>
                  <div class="settings-page">
                    <Dynamic component={p().component} />
                  </div>
                </>
              )}
            </Show>
          </section>
        </div>
        <div class="dialog-buttons">
          <button class="btn-secondary" onClick={close}>
            Cancel
          </button>
          <button class="btn-primary" onClick={close}>
            OK
          </button>
        </div>
      </div>
    </div>
  );
}
