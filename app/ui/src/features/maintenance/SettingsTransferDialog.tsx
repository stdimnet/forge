/**
 * File | Manage IDE Settings | Export Settings / Import Settings: PHPStorm's "Select
 * Components to Export / Import" chooser. The groups and their sizes come from Rust
 * (`forge_maintenance::groups`); export also asks for the archive's path.
 */
import { createSignal, For, Show } from "solid-js";
import { FolderOpen } from "lucide-solid";
import { isTauri } from "@/ipc";
import { closeDialog } from "@/shell/window/dialogs";
import type { SettingsGroupCount } from "@/ipc/maintenance";
import { t } from "@/i18n";

export const TRANSFER_DIALOG = "settings-transfer";

export interface TransferProps {
  mode: "export" | "import";
  groups: SettingsGroupCount[];
  /** Export: the suggested archive path. Import: the chosen archive. */
  path: string;
}

export interface TransferResult {
  groups: string[];
  path: string;
}

const countText = (g: SettingsGroupCount) => (g.count === 1 ? "1 setting" : `${g.count} settings`);

export function SettingsTransferDialog(props: { props: TransferProps }) {
  const p = props.props;
  const [chosen, setChosen] = createSignal<string[]>(p.groups.map((g) => g.id));
  const [path, setPath] = createSignal(p.path);
  const all = () => chosen().length === p.groups.length;
  const toggle = (id: string, on: boolean) => setChosen((list) => (on ? [...list.filter((x) => x !== id), id] : list.filter((x) => x !== id)));
  const ok = () => {
    if (!chosen().length || !path().trim()) return;
    closeDialog(TRANSFER_DIALOG, { groups: chosen(), path: path().trim() } satisfies TransferResult);
  };
  const browse = async () => {
    const { save } = await import("@tauri-apps/plugin-dialog");
    const picked = await save({ defaultPath: path(), filters: [{ name: "Settings archive", extensions: ["zip"] }] });
    if (picked) setPath(picked);
  };
  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Escape") closeDialog(TRANSFER_DIALOG, null);
    else if (e.key === "Enter" && (e.target as HTMLElement).tagName !== "BUTTON") ok();
    else return;
    e.preventDefault();
    e.stopPropagation();
  };
  return (
    <div class="mnt-dialog" onKeyDown={onKeyDown}>
      <div class="mnt-body">
        <div class="mnt-section">{p.mode === "export" ? t("tools.maintenance.selectComponentsToExport") : t("tools.maintenance.selectComponentsToImport")}</div>
        <div class="mnt-groups" data-testid="settings-groups">
          <label class="mnt-check mnt-all">
            <input type="checkbox" checked={all()} onChange={(e) => setChosen(e.currentTarget.checked ? p.groups.map((g) => g.id) : [])} />
            <span>{t("tools.maintenance.selectAll")}</span>
          </label>
          <For each={p.groups} fallback={<div class="mnt-muted">{t("tools.maintenance.thereAreNoChangedSettings")}</div>}>
            {(g) => (
              <label class="mnt-check" data-group={g.id}>
                <input type="checkbox" checked={chosen().includes(g.id)} onChange={(e) => toggle(g.id, e.currentTarget.checked)} />
                <span>
                  {g.label}
                  <span class="mnt-detail">{countText(g)}</span>
                </span>
              </label>
            )}
          </For>
        </div>
        <Show
          when={p.mode === "export"}
          fallback={
            <p class="mnt-muted">
              {t("tools.maintenance.theSelectedComponentsReplaceYour")}
              <br />
              {t("tools.maintenance.from", { path: p.path })}
            </p>
          }
        >
          <label class="mnt-path">
            <span>{t("tools.maintenance.exportSettingsToLabel")}</span>
            <input value={path()} spellcheck={false} onInput={(e) => setPath(e.currentTarget.value)} data-autofocus />
            <Show when={isTauri()}>
              <button class="icon-btn" title={t("common.browseEllipsis")} onClick={() => void browse()}>
                <FolderOpen size={15} />
              </button>
            </Show>
          </label>
          <p class="mnt-muted">{t("tools.maintenance.apiKeysAndPasswordsStay")}</p>
        </Show>
      </div>
      <footer class="mnt-footer">
        <span class="mnt-spacer" />
        <button class="btn-secondary set-btn" onClick={() => closeDialog(TRANSFER_DIALOG, null)}>
          {t("common.cancel")}
        </button>
        <button class="btn-primary set-btn" disabled={!chosen().length || !path().trim()} onClick={ok}>
          OK
        </button>
      </footer>
    </div>
  );
}
