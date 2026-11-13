/**
 * The scope chooser of Find in Files, Find Usages and Structural Search: PHPStorm's
 * predefined scopes, then the named scopes of the Scopes page, and "…" to edit them.
 */
import { For } from "solid-js";
import { Ellipsis } from "lucide-solid";
import { openSettings } from "@/features/settings/dialogState";
import { scopeEntries } from "./model";
import { t } from "@/i18n";

export const SCOPES_PAGE = "appearance.scopes";

export function ScopeSelect(props: { value: string; onChange: (name: string) => void; class?: string; ariaLabel?: string }) {
  const entries = () => scopeEntries().entries;
  const predefined = () => entries().filter((e) => e.predefined);
  const named = () => entries().filter((e) => !e.predefined);
  return (
    <span class={`scope-select ${props.class ?? ""}`}>
      <select
        value={props.value}
        aria-label={props.ariaLabel ?? t("common.scope")}
        onMouseDown={(e) => e.stopPropagation()}
        onChange={(e) => props.onChange(e.currentTarget.value)}
      >
        <For each={predefined()}>{(e) => <option value={e.name}>{e.name}</option>}</For>
        <optgroup label={t("tools.scopes.scopes")}>
          <For each={named()}>{(e) => <option value={e.name}>{e.name}</option>}</For>
        </optgroup>
        {!entries().some((e) => e.name === props.value) && <option value={props.value}>{props.value}</option>}
      </select>
      <button
        class="icon-btn"
        title={t("tools.scopes.editScopesEllipsis")}
        aria-label={t("tools.scopes.editScopes")}
        onMouseDown={(e) => e.preventDefault()}
        onClick={() => openSettings(SCOPES_PAGE)}
      >
        <Ellipsis size={14} />
      </button>
    </span>
  );
}
