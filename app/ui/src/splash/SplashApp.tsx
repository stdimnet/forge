/**
 * The startup splash window (`splash.html`, 640×400): the Forge wordmark in cold steel
 * fills with molten metal as Rust reports the loading steps (`splash://progress`), the
 * version below it, the current step at the bottom, and the update offer when the last step
 * finds one. Rust closes the window when the workbench is shown. In browser mode `index.html?splash`
 * previews it with the mocked steps and then opens the workbench (`onDone`).
 */
import { createSignal, onCleanup, onMount, Show } from "solid-js";
import { isTauri } from "@/ipc";
import {
  startupApi,
  updatesApi,
  type DownloadProgress,
  type StartupProgress,
  type UpdateAction,
  type UpdateOffer,
} from "@/ipc/updates";
import { UpdatePanel } from "@/features/updates/UpdatePanel";
import { downloadFraction, isFinished } from "@/features/updates/format";
import { version as uiVersion } from "../../package.json";
import { startFire, type Fire } from "./fire";
import "./splash.css";

/** Where the fill stands while an update is offered and downloaded (88 → 100). */
const OFFER_PERCENT = 88;

export function SplashApp(props: { onDone?: () => void }) {
  const [progress, setProgress] = createSignal<StartupProgress>({ step: "Starting", detail: "", percent: 0 });
  const [version, setVersion] = createSignal(uiVersion);
  const [offer, setOffer] = createSignal<UpdateOffer | null>(null);
  const [panelShown, setPanelShown] = createSignal(false);
  const [downloading, setDownloading] = createSignal(false);
  const [download, setDownload] = createSignal<DownloadProgress | null>(null);
  let splash!: HTMLDivElement;
  let word!: HTMLHeadingElement;
  let canvas!: HTMLCanvasElement;
  let fire: Fire | undefined;
  let done = false;

  const apply = (p: StartupProgress) => {
    const stepChanged = p.step !== progress().step || p.detail !== progress().detail;
    setProgress(p);
    // The startup continues after an answer, a failed or a simulated download.
    if (panelShown() && (!downloading() || isFinished(download()?.phase))) setPanelShown(false);
    fire?.setProgress(p.percent);
    if (p.percent >= 100) {
      fire?.burst(30);
      if (!done && props.onDone) {
        done = true;
        setTimeout(() => props.onDone?.(), 700);
      }
    } else if (stepChanged) {
      fire?.burst(8);
    }
  };

  const showOffer = (o: UpdateOffer) => {
    setOffer(o);
    setPanelShown(true);
  };

  const onDownload = (p: DownloadProgress) => {
    setDownload(p);
    fire?.setProgress(OFFER_PERCENT + (100 - OFFER_PERCENT) * downloadFraction(p));
    if (p.phase === "restarting" || p.phase === "simulated") fire?.burst(40);
  };

  const act = (action: UpdateAction) => {
    if (action === "download") setDownloading(true);
    updatesApi.respond(action).catch((e) => {
      setDownloading(false);
      setDownload({
        version: offer()?.version ?? "",
        phase: "failed",
        downloaded: 0,
        bytesPerSecond: 0,
        message: String((e as Error)?.message ?? e),
      });
    });
  };

  onMount(async () => {
    fire = startFire(splash, word, canvas);
    const unlisten = await Promise.all([
      startupApi.onProgress(apply),
      startupApi.onOffer(showOffer),
      updatesApi.onDownload(onDownload),
    ]);
    onCleanup(() => unlisten.forEach((u) => u()));
    const snapshot = await startupApi.splashState();
    setVersion(snapshot.version);
    if (isTauri()) document.documentElement.dataset.theme = snapshot.theme;
    apply(snapshot.progress);
    if (snapshot.offer) showOffer(snapshot.offer);
  });
  onCleanup(() => fire?.dispose());

  return (
    <div class="splash-stage" classList={{ preview: !isTauri() }}>
      <div class="splash" ref={splash} data-tauri-drag-region role="img" aria-label={`Forge ${version()} is starting`}>
        <canvas class="splash-sparks" ref={canvas} />
        <div class="splash-mark" data-tauri-drag-region>
          <h1 class="splash-word" ref={word} data-tauri-drag-region>
            <span class="cold">Forge</span>
            <span class="hot" aria-hidden="true">
              Forge
            </span>
            <span class="front" aria-hidden="true" />
          </h1>
          <div class="splash-version">
            Version <b>{version()}</b>
          </div>
        </div>
        <div class="splash-status" classList={{ hidden: panelShown() }} data-testid="splash-status">
          <span>
            <b>{progress().step}</b>
          </span>
          <span class="detail">{progress().detail}</span>
        </div>
        <Show when={offer()}>
          {(o) => (
            <UpdatePanel
              class="splash-update"
              offer={o()}
              show={panelShown()}
              downloading={downloading()}
              progress={download()}
              onAction={act}
              autofocus
            />
          )}
        </Show>
      </div>
    </div>
  );
}
