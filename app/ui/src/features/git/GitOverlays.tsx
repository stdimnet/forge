/** Floating git UI: diff viewer, branches popup, blame hover card, dialogs, balloons. */
import { createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { CircleAlert, CircleCheck, Info, X } from "lucide-solid";
import { DiffViewer } from "./DiffViewer";
import { ConflictsDialog } from "./ConflictsDialog";
import { MergeWindow } from "./MergeWindow";
import { conflictsOpen, mergeFile, setMergeFile } from "./conflicts";
import { BranchesPopup } from "./BranchesPopup";
import { Chooser } from "./Chooser";
import { blameCard, branchesOpen, chooserDialog, diffRequest, inputDialog, setDiffRequest, setInputDialog, type InputDialog } from "./overlays";
import { dismissNotice, notices } from "./store";
import { fullDate, timeAgo } from "./format";
import { t } from "@/i18n";

function Dialog(props: { d: InputDialog }) {
  const [value, setValue] = createSignal(props.d.value);
  const [checked, setChecked] = createSignal(props.d.checkbox?.checked ?? false);
  let input: HTMLInputElement | undefined;
  let okBtn!: HTMLButtonElement;
  const ok = async () => {
    if (!props.d.noInput && !props.d.optional && !value().trim()) return;
    setInputDialog(null);
    await props.d.onOk(value(), checked());
  };
  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      setInputDialog(null);
    } else if (e.key === "Enter") {
      e.preventDefault();
      void ok();
    }
  };
  onMount(() => {
    (input ?? okBtn).focus();
    window.addEventListener("keydown", onKey, true);
  });
  onCleanup(() => window.removeEventListener("keydown", onKey, true));
  return (
    <div class="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && setInputDialog(null)}>
      <div class="git-dialog" role="dialog" aria-label={props.d.title}>
        <header class="dialog-title">
          <span>{props.d.title}</span>
        </header>
        <div class="git-dialog-body">
          <label class="git-dialog-label">{props.d.label}</label>
          <Show when={!props.d.noInput}>
            <input ref={input} value={value()} onInput={(e) => setValue(e.currentTarget.value)} spellcheck={false} />
          </Show>
          <Show when={props.d.checkbox}>
            <label class="git-dialog-check">
              <input type="checkbox" checked={checked()} onChange={(e) => setChecked(e.currentTarget.checked)} />
              {props.d.checkbox!.label}
            </label>
          </Show>
        </div>
        <footer class="git-dialog-actions">
          <button class="btn-secondary" onClick={() => setInputDialog(null)}>
            {t("common.cancel")}
          </button>
          <button class="btn-primary" ref={okBtn} onClick={() => void ok()} disabled={!props.d.noInput && !props.d.optional && !value().trim()}>
            {props.d.okLabel}
          </button>
        </footer>
      </div>
    </div>
  );
}

export function GitOverlays() {
  return (
    <>
      <Show when={conflictsOpen()}>
        <ConflictsDialog />
      </Show>
      <Show when={mergeFile()} keyed>
        {(file) => <MergeWindow file={file} onClose={() => setMergeFile(null)} />}
      </Show>
      <Show when={diffRequest()} keyed>
        {(req) => <DiffViewer request={req} onClose={() => setDiffRequest(null)} />}
      </Show>
      <Show when={branchesOpen()}>
        <BranchesPopup />
      </Show>
      <Show when={blameCard()} keyed>
        {(c) => (
          <div class="blame-card" style={{ left: `${c.x}px`, top: `${c.y}px` }}>
            <div class="blame-card-head">
              <span class="log-sha">{c.commit.sha.slice(0, 8)}</span>
              <span class="strong">{c.commit.author}</span>
              <span class="muted">{t("tools.git.ltGt", { email: c.commit.email })}</span>
            </div>
            <div class="muted">
              {fullDate(c.commit.time)} · {timeAgo(c.commit.time)}
            </div>
            <div class="blame-card-msg">{c.commit.summary}</div>
          </div>
        )}
      </Show>
      <Show when={inputDialog()} keyed>
        {(d) => <Dialog d={d} />}
      </Show>
      <Show when={chooserDialog()} keyed>
        {(d) => <Chooser d={d} />}
      </Show>
      <div class="balloons">
        <For each={notices()}>
          {(n) => (
            <div class={`balloon balloon-${n.kind}`} role="status">
              <span class="balloon-icon">
                {n.kind === "error" ? <CircleAlert size={16} /> : n.kind === "success" ? <CircleCheck size={16} /> : <Info size={16} />}
              </span>
              <div class="balloon-text">
                <div class="balloon-title">{n.title}</div>
                <Show when={n.body}>
                  <div class="balloon-body">{n.body}</div>
                </Show>
              </div>
              <button class="icon-btn" title={t("common.close")} onClick={() => dismissNotice(n.id)}>
                <X size={13} />
              </button>
            </div>
          )}
        </For>
      </div>
    </>
  );
}
