/**
 * Toolbar of `.http` editors (PHPStorm's HTTP Client): "Run with" environment selector
 * (environments from `http-client.env.json` + `.private.env.json`, read in Rust; the choice
 * is the project setting `httpClient.environment`), a link to the environment file, and
 * Import from cURL / Copy as cURL (`features/webTooling/httpExtras.tsx`).
 */
import { createResource, createSignal, For, onCleanup, Show } from "solid-js";
import { render } from "solid-js/web";
import { showPanel, type Panel } from "@codemirror/view";
import type { Extension } from "@codemirror/state";
import { Check, ChevronDown, Copy, FileJson, Import } from "lucide-solid";
import { scratchesApi, type HttpEnvironments } from "@/ipc/scratches";
import { workspaceApi } from "@/ipc/workspace";
import { openFile } from "@/state/editor";
import { workspace } from "@/state/workspace";
import { showBalloon } from "@/keymap";
import { runCommand } from "@/state/commands";
import { NO_HTTP_ENV, httpEnvironment, setHttpEnvironment } from "./store";
import { t } from "@/i18n";

const ENV_FILE = "http-client.env.json";
const ENV_TEMPLATE = `{
  "dev": {
    "host": "localhost:8000"
  }
}
`;

function HttpToolbar() {
  const [open, setOpen] = createSignal(false);
  const [envs, { refetch }] = createResource(() => scratchesApi.httpEnvironments().catch((): HttpEnvironments => ({ names: [] })));
  const label = () => {
    const chosen = httpEnvironment();
    if (chosen === NO_HTTP_ENV) return t("tools.scratches.noEnvironment");
    return chosen || envs()?.default || t("tools.scratches.noEnvironment");
  };
  const isCurrent = (name: string) => (httpEnvironment() || envs()?.default || NO_HTTP_ENV) === name;
  let menu!: HTMLDivElement;
  const away = (e: MouseEvent) => {
    if (open() && !menu.contains(e.target as Node)) setOpen(false);
  };
  window.addEventListener("mousedown", away, true);
  onCleanup(() => window.removeEventListener("mousedown", away, true));

  const choose = (name: string) => {
    setOpen(false);
    void setHttpEnvironment(name);
  };
  const openEnvFile = async () => {
    if (!workspace()) return void showBalloon(t("tools.scratches.openAProjectFirst"), t("tools.scratches.environmentFilesBelongToA"));
    if (!envs()?.names.length) {
      try {
        await workspaceApi.writeFile(ENV_FILE, ENV_TEMPLATE);
      } catch (err) {
        return void showBalloon(t("tools.scratches.cannotCreateTheEnvironmentFile"), String(err));
      }
    }
    await openFile(ENV_FILE);
    void refetch();
  };

  return (
    <>
      <span class="http-tb-label">{t("tools.scratches.runWithLabel")}</span>
      <div class="http-tb-env" ref={menu}>
        <button
          class="http-tb-combo"
          aria-haspopup="listbox"
          aria-expanded={open()}
          data-testid="http-env"
          onMouseDown={(e) => e.preventDefault()}
          onClick={() => {
            if (!open()) void refetch();
            setOpen(!open());
          }}
        >
          <span>{label()}</span>
          <ChevronDown size={12} />
        </button>
        <Show when={open()}>
          <div class="http-tb-menu" role="listbox">
            <For each={envs()?.names ?? []}>
              {(name) => (
                <button class="http-tb-item" role="option" aria-selected={isCurrent(name)} onClick={() => choose(name)}>
                  <span class="http-tb-check">{isCurrent(name) && <Check size={12} />}</span>
                  {name}
                  <Show when={name === envs()?.default}>
                    <span class="http-tb-hint">default</span>
                  </Show>
                </button>
              )}
            </For>
            <Show when={envs()?.names.length}>
              <div class="http-tb-menu-sep" />
            </Show>
            <button class="http-tb-item" role="option" aria-selected={isCurrent(NO_HTTP_ENV)} onClick={() => choose(NO_HTTP_ENV)}>
              <span class="http-tb-check">{isCurrent(NO_HTTP_ENV) && <Check size={12} />}</span>
              {t("tools.scratches.noEnvironment")}
            </button>
          </div>
        </Show>
      </div>
      <button
        class="http-tb-button http-tb-link"
        title={envs()?.names.length ? `Open ${ENV_FILE}` : `Create ${ENV_FILE}`}
        onMouseDown={(e) => e.preventDefault()}
        onClick={() => void openEnvFile()}
      >
        <FileJson size={13} />
        <span>{envs()?.names.length ? ENV_FILE : t("tools.scratches.addEnvironmentFile")}</span>
      </button>
      <span class="http-tb-spacer" />
      <button
        class="http-tb-button"
        title={t("tools.scratches.convertACurlCommandTo")}
        data-testid="http-import-curl"
        onMouseDown={(e) => e.preventDefault()}
        onClick={() => void runCommand("HttpClient.ImportFromCurl")}
      >
        <Import size={13} />
        <span>{t("tools.scratches.importFromCurl")}</span>
      </button>
      <button
        class="http-tb-button"
        title={t("tools.scratches.copyTheRequestAtThe")}
        onMouseDown={(e) => e.preventDefault()}
        onClick={() => void runCommand("HttpClient.CopyAsCurl")}
      >
        <Copy size={13} />
        <span>{t("tools.scratches.copyAsCurl")}</span>
      </button>
    </>
  );
}

/** The toolbar as a CodeMirror panel above the text. */
export function httpToolbar(): Extension {
  return showPanel.of((): Panel => {
    const dom = document.createElement("div");
    dom.className = "http-toolbar";
    const dispose = render(() => <HttpToolbar />, dom);
    return { dom, top: true, destroy: dispose };
  });
}
