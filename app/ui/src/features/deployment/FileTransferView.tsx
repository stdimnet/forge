/**
 * The File Transfer tool window: every upload, download and delete of this session with
 * the file being transferred, the result per file and the summary, like PHPStorm's.
 */
import { createMemo, createSignal, For, Show } from "solid-js";
import {
  ArrowDown,
  ArrowUp,
  Ban,
  ChevronDown,
  ChevronRight,
  CircleAlert,
  CircleCheck,
  CircleX,
  Clock,
  Equal,
  EyeOff,
  LoaderCircle,
  Trash2,
  X,
} from "lucide-solid";
import type { FileResult, FileStatus } from "@/ipc/deployment";
import { formatSize } from "@/features/compare/DirDiffView";
import { cancelOperation, clearLog, log, type Operation } from "./store";
import { t } from "@/i18n";

const STATUS: Record<FileStatus, { title: string; icon: typeof CircleCheck; cls: string }> = {
  transferred: { title: "Transferred", icon: CircleCheck, cls: "ok" },
  upToDate: { title: "Up to date, skipped", icon: Equal, cls: "muted" },
  newerOnTarget: { title: "Newer on the target, not overwritten", icon: Clock, cls: "warn" },
  excluded: { title: "Excluded", icon: EyeOff, cls: "muted" },
  notMapped: { title: "Not mapped", icon: Ban, cls: "muted" },
  deleted: { title: "Deleted", icon: Trash2, cls: "ok" },
  failed: { title: "Failed", icon: CircleX, cls: "bad" },
};

const time = (ms: number) => new Date(ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" });

export function summaryText(op: Operation): string {
  if (!op.done) {
    const n = op.files.filter((f) => f.status === "transferred" || f.status === "deleted").length;
    return n ? `${n} file${n === 1 ? "" : "s"} so far…` : "Connecting…";
  }
  if (op.error) return `failed: ${op.error}`;
  const s = op.summary!;
  const parts: string[] = [];
  if (s.cancelled) parts.push("cancelled");
  if (s.transferred) parts.push(`${s.transferred} file${s.transferred === 1 ? "" : "s"} transferred (${formatSize(s.bytes)})`);
  if (s.deleted) parts.push(`${s.deleted} deleted`);
  if (s.skipped) parts.push(`${s.skipped} skipped`);
  if (s.failed) parts.push(`${s.failed} failed`);
  return parts.length ? `completed: ${parts.join(", ")}` : "completed: nothing to transfer";
}

function OperationIcon(props: { op: Operation }) {
  return (
    <Show when={props.op.done} fallback={<LoaderCircle size={14} class="ssh-spin ft-running" />}>
      <Show
        when={!props.op.error && !props.op.summary?.failed}
        fallback={<CircleAlert size={14} class="ft-bad" />}
      >
        {props.op.operation.startsWith("Download") ? <ArrowDown size={14} class="ft-ok" /> : props.op.operation.startsWith("Delete") ? <Trash2 size={14} class="ft-ok" /> : <ArrowUp size={14} class="ft-ok" />}
      </Show>
    </Show>
  );
}

function FileRow(props: { f: FileResult }) {
  const s = () => STATUS[props.f.status];
  const Icon = s().icon;
  return (
    <div class={`ft-file ft-${s().cls}`} title={props.f.message ?? s().title} data-status={props.f.status}>
      <Icon size={13} class="ft-file-icon" />
      <span class="ft-local">{props.f.local || "/"}</span>
      <Show when={props.f.remote}>
        <span class="ft-arrow muted">→</span>
        <span class="ft-remote muted">{props.f.remote}</span>
      </Show>
      <span class="spacer" />
      <Show when={props.f.status === "transferred"}>
        <span class="ft-size muted">{formatSize(props.f.bytes)}</span>
      </Show>
      <Show when={props.f.status !== "transferred"}>
        <span class={`ft-state ft-${s().cls}`}>{props.f.message ?? s().title}</span>
      </Show>
    </div>
  );
}

export function FileTransferView() {
  const [collapsed, setCollapsed] = createSignal(new Set<string>());
  const [hideSkipped, setHideSkipped] = createSignal(true);
  const ops = createMemo(() => [...log.operations].reverse());
  const toggle = (id: string) =>
    setCollapsed((prev) => {
      const next = new Set(prev);
      next.has(id) ? next.delete(id) : next.add(id);
      return next;
    });
  const shown = (op: Operation) =>
    hideSkipped() ? op.files.filter((f) => f.status !== "upToDate" && f.status !== "excluded" && f.status !== "notMapped") : op.files;

  return (
    <div class="ft-view">
      <div class="panel-toolbar ft-toolbar">
        <button class="icon-btn" title={t("common.cancel")} disabled={!log.operations.some((o) => !o.done)} onClick={() => log.operations.filter((o) => !o.done).forEach((o) => cancelOperation(o.id))}>
          <X size={15} />
        </button>
        <button class="icon-btn" title={t("common.clearAll")} disabled={!log.operations.some((o) => o.done)} onClick={clearLog}>
          <Trash2 size={14} />
        </button>
        <div class="toolbar-sep" />
        <button class="pill-btn" classList={{ active: !hideSkipped() }} aria-pressed={!hideSkipped()} title={t("tools.deployment.showSkippedFilesUpTo")} onClick={() => setHideSkipped((v) => !v)}>
          <EyeOff size={13} /> {t("tools.deployment.showSkipped")}
        </button>
      </div>
      <div class="ft-list" tabIndex={0}>
        <Show when={!ops().length}>
          <div class="commit-empty">{t("tools.deployment.noFileTransfersYetUse")}</div>
        </Show>
        <For each={ops()}>
          {(op) => (
            <div class="ft-op" data-op={op.id}>
              <div class="ft-op-row" onClick={() => toggle(op.id)}>
                <span class="tree-chevron">{collapsed().has(op.id) ? <ChevronRight size={14} /> : <ChevronDown size={14} />}</span>
                <OperationIcon op={op} />
                <span class="ft-time muted">[{time(op.started)}]</span>
                <span class="ft-op-title">{op.operation}</span>
                <Show when={op.automatic}>
                  <span class="ft-badge">automatic</span>
                </Show>
                <span class="ft-op-summary" classList={{ "ft-bad": !!op.error || !!op.summary?.failed, muted: !op.error && !op.summary?.failed }}>
                  {summaryText(op)}
                </span>
                <Show when={!op.done}>
                  <button
                    class="icon-btn ft-cancel"
                    title={t("common.cancel")}
                    onClick={(e) => {
                      e.stopPropagation();
                      cancelOperation(op.id);
                    }}
                  >
                    <X size={13} />
                  </button>
                </Show>
              </div>
              <Show when={!collapsed().has(op.id)}>
                <For each={shown(op)}>{(f) => <FileRow f={f} />}</For>
              </Show>
            </div>
          )}
        </For>
      </div>
    </div>
  );
}
