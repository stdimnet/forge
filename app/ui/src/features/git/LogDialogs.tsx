/**
 * Dialogs of the Git Log actions, laid out like PHPStorm's: Edit Commit Message, Squash
 * Commits and the interactive rebase ("Rebasing Commits": the commits with their actions, a
 * toolbar with Pick / Edit / Reword / Squash / Fixup / Drop, Move Up / Down (Alt+Up / Alt+Down
 * through the keymap), Reset, drag and drop, and the commit details or the message editor).
 */
import { createMemo, createResource, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { ArrowDown, ArrowUp, Check, Combine, Pencil, RotateCcw, SquarePen, Trash2, Wrench } from "lucide-solid";
import { FileIcon } from "@/shell/FileIcon";
import { shortcutLabel } from "@/keymap";
import { gitApi } from "./api";
import type { RebaseAction, RebasePlan } from "./logApi";
import { closeGitDialog } from "./gitDialogs";
import { Frame } from "./DialogFrame";
import { reword, squashCommits, startRebase } from "./logActions";
import { logDate } from "./format";
import { shortSha } from "./history";
import { changed, groupHead, initialRows, messageRow, moveRows, moveTo, problem, setAction, setMessage, toEntries, unite, type RebaseRow } from "./rebaseModel";
import { t as tr } from "@/i18n";

// ---- Edit Commit Message / Squash Commits ------------------------------------------------------

export function CommitMessageDialog(props: { title: string; okLabel: string; message: string; onOk: (message: string) => Promise<void> }) {
  const [message, setMsg] = createSignal(props.message);
  return (
    <Frame
      title={props.title}
      class="gd-wide log-message-dialog"
      okLabel={props.okLabel}
      okDisabled={!message().trim()}
      onOk={async () => {
        const m = message();
        closeGitDialog();
        await props.onOk(m);
      }}
    >
      <textarea class="gd-textarea log-message-editor" data-autofocus value={message()} onInput={(e) => setMsg(e.currentTarget.value)} spellcheck={false} />
    </Frame>
  );
}

export const RewordDialog = (props: { sha: string; message: string }) => (
  <CommitMessageDialog title={tr("git.logDialogs.editCommitMessageForSha", { sha: shortSha(props.sha) })} okLabel="OK" message={props.message} onOk={(m) => reword(props.sha, m)} />
);

export const SquashCommitsDialog = (props: { shas: string[]; message: string }) => (
  <CommitMessageDialog title={tr("git.logDialogs.squashCountCommits", { count: props.shas.length })} okLabel="OK" message={props.message} onOk={(m) => squashCommits(props.shas, m)} />
);

// ---- Rebasing Commits ----------------------------------------------------------------------------

const ACTIONS: { action: RebaseAction; label: string; icon: typeof Check; hint: string }[] = [
  { action: "pick", label: tr("git.logDialogs.pick"), icon: Check, hint: tr("git.logDialogs.useTheCommitAsIs") },
  { action: "edit", label: tr("git.logDialogs.edit"), icon: Pencil, hint: tr("git.logDialogs.stopAfterTheCommitTo") },
  { action: "reword", label: tr("git.logDialogs.reword"), icon: SquarePen, hint: tr("git.logDialogs.changeTheCommitMessage") },
  { action: "squash", label: tr("git.logDialogs.squash"), icon: Combine, hint: tr("git.logDialogs.meldIntoThePreviousCommit") },
  { action: "fixup", label: tr("git.logDialogs.fixup"), icon: Wrench, hint: tr("git.logDialogs.meldIntoThePreviousCommit2") },
  { action: "drop", label: tr("git.logDialogs.drop"), icon: Trash2, hint: tr("git.logDialogs.removeTheCommit") },
];

/** What the open rebase dialog offers to its keymap actions (Alt+Up / Alt+Down). */
let activeRebase: { move: (dir: -1 | 1) => void } | null = null;
export const rebaseDialogActions = () => activeRebase;

export function InteractiveRebaseDialog(props: { sha: string; plan: RebasePlan }) {
  const initial = initialRows(props.plan);
  const [rows, setRows] = createSignal<RebaseRow[]>(initial);
  const [selected, setSelected] = createSignal<Set<string>>(new Set(initial[0] ? [initial[0].sha] : []));
  const [lead, setLead] = createSignal<string | null>(initial[0]?.sha ?? null);
  const [dragging, setDragging] = createSignal<Set<string> | null>(null);
  const [dropAt, setDropAt] = createSignal<number | null>(null);
  let root!: HTMLDivElement;
  let table!: HTMLDivElement;

  const leadIndex = () => rows().findIndex((r) => r.sha === lead());
  const leadRow = () => rows()[leadIndex()];
  const msgAt = createMemo(() => (leadIndex() < 0 ? null : messageRow(rows(), leadIndex())));
  const issue = () => problem(rows());
  const meldTarget = (i: number) => {
    const h = groupHead(rows(), i);
    return h === null ? null : rows()[h];
  };

  const apply = (action: RebaseAction) => setRows((r) => (action === "squash" && selected().size > 1 ? unite(r, selected()) : setAction(r, selected(), action)));
  const move = (dir: -1 | 1) => {
    setRows((r) => moveRows(r, selected(), dir));
    queueMicrotask(() => table.querySelector(`[data-sha="${lead()}"]`)?.scrollIntoView({ block: "nearest" }));
  };
  activeRebase = { move };
  onCleanup(() => (activeRebase = null));
  onMount(() => table.focus());

  const click = (e: MouseEvent, row: RebaseRow) => {
    const list = rows();
    if (e.shiftKey && lead()) {
      const a = list.findIndex((r) => r.sha === lead());
      const b = list.indexOf(row);
      setSelected(new Set(list.slice(Math.min(a, b), Math.max(a, b) + 1).map((r) => r.sha)));
      return;
    }
    if (e.ctrlKey || e.metaKey) {
      const next = new Set(selected());
      next.has(row.sha) ? next.delete(row.sha) : next.add(row.sha);
      setSelected(next);
    } else setSelected(new Set([row.sha]));
    setLead(row.sha);
  };

  const step = (dir: -1 | 1) => {
    const list = rows();
    const next = list[Math.max(0, Math.min(list.length - 1, leadIndex() + dir))];
    if (!next) return;
    setLead(next.sha);
    setSelected(new Set([next.sha]));
    table.querySelector(`[data-sha="${next.sha}"]`)?.scrollIntoView({ block: "nearest" });
  };

  const start = async () => {
    if (issue()) return;
    const entries = toEntries(rows());
    closeGitDialog();
    await startRebase(props.sha, entries);
  };

  const [details] = createResource(lead, (sha) => gitApi.commitDetails(sha).catch(() => null));
  const branch = props.plan.branch ?? "HEAD";
  const onto = props.plan.base ? shortSha(props.plan.base) : "the root";

  return (
    <div class="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && closeGitDialog()}>
      <div
        ref={root}
        class="git-dialog rebase-dialog"
        role="dialog"
        aria-label={tr("git.logDialogs.rebasingCommits")}
        data-focus-context="toolwindow"
        onKeyDown={(e) => {
          const t = e.target as HTMLElement;
          if (e.key === "Escape" && !e.defaultPrevented) closeGitDialog();
          else if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) void start();
          else if (t === table && !e.altKey && (e.key === "ArrowDown" || e.key === "ArrowUp")) step(e.key === "ArrowDown" ? 1 : -1);
          else return;
          e.preventDefault();
          e.stopPropagation();
        }}
      >
        <header class="dialog-title">
          <span>{tr("git.logDialogs.rebasingCommits")}</span>
          <span class="rebase-subtitle">
            {rows().length} commits of <b>{branch}</b> onto {onto}
          </span>
        </header>
        <div class="panel-toolbar rebase-toolbar" role="toolbar" aria-label={tr("git.logDialogs.rebaseActions")}>
          <For each={ACTIONS}>
            {(a) => (
              <button
                class="rebase-tool"
                classList={{ active: leadRow()?.action === a.action }}
                title={a.hint}
                disabled={!selected().size}
                onClick={() => apply(a.action)}
              >
                <a.icon size={14} />
                {a.action === "squash" && selected().size > 1 ? tr("git.logDialogs.unite") : a.label}
              </button>
            )}
          </For>
          <span class="rebase-sep" />
          <button class="icon-btn" title={`Move Up (${shortcutLabel("Git.Interactive.Rebase.MoveUp") || "Alt+Up"})`} disabled={!selected().size} onClick={() => move(-1)}>
            <ArrowUp size={15} />
          </button>
          <button class="icon-btn" title={`Move Down (${shortcutLabel("Git.Interactive.Rebase.MoveDown") || "Alt+Down"})`} disabled={!selected().size} onClick={() => move(1)}>
            <ArrowDown size={15} />
          </button>
          <span class="spacer" />
          <button class="rebase-tool" title={tr("git.logDialogs.undoAllChangesOfThis")} disabled={!changed(rows(), initial)} onClick={() => setRows(initial)}>
            <RotateCcw size={14} />
            Reset
          </button>
        </div>
        <div class="rebase-body">
          <div class="rebase-table" ref={table} tabIndex={0} role="listbox" aria-multiselectable="true" onDragLeave={() => setDropAt(null)}>
            <For each={rows()}>
              {(row, i) => (
                <div
                  class={`rebase-row action-${row.action}`}
                  classList={{ selected: selected().has(row.sha), lead: lead() === row.sha, "drop-before": dropAt() === i() }}
                  role="option"
                  aria-selected={selected().has(row.sha)}
                  data-sha={row.sha}
                  draggable={true}
                  onMouseDown={(e) => e.button === 0 && click(e, row)}
                  onDragStart={(e) => {
                    const set = selected().has(row.sha) ? selected() : new Set([row.sha]);
                    setDragging(set);
                    e.dataTransfer?.setData("text/plain", [...set].join(" "));
                    if (e.dataTransfer) e.dataTransfer.effectAllowed = "move";
                  }}
                  onDragOver={(e) => {
                    if (!dragging()) return;
                    e.preventDefault();
                    const rect = e.currentTarget.getBoundingClientRect();
                    setDropAt(e.clientY > rect.top + rect.height / 2 ? i() + 1 : i());
                  }}
                  onDrop={(e) => {
                    e.preventDefault();
                    const set = dragging();
                    const at = dropAt();
                    if (set && at !== null) setRows((r) => moveTo(r, set, at));
                    setDragging(null);
                    setDropAt(null);
                  }}
                  onDragEnd={() => {
                    setDragging(null);
                    setDropAt(null);
                  }}
                >
                  <span class="rebase-action">{row.action}</span>
                  <span class="rebase-dot" aria-hidden="true" />
                  <span class="rebase-subject" title={row.message}>
                    {row.action === "reword" && row.newMessage !== null ? row.newMessage.split("\n")[0] : row.subject}
                    <Show when={(row.action === "squash" || row.action === "fixup") && meldTarget(i())}>
                      {(t) => <span class="rebase-into"> into {t().subject}</span>}
                    </Show>
                  </span>
                  <span class="rebase-author">{row.author}</span>
                  <span class="rebase-date">{logDate(row.time)}</span>
                </div>
              )}
            </For>
          </div>
          <div class="rebase-details">
            <Show
              when={msgAt() !== null && leadRow()}
              fallback={
                <Show when={details()} keyed fallback={<div class="commit-empty">{lead() ? tr("git.logDialogs.loading") : tr("git.logDialogs.selectACommit")}</div>}>
                  {(d) => (
                    <>
                      <div class="log-files">
                        <div class="log-files-title">
                          {d.files.length} file{d.files.length === 1 ? "" : "s"} changed
                        </div>
                        <For each={d.files}>
                          {(f) => (
                            <div class="tree-row" style={{ "padding-left": "10px" }}>
                              <FileIcon name={f.path.split("/").pop()!} />
                              <span class={`tree-label ${f.status === "A" ? "vcs-added" : f.status === "D" ? "vcs-deleted" : "vcs-modified"}`}>{f.path.split("/").pop()}</span>
                              <span class="tree-hint">{f.path.includes("/") ? f.path.slice(0, f.path.lastIndexOf("/")) : ""}</span>
                            </div>
                          )}
                        </For>
                      </div>
                      <div class="log-message">
                        <div class="log-subject">{d.message.split("\n")[0]}</div>
                        <Show when={d.message.split("\n").slice(1).join("\n").trim()}>
                          <p class="log-body">{d.message.split("\n").slice(1).join("\n").trim()}</p>
                        </Show>
                        <div class="log-meta">
                          <span class="log-sha">{shortSha(d.sha)}</span> {d.author}
                        </div>
                      </div>
                    </>
                  )}
                </Show>
              }
            >
              <label class="rebase-editor">
                <span class="gd-group-title">{rows()[msgAt()!]?.action === "squash" ? tr("git.logDialogs.messageOfTheSquashedCommit") : tr("git.logDialogs.newCommitMessage")}</span>
                <textarea
                  class="gd-textarea"
                  spellcheck={false}
                  value={rows()[msgAt()!]?.newMessage ?? ""}
                  onInput={(e) => {
                    const v = e.currentTarget.value;
                    setRows((r) => setMessage(r, leadIndex(), v));
                  }}
                />
              </label>
            </Show>
          </div>
        </div>
        <footer class="git-dialog-actions">
          <Show when={issue()} fallback={<span class="gd-note">{tr("git.logDialogs.reorderHint", { up: shortcutLabel("Git.Interactive.Rebase.MoveUp") || "Alt+Up", down: shortcutLabel("Git.Interactive.Rebase.MoveDown") || "Alt+Down" })}</span>}>
            <span class="gd-invalid">{issue()}</span>
          </Show>
          <span class="spacer" />
          <button class="btn-secondary" onClick={closeGitDialog}>
            Cancel
          </button>
          <button class="btn-primary" disabled={!!issue()} onClick={() => void start()} title={tr("git.logDialogs.startRebasing", { shortcut: "Ctrl+Enter" })}>
            {tr("git.logDialogs.startRebasing2")}
          </button>
        </footer>
      </div>
    </div>
  );
}
