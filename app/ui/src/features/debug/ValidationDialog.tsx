/**
 * Run | Web Server Debug Validation (PHPStorm's "Validate Debugger Configuration on Web
 * Server"): checks the Xdebug configuration of a web server (through a temporary script in
 * its document root, created with the user's consent and deleted again) or of a CLI
 * interpreter (`php -i`). The checks themselves are computed in Rust (`forge_debug::validation`).
 */
import { createSignal, For, Match, Show, Switch } from "solid-js";
import { CircleCheck, CircleX, Info, TriangleAlert, X } from "lucide-solid";
import { getSetting, saveSetting } from "@/state/settings";
import { openSettings } from "@/features/settings/dialogState";
import { decodeInterpreters, GLOBAL_KEY, PROJECT_KEY, SELECTED_KEY } from "@/features/php/interpreters";
import { debugApi, type CheckStatus, type ValidationReport } from "./api";
import { setValidationDialog } from "./overlays";
import { port } from "./store";
import { t } from "@/i18n";

const ROOT_KEY = "php.debug.validation.documentRoot";
const URL_KEY = "php.debug.validation.url";

function StatusIcon(props: { status: CheckStatus }) {
  return (
    <Switch>
      <Match when={props.status === "ok"}>
        <CircleCheck size={15} class="dbg-status-ok" aria-label="OK" />
      </Match>
      <Match when={props.status === "warning"}>
        <TriangleAlert size={15} class="dbg-status-warning" aria-label={t("debug.validationDialog.warning")} />
      </Match>
      <Match when={props.status === "error"}>
        <CircleX size={15} class="dbg-status-error" aria-label={t("debug.validationDialog.error")} />
      </Match>
      <Match when={props.status === "info"}>
        <Info size={15} class="dbg-status-info" aria-label={t("debug.validationDialog.information")} />
      </Match>
    </Switch>
  );
}

export function ValidationDialog() {
  const [mode, setMode] = createSignal<"web" | "cli">("web");
  const [root, setRoot] = createSignal(getSetting<string>(ROOT_KEY, "public"));
  const [url, setUrl] = createSignal(getSetting<string>(URL_KEY, "http://localhost:8080"));
  const interpreters = () => decodeInterpreters(getSetting(GLOBAL_KEY, []), getSetting(PROJECT_KEY, []));
  const [interpreter, setInterpreter] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [report, setReport] = createSignal<ValidationReport | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const close = () => setValidationDialog(false);

  const validate = async () => {
    setBusy(true);
    setError(null);
    setReport(null);
    try {
      if (mode() === "web") {
        if (root() !== getSetting<string>(ROOT_KEY, "public")) void saveSetting(ROOT_KEY, root());
        if (url() !== getSetting<string>(URL_KEY, "http://localhost:8080")) void saveSetting(URL_KEY, url());
        setReport(await debugApi.validateWeb(root().trim(), url().trim()));
      } else {
        setReport(await debugApi.validateCli(interpreter() || undefined));
      }
    } catch (e) {
      setError(String(e).replace(/^Error: /, ""));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div class="dbg-backdrop" onMouseDown={(e) => e.target === e.currentTarget && close()} onKeyDown={(e) => e.key === "Escape" && (e.stopPropagation(), close())}>
      <div class="dbg-dialog dbg-validate" role="dialog" aria-label={t("debug.validationDialog.validateDebuggerConfiguration")}>
        <header class="dbg-dialog-title">
          <span>{t("debug.validationDialog.validateDebuggerConfiguration")}</span>
          <button class="icon-btn" title={t("common.close")} onClick={close}>
            <X size={14} />
          </button>
        </header>
        <div class="dbg-dialog-body">
          <div class="dbg-validate-mode" role="radiogroup">
            <label class="dbg-check">
              <input type="radio" name="dbg-validate-mode" checked={mode() === "web"} onChange={() => setMode("web")} />
              {t("debug.validationDialog.localWebServerOrShared")}
            </label>
            <label class="dbg-check">
              <input type="radio" name="dbg-validate-mode" checked={mode() === "cli"} onChange={() => setMode("cli")} />
              {t("debug.validationDialog.cliInterpreter")}
            </label>
          </div>
          <Show
            when={mode() === "web"}
            fallback={
              <div class="dbg-validate-form">
                <label for="dbg-validate-interp">{t("debug.validationDialog.interpreter")}</label>
                <select id="dbg-validate-interp" class="dbg-input" value={interpreter()} onChange={(e) => setInterpreter(e.currentTarget.value)}>
                  <option value="">{t("tools.debug.projectDefault", { value: getSetting<string>(SELECTED_KEY, "php") || "php" })}</option>
                  <For each={interpreters()}>{(i) => <option value={i.name}>{i.name}</option>}</For>
                </select>
              </div>
            }
          >
            <div class="dbg-validate-form">
              <label for="dbg-validate-root">{t("debug.validationDialog.pathToCreateValidationScript")}</label>
              <input id="dbg-validate-root" class="dbg-input dbg-mono" value={root()} placeholder="public" onInput={(e) => setRoot(e.currentTarget.value)} />
              <label for="dbg-validate-url">{t("debug.validationDialog.urlToValidationScript")}</label>
              <input id="dbg-validate-url" class="dbg-input dbg-mono" value={url()} placeholder="http://localhost:8080" onInput={(e) => setUrl(e.currentTarget.value)} />
            </div>
            <p class="muted dbg-hint">
              {t("debug.validationDialog.validateHint", { file: "forge_xdebug_validator_*.php" })}
            </p>
          </Show>
          <Show when={error()}>
            <div class="dbg-check-row">
              <StatusIcon status="error" />
              <span class="dbg-error-text" style={{ "grid-column": "2 / 4" }}>
                {error()}
              </span>
            </div>
          </Show>
          <Show when={report()}>
            {(r) => (
              <>
                <div class="dbg-checks" role="list">
                  <For each={r().checks}>
                    {(c) => (
                      <div class="dbg-check-row" role="listitem" data-status={c.status}>
                        <StatusIcon status={c.status} />
                        <span>{c.title}</span>
                        <span class="dbg-mono">{c.value}</span>
                        <Show when={c.hint}>
                          <span class="dbg-check-hint">{c.hint}</span>
                        </Show>
                      </div>
                    )}
                  </For>
                </div>
                <p class="muted dbg-hint">
                  {r().ok
                    ? t("debug.validationDialog.xdebugCanConnectToForge", { port: port() })
                    : t("tools.debug.fixTheProblemsMarkedAbove")}
                </p>
              </>
            )}
          </Show>
        </div>
        <footer class="dbg-dialog-actions">
          <button class="dbg-link dbg-left" onClick={() => (close(), openSettings("lang.php.debug"))}>
            {t("debug.validationDialog.debugSettings")}
          </button>
          <button class="btn-secondary" onClick={close}>
            {t("common.close")}
          </button>
          <button class="btn-primary" disabled={busy() || (mode() === "web" && !url().trim())} onClick={() => void validate()}>
            {busy() ? t("debug.validationDialog.validating") : t("debug.validationDialog.validate")}
          </button>
        </footer>
      </div>
    </div>
  );
}
