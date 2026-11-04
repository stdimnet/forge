/**
 * Settings | PHP | Test Frameworks, like PHPStorm's: the list of framework configurations
 * (PHPUnit, Pest, Codeception, Behat, PHPSpec, each Local or by a CLI interpreter) with Add
 * and Remove, and the form of the selected one: "Use Composer autoloader" with the path to
 * `vendor/autoload.php` or the path to the executable / phar, and the default configuration
 * and bootstrap files. Frameworks the project uses but that are not configured are listed as
 * detected; changing one adds it to the list. Detection runs in Rust (`test_frameworks_list`).
 */
import { createMemo, createResource, createSignal, For, Show } from "solid-js";
import { Minus, Plus } from "lucide-solid";
import { FRAMEWORKS, frameworkLabel, testFrameworksApi, type FrameworkConfig, type FrameworkId } from "@/ipc/testFrameworks";
import type { SettingObject, SettingSpec } from "@/ipc/settings";
import { decodeInterpreters, GLOBAL_KEY, PROJECT_KEY } from "@/features/php/interpreters";
import { draftValue, setDraft } from "@/features/settings/dialogState";
import { t } from "@/i18n";

/** A row: a configured entry (by index) or a detected framework. */
interface Row {
  entry: SettingObject;
  index: number | null;
}

const toEntry = (c: FrameworkConfig): SettingObject => ({
  framework: c.framework,
  mode: c.mode,
  path: c.path,
  ...(c.interpreter ? { interpreter: c.interpreter } : {}),
  ...(c.configFile ? { configFile: c.configFile } : {}),
  ...(c.bootstrapFile ? { bootstrapFile: c.bootstrapFile } : {}),
});

const binOf = (id: string) => FRAMEWORKS.find((f) => f.id === id)?.bin ?? id;

/** "PHPUnit" / "Pest by Remote Interpreter: Docker PHP 8.3". */
export const rowTitle = (e: SettingObject) =>
  e.interpreter ? `${frameworkLabel(e.framework)} by Remote Interpreter: ${e.interpreter}` : `${frameworkLabel(e.framework)} (Local)`;

