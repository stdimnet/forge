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
import { decodeInterpreters, encodeInterpreters, findInterpreter, GLOBAL_KEY, interpreterDetail, levelForVersion, PROJECT_KEY, type PhpInterpreter } from "./interpreters";
import { InterpretersDialog } from "./InterpretersDialog";
import { LANGUAGE_LEVELS, LEVEL_FROM_INTERPRETER, PHP_SETTING_KEYS as KEYS } from "./settings";

/** phpstorm-stubs used for PHP's own symbols (Go to Declaration on `\Throwable`). */
const STUBS_PATH_KEY = "php.stubs.path";

const levelLabel = (v: string) =>
  ({ "7.4": "7.4 (arrow functions, typed properties)", "8.0": "8.0 (union types, attributes, match)", "8.1": "8.1 (enums, readonly properties)", "8.2": "8.2 (readonly classes, DNF types)", "8.3": "8.3 (typed class constants, #[\\Override])", "8.4": "8.4 (property hooks, asymmetric visibility)" } as Record<string, string>)[v] ?? v;

/** "CLI Interpreter": the dropdown of configured interpreters and the "…" button. */
function CliInterpreterRow() {
  const [dialog, setDialog] = createSignal(false);
  const list = () => decodeInterpreters(draftValue(GLOBAL_KEY), draftValue(PROJECT_KEY));
  const selected = () => draftValue<string>(KEYS.cliInterpreter) || "php";
  const current = () => findInterpreter(list(), selected());
  const apply = (next: PhpInterpreter[], chosen: string | null) => {
    const { global, project } = encodeInterpreters(next);
    setDraft(GLOBAL_KEY, global);
    setDraft(PROJECT_KEY, project);
    // The interpreter selected in the dialog becomes the project's (like PHPStorm).
    if (chosen) setDraft(KEYS.cliInterpreter, chosen);
    else if (current() && !next.some((i) => i.name === selected())) setDraft(KEYS.cliInterpreter, "php");
    setDialog(false);
  };
  return (
    <>
      <div class="php-field">
        <label for="php-cli">CLI Interpreter:</label>
        <div class="php-interp-picker">
          <select id="php-cli" value={selected()} onChange={(e) => setDraft(KEYS.cliInterpreter, e.currentTarget.value)}>
            <Show when={!current()}>
              <option value={selected()}>{selected() === "php" ? "php (from PATH)" : selected()}</option>
            </Show>
            <For each={list()}>
              {(i) => (
                <option value={i.name}>
                  {i.name} ({i.version ? `${i.version}, ` : ""}
                  {interpreterDetail(i)})
                </option>
              )}
            </For>
          </select>
          <button class="btn-secondary php-interp-more" title="CLI Interpreters" data-testid="php-interpreters-open" onClick={() => setDialog(true)}>
            …
          </button>
        </div>
      </div>
      <Show when={current()}>
        {(i) => (
          <div class="php-field">
            <span />
            <span class="php-hint" data-testid="php-interp-summary">
              {i().version ? `PHP ${i().version}` : "Version not detected"}
              {i().xdebug ? ` · Xdebug ${i().xdebug === "unknown" ? "" : i().xdebug}` : ""}
              {i().configFile ? ` · ${i().configFile}` : ""}
              {i().project ? " · visible only for this project" : ""}
            </span>
          </div>
        )}
      </Show>
      <Show when={dialog()}>
        <InterpretersDialog interpreters={list()} selected={selected()} onOk={apply} onClose={() => setDialog(false)} />
      </Show>
    </>
  );
}

/** "From the CLI interpreter (8.2)" when the interpreter's version is known. */
function interpreterLevelLabel(): string {
  const list = decodeInterpreters(draftValue(GLOBAL_KEY), draftValue(PROJECT_KEY));
  const i = findInterpreter(list, draftValue<string>(KEYS.cliInterpreter) || "php");
  const level = i?.version ? levelForVersion(i.version, LANGUAGE_LEVELS) : null;
  return level ? `From the CLI interpreter (${level})` : "From the CLI interpreter";
}

export function PhpSettingsPage() {
  const [newPath, setNewPath] = createSignal("");
  const [detected, { refetch }] = createResource(() => phpApi.detectServers().catch((): DetectedServers => ({})));
  const server = () => draftValue<string>(KEYS.server) as ServerChoice;
  const includePaths = () => draftValue<string[]>(KEYS.includePaths) ?? [];
  const builtin = () => detected()?.phpLsp;
  const servers: { id: ServerChoice; label: string; hint: () => string }[] = [
    {
      id: "php-lsp",
      label: "php-lsp (built-in)",
      hint: () => (builtin() ? `version ${builtin()!.version}, nothing to install` : detected.state === "ready" ? "not in this build" : ""),
    },
    {
      id: "auto",
      label: "Auto",
      hint: () => (builtin() ? "php-lsp" : detected()?.intelephense ? "Intelephense" : detected()?.phpactor ? "phpactor" : "php-lsp, Intelephense or phpactor"),
    },
    { id: "intelephense", label: "Intelephense", hint: () => detected()?.intelephense ?? "not installed" },
    { id: "phpactor", label: "phpactor", hint: () => detected()?.phpactor ?? "not found on PATH" },
    { id: "none", label: "None (Forge's index only)", hint: () => "" },
  ];
  /** Intelephense install and licence options matter only when it is chosen. */
  const intelephenseChosen = () => server() === "intelephense";
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
            <option value={LEVEL_FROM_INTERPRETER}>{interpreterLevelLabel()}</option>
          </select>
        </div>
        <CliInterpreterRow />
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
        <Show when={intelephenseChosen() && detected.state === "ready" && !detected()?.intelephense}>
          <div class="php-licence">
            <span class="php-hint">Intelephense is not installed. Forge can download it for you; nothing else is needed.</span>
            <button class="btn-secondary" onClick={() => void installIntelephense().then(() => refetch())}>
              <Download size={13} /> Install Intelephense
            </button>
          </div>
        </Show>
        <Show when={intelephenseChosen()}>
          <For each={[AUTO_INSTALL_KEY, LICENCE_ACCEPTED_KEY].map(findSpec).filter((s) => !!s)}>
            {(spec) => <SettingRow spec={spec!} />}
          </For>
          <div class="php-licence">
            <IntelephenseLicence />
          </div>
        </Show>
        <p class="php-hint">
          <Show
            when={intelephenseChosen()}
            fallback={
              <>
                The server starts when a PHP project opens and restarts when these settings change. php-lsp is built into Forge and free (MIT OR Apache-2.0): rename, code actions, type hierarchy and the
                other premium features of Intelephense need no licence.
              </>
            }
          >
            The server starts when a PHP project opens and restarts when these settings change. Premium features need your own licence from intelephense.com. The key is kept in the system keychain and
            passed to the server as licenceKey.
          </Show>
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
