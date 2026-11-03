/** Dialogs: Create Merge/Pull Request, Merge (with confirmation) and simple confirmations. */
import { createMemo, createResource, createSignal, For, Match, onMount, Show, Switch } from "solid-js";
import { ArrowLeft, GitMerge, GitPullRequest, LoaderCircle, Plus, TriangleAlert, X } from "lucide-solid";
import { forgesApi, type MergeMethod, type MergeRequestDetails, type User } from "@/ipc/forges";
import { gitApi } from "@/features/git/api";
import { status } from "@/features/git/store";
import { notify } from "@/features/git/store";
import { dialog, openMergeRequest, setDialog, type Dialog } from "./actions";
import { caps, context, errorText, me, ref, refreshAll, requestName } from "./store";
import { Avatar, displayName } from "./ui";

export function ForgeDialogs() {
  return (
    <Show when={dialog()}>
      {(d) => (
        <Switch>
          <Match when={d().kind === "create"}>
            <CreateDialog />
          </Match>
          <Match when={d().kind === "merge"}>
            <MergeDialog d={d() as Extract<Dialog, { kind: "merge" }>} />
          </Match>
          <Match when={d().kind === "confirm"}>
            <ConfirmDialog d={d() as Extract<Dialog, { kind: "confirm" }>} />
          </Match>
        </Switch>
      )}
    </Show>
  );
}

function Frame(props: { title: string; icon?: unknown; class?: string; children: unknown; onClose: () => void }) {
  return (
    <div class="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && props.onClose()}>
      <div
        class={`git-dialog fg-dialog ${props.class ?? ""}`}
        role="dialog"
        aria-label={props.title}
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            e.stopPropagation();
            props.onClose();
          }
        }}
      >
        <header class="dialog-title">
          {props.icon as never}
          <span>{props.title}</span>
          <span class="spacer" />
          <button class="icon-btn" title="Close (Escape)" onClick={props.onClose}>
            <X size={15} />
          </button>
        </header>
        {props.children as never}
      </div>
    </div>
  );
}

const close = () => setDialog(null);