export function TestFrameworksEditor(props: { spec: SettingSpec }) {
  const [list] = createResource(() => testFrameworksApi.list().catch(() => [] as FrameworkConfig[]), { initialValue: [] });
  const entries = () => draftValue<SettingObject[]>(props.spec.key) ?? [];
  const detected = createMemo(() =>
    list()
      .filter((c) => c.detected)
      .filter((c) => !entries().some((e) => e.framework === c.framework))
      .map(toEntry),
  );
  const rows = createMemo<Row[]>(() => [
    ...entries().map((entry, index) => ({ entry, index })),
    ...detected().map((entry) => ({ entry, index: null })),
  ]);
  const interpreters = createMemo(() => decodeInterpreters(draftValue(GLOBAL_KEY), draftValue(PROJECT_KEY)).map((i) => i.name));
  const [sel, setSel] = createSignal(0);
  const [adding, setAdding] = createSignal(false);
  const selected = () => rows()[sel()];

  const update = (next: SettingObject[]) => setDraft(props.spec.key, next);
  const add = (framework: FrameworkId, interpreter: string) => {
    setAdding(false);
    const next = [...entries(), { framework, mode: "composer", path: "vendor/autoload.php", ...(interpreter ? { interpreter } : {}) }];
    update(next);
    setSel(next.length - 1);
  };
  const remove = () => {
    const r = selected();
    if (!r || r.index === null) return;
    update(entries().filter((_, j) => j !== r.index));
    setSel(Math.max(0, sel() - 1));
  };
  /** Change a field; a detected framework becomes a configured one. */
  const set = (key: string, value: string) => {
    const r = selected();
    if (!r) return;
    const entry: SettingObject = { ...r.entry, [key]: value };
    if (!value && key !== "path") delete entry[key];
    if (r.index === null) {
      update([...entries(), entry]);
      setSel(entries().length - 1);
    } else update(entries().map((e, j) => (j === r.index ? entry : e)));
  };

  const current = () => selected()?.entry;
  const fw = () => current()?.framework ?? "phpunit";
  const composer = () => (current()?.mode ?? "composer") !== "path";

  return (
    <div class="tf-editor">
      <div class="set-list tf-list">
        <div class="set-list-toolbar">
          <button class="icon-btn" title={t("common.add")} data-testid="tf-add" onClick={() => setAdding((v) => !v)}>
            <Plus size={14} />
          </button>
          <button class="icon-btn" title={t("common.remove")} disabled={!selected() || selected()!.index === null} onClick={remove}>
            <Minus size={14} />
          </button>
        </div>
        <Show when={adding()}>
          <div class="tf-add-menu" role="menu">
            <For each={FRAMEWORKS}>
              {(f) => (
                <>
                  <div class="tf-add-item" role="menuitem" data-framework={f.id} onClick={() => add(f.id, "")}>
                    {t("langs.testFrameworks.local", { label: f.label })}
                  </div>
                  <For each={interpreters()}>
                    {(name) => (
                      <div class="tf-add-item tf-add-remote" role="menuitem" onClick={() => add(f.id, name)}>
                        {t("langs.testFrameworks.byRemoteInterpreter", { label: f.label, name })}
                      </div>
                    )}
                  </For>
                </>
              )}
            </For>
          </div>
        </Show>
        <div class="tf-items">
          <For each={rows()}>
            {(row, i) => (
              <div class="tf-item" classList={{ selected: sel() === i() }} data-framework={row.entry.framework} onClick={() => setSel(i())}>
                <span>{rowTitle(row.entry)}</span>
                <Show when={row.index === null}>
                  <span class="muted"> detected</span>
                </Show>
              </div>
            )}
          </For>
          <Show when={!rows().length}>
            <div class="set-list-empty">{t("langs.testFrameworks.noTestFrameworksClickTo")}</div>
          </Show>
        </div>
      </div>
      <Show when={current()}>
        <div class="tf-form" data-testid="tf-form">
          <div class="tf-section">{frameworkLabel(fw())} library</div>
          <label class="tf-radio">
            <input type="radio" name="tf-mode" checked={composer()} onChange={() => set("mode", "composer")} /> {t("langs.testFrameworks.useComposerAutoloader")}
          </label>
          <label class="tf-radio">
            <input type="radio" name="tf-mode" checked={!composer()} onChange={() => set("mode", "path")} /> {t("langs.testFrameworks.pathTo", { value: binOf(fw()), value2: fw() === "phpunit" ? ".phar" : " executable" })}
          </label>
          <div class="set-delims-grid tf-grid">
            <label class="set-label">{composer() ? t("langs.testFrameworks.pathToScriptLabel") : t("langs.testFrameworks.pathToLabel", { value: binOf(fw()) })}</label>
            <input
              class="set-input set-table-mono"
              spellcheck={false}
              placeholder={composer() ? "vendor/autoload.php" : `vendor/bin/${binOf(fw())}`}
              value={current()!.path ?? ""}
              onChange={(e) => set("path", e.currentTarget.value.trim())}
            />
          </div>
          <div class="tf-section">{t("langs.testFrameworks.testRunner")}</div>
          <div class="set-delims-grid tf-grid">
            <label class="set-label">{t("langs.testFrameworks.defaultConfigurationFileLabel")}</label>
            <input
              class="set-input set-table-mono"
              spellcheck={false}
              placeholder={{ phpunit: "phpunit.xml", pest: "phpunit.xml", codeception: "codeception.yml", behat: "behat.yml", phpspec: "phpspec.yml" }[fw()]}
              value={current()!.configFile ?? ""}
              onChange={(e) => set("configFile", e.currentTarget.value.trim())}
            />
            <Show when={fw() === "phpunit" || fw() === "phpspec"}>
              <label class="set-label">{t("langs.testFrameworks.defaultBootstrapFileLabel")}</label>
              <input
                class="set-input set-table-mono"
                spellcheck={false}
                placeholder={fw() === "phpunit" ? "tests/bootstrap.php" : "spec/bootstrap.php"}
                value={current()!.bootstrapFile ?? ""}
                onChange={(e) => set("bootstrapFile", e.currentTarget.value.trim())}
              />
            </Show>
          </div>
        </div>
      </Show>
    </div>
  );
}
