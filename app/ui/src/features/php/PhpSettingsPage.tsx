/** Settings > Languages & Frameworks > PHP */
import { createResource, createSignal, For, Show } from "solid-js";
import { Minus, Plus } from "lucide-solid";
import { IntelephenseLicence } from "@/features/langservers/Licence";
import { phpApi, type DetectedServers, type ServerChoice } from "./api";
import { LANGUAGE_LEVELS, phpSettings, toLspSettings, updatePhpSettings } from "./settings";

const levelLabel = (v: string) =>
  ({ "7.4": "7.4 (arrow functions, typed properties)", "8.0": "8.0 (union types, attributes, match)", "8.1": "8.1 (enums, readonly properties)", "8.2": "8.2 (readonly classes, DNF types)", "8.3": "8.3 (typed class constants, #[\\Override])", "8.4": "8.4 (property hooks, asymmetric visibility)" } as Record<string, string>)[v] ?? v;

export function PhpSettingsPage() {
  const [newPath, setNewPath] = createSignal("");
  const [detected] = createResource(() => phpApi.detectServers(toLspSettings()).catch((): DetectedServers => ({})));
  const servers: { id: ServerChoice; label: string; hint: () => string }[] = [
    { id: "intelephense", label: "Intelephense", hint: () => detected()?.intelephense ?? "not found on PATH (npm i -g intelephense)" },
    { id: "phpactor", label: "phpactor", hint: () => detected()?.phpactor ?? "not found on PATH" },
    { id: "none", label: "None (built-in index only)", hint: () => "" },
  ];
  const addPath = () => {
    const p = newPath().trim();
    if (p && !phpSettings.includePaths.includes(p)) updatePhpSettings({ includePaths: [...phpSettings.includePaths, p] });
    setNewPath("");
  };
  return (
    <div class="php-settings">
      <section>
        <h3>Development environment</h3>
        <div class="php-field">
          <label for="php-level">PHP language level:</label>
          <select id="php-level" value={phpSettings.languageLevel} onChange={(e) => updatePhpSettings({ languageLevel: e.currentTarget.value })}>
            <For each={[...LANGUAGE_LEVELS]}>{(v) => <option value={v}>{levelLabel(v)}</option>}</For>
          </select>
        </div>
        <div class="php-field">
          <label for="php-cli">CLI Interpreter:</label>
          <input
            id="php-cli"
            type="text"
            placeholder="php"
            value={phpSettings.cliInterpreter}
            onChange={(e) => updatePhpSettings({ cliInterpreter: e.currentTarget.value.trim() || "php" })}
          />
        </div>
      </section>
      <section>
        <h3>Language server</h3>
        <For each={servers}>
          {(s) => (
            <label class="php-radio">
              <input type="radio" name="php-server" checked={phpSettings.server === s.id} onChange={() => updatePhpSettings({ server: s.id })} />
              <span>{s.label}</span>
              <Show when={s.hint()}>
                <span class="php-hint">{s.hint()}</span>
              </Show>
            </label>
          )}
        </For>
        <div class="php-licence">
          <IntelephenseLicence />
        </div>
        <p class="php-hint">
          Premium features need your own licence from intelephense.com. The key is kept in the system keychain and passed
          to the server as licenceKey.
        </p>
      </section>
      <section>
        <h3>Include Path</h3>
        <div class="php-list">
          <For each={phpSettings.includePaths} fallback={<div class="php-hint">Nothing to show</div>}>
            {(p) => (
              <div class="php-list-row">
                <span>{p}</span>
                <button
                  class="icon-btn"
                  title="Remove"
                  onClick={() => updatePhpSettings({ includePaths: phpSettings.includePaths.filter((x) => x !== p) })}
                >
                  <Minus size={14} />
                </button>
              </div>
            )}
          </For>
        </div>
        <div class="php-secret">
          <input
            type="text"
            placeholder="/usr/share/php"
            value={newPath()}
            onInput={(e) => setNewPath(e.currentTarget.value)}
            onKeyDown={(e) => e.key === "Enter" && addPath()}
          />
          <button class="icon-btn" title="Add" onClick={addPath}>
            <Plus size={14} />
          </button>
        </div>
      </section>
    </div>
  );
}
