/**
 * Settings | Appearance & Behavior | File Icons: the icon theme picker, the look hooks
 * (tree arrows, monochrome, accent color, size, light / dark variant, tint mode, saturation,
 * opacity, hollow folders; plain schema settings), the folder colors table and the custom
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
  FOLDER_COLORS_KEY,
  iconStyle,
  iconThemeError,
  iconThemeProblems,
  iconThemes,
  THEME_KEY,
  type FolderColorRow,
  type IconAssociation,
} from "./state";
import "./iconThemes.css";

export { registerIconTheme, themedIcon, iconLook } from "./state";
export { UiIcon } from "./UiIcon";

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
      <span class="file-icon themed-file-icon" style={iconStyle(props.icon, props.kind === "folderName" ? "folder" : "file", 14)} />
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

const HEX = /^#(?:[0-9a-f]{3}|[0-9a-f]{6})$/i;
/** `#rgb` → `#rrggbb` for `<input type="color">`, which takes only the long form. */
const longHex = (c: string) => (/^#[0-9a-f]{3}$/i.test(c) ? `#${[...c.slice(1)].map((x) => x + x).join("")}` : c);

/** The folder colors table: folder name or glob → color. Rust matches them (`FolderColors`). */
function FolderColorsEditor(props: { spec: SettingSpec }) {
  const entries = () => (draftValue<SettingObject[]>(props.spec.key) ?? []) as unknown as FolderColorRow[];
  const [sel, setSel] = createSignal<number | null>(null);
  const update = (next: FolderColorRow[]) => setDraft(props.spec.key, next as unknown as SettingObject[]);
  const set = (i: number, field: keyof FolderColorRow, value: string) =>
    update(entries().map((e, j) => (j === i ? { ...e, [field]: value } : e)));
  const add = () => {
    update([...entries(), { pattern: "", color: "#e0a040" }]);
    setSel(entries().length - 1);
  };
  const remove = () => {
    const i = sel();
    if (i === null) return;
    update(entries().filter((_, j) => j !== i));
    setSel(null);
  };
  return (
    <div class="set-list set-table-box icon-folder-colors">
      <div class="set-list-toolbar">
        <button class="icon-btn" title={t("common.add")} aria-label={t("common.add")} onClick={add}>
          <Plus size={14} />
        </button>
        <button class="icon-btn" title={t("common.remove")} aria-label={t("common.remove")} disabled={sel() === null} onClick={remove}>
          <Minus size={14} />
        </button>
      </div>
      <table class="set-table">
        <thead>
          <tr>
            <th>{t("iconThemes.folder")}</th>
            <th>{t("iconThemes.color")}</th>
          </tr>
        </thead>
        <tbody>
          <Index each={entries()}>
            {(entry, i) => (
              <tr classList={{ selected: sel() === i }} onClick={() => setSel(i)}>
                <td>
                  <input
                    class="set-input"
                    value={entry().pattern}
                    placeholder="vendor"
                    aria-label={t("iconThemes.folder")}
                    onInput={(e) => set(i, "pattern", e.currentTarget.value)}
                  />
                </td>
                <td class="icon-folder-color">
                  <input
                    type="color"
                    class="icon-color-swatch"
                    value={HEX.test(entry().color) ? longHex(entry().color) : "#000000"}
                    aria-label={t("iconThemes.colorPicker", { folder: entry().pattern || t("iconThemes.folder") })}
                    onInput={(e) => set(i, "color", e.currentTarget.value)}
                  />
                  <input
                    class="set-input"
                    value={entry().color}
                    aria-label={t("iconThemes.color")}
                    onInput={(e) => set(i, "color", e.currentTarget.value)}
                  />
                </td>
              </tr>
            )}
          </Index>
        </tbody>
      </table>
      <Show when={!entries().length}>
        <div class="set-list-empty">{t("iconThemes.folderColorsEmpty")}</div>
      </Show>
    </div>
  );
}

registerObjectSettingEditor(THEME_KEY, ThemePicker);
registerObjectSettingEditor(FOLDER_COLORS_KEY, FolderColorsEditor);
registerObjectSettingEditor(ASSOCIATIONS_KEY, AssociationsEditor);

registerSettingsPage({
  id: "appearance.fileIcons",
  title: t("settings.page.appearance.fileIcons"),
  group: "Appearance & Behavior",
  order: 22,
  keywords: [
    "icon theme",
    "file icons",
    "folder icons",
    "atom material",
    "monochrome",
    "accent",
    "tree arrows",
    "associations",
    "hollow folders",
    "folder colors",
    "saturation",
    "opacity",
    "light icons",
  ],
  component: () => <SchemaPage pageId="appearance.fileIcons" />,
});
