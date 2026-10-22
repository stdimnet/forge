/**
 * PHPStorm's Find Usages dialog (Ctrl+Shift+Alt+F7, the "Settings" buttons of the Find
 * tool window and the Show Usages popup): what to find for the element (from Rust), the
 * options, the scope and "Open in new tab". "Find" saves the choices as settings, so later
 * Find Usages runs use them too, and searches.
 */
import { createSignal, For, Show } from "solid-js";
import { getSetting, saveSettings } from "@/state/settings";
import type { SettingValue } from "@/ipc/settings";
import type { OptionField, SearchScope } from "@/ipc/codeintel";
import { closeFindUsagesDialog, SCOPES, type FindUsagesDialogRequest } from "./findSettings";

const capitalized = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);

export function FindUsagesDialog(props: { req: FindUsagesDialogRequest }) {
  const target = props.req.target;
  const [values, setValues] = createSignal<Record<string, SettingValue>>({
    ...Object.fromEntries(target.fields.map((f) => [f.key, getSetting<boolean>(f.key, false)])),
    "findUsages.scope.kind": getSetting<string>("findUsages.scope.kind", "projectFiles"),
    "findUsages.scope.directory": getSetting<string>("findUsages.scope.directory", "") || props.req.directory,
    "findUsages.scope.includeSubdirectories": getSetting<boolean>("findUsages.scope.includeSubdirectories", true),
    "findUsages.scope.pattern": getSetting<string>("findUsages.scope.pattern", ""),
    "findUsages.openInNewTab": getSetting<boolean>("findUsages.openInNewTab", true),
  });
  const value = <T extends SettingValue>(key: string) => values()[key] as T;
  const set = (key: string, v: SettingValue) => setValues((all) => ({ ...all, [key]: v }));
  const scope = () => value<SearchScope>("findUsages.scope.kind");
  const find = target.fields.filter((f) => f.section === "find");
  const options = target.fields.filter((f) => f.section === "options");
  // Nothing to look for: "Find" is disabled, like PHPStorm.
  const nothing = () => find.every((f) => !value<boolean>(f.key)) && options.every((f) => !value<boolean>(f.key));

  const close = () => closeFindUsagesDialog();
  const run = async () => {
    if (nothing()) return;
    // Only what changed, so untouched options keep following their defaults.
    const changed = Object.fromEntries(Object.entries(values()).filter(([k, v]) => getSetting(k) !== v));
    if (Object.keys(changed).length) await saveSettings(changed);
    close();
    props.req.find();
  };

  const checkbox = (f: OptionField) => (
    <label class="form-label check">
      <input type="checkbox" checked={value<boolean>(f.key)} onChange={(e) => set(f.key, e.currentTarget.checked)} />
      {f.label}
    </label>
  );

  return (
    <div class="popup-backdrop modal refactoring-backdrop" onMouseDown={(e) => e.target === e.currentTarget && close()}>
      <div
        class="dialog find-usages-dialog"
        role="dialog"
        aria-label="Find Usages"
        onKeyDown={(e) => {
          if (e.key === "Escape") close();
          else if (e.key === "Enter" && (e.target as HTMLElement).tagName !== "SELECT") void run();
          else return;
          e.preventDefault();
          e.stopPropagation();
        }}
      >
        <div class="dialog-title">Find Usages</div>
        <div class="dialog-body">
          <div class="find-usages-element">
            {capitalized(target.label.slice(0, target.label.length - target.title.length).trim())} <strong>{target.title}</strong>
          </div>
          <div class="find-usages-groups">
            <fieldset class="find-usages-group">
              <legend>Find</legend>
              <For each={find}>{checkbox}</For>
            </fieldset>
            <Show when={options.length}>
              <fieldset class="find-usages-group">
                <legend>Options</legend>
                <For each={options}>{checkbox}</For>
              </fieldset>
            </Show>
          </div>
          <Show when={target.scoped}>
            <div class="form-row">
              <label class="form-label" for="find-usages-scope">
                Scope:
              </label>
              <select id="find-usages-scope" value={scope()} onChange={(e) => set("findUsages.scope.kind", e.currentTarget.value)}>
                <For each={SCOPES}>{(s) => <option value={s.value}>{s.label}</option>}</For>
              </select>
            </div>
            <Show when={scope() === "directory"}>
              <div class="form-row">
                <label class="form-label">Directory:</label>
                <input
                  value={value<string>("findUsages.scope.directory")}
                  placeholder="app/Models"
                  spellcheck={false}
                  onInput={(e) => set("findUsages.scope.directory", e.currentTarget.value)}
                />
              </div>
              <div class="form-row">
                <span />
                <label class="form-label check">
                  <input
                    type="checkbox"
                    checked={value<boolean>("findUsages.scope.includeSubdirectories")}
                    onChange={(e) => set("findUsages.scope.includeSubdirectories", e.currentTarget.checked)}
                  />
                  Include subdirectories
                </label>
              </div>
            </Show>
            <Show when={scope() === "custom"}>
              <div class="form-row">
                <label class="form-label">Pattern:</label>
                <input
                  value={value<string>("findUsages.scope.pattern")}
                  placeholder="app/**/*.php; !app/Legacy/**"
                  spellcheck={false}
                  onInput={(e) => set("findUsages.scope.pattern", e.currentTarget.value)}
                />
              </div>
            </Show>
          </Show>
        </div>
        <div class="dialog-buttons">
          <label class="dialog-checkbox">
            <input
              type="checkbox"
              checked={value<boolean>("findUsages.openInNewTab")}
              onChange={(e) => set("findUsages.openInNewTab", e.currentTarget.checked)}
            />
            Open in new tab
          </label>
          <button class="btn-secondary" onClick={close}>
            Cancel
          </button>
          <button class="btn-primary" disabled={nothing()} onClick={() => void run()} ref={(el) => queueMicrotask(() => el.focus())}>
            Find
          </button>
        </div>
      </div>
    </div>
  );
}
