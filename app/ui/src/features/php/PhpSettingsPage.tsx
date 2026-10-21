/**
 * Settings > Languages & Frameworks > PHP. Edits go to the Settings dialog draft, so Apply,
 * Cancel and Reset work as on every other page; the backend restarts the language server
 * when the server choice, language level or include path changes.
 */
import { createResource, createSignal, For, Show } from "solid-js";
import { Download, Minus, Plus } from "lucide-solid";
import { IntelephenseLicence } from "@/features/langservers/Licence";
import { AUTO_INSTALL_KEY, LICENCE_ACCEPTED_KEY } from "@/features/langservers/autoInstall";
import { installIntelephense } from "@/features/langservers/managed";
import { draftValue, setDraft } from "@/features/settings/dialogState";
import { SettingRow } from "@/features/settings/SchemaPage";
import { findSpec } from "@/state/settings";
import { phpApi, type DetectedServers, type ServerChoice } from "./api";
import { LANGUAGE_LEVELS, PHP_SETTING_KEYS as KEYS } from "./settings";

/** phpstorm-stubs used for PHP's own symbols (Go to Declaration on `\Throwable`). */
const STUBS_PATH_KEY = "php.stubs.path";

const levelLabel = (v: string) =>
  ({ "7.4": "7.4 (arrow functions, typed properties)", "8.0": "8.0 (union types, attributes, match)", "8.1": "8.1 (enums, readonly properties)", "8.2": "8.2 (readonly classes, DNF types)", "8.3": "8.3 (typed class constants, #[\\Override])", "8.4": "8.4 (property hooks, asymmetric visibility)" } as Record<string, string>)[v] ?? v;

export function PhpSettingsPage() {
  const [newPath, setNewPath] = createSignal("");
  const [detected, { refetch }] = createResource(() => phpApi.detectServers().catch((): DetectedServers => ({})));
  const server = () => draftValue<string>(KEYS.server) as ServerChoice;
  const includePaths = () => draftValue<string[]>(KEYS.includePaths) ?? [];
  const servers: { id: ServerChoice; label: string; hint: () => string }[] = [
    {
      id: "auto",
      label: "Auto",
      hint: () => (detected()?.intelephense ? "Intelephense" : detected()?.phpactor ? "phpactor" : "Intelephense or phpactor, whichever is installed"),
    },
    { id: "intelephense", label: "Intelephense", hint: () => detected()?.intelephense ?? "not installed" },
    { id: "phpactor", label: "phpactor", hint: () => detected()?.phpactor ?? "not found on PATH" },
    { id: "none", label: "None (built-in index only)", hint: () => "" },
  ];
  const addPath = () => {
    const p = newPath().trim();
    if (p && !includePaths().includes(p)) setDraft(KEYS.includePaths, [...includePaths(), p]);
    setNewPath("");
  };
  return (
    <div class="php-settings">
      <section>
        <h3>Development environment</h3>
        <div class="php-field">
          <label for="php-level">PHP language level:</label>
          <select id="php-level" value={draftValue<string>(KEYS.languageLevel)} onChange={(e) => setDraft(KEYS.languageLevel, e.currentTarget.value)}>
            <For each={[...LANGUAGE_LEVELS]}>{(v) => <option value={v}>{levelLabel(v)}</option>}</For>
          </select>
        </div>
        <div class="php-field">
          <label for="php-cli">CLI Interpreter:</label>
          <input
            id="php-cli"
            type="text"
            placeholder="php"
            value={draftValue<string>(KEYS.cliInterpreter)}
            onChange={(e) => setDraft(KEYS.cliInterpreter, e.currentTarget.value.trim() || "php")}
          />
        </div>
      </section>
      <section>
        <h3>Language server</h3>
        <For each={servers}>
          {(s) => (
            <label class="php-radio">
              <input type="radio" name="php-server" checked={server() === s.id} onChange={() => setDraft(KEYS.server, s.id)} />
              <span>{s.label}</span>
              <Show when={s.hint()}>
                <span class="php-hint">{s.hint()}</span>
              </Show>
            </label>
          )}
        </For>
        <Show when={detected.state === "ready" && !detected()?.intelephense}>
          <div class="php-licence">
            <span class="php-hint">Intelephense is not installed. Forge can download it for you; nothing else is needed.</span>
            <button class="btn-secondary" onClick={() => void installIntelephense().then(() => refetch())}>
              <Download size={13} /> Install Intelephense
            </button>
          </div>
        </Show>
        <For each={[AUTO_INSTALL_KEY, LICENCE_ACCEPTED_KEY].map(findSpec).filter((s) => !!s)}>
          {(spec) => <SettingRow spec={spec!} />}
        </For>
        <div class="php-licence">
          <IntelephenseLicence />
        </div>
        <p class="php-hint">
          The server starts when a PHP project opens and restarts when these settings change. Premium features need your own licence from intelephense.com. The key is kept in the system keychain and passed
          to the server as licenceKey.
        </p>
      </section>
      <section>
        <h3>Include Path</h3>
        <div class="php-list">
          <For each={includePaths()} fallback={<div class="php-hint">Nothing to show</div>}>
            {(p) => (
              <div class="php-list-row">
                <span>{p}</span>
                <button
                  class="icon-btn"
                  title="Remove"
                  onClick={() => setDraft(KEYS.includePaths, includePaths().filter((x) => x !== p))}
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
      <section>
        <h3>PHP Runtime</h3>
        <Show when={findSpec(STUBS_PATH_KEY)}>{(spec) => <SettingRow spec={spec()} />}</Show>
      </section>
    </div>
  );
}
