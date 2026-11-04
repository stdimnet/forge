/**
 * Settings editor of `vim.shortcutHandlers` (IdeaVim's Settings | Editor | Vim): for each
 * Ctrl shortcut and mode, whether Vim or the IDE handles it. The rows and their defaults
 * come from Rust (`forge_vim::handlers`); the setting keeps the rows the user changed.
 * Rows decided by `sethandler` in ~/.ideavimrc are shown read-only.
 */
import { createMemo, For, Show } from "solid-js";
import type { SettingObject, SettingSpec } from "@/ipc/settings";
import type { VimHandler, VimHandlerOverride, VimHandlerRow } from "@/ipc/vim";
import { draftValue, setDraft } from "@/features/settings/dialogState";
import { Hl } from "@/features/settings/dialogSearch";
import { vimEngine, vimReady } from "./engine";
import { t } from "@/i18n";

type ModeKey = "normal" | "insert" | "visual";
const MODES: { key: ModeKey; title: string }[] = [
  { key: "normal", title: "Normal" },
  { key: "insert", title: "Insert" },
  { key: "visual", title: "Visual" },
];

export function HandlersEditor(props: { spec: SettingSpec }) {
  const overrides = () => (draftValue<SettingObject[]>(props.spec.key) ?? []) as unknown as VimHandlerOverride[];
  const rows = createMemo<VimHandlerRow[]>(() => (vimReady() ? vimEngine.handlers(overrides()) : []));
  const defaults = createMemo<VimHandlerRow[]>(() => (vimReady() ? vimEngine.handlers([]) : []));

  const set = (row: VimHandlerRow, mode: ModeKey, value: VimHandler) => {
    const next = { shortcut: row.shortcut, normal: row.normal, insert: row.insert, visual: row.visual, [mode]: value };
    const def = defaults().find((d) => d.shortcut === row.shortcut);
    const rest = overrides().filter((o) => o.shortcut !== row.shortcut);
    const same = def && def.normal === next.normal && def.insert === next.insert && def.visual === next.visual;
    setDraft(props.spec.key, (same ? rest : [...rest, next]) as unknown as SettingObject[]);
  };
  const setAll = (value: VimHandler) => {
    const list = rows().map((r) => ({
      shortcut: r.shortcut,
      normal: r.normalAction ? value : "ide",
      insert: r.insertAction ? value : "ide",
      visual: r.visualAction ? value : "ide",
    }));
    setDraft(props.spec.key, list as unknown as SettingObject[]);
  };

  return (
    <div class="vim-handlers">
      <div class="vim-handlers-toolbar">
        <button class="btn-secondary" onClick={() => setAll("vim")}>
          {t("tools.vim.allVim")}
        </button>
        <button class="btn-secondary" onClick={() => setAll("ide")}>
          {t("tools.vim.allIde")}
        </button>
        <button class="btn-secondary" onClick={() => setDraft(props.spec.key, [])}>
          {t("tools.vim.defaults")}
        </button>
      </div>
      <table class="vim-handlers-table">
        <thead>
          <tr>
            <th>{t("tools.vim.shortcut")}</th>
            <th>{t("tools.vim.vimAction")}</th>
            <For each={MODES}>{(m) => <th>{m.title}</th>}</For>
          </tr>
        </thead>
        <tbody>
          <For each={rows()}>
            {(row) => (
              <tr data-shortcut={row.shortcut} classList={{ "from-rc": row.fromRc }}>
                <td class="vim-handlers-key">
                  <Hl text={row.shortcut} />
                  <Show when={row.fromRc}>
                    <span class="vim-handlers-rc" title={t("tools.vim.setBySethandlerInIdeavimrc")}>
                      .ideavimrc
                    </span>
                  </Show>
                </td>
                <td class="vim-handlers-action">
                  <Hl text={row.normalAction ?? row.insertAction ?? row.visualAction ?? ""} />
                </td>
                <For each={MODES}>
                  {(m) => {
                    const action = () => row[`${m.key}Action` as const];
                    return (
                      <td>
                        <select
                          class="set-input set-select"
                          value={row[m.key]}
                          disabled={row.fromRc}
                          title={action() ? `Vim: ${action()}` : t("tools.vim.vimDoesNothingWithThis")}
                          onChange={(e) => set(row, m.key, e.currentTarget.value as VimHandler)}
                        >
                          <option value="vim">Vim</option>
                          <option value="ide">IDE</option>
                        </select>
                      </td>
                    );
                  }}
                </For>
              </tr>
            )}
          </For>
        </tbody>
      </table>
    </div>
  );
}
