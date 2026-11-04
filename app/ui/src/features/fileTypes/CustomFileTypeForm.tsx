/**
 * PHPStorm's "New File Type" dialog: name, description and the syntax highlighting of a
 * custom file type (comments, number formats, options and keyword lists 1–4). Shown over the
 * File Types page; OK stores the definition in the page's draft (`fileTypes.custom`).
 */
import { createSignal, For, onCleanup, Show } from "solid-js";
import type { SettingObject } from "@/ipc/settings";
import { suspendKeymap } from "@/keymap";
import { newCustomId } from "./model";
import { t as tr } from "@/i18n";

const TEXT_FIELDS: [key: string, label: string][] = [
  ["blockCommentStart", "Block comment start:"],
  ["blockCommentEnd", "Block comment end:"],
  ["hexPrefix", "Hex prefix:"],
  ["numberPostfixes", "Number postfixes:"],
];

const FLAGS: [key: string, label: string, def: boolean][] = [
  ["pairedBraces", "Support paired braces", true],
  ["pairedBrackets", "Support paired brackets", true],
  ["pairedParens", "Support paired parens", true],
  ["stringEscapes", "Support string escapes", true],
  ["ignoreCase", "Ignore case", false],
];

export function CustomFileTypeForm(props: {
  entry: SettingObject | null;
  existingIds: string[];
  /** Names of the other file types (a name must be unique). */
  names: string[];
  onSave: (entry: SettingObject) => void;
  onCancel: () => void;
}) {
  onCleanup(suspendKeymap());
  const [entry, setEntry] = createSignal<SettingObject>(
    props.entry ? { ...props.entry } : { id: newCustomId(props.existingIds), name: "", lineComment: "#", hexPrefix: "0x" },
  );
  const [tab, setTab] = createSignal(1);
  const set = (key: string, value: string) => setEntry((e) => ({ ...e, [key]: value }));
  const flag = (key: string, def: boolean) => (entry()[key] === undefined ? def : entry()[key] === "true");
  const problem = () => {
    const name = entry().name?.trim() ?? "";
    if (!name) return "Enter a name";
    if (props.names.some((n) => n.toLowerCase() === name.toLowerCase())) return `A file type named "${name}" already exists`;
    return null;
  };
  const save = () => {
    if (problem()) return;
    // Keep only the fields that say something.
    const out: SettingObject = {};
    for (const [k, v] of Object.entries(entry())) if (v.trim() !== "") out[k] = k.startsWith("keywords") ? v : v.trim();
    props.onSave(out);
  };
  const onKeyDown = (e: KeyboardEvent) => {
    const t = e.target as HTMLElement;
    if (e.key === "Escape") props.onCancel();
    else if (e.key === "Enter" && t.tagName !== "TEXTAREA" && t.tagName !== "BUTTON") save();
    else return;
    e.preventDefault();
    e.stopPropagation();
  };
  return (
    <div class="popup-backdrop modal ft-backdrop" onMouseDown={(e) => e.target === e.currentTarget && props.onCancel()}>
      <div class="dialog ft-dialog" role="dialog" aria-label={tr("tools.fileTypes.newFileType")} onKeyDown={onKeyDown}>
        <div class="dialog-title">{props.entry ? tr("tools.fileTypes.editFileType") : tr("tools.fileTypes.newFileType")}</div>
        <div class="ft-form">
          <label class="set-label">{tr("common.nameLabel")}</label>
          <input class="set-input" value={entry().name ?? ""} ref={(el) => queueMicrotask(() => el.focus())} onInput={(e) => set("name", e.currentTarget.value)} data-testid="ft-name" />
          <label class="set-label">{tr("common.descriptionLabel")}</label>
          <input class="set-input" value={entry().description ?? ""} onInput={(e) => set("description", e.currentTarget.value)} />
        </div>
        <div class="ft-group-title">{tr("tools.fileTypes.syntaxHighlighting")}</div>
        <div class="ft-form">
          <label class="set-label">{tr("tools.fileTypes.lineCommentLabel")}</label>
          <div class="ft-inline">
            <input class="set-input ft-short" value={entry().lineComment ?? ""} spellcheck={false} onInput={(e) => set("lineComment", e.currentTarget.value)} />
            <label class="set-check">
              <input type="checkbox" checked={flag("lineCommentAtStart", false)} onChange={(e) => set("lineCommentAtStart", String(e.currentTarget.checked))} />
              <span>{tr("tools.fileTypes.onlyAtLineStart")}</span>
            </label>
          </div>
          <For each={TEXT_FIELDS}>
            {([key, label]) => (
              <>
                <label class="set-label">{label}</label>
                <input class="set-input ft-short" value={entry()[key] ?? ""} spellcheck={false} onInput={(e) => set(key, e.currentTarget.value)} />
              </>
            )}
          </For>
        </div>
        <div class="ft-flags">
          <For each={FLAGS}>
            {([key, label, def]) => (
              <label class="set-check">
                <input type="checkbox" checked={flag(key, def)} onChange={(e) => set(key, String(e.currentTarget.checked))} />
                <span>{label}</span>
              </label>
            )}
          </For>
        </div>
        <div class="ft-tabs" role="tablist">
          <For each={[1, 2, 3, 4]}>
            {(n) => (
              <button class="ft-tab" classList={{ active: tab() === n }} role="tab" onClick={() => setTab(n)}>
                {n}
              </button>
            )}
          </For>
          <span class="ft-muted">{tr("tools.fileTypes.keywordsOnePerLine")}</span>
        </div>
        <textarea
          class="set-input ft-keywords"
          spellcheck={false}
          value={entry()[`keywords${tab()}`] ?? ""}
          onInput={(e) => set(`keywords${tab()}`, e.currentTarget.value)}
          data-testid="ft-keywords"
        />
        <div class="dialog-buttons">
          <Show when={problem() && entry().name}>
            <span class="set-error">{problem()}</span>
          </Show>
          <button class="btn-secondary" onClick={() => props.onCancel()}>
            {tr("common.cancel")}
          </button>
          <button class="btn-primary" disabled={!!problem()} onClick={save}>
            OK
          </button>
        </div>
      </div>
    </div>
  );
}
