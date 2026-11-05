/**
 * The host's Session Log: who joined, permission changes, what guests changed and what was
 * refused. The backend keeps the entries (`forge_collab::audit`); the dialog refreshes while
 * it is open.
 */
import { createSignal, For, onCleanup, Show } from "solid-js";
import { Portal } from "solid-js/web";
import { TriangleAlert } from "lucide-solid";
import { suspendKeymap } from "@/keymap";
import { collabApi, type SessionLogView } from "@/ipc/collab";
import { t } from "@/i18n";

const [open, setOpen] = createSignal(false);

export const showSessionLog = () => setOpen(true);

/** `14:03:27` in local time. */
export const clock = (ms: number) => {
  const d = new Date(ms);
  return [d.getHours(), d.getMinutes(), d.getSeconds()].map((n) => String(n).padStart(2, "0")).join(":");
};

/** The log lines and where they are saved; refreshes every two seconds while shown. */
export function SessionLogList() {
  const [log, setLog] = createSignal<SessionLogView>({ lines: [] });
  let list: HTMLDivElement | undefined;
  const load = async () => {
    const atEnd = !list || list.scrollTop + list.clientHeight >= list.scrollHeight - 4;
    setLog(await collabApi.sessionLog().catch(() => ({ lines: [] })));
    if (atEnd && list) list.scrollTop = list.scrollHeight;
  };
  void load();
  const timer = setInterval(() => void load(), 2000);
  onCleanup(() => clearInterval(timer));
  return (
    <>
      <div class="cwm-log" ref={list} role="log" aria-label={t("tools.codeWithMe.sessionLogLabel")} tabIndex={0}>
        <For each={log().lines} fallback={<div class="muted">{t("tools.codeWithMe.noSessionHosted")}</div>}>
          {(l) => (
            <div class="cwm-log-line" classList={{ warning: l.warning }}>
              <span class="cwm-log-time muted">{clock(l.time)}</span>
              <Show when={l.warning}>
                <TriangleAlert size={13} class="cwm-log-icon" />
              </Show>
              <span class="cwm-log-text">{l.message}</span>
            </div>
          )}
        </For>
      </div>
      <div class="cwm-log-file muted">
        {log().file ? t("tools.codeWithMe.logSavedTo", { file: log().file }) : t("tools.codeWithMe.logNotSaved")}
      </div>
    </>
  );
}

function Dialog() {
  const release = suspendKeymap();
  onCleanup(release);
  return (
    <div class="popup-backdrop modal">
      <div
        class="dialog cwm-log-dialog"
        style={{ width: "min(640px, calc(100vw - 32px))" }}
        role="dialog"
        aria-modal="true"
        aria-label={t("tools.codeWithMe.sessionLog")}
        tabIndex={-1}
        ref={(el) => queueMicrotask(() => el.focus())}
        onKeyDown={(e) => {
          if (e.key !== "Escape" && e.key !== "Enter") return;
          e.preventDefault();
          e.stopPropagation();
          setOpen(false);
        }}
      >
        <div class="dialog-title">{t("tools.codeWithMe.sessionLog")}</div>
        <div class="dialog-body">
          <SessionLogList />
        </div>
        <div class="dialog-buttons">
          <button class="btn-primary" onClick={() => setOpen(false)}>
            {t("common.close")}
          </button>
        </div>
      </div>
    </div>
  );
}

/** Mounted once (with the toolbar button). */
export function SessionLogHost() {
  return (
    <Show when={open()}>
      <Portal>
        <Dialog />
      </Portal>
    </Show>
  );
}
