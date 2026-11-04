/**
 * HTTP client extras: "Import from cURL" (paste a `curl …` command, Rust turns it into a
 * request), "Copy as cURL" (the request at the caret, resolved with the environment),
 * "Clear OAuth Tokens", and the browser login of the OAuth 2.0 Authorization Code grant
 * (Rust asks the UI to open the authorization page with `http-client://open-browser`).
 * WebSocket and gRPC requests and OAuth tokens are handled by `scratches_http_run`.
 */
import { createSignal, Show } from "solid-js";
import { render } from "solid-js/web";
import { listen } from "@/ipc";
import { httpExtrasApi } from "@/ipc/webTooling";
import { registerCommand } from "@/state/commands";
import { registerKeymapActions, balloonsFor } from "@/keymap";
import { activeView } from "@/features/lsp/navigation";
import { openInBrowser } from "@/features/run/runStore";
import { openHttpRequest, httpEnvironment, NO_HTTP_ENV, isHttpPath } from "@/features/scratches/store";
import { isDialogWindow } from "@/shell/window/mode";
import { t } from "@/i18n";

const showBalloon = balloonsFor("HTTP Client");

const [importOpen, setImportOpen] = createSignal(false);

/** The environment requests run with ("" = the default one). */
const envArg = () => {
  const e = httpEnvironment();
  return e === NO_HTTP_ENV ? "$none" : e || undefined;
};

/** Insert converted requests into the active `.http` editor, or a new HTTP scratch. */
async function importCurl(command: string): Promise<boolean> {
  let text: string;
  try {
    text = await httpExtrasApi.fromCurl(command);
  } catch (err) {
    showBalloon(t("langs.webTooling.cannotImportTheCurlCommand"), String(err));
    return false;
  }
  const a = activeView();
  if (a && isHttpPath(a.path)) {
    const doc = a.view.state.doc;
    const line = doc.lineAt(a.view.state.selection.main.head);
    const insertAt = line.length ? line.to : line.from;
    const prefix = doc.length && insertAt > 0 ? "\n###\n" : "";
    a.view.dispatch({ changes: { from: insertAt, insert: `${prefix}${text}` }, selection: { anchor: insertAt + prefix.length } });
    a.view.focus();
  } else {
    await openHttpRequest(text);
  }
  return true;
}

async function copyAsCurl(): Promise<void> {
  const a = activeView();
  if (!a || !isHttpPath(a.path)) return void showBalloon(t("langs.webTooling.copyAsCurl"), t("langs.webTooling.putTheCaretOnA"));
  const state = a.view.state;
  const line = state.doc.lineAt(state.selection.main.head).number - 1;
  try {
    const cmd = await httpExtrasApi.toCurl(state.doc.toString(), line, envArg(), a.path);
    await navigator.clipboard?.writeText(cmd);
    showBalloon(t("langs.webTooling.copiedAsCurl"), cmd.split("\n")[0]);
  } catch (err) {
    showBalloon(t("langs.webTooling.cannotCopyAsCurl"), String(err));
  }
}

registerKeymapActions([
  { id: "HttpClient.ImportFromCurl", title: "Import from cURL…", group: "Plugins/HTTP Client" },
  { id: "HttpClient.CopyAsCurl", title: "Copy as cURL", group: "Plugins/HTTP Client" },
  { id: "HttpClient.ClearOAuthTokens", title: "Clear OAuth Tokens", group: "Plugins/HTTP Client" },
]);
registerCommand({ id: "HttpClient.ImportFromCurl", title: "Import from cURL…", category: "Tools", run: () => void setImportOpen(true) });
registerCommand({ id: "HttpClient.CopyAsCurl", title: "Copy as cURL", category: "Tools", run: () => void copyAsCurl() });
registerCommand({
  id: "HttpClient.ClearOAuthTokens",
  title: "Clear OAuth Tokens",
  category: "Tools",
  run: () => void httpExtrasApi.clearTokens().then(() => showBalloon(t("langs.webTooling.oauthTokensCleared"), t("langs.webTooling.theNextRequestLogsIn"))),
});

function ImportCurlDialog() {
  const [text, setText] = createSignal("");
  const close = () => {
    setImportOpen(false);
    setText("");
  };
  const ok = async () => {
    if (await importCurl(text())) close();
  };
  return (
    <div class="popup-backdrop modal et-modal" onMouseDown={(e) => e.target === e.currentTarget && close()}>
      <div
        class="popup et-dialog wt-curl-dialog"
        role="dialog"
        aria-label={t("langs.webTooling.convertCurlToHttpRequest")}
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            e.preventDefault();
            e.stopPropagation();
            close();
          }
        }}
      >
        <div class="et-title">{t("langs.webTooling.convertCurlToHttpRequest")}</div>
        <textarea
          class="set-input et-mono wt-curl-input"
          rows={9}
          placeholder={t("langs.webTooling.curlHttpsApiExampleCom")}
          value={text()}
          ref={(el) => queueMicrotask(() => el.focus())}
          onInput={(e) => setText(e.currentTarget.value)}
        />
        <div class="dialog-buttons et-buttons">
          <span class="et-spacer" />
          <button class="btn-primary" disabled={!text().trim()} onClick={() => void ok()}>
            {t("langs.webTooling.convert")}
          </button>
          <button class="btn-secondary" onClick={close}>
            {t("common.cancel")}
          </button>
        </div>
      </div>
    </div>
  );
}

if (typeof document !== "undefined" && !isDialogWindow()) {
  const host = document.createElement("div");
  document.body.append(host);
  render(() => <Show when={importOpen()}>{<ImportCurlDialog />}</Show>, host);
  // OAuth 2.0 Authorization Code grant: Rust waits for the redirect on a loopback port.
  void listen<string>("http-client://open-browser", (url) => {
    showBalloon(t("langs.webTooling.oauthLogin"), t("langs.webTooling.logInInTheBrowser"));
    openInBrowser(url);
  });
}

/** Open the Import from cURL dialog (toolbar of `.http` editors). */
export const openImportCurl = () => setImportOpen(true);
