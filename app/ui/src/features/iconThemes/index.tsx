/**
 * Settings | Appearance & Behavior | File Icons: the icon theme picker, the look hooks
 * (tree arrows, monochrome, accent color, size; plain schema settings) and the custom
 * associations table. Themes come from plugins (`contributes.iconThemes`, registered by the
 * plugin host) or from `registerIconTheme`.
 */
import { createSignal, For, Index, Show } from "solid-js";
import { Minus, Plus } from "lucide-solid";
import type { SettingObject, SettingSpec } from "@/ipc/settings";
import { registerObjectSettingEditor, SchemaPage } from "@/features/settings/SchemaPage";
import { registerSettingsPage } from "@/features/settingsPages";
import { draftValue, setDraft } from "@/features/settings/dialogState";
import { t } from "@/i18n";
import {
  activeIconTheme,
  ASSOCIATIONS_KEY,
  iconStyle,
  iconThemeError,
  iconThemeProblems,
  iconThemes,
  THEME_KEY,
  type IconAssociation,
} from "./state";
import "./iconThemes.css";

export { registerIconTheme, themedIcon, iconLook } from "./state";

function ThemePicker(props: { spec: SettingSpec }) {
  const value = () => draftValue<string>(props.spec.key) ?? "";
  const known = () => !value() || iconThemes().some((th) => th.id === value());
  return (
    <div class="set-line-block">
      <div class="set-line">
        <label class="set-label" for="setting-fileIcons-theme">
          {t("iconThemes.theme")}:
        </label>
        <select
          id="setting-fileIcons-theme"
          class="set-input set-select"
          value={value()}
          onChange={(e) => setDraft(props.spec.key, e.currentTarget.value)}
        >
          <option value="">{t("iconThemes.builtIn")}</option>
          <For each={iconThemes()}>
            {(th) => <option value={th.id}>{th.source ? `${th.label} (${th.source})` : th.label}</option>}
          </For>
          <Show when={!known()}>
            <option value={value()}>{t("iconThemes.notInstalled", { id: value() })}</option>
          </Show>
        </select>
      </div>
      <div class="set-desc">{t("iconThemes.themeDescription")}</div>
      <Show when={iconThemeError()}>{(e) => <div class="set-desc icon-theme-error">{e()}</div>}</Show>
      <Show when={iconThemeProblems().length}>
        <div class="set-desc icon-theme-error">{iconThemeProblems().join("; ")}</div>
      </Show>
    </div>
  );
}

const KINDS: IconAssociation["kind"][] = ["fileName", "extension", "pattern", "regex", "folderName"];

function IconPreview(props: { icon: string; kind: IconAssociation["kind"] }) {
  const url = () => {
    const a = activeIconTheme();
    return a ? a.urls.get(props.icon) : undefined;
  };
  return (
    <Show when={url()}>
      {(u) => <span class="file-icon themed-file-icon" style={iconStyle(u(), props.kind === "folderName" ? "folder" : "file", 14)} />}
    </Show>
  );
}

function AssociationsEditor(props: { spec: SettingSpec }) {
  const entries = () => (draftValue<SettingObject[]>(props.spec.key) ?? []) as unknown as IconAssociation[];
  const [sel, setSel] = createSignal<number | null>(null);
  const icons = () => Object.keys(activeIconTheme()?.definition.icons ?? {}).sort();
  const update = (next: IconAssociation[]) => setDraft(props.spec.key, next as unknown as SettingObject[]);
  const set = (i: number, field: keyof IconAssociation, value: string) =>
    update(entries().map((e, j) => (j === i ? { ...e, [field]: value } : e)));
  const add = () => {
    update([...entries(), { kind: "extension", value: "", icon: icons()[0] ?? "" }]);
    setSel(entries().length - 1);
  };
  const remove = () => {
    const i = sel();
    if (i === null) return;
    update(entries().filter((_, j) => j !== i));
    setSel(null);
  };
  return (
    <div class="set-list set-table-box icon-associations">
      <div class="set-list-toolbar">
        <button class="icon-btn" title={t("common.add")} disabled={!activeIconTheme()} onClick={add}>
          <Plus size={14} />
        </button>
        <button class="icon-btn" title={t("common.remove")} disabled={sel() === null} onClick={remove}>
          <Minus size={14} />
        </button>
      </div>
      <table class="set-table">
        <thead>
          <tr>
            <th>{t("iconThemes.kind")}</th>
            <th>{t("iconThemes.value")}</th>
            <th>{t("iconThemes.icon")}</th>
          </tr>
        </thead>
        <tbody>
          <Index each={entries()}>
            {(entry, i) => (
              <tr classList={{ selected: sel() === i }} onClick={() => setSel(i)}>
                <td>
                  <select class="set-input set-select" value={entry().kind} onChange={(e) => set(i, "kind", e.currentTarget.value)}>
                    <For each={KINDS}>{(k) => <option value={k}>{t(`iconThemes.kind.${k}`)}</option>}</For>
                  </select>
                </td>
                <td>
                  <input
                    class="set-input"
                    value={entry().value}
                    placeholder={entry().kind === "extension" ? "php" : entry().kind === "pattern" ? "*.test.ts" : ""}
                    onInput={(e) => set(i, "value", e.currentTarget.value)}
                  />
                </td>
                <td class="icon-association-icon">
                  <IconPreview icon={entry().icon} kind={entry().kind} />
                  <select class="set-input set-select" value={entry().icon} onChange={(e) => set(i, "icon", e.currentTarget.value)}>
                    <Show when={!icons().includes(entry().icon)}>
                      <option value={entry().icon}>{entry().icon}</option>
                    </Show>
                    <For each={icons()}>{(id) => <option value={id}>{id}</option>}</For>
                  </select>
                </td>
              </tr>
            )}
          </Index>
        </tbody>
      </table>
      <Show when={!entries().length}>
        <div class="set-list-empty">{activeIconTheme() ? t("common.nothingToShow") : t("iconThemes.chooseThemeFirst")}</div>
      </Show>
    </div>
  );
}

registerObjectSettingEditor(THEME_KEY, ThemePicker);
registerObjectSettingEditor(ASSOCIATIONS_KEY, AssociationsEditor);

registerSettingsPage({
  id: "appearance.fileIcons",
  title: t("settings.page.appearance.fileIcons"),
  group: "Appearance & Behavior",
  order: 22,
  keywords: ["icon theme", "file icons", "folder icons", "atom material", "monochrome", "accent", "tree arrows", "associations"],
  component: () => <SchemaPage pageId="appearance.fileIcons" />,
});
