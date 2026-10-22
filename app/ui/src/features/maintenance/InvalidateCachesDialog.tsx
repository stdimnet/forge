/**
 * File | Invalidate Caches…: PHPStorm's dialog. The texts and options come from Rust
 * (`maintenance_invalidate_model`); "Invalidate and Restart" schedules the cleanup for the next
 * start and restarts Forge, "Just Restart" only restarts.
 */
import { createResource, createSignal, For, Show } from "solid-js";
import { TriangleAlert } from "lucide-solid";
import { closeDialog } from "@/shell/window/dialogs";
import { maintenanceApi, type CacheOption } from "@/ipc/maintenance";

export const INVALIDATE_DIALOG = "invalidate-caches";

export interface InvalidateResult {
  options: CacheOption[];
  invalidate: boolean;
}

export function InvalidateCachesDialog() {
  const [model] = createResource(() => maintenanceApi.invalidateModel());
  const [chosen, setChosen] = createSignal<CacheOption[]>([]);
  const toggle = (id: CacheOption, on: boolean) =>
    setChosen((list) => (on ? [...list.filter((x) => x !== id), id] : list.filter((x) => x !== id)));
  const close = (result: InvalidateResult | null) => closeDialog(INVALIDATE_DIALOG, result);
  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Escape") close(null);
    else if (e.key === "Enter" && (e.target as HTMLElement).tagName !== "BUTTON") close({ options: chosen(), invalidate: true });
    else return;
    e.preventDefault();
    e.stopPropagation();
  };
  return (
    <div class="mnt-dialog" onKeyDown={onKeyDown} tabIndex={-1} ref={(el) => queueMicrotask(() => el.focus())}>
      <Show when={model()}>
        {(m) => (
          <>
            <div class="mnt-body">
              <p>{m().message}</p>
              <Show when={chosen().includes("fileSystemAndLocalHistory")}>
                <p class="mnt-warning">
                  <TriangleAlert size={14} /> {m().warning}
                </p>
              </Show>
              <p class="mnt-muted">Always cleared: {m().always.join(", ")}.</p>
              <div class="mnt-section">Optional:</div>
              <For each={m().options}>
                {(o) => (
                  <label class="mnt-check" data-option={o.id}>
                    <input type="checkbox" checked={chosen().includes(o.id)} onChange={(e) => toggle(o.id, e.currentTarget.checked)} />
                    <span>
                      {o.label}
                      <span class="mnt-detail">{o.detail}</span>
                    </span>
                  </label>
                )}
              </For>
            </div>
            <footer class="mnt-footer">
              <button class="btn-secondary set-btn" onClick={() => close({ options: [], invalidate: false })}>
                {m().justRestartLabel}
              </button>
              <span class="mnt-spacer" />
              <button class="btn-secondary set-btn" onClick={() => close(null)}>
                Cancel
              </button>
              <button class="btn-primary set-btn" data-testid="invalidate-restart" onClick={() => close({ options: chosen(), invalidate: true })}>
                {m().invalidateLabel}
              </button>
            </footer>
          </>
        )}
      </Show>
    </div>
  );
}
