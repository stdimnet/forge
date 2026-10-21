/**
 * The update offer: "Update available: 0.2.0", one line about the download, and
 * Download and restart / Later / Skip this version. While downloading it shows the megabytes,
 * the speed and the signature check. Used by the splash window and the Check for Updates
 * dialog; sizes scale with `--u` (1cqw in the splash).
 */
import { Show } from "solid-js";
import type { DownloadProgress, UpdateAction, UpdateOffer } from "@/ipc/updates";
import { downloadFraction, downloadLabel, formatSpeed, formatTransferred, signatureLabel } from "./format";
import "./updates.css";

export function UpdatePanel(props: {
  offer: UpdateOffer;
  /** Download was started. */
  downloading: boolean;
  progress: DownloadProgress | null;
  onAction: (action: UpdateAction) => void;
  class?: string;
  show?: boolean;
  /** Focus "Download and restart" when shown. */
  autofocus?: boolean;
}) {
  const signature = () => signatureLabel(props.progress);
  const total = () => props.progress?.total ?? props.offer.size ?? null;
  let downloadButton: HTMLButtonElement | undefined;
  return (
    <section
      class={`upd-panel ${props.class ?? ""}`}
      classList={{ show: props.show ?? true, downloading: props.downloading }}
      aria-live="polite"
      data-testid="update-panel"
    >
      <h2>
        Update available: <em>{props.offer.version}</em>
      </h2>
      <p>{props.offer.summary}</p>
      <Show
        when={props.downloading}
        fallback={
          <div class="upd-actions">
            <button
              class="upd-btn primary"
              type="button"
              ref={(el) => {
                downloadButton = el;
                if (props.autofocus) queueMicrotask(() => downloadButton?.focus({ preventScroll: true }));
              }}
              onClick={() => props.onAction("download")}
            >
              Download and restart
            </button>
            <button class="upd-btn" type="button" onClick={() => props.onAction("later")}>
              Later
            </button>
            <span class="grow" />
            <button class="upd-btn link" type="button" onClick={() => props.onAction("skip")}>
              Skip this version
            </button>
          </div>
        }
      >
        <div class="upd-dl" data-testid="update-download">
          <div class="upd-row">
            <span>{downloadLabel(props.offer.version, props.progress)}</span>
            <span>{formatTransferred(props.progress?.downloaded ?? 0, total())}</span>
          </div>
          <div class="upd-bar">
            <i style={{ width: `${downloadFraction(props.progress) * 100}%` }} />
          </div>
          <div class="upd-row">
            <span classList={{ "upd-ok": signature().tone === "ok", "upd-error": signature().tone === "error" }}>
              {signature().text}
            </span>
            <span>{props.progress?.phase === "downloading" ? formatSpeed(props.progress.bytesPerSecond) : ""}</span>
          </div>
        </div>
      </Show>
    </section>
  );
}