function CreateDialog() {
  const [branches] = createResource(() => gitApi.branches());
  const [members] = createResource(() => forgesApi.members().catch(() => [] as User[]));
  const current = () => status()?.branch ?? branches()?.current ?? "";
  const remoteName = () => context()?.repo.remote ?? "origin";
  const targets = createMemo(() => {
    const prefix = `${remoteName()}/`;
    const names = (branches()?.remote ?? []).map((b) => b.name).filter((n) => n.startsWith(prefix) && !n.endsWith("/HEAD")).map((n) => n.slice(prefix.length));
    return names.length ? names : (branches()?.local ?? []).map((b) => b.name);
  });
  const sources = createMemo(() => (branches()?.local ?? []).map((b) => b.name));
  const [source, setSource] = createSignal<string | null>(null);
  const [target, setTarget] = createSignal<string | null>(null);
  const [title, setTitle] = createSignal("");
  const [titleTouched, setTitleTouched] = createSignal(false);
  const [description, setDescription] = createSignal("");
  const [reviewers, setReviewers] = createSignal<string[]>([]);
  const [labels, setLabels] = createSignal("");
  const [draft, setDraft] = createSignal(false);
  const [removeSource, setRemoveSource] = createSignal(true);
  const [pushFirst, setPushFirst] = createSignal(true);
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [picker, setPicker] = createSignal(false);

  const src = () => source() ?? current();
  const tgt = () => target() ?? (["main", "master", "develop"].find((n) => targets().includes(n)) ?? targets()[0] ?? "main");
  const branchInfo = () => branches()?.local.find((b) => b.name === src());
  const needsPush = () => {
    const b = branchInfo();
    return !b?.upstream || b.ahead > 0;
  };

  // Default title: subject of the branch's last commit, like PHPStorm.
  const [lastSubject] = createResource(src, (rev) => (rev ? gitApi.log({ rev, limit: 1 }).then((c) => c[0]?.subject ?? "") : Promise.resolve("")));
  const effectiveTitle = () => (titleTouched() ? title() : (lastSubject() ?? ""));

  const create = async () => {
    setBusy(true);
    setError(null);
    try {
      if (needsPush() && pushFirst()) {
        if (src() !== current()) throw new Error(`Check out ${src()} to push it first, or push it manually`);
        await gitApi.push();
      }
      const mr = await forgesApi.create({
        sourceBranch: src(),
        targetBranch: tgt(),
        title: effectiveTitle().trim(),
        description: description(),
        draft: draft(),
        reviewers: reviewers(),
        labels: labels()
          .split(",")
          .map((l) => l.trim())
          .filter(Boolean),
        removeSourceBranch: removeSource(),
      });
      close();
      notify("success", `${requestName()} ${ref(mr.number)} created`, mr.title);
      void refreshAll();
      openMergeRequest(mr.number, mr.title);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const memberName = (u: string) => {
    const m = members()?.find((x) => x.username === u);
    return m ? displayName(m) : u;
  };

  return (
    <Frame title={`Create ${requestName()}`} icon={<GitPullRequest size={15} />} class="fg-create" onClose={close}>
      <div class="fg-form">
        <div class="fg-branches">
          <label class="fg-branch-field">
            <span class="muted fg-small">Base branch</span>
            <select value={tgt()} onChange={(e) => setTarget(e.currentTarget.value)} data-field="target">
              <For each={targets()}>{(b) => <option value={b}>{`${remoteName()}/${b}`}</option>}</For>
            </select>
          </label>
          <ArrowLeft size={16} class="muted fg-branch-arrow" />
          <label class="fg-branch-field">
            <span class="muted fg-small">Head branch</span>
            <select value={src()} onChange={(e) => setSource(e.currentTarget.value)} data-field="source">
              <For each={sources()}>{(b) => <option value={b}>{b}</option>}</For>
            </select>
          </label>
        </div>
        <Show when={needsPush()}>
          <label class="git-dialog-check fg-push-hint">
            <input type="checkbox" checked={pushFirst()} onChange={(e) => setPushFirst(e.currentTarget.checked)} />
            <span>
              Push <b>{src()}</b> to {remoteName()} first
              <span class="muted"> ({branchInfo()?.upstream ? `${branchInfo()!.ahead} unpushed commit${branchInfo()!.ahead === 1 ? "" : "s"}` : "not on the remote yet"})</span>
            </span>
          </label>
        </Show>
        <label class="fg-field">
          <span>Title:</span>
          <input
            value={effectiveTitle()}
            onInput={(e) => {
              setTitleTouched(true);
              setTitle(e.currentTarget.value);
            }}
            data-field="title"
          />
        </label>
        <label class="fg-field fg-field-top">
          <span>Description:</span>
          <textarea rows={7} placeholder="Describe the change (Markdown)" value={description()} onInput={(e) => setDescription(e.currentTarget.value)} data-field="description" />
        </label>
        <div class="fg-field">
          <span>Reviewers:</span>
          <div class="fg-reviewer-picker">
            <For each={reviewers()}>
              {(u) => (
                <span class="fg-token">
                  <Avatar user={members()?.find((m) => m.username === u) ?? { username: u, name: null, avatarUrl: null }} size={16} />
                  {memberName(u)}
                  <button class="fg-token-x" title="Remove" onClick={() => setReviewers((r) => r.filter((x) => x !== u))}>
                    <X size={11} />
                  </button>
                </span>
              )}
            </For>
            <button class="fg-link fg-add-reviewer" onClick={() => setPicker(!picker())} data-field="add-reviewer">
              <Plus size={12} /> Add
            </button>
            <Show when={picker()}>
              <div class="popup fg-picker">
                <For each={(members() ?? []).filter((m) => m.username !== me() && !reviewers().includes(m.username))} fallback={<div class="popup-row muted">No more members</div>}>
                  {(m) => (
                    <div
                      class="popup-row"
                      onClick={() => {
                        setReviewers((r) => [...r, m.username]);
                        setPicker(false);
                      }}
                    >
                      <Avatar user={m} size={16} />
                      <span class="popup-label">{displayName(m)}</span>
                      <span class="popup-hint">{m.username}</span>
                    </div>
                  )}
                </For>
              </div>
            </Show>
          </div>
        </div>
        <label class="fg-field">
          <span>Labels:</span>
          <input placeholder="Comma-separated, e.g. backend, security" value={labels()} onInput={(e) => setLabels(e.currentTarget.value)} />
        </label>
        <div class="fg-field">
          <span />
          <div class="fg-checks">
            <Show when={caps().draft}>
              <label class="git-dialog-check">
                <input type="checkbox" checked={draft()} onChange={(e) => setDraft(e.currentTarget.checked)} data-field="draft" />
                Mark as draft
              </label>
            </Show>
            <Show when={context()?.repo.kind === "gitlab" || context()?.repo.kind === "azure"}>
              <label class="git-dialog-check">
                <input type="checkbox" checked={removeSource()} onChange={(e) => setRemoveSource(e.currentTarget.checked)} />
                Delete source branch when merged
              </label>
            </Show>
          </div>
        </div>
        <Show when={error()}>
          <div class="fg-error-box">
            <TriangleAlert size={14} />
            {error()}
          </div>
        </Show>
      </div>
      <div class="git-dialog-actions">
        <span class="muted fg-small fg-dialog-hint">
          {context()?.repo.path} · {src()} → {tgt()}
        </span>
        <button class="btn-secondary" onClick={close}>
          Cancel
        </button>
        <button class="btn-primary" disabled={busy() || !effectiveTitle().trim() || !src() || src() === tgt()} onClick={() => void create()} data-action="create">
          <Show when={busy()}>
            <LoaderCircle size={13} class="spin" />
          </Show>
          {draft() ? `Create Draft ${requestName()}` : `Create ${requestName()}`}
        </button>
      </div>
    </Frame>
  );
}

const methodLabels: Record<MergeMethod, string> = {
  merge: "Create a merge commit",
  squash: "Squash and merge",
  rebase: "Rebase and merge",
};

function MergeDialog(props: { d: Extract<Dialog, { kind: "merge" }> }) {
  // Captured up front: the dialog prop goes stale once the dialog closes.
  const mr: MergeRequestDetails = props.d.mr;
  const onDone = props.d.onDone;
  const [method, setMethod] = createSignal<MergeMethod>(caps().mergeMethods[0] ?? "merge");
  const [deleteBranch, setDeleteBranch] = createSignal(true);
  const defaultMessage = (m: MergeMethod) =>
    m === "squash" ? `${mr.title} (${ref(mr.number)})` : `Merge branch '${mr.sourceBranch}' into '${mr.targetBranch}'\n\n${mr.title} (${ref(mr.number)})`;
  const [message, setMessage] = createSignal(defaultMessage(method()));
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  let first!: HTMLInputElement;
  onMount(() => first?.focus());

  const merge = async () => {
    setBusy(true);
    setError(null);
    try {
      await forgesApi.merge(mr.number, { method: method(), deleteSourceBranch: deleteBranch(), commitMessage: method() === "rebase" ? null : message() });
      close();
      notify("success", `Merged ${ref(mr.number)}`, `${mr.sourceBranch} into ${mr.targetBranch}`);
      onDone();
      void refreshAll();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const warnings = () => {
    const w: string[] = [];
    if (mr.conflicts) w.push(`There are conflicts with ${mr.targetBranch}.`);
    if (mr.pipeline && mr.pipeline.status !== "success") w.push(`The pipeline has not passed (${mr.pipeline.status}).`);
    if (mr.approvals.required > mr.approvals.approvedBy.length) w.push(`${mr.approvals.required - mr.approvals.approvedBy.length} more approval(s) required.`);
    return w;
  };

  return (
    <Frame title={`Merge ${requestName()} ${ref(mr.number)}`} icon={<GitMerge size={15} />} class="fg-merge" onClose={close}>
      <div class="fg-form">
        <div class="fg-merge-summary">
          Merge <b>{mr.sourceBranch}</b> into <b>{mr.targetBranch}</b>: “{mr.title}”
        </div>
        <For each={warnings()}>
          {(w) => (
            <div class="fg-warning">
              <TriangleAlert size={14} /> {w}
            </div>
          )}
        </For>
        <div class="fg-methods">
          <For each={caps().mergeMethods}>
            {(m, i) => (
              <label class="git-dialog-check">
                <input
                  ref={(el) => i() === 0 && (first = el)}
                  type="radio"
                  name="merge-method"
                  checked={method() === m}
                  onChange={() => {
                    setMethod(m);
                    setMessage(defaultMessage(m));
                  }}
                />
                {methodLabels[m]}
              </label>
            )}
          </For>
        </div>
        <Show when={method() !== "rebase"}>
          <label class="fg-field fg-field-top">
            <span>Commit message:</span>
            <textarea rows={4} class="mono" value={message()} onInput={(e) => setMessage(e.currentTarget.value)} />
          </label>
        </Show>
        <label class="git-dialog-check">
          <input type="checkbox" checked={deleteBranch()} onChange={(e) => setDeleteBranch(e.currentTarget.checked)} />
          Delete source branch <b>{mr.sourceBranch}</b> after merge
        </label>
        <Show when={error()}>
          <div class="fg-error-box">
            <TriangleAlert size={14} />
            {error()}
          </div>
        </Show>
      </div>
      <div class="git-dialog-actions">
        <button class="btn-secondary" onClick={close}>
          Cancel
        </button>
        <button class="btn-primary" disabled={busy()} onClick={() => void merge()} data-action="confirm-merge">
          <Show when={busy()}>
            <LoaderCircle size={13} class="spin" />
          </Show>
          Merge
        </button>
      </div>
    </Frame>
  );
}

function ConfirmDialog(props: { d: Extract<Dialog, { kind: "confirm" }> }) {
  const [busy, setBusy] = createSignal(false);
  const action = props.d.run;
  return (
    <Frame title={props.d.title} onClose={close}>
      <div class="git-dialog-body">{props.d.text}</div>
      <div class="git-dialog-actions">
        <button class="btn-secondary" onClick={close}>
          Cancel
        </button>
        <button
          class="btn-primary"
          classList={{ "fg-danger-btn": props.d.danger }}
          disabled={busy()}
          ref={(el) => queueMicrotask(() => el.focus())}
          onClick={async () => {
            setBusy(true);
            try {
              await action();
            } finally {
              setBusy(false);
              close();
            }
          }}
        >
          {props.d.ok}
        </button>
      </div>
    </Frame>
  );
}
