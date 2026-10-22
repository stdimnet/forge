/**
 * The language checkboxes of "Show sticky lines for" and "Show breadcrumbs for": the setting
 * stores the languages that are switched off.
 */
import { For } from "solid-js";
import type { SettingSpec } from "@/ipc/settings";
import { draftValue, setDraft } from "@/features/settings/dialogState";
import { Hl } from "@/features/settings/dialogSearch";
import { structureLanguages } from "./model";

export function LanguageToggles(props: { spec: SettingSpec }) {
  const off = () => draftValue<string[]>(props.spec.key) ?? [];
  const toggle = (id: string, on: boolean) => {
    const rest = off().filter((l) => l !== id);
    setDraft(props.spec.key, on ? rest : [...rest, id]);
  };
  return (
    <div class="structure-languages">
      <For each={structureLanguages()}>
        {(lang) => (
          <label class="set-check">
            <input type="checkbox" checked={!off().includes(lang.id)} onChange={(e) => toggle(lang.id, e.currentTarget.checked)} />
            <span>
              <Hl text={lang.label} />
            </span>
          </label>
        )}
      </For>
    </div>
  );
}
