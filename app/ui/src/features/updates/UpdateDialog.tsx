/**
 * Help | Check for Updates… (and Check Now on the Updates settings page): checks right away,
 * then offers the update with the same panel as the splash window, or says why there is none.
 */
import { createSignal, Match, onCleanup, Show, Switch } from "solid-js";
import { CircleAlert, CircleCheck, Info, LoaderCircle } from "lucide-solid";
import { suspendKeymap } from "@/keymap";
import { updatesApi, type CheckResult, type DownloadProgress, type UpdateAction, type UpdateOffer } from "@/ipc/updates";
import { isFinished } from "./format";
import { UpdatePanel } from "./UpdatePanel";
import { t } from "@/i18n";
import { latestOnly } from "@/state/async";

type DialogState = { kind: "checking" } | { kind: "result"; result: CheckResult };

const [state, setState] = createSignal<DialogState | null>(null);

/** Checks may overlap (closed while checking, then checked again): only the newest answers. */
const checks = latestOnly();

/** Check for updates now and show the result. */
export async function openCheckForUpdates(): Promise<void> {
  if (state()?.kind === "checking") return;
  const current = checks.begin();
  setState({ kind: "checking" });
  const result = await updatesApi.check().catch(
    (e): CheckResult => ({ status: "error", message: String((e as Error)?.message ?? e) }),
  );
  if (current() && state()?.kind === "checking") setState({ kind: "result", result });
}

/** Offer an update found while Forge started without a splash window. */
export function openUpdateOffer(offer: UpdateOffer): void {
  checks.invalidate();
  setState({ kind: "result", result: { status: "available", offer } });
}

/** What the dialog shows (exported for tests). */
export const updateDialogState = state;

function UpdateDialog(props: { state: DialogState }) {
  const release = suspendKeymap();
  onCleanup(release);
  const [downloading, setDownloading] = createSignal(false);
  const [progress, setProgress] = createSignal<DownloadProgress | null>(null);
  const unlisten = updatesApi.onDownload(setProgress);
  onCleanup(() => void unlisten.then((u) => u()));
  const busy = () => downloading() && !isFinished(progress()?.phase);
  const close = () => {
    if (!busy()) setState(null);
  };
  const act = (action: UpdateAction) => {
    if (action === "download") setDownloading(true);
    updatesApi
      .respond(action)
      .then(() => {
        if (action !== "download") setState(null);
      })
      .catch((e) => {
        setDownloading(true);
        setProgress({ version: "", phase: "failed", downloaded: 0, bytesPerSecond: 0, message: String((e as Error)?.message ?? e) });
      });
  };
  const result = () => (props.state.kind === "result" ? props.state.result : null);
  const title = () => {
    const r = result();
    if (!r) return t("tools.updates.checkingForUpdates");
    return r.status === "available" ? t("tools.updates.updateAvailable") : t("tools.updates.checkForUpdates");
  };
  return (
    <div class="popup-backdrop modal" onMouseDown={(e) => e.target === e.currentTarget && close()}>
      <div
        class="dialog upd-dialog"
        role="dialog"
        aria-label={title()}
        tabindex="-1"
        data-testid="update-dialog"
        ref={(el) => queueMicrotask(() => el.focus())}
        onKeyDown={(e) => {
          if (e.key !== "Escape") return;
          e.preventDefault();
          e.stopPropagation();
          close();
        }}
      >
        <div class="dialog-title">{title()}</div>
        <Switch>
          <Match when={!result()}>
            <div class="upd-message">
              <LoaderCircle size={16} class="upd-spin" />
              <span>{t("tools.updates.checkingForUpdatesEllipsis")}</span>
            </div>
          </Match>
          <Match when={result()?.status === "available" && result()}>
            {(r) => (
              <UpdatePanel
                offer={(r() as { offer: UpdateOffer }).offer}
                downloading={downloading()}
                progress={progress()}
                onAction={act}
                autofocus
              />
            )}
          </Match>
          <Match when={result()?.status === "upToDate" && result()}>
            {(r) => (
              <div class="upd-message ok">
                <CircleCheck size={16} />
                <span>
                  <b>Forge {(r() as { version: string }).version}</b> {t("tools.updates.isUpToDate")}
                </span>
              </div>
            )}
          </Match>
          <Match when={result()?.status === "notChecked" && result()}>
            {(r) => (
              <div class="upd-message">
                <Info size={16} />
                <span>{(r() as { message: string }).message}</span>
              </div>
            )}
          </Match>
          <Match when={result()?.status === "error" && result()}>
            {(r) => (
              <div class="upd-message error">
                <CircleAlert size={16} />
                <span>{(r() as { message: string }).message}</span>
              </div>
            )}
          </Match>
        </Switch>
        <Show when={result()?.status !== "available" || (downloading() && !busy())}>
          <div class="dialog-buttons">
            <Show when={result()?.status === "error"}>
              <button class="btn-secondary" onClick={() => void openCheckForUpdates()}>
                {t("common.retry")}
              </button>
            </Show>
            <button class="btn-primary" onClick={close}>
              {t("common.close")}
            </button>
          </div>
        </Show>
      </div>
    </div>
  );
}

export function UpdateDialogHost() {
  return <Show when={state()}>{(s) => <UpdateDialog state={s()} />}</Show>;
}
