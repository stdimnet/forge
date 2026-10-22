/**
 * The Git menu dialogs (see `gitDialogs.ts`), laid out like PHPStorm's: Stash, Unstash,
 * Pull/Merge/Rebase with "Modify options", Reset HEAD, Tag, Git Remotes, Shelve Changes and
 * Unshelve Changes. Keys inside a dialog are handled on the dialog itself while the keymap is
 * suspended, like the other git modals.
 */
import { createMemo, createResource, createSignal, For, Show, type JSX } from "solid-js";
import { ChevronDown, FileDiff, Pencil, Plus, Trash2, X } from "lucide-solid";
import { FileIcon } from "@/shell/FileIcon";
import { gitApi, type ChangedFile, type Remote, type StashEntry } from "./api";
import { opsApi, type ResetMode } from "./opsApi";
import { status, runGit } from "./store";
import { timeAgo } from "./format";
import {
  applyStash,
  clearStashes,
  closeGitDialog,
  createStash,
  createTag,
  dropStash,
  gitDialog,
  gitRoot,
  merge,
  pull,
  rebase,
  reloadStashes,
  resetHead,
  shelve,
  showStashDiff,
  unshelve,
  type GitDialog,
} from "./gitDialogs";
import { Check, Frame, Row } from "./DialogFrame";
import { InteractiveRebaseDialog, RewordDialog, SquashCommitsDialog } from "./LogDialogs";
import "./dialogs.css";

const currentBranch = () => status()?.branch ?? "HEAD";
const nameOf = (p: string) => p.slice(p.lastIndexOf("/") + 1);
const dirOf = (p: string) => (p.includes("/") ? p.slice(0, p.lastIndexOf("/")) : "");

function RootRows() {
  return (
    <>
      <Row label="Git root:">
        <span class="gd-static" title={gitRoot()}>
          {gitRoot() || "…"}
        </span>
      </Row>
      <Row label="Current branch:">
        <span class="gd-static">{currentBranch()}</span>
      </Row>
    </>
  );
}

// ---- "Modify options" ------------------------------------------------------------------------

interface OptionDef {
  key: string;
  flag: string;
  description: string;
  /** Keys this option cannot be combined with. */
  conflicts?: string[];
  /** A choice in words rather than a git flag. */
  plain?: boolean;
}

function OptionsBar(props: { options: OptionDef[]; selected: Set<string>; onChange: (s: Set<string>) => void }) {
  const [open, setOpen] = createSignal(false);
  const blocked = (o: OptionDef) =>
    !props.selected.has(o.key) &&
    [...props.selected].some((k) => o.conflicts?.includes(k) || props.options.find((x) => x.key === k)?.conflicts?.includes(o.key));
  const toggle = (key: string) => {
    const next = new Set(props.selected);
    next.has(key) ? next.delete(key) : next.add(key);
    props.onChange(next);
  };
  return (
    <div class="gd-options">
      <div class="gd-chips">
        <For each={props.options.filter((o) => props.selected.has(o.key))}>
          {(o) => (
            <span class="gd-chip" classList={{ plain: o.plain }}>
              {o.flag}
              <button class="gd-chip-x" title={`Remove ${o.flag}`} onClick={() => toggle(o.key)}>
                <X size={11} />
              </button>
            </span>
          )}
        </For>
        <button class="link-btn gd-modify" onClick={() => setOpen(!open())} aria-expanded={open()}>
          Modify options <ChevronDown size={12} />
        </button>
      </div>
      <Show when={open()}>
        <div class="gd-option-list" role="menu">
          <For each={props.options}>
            {(o) => (
              <label class="gd-option" classList={{ disabled: blocked(o) }} role="menuitemcheckbox" aria-checked={props.selected.has(o.key)}>
                <input type="checkbox" checked={props.selected.has(o.key)} disabled={blocked(o)} onChange={() => toggle(o.key)} />
                <span class="gd-option-flag" classList={{ plain: o.plain }}>{o.flag}</span>
                <span class="gd-option-desc">{o.description}</span>
              </label>
            )}
          </For>
        </div>
      </Show>
    </div>
  );
}

/** Branch names for the branch fields: local, remote and tags. */
function useBranches() {
  const [branches] = createResource(() => gitApi.branches().catch(() => null));
  return branches;
}

function BranchInput(props: { id: string; value: string; onInput: (v: string) => void; names: string[]; placeholder?: string; autofocus?: boolean }) {
  return (
    <>
      <input
        class="gd-input"
        list={props.id}
        value={props.value}
        placeholder={props.placeholder}
        spellcheck={false}
        data-autofocus={props.autofocus ? "" : undefined}
        onInput={(e) => props.onInput(e.currentTarget.value)}
      />
      <datalist id={props.id}>
        <For each={props.names}>{(n) => <option value={n} />}</For>
      </datalist>
    </>
  );
}

// ---- Stash ---------------------------------------------------------------------------------

function StashDialog() {
  const [message, setMessage] = createSignal("");
  const [keepIndex, setKeepIndex] = createSignal(false);
  return (
    <Frame
      title="Stash"
      okLabel="Create Stash"
      onOk={async () => {
        closeGitDialog();
        await createStash(message(), keepIndex());
      }}
    >
      <div class="gd-grid">
        <RootRows />
        <Row label="Message:">
          <input class="gd-input" value={message()} onInput={(e) => setMessage(e.currentTarget.value)} spellcheck={false} />
        </Row>
      </div>
      <Check label="Keep index" checked={keepIndex()} onChange={setKeepIndex} hint="Keep the staged changes in the index and the working tree (--keep-index)" />
    </Frame>
  );
}

// ---- Unstash -------------------------------------------------------------------------------

function UnstashDialog(props: { stashes: StashEntry[] }) {
  const [stashes, setStashes] = createSignal(props.stashes);
  const [selected, setSelected] = createSignal<number | null>(props.stashes[0]?.index ?? null);
  const [pop, setPop] = createSignal(false);
  const [reinstate, setReinstate] = createSignal(false);
  const [branch, setBranch] = createSignal("");
  const [viewing, setViewing] = createSignal<{ entry: StashEntry; files: ChangedFile[] } | null>(null);
  const current = () => stashes().find((s) => s.index === selected()) ?? null;
  const reload = async () => {
    const list = await reloadStashes();
    setStashes(list);
    setSelected(list[0]?.index ?? null);
    setViewing(null);
  };
  const view = async () => {
    const entry = current();
    if (!entry) return;
    const files = await runGit("Could not read the stash", () => opsApi.stashFiles(entry.index));
    if (files) setViewing({ entry, files });
  };
  const okLabel = () => (branch().trim() ? "Branch" : pop() ? "Pop Stash" : "Apply Stash");
  return (
    <Frame
      title="Unstash Changes"
      class="gd-wide"
      okLabel={okLabel()}
      okDisabled={!current()}
      onOk={async () => {
        const entry = current();
        if (!entry) return;
        closeGitDialog();
        await applyStash(entry, { pop: pop(), reinstateIndex: reinstate(), branch: branch() });
      }}
    >
      <div class="gd-grid">
        <RootRows />
      </div>
      <div class="gd-split">
        <div class="git-chooser-list gd-list" role="listbox" aria-label="Stashes">
          <For each={stashes()} fallback={<div class="git-chooser-empty">No stashes</div>}>
            {(s) => (
              <div
                class="git-chooser-row"
                classList={{ selected: s.index === selected() }}
                role="option"
                aria-selected={s.index === selected()}
                data-stash={s.index}
                onClick={() => setSelected(s.index)}
                onDblClick={() => void view()}
              >
                <span class="gd-stash-ref">stash@{`{${s.index}}`}</span>
                <span class="git-chooser-label">
                  {s.branch ? `On ${s.branch}: ` : ""}
                  {s.message}
                </span>
                <span class="git-chooser-detail">{timeAgo(s.time)}</span>
              </div>
            )}
          </For>
        </div>
        <div class="gd-side-buttons">
          <button class="btn-secondary" disabled={!current()} onClick={() => void view()}>
            View
          </button>
          <button
            class="btn-secondary"
            disabled={!current()}
            onClick={async () => {
              const entry = current();
              if (entry && (await dropStash(entry))) await reload();
            }}
          >
            Drop
          </button>
          <button
            class="btn-secondary"
            disabled={!stashes().length}
            onClick={async () => {
              if (await clearStashes()) await reload();
            }}
          >
            Clear
          </button>
        </div>
      </div>
      <Show when={viewing()} keyed>
        {(v) => (
          <div class="gd-files" aria-label="Paths affected in stash">
            <div class="gd-files-title">
              Paths affected in stash@{`{${v.entry.index}}`}: {v.entry.message}
            </div>
            <For each={v.files} fallback={<div class="git-chooser-empty">No files</div>}>
              {(f) => (
                <div class="gd-file" title="Double-click to show the diff" onDblClick={() => void showStashDiff(v.entry, f.path)}>
                  <FileIcon name={nameOf(f.path)} />
                  <span class={`vcs-${f.status === "A" ? "added" : f.status === "D" ? "deleted" : "modified"}`}>{nameOf(f.path)}</span>
                  <span class="tree-hint">{dirOf(f.path)}</span>
                  <button class="icon-btn gd-file-diff" title="Show Diff" onClick={() => void showStashDiff(v.entry, f.path)}>
                    <FileDiff size={13} />
                  </button>
                </div>
              )}
            </For>
          </div>
        )}
      </Show>
      <div class="gd-checks">
        <Check label="Pop stash" checked={pop()} onChange={setPop} disabled={!!branch().trim()} hint="Remove the stash after applying it" />
        <Check label="Reinstate index" checked={reinstate()} onChange={setReinstate} disabled={!!branch().trim()} hint="Restore the staged changes too (--index)" />
      </div>
      <div class="gd-grid">
        <Row label="As new branch:">
          <input class="gd-input" value={branch()} onInput={(e) => setBranch(e.currentTarget.value)} spellcheck={false} placeholder="Branch name (optional)" />
        </Row>
      </div>
    </Frame>
  );
}

// ---- Pull ----------------------------------------------------------------------------------

const PULL_OPTIONS: OptionDef[] = [
  { key: "rebase", flag: "--rebase", description: "Incorporate changes by rebasing rather than merging", conflicts: ["ffOnly", "noFf", "squash", "noCommit"] },
  { key: "ffOnly", flag: "--ff-only", description: "Fast-forward only", conflicts: ["noFf", "squash"] },
  { key: "noFf", flag: "--no-ff", description: "Create a merge commit even when the merge resolves as a fast-forward", conflicts: ["squash"] },
  { key: "squash", flag: "--squash", description: "Create a single commit instead of doing a merge" },
  { key: "noCommit", flag: "--no-commit", description: "Perform the merge but do not commit the result" },
  { key: "noVerify", flag: "--no-verify", description: "Bypass the pre-merge and commit-msg hooks" },
];

function PullDialog() {
  const branches = useBranches();
  const [remotes] = createResource(() => gitApi.remotes().catch(() => [] as Remote[]));
  const [remote, setRemote] = createSignal<string | null>(null);
  const remoteName = () => remote() ?? (status()?.upstream?.split("/")[0] || remotes()?.[0]?.name || "origin");
  const [branch, setBranch] = createSignal<string | null>(null);
  const defaultBranch = () => {
    const up = status()?.upstream;
    return up && up.startsWith(`${remoteName()}/`) ? up.slice(remoteName().length + 1) : (status()?.branch ?? "");
  };
  const branchValue = () => branch() ?? defaultBranch();
  const remoteBranches = createMemo(() =>
    (branches()?.remote ?? []).map((b) => b.name).filter((n) => n.startsWith(`${remoteName()}/`)).map((n) => n.slice(remoteName().length + 1)),
  );
  const [opts, setOpts] = createSignal(new Set<string>());
  return (
    <Frame
      title={`Pull to ${currentBranch()}`}
      class="gd-wide"
      okLabel="Pull"
      okDisabled={!remoteName()}
      onOk={async () => {
        const o = opts();
        closeGitDialog();
        await pull({
          remote: remoteName(),
          branches: branchValue().split(/[\s,]+/).filter(Boolean),
          rebase: o.has("rebase"),
          ffOnly: o.has("ffOnly"),
          noFf: o.has("noFf"),
          squash: o.has("squash"),
          noCommit: o.has("noCommit"),
          noVerify: o.has("noVerify"),
        });
      }}
    >
      <div class="gd-line">
        <select class="gd-select" value={remoteName()} onChange={(e) => setRemote(e.currentTarget.value)} aria-label="Remote">
          <For each={remotes()?.length ? remotes() : [{ name: remoteName(), fetchUrl: "", pushUrl: null }]}>
            {(r) => <option value={r.name}>{r.name}</option>}
          </For>
        </select>
        <BranchInput id="gd-pull-branches" value={branchValue()} onInput={setBranch} names={remoteBranches()} placeholder="Branch to pull" autofocus />
      </div>
      <OptionsBar options={PULL_OPTIONS} selected={opts()} onChange={setOpts} />
    </Frame>
  );
}

// ---- Merge ---------------------------------------------------------------------------------

const MERGE_OPTIONS: OptionDef[] = [
  { key: "noFf", flag: "--no-ff", description: "Create a merge commit even when the merge resolves as a fast-forward", conflicts: ["ffOnly", "squash"] },
  { key: "ffOnly", flag: "--ff-only", description: "Fast-forward only", conflicts: ["squash", "message"] },
  { key: "squash", flag: "--squash", description: "Create a single commit instead of doing a merge", conflicts: ["message"] },
  { key: "message", flag: "-m", description: "Specify the merge commit message" },
  { key: "noCommit", flag: "--no-commit", description: "Perform the merge but do not commit the result" },
  { key: "noVerify", flag: "--no-verify", description: "Bypass the pre-merge and commit-msg hooks" },
  { key: "allowUnrelated", flag: "--allow-unrelated-histories", description: "Allow merging histories that do not share a common ancestor" },
];

function MergeDialog() {
  const branches = useBranches();
  const names = createMemo(() => [
    ...(branches()?.local ?? []).filter((b) => !b.current).map((b) => b.name),
    ...(branches()?.remote ?? []).map((b) => b.name),
  ]);
  const [branch, setBranch] = createSignal("");
  const [message, setMessage] = createSignal("");
  const [opts, setOpts] = createSignal(new Set<string>());
  const list = () => branch().split(/[\s,]+/).filter(Boolean);
  return (
    <Frame
      title={`Merge into ${currentBranch()}`}
      class="gd-wide"
      okLabel="Merge"
      okDisabled={!list().length}
      onOk={async () => {
        const o = opts();
        closeGitDialog();
        await merge({
          branches: list(),
          noFf: o.has("noFf"),
          ffOnly: o.has("ffOnly"),
          squash: o.has("squash"),
          message: o.has("message") ? message() : null,
          noCommit: o.has("noCommit"),
          noVerify: o.has("noVerify"),
          allowUnrelatedHistories: o.has("allowUnrelated"),
        });
      }}
    >
      <div class="gd-line">
        <span class="gd-cmd">merge</span>
        <BranchInput id="gd-merge-branches" value={branch()} onInput={setBranch} names={names()} placeholder="Select branch" autofocus />
      </div>
      <Show when={opts().has("message")}>
        <textarea class="gd-textarea" placeholder="Commit message" value={message()} onInput={(e) => setMessage(e.currentTarget.value)} spellcheck={false} />
      </Show>
      <OptionsBar options={MERGE_OPTIONS} selected={opts()} onChange={setOpts} />
    </Frame>
  );
}

// ---- Rebase --------------------------------------------------------------------------------

const REBASE_OPTIONS: OptionDef[] = [
  { key: "onto", flag: "--onto", description: "Rebase the commits after a base revision onto a new base", conflicts: ["root"] },
  { key: "branch", flag: "Select another branch to rebase", description: "Rebase a branch other than the current one", plain: true },
  { key: "rebaseMerges", flag: "--rebase-merges", description: "Recreate the merge commits instead of flattening the history" },
  { key: "keepEmpty", flag: "--keep-empty", description: "Keep the commits that do not change anything" },
  { key: "root", flag: "--root", description: "Rebase all reachable commits up to the root" },
  { key: "updateRefs", flag: "--update-refs", description: "Update branches that point to the rebased commits" },
];

function RebaseDialog() {
  const branches = useBranches();
  const names = createMemo(() => [
    ...(branches()?.local ?? []).map((b) => b.name),
    ...(branches()?.remote ?? []).map((b) => b.name),
    ...(branches()?.tags ?? []).map((b) => b.name),
  ]);
  const local = createMemo(() => (branches()?.local ?? []).map((b) => b.name));
  const [onto, setOnto] = createSignal("");
  const [from, setFrom] = createSignal("");
  const [branch, setBranch] = createSignal("");
  const [opts, setOpts] = createSignal(new Set<string>());
  const valid = () => (opts().has("root") || !!onto().trim()) && (!opts().has("onto") || !!from().trim());
  return (
    <Frame
      title={`Rebase ${opts().has("branch") && branch().trim() ? branch().trim() : currentBranch()}`}
      class="gd-wide"
      okLabel="Rebase"
      okDisabled={!valid()}
      onOk={async () => {
        const o = opts();
        closeGitDialog();
        await rebase({
          onto: onto().trim(),
          from: o.has("onto") ? from().trim() : null,
          branch: o.has("branch") ? branch().trim() || null : null,
          root: o.has("root"),
          rebaseMerges: o.has("rebaseMerges"),
          keepEmpty: o.has("keepEmpty"),
          updateRefs: o.has("updateRefs"),
        });
      }}
    >
      <div class="gd-line">
        <span class="gd-cmd">rebase</span>
        <Show when={opts().has("onto")}>
          <span class="gd-cmd">--onto</span>
        </Show>
        <BranchInput
          id="gd-rebase-onto"
          value={onto()}
          onInput={setOnto}
          names={names()}
          placeholder={opts().has("root") ? "New base (optional)" : opts().has("onto") ? "New base" : "Select upstream"}
          autofocus
        />
        <Show when={opts().has("onto")}>
          <BranchInput id="gd-rebase-from" value={from()} onInput={setFrom} names={names()} placeholder="From (old base)" />
        </Show>
        <Show when={opts().has("branch")}>
          <BranchInput id="gd-rebase-branch" value={branch()} onInput={setBranch} names={local()} placeholder={currentBranch()} />
        </Show>
      </div>
      <OptionsBar options={REBASE_OPTIONS} selected={opts()} onChange={setOpts} />
      <div class="gd-note">Interactive rebase (--interactive) is not available yet.</div>
    </Frame>
  );
}

// ---- Reset ---------------------------------------------------------------------------------

const RESET_MODES: { mode: ResetMode; label: string; text: string }[] = [
  { mode: "soft", label: "Soft", text: "Files won't change, differences will be staged for commit." },
  { mode: "mixed", label: "Mixed", text: "Files won't change, differences won't be staged." },
  { mode: "hard", label: "Hard", text: "Files will be reverted to the state of the selected commit. Warning: any local changes will be lost." },
  { mode: "keep", label: "Keep", text: "Files will be reverted to the state of the selected commit, but local changes will be kept intact." },
];

/** A commit field with PHPStorm's "Validate" button showing the resolved hash. */
function CommitField(props: { value: string; onInput: (v: string) => void; placeholder?: string }) {
  const [resolved, setResolved] = createSignal<string | null | undefined>(undefined);
  const validate = async () => setResolved(await opsApi.resolveCommit(props.value.trim() || "HEAD").catch(() => null));
  return (
    <div class="gd-commit">
      <input
        class="gd-input"
        value={props.value}
        placeholder={props.placeholder}
        spellcheck={false}
        onInput={(e) => {
          setResolved(undefined);
          props.onInput(e.currentTarget.value);
        }}
      />
      <button class="btn-secondary" onClick={() => void validate()}>
        Validate
      </button>
      <Show when={resolved() !== undefined}>
        <span class={resolved() ? "gd-valid" : "gd-invalid"}>{resolved() ? resolved()!.slice(0, 8) : "Not a commit"}</span>
      </Show>
    </div>
  );
}

function ResetDialog(props: { rev: string }) {
  const [rev, setRev] = createSignal(props.rev);
  const [mode, setMode] = createSignal<ResetMode>("mixed");
  return (
    <Frame
      title="Git Reset"
      class="gd-wide"
      okLabel="Reset"
      onOk={async () => {
        closeGitDialog();
        await resetHead(rev().trim() || "HEAD", mode());
      }}
    >
      <div class="gd-grid">
        <RootRows />
        <Row label="To commit:">
          <CommitField value={rev()} onInput={setRev} placeholder="HEAD" />
        </Row>
      </div>
      <div class="gd-group-title">Reset type</div>
      <div class="gd-radios" role="radiogroup" aria-label="Reset type">
        <For each={RESET_MODES}>
          {(m) => (
            <label class="gd-radio">
              <input type="radio" name="gd-reset-mode" checked={mode() === m.mode} onChange={() => setMode(m.mode)} />
              <span class="gd-radio-label">{m.label}</span>
              <span class="gd-radio-text" classList={{ warn: m.mode === "hard" }}>
                {m.text}
              </span>
            </label>
          )}
        </For>
      </div>
    </Frame>
  );
}

// ---- Tag -----------------------------------------------------------------------------------

function TagDialog(props: { commit: string }) {
  const [name, setName] = createSignal("");
  const [force, setForce] = createSignal(false);
  const [commit, setCommit] = createSignal(props.commit);
  const [message, setMessage] = createSignal("");
  return (
    <Frame
      title="Tag"
      class="gd-wide"
      okLabel="Create Tag"
      okDisabled={!name().trim()}
      onOk={async () => {
        closeGitDialog();
        await createTag({ name: name(), force: force(), commit: commit().trim() || null, message: message().trim() || null });
      }}
    >
      <div class="gd-grid">
        <RootRows />
        <Row label="Tag name:">
          <div class="gd-inline">
            <input class="gd-input" data-autofocus value={name()} onInput={(e) => setName(e.currentTarget.value)} spellcheck={false} />
            <Check label="Force" checked={force()} onChange={setForce} hint="Replace an existing tag of the same name (-f)" />
          </div>
        </Row>
        <Row label="Commit:">
          <CommitField value={commit()} onInput={setCommit} placeholder="HEAD" />
        </Row>
        <Row label="Message:">
          <textarea class="gd-textarea" value={message()} onInput={(e) => setMessage(e.currentTarget.value)} spellcheck={false} placeholder="Optional: creates an annotated tag" />
        </Row>
      </div>
    </Frame>
  );
}

// ---- Git Remotes ---------------------------------------------------------------------------

function RemotesDialog() {
  const [remotes, { refetch }] = createResource(() => gitApi.remotes().catch(() => [] as Remote[]));
  const [selected, setSelected] = createSignal<string | null>(null);
  /** The Define Remote form: adding (old = null) or editing. */
  const [form, setForm] = createSignal<{ old: string | null; name: string; url: string } | null>(null);
  const current = () => remotes()?.find((r) => r.name === selected()) ?? null;
  const save = async () => {
    const f = form();
    if (!f || !f.name.trim() || !f.url.trim()) return;
    const ok = await runGit(f.old ? "Could not change the remote" : "Could not add the remote", () =>
      (f.old ? opsApi.editRemote(f.old, f.name.trim(), f.url.trim()) : gitApi.addRemote(f.name.trim(), f.url.trim())).then(() => true),
    );
    if (!ok) return;
    setForm(null);
    setSelected(f.name.trim());
    await refetch();
  };
  const remove = async (name: string) => {
    await runGit("Could not remove the remote", () => gitApi.removeRemote(name));
    setSelected(null);
    await refetch();
  };
  return (
    <Show
      when={form()}
      fallback={
        <Frame title="Git Remotes" class="gd-wide" okLabel="OK" onOk={() => void closeGitDialog()}>
          <div class="gd-table-box">
            <div class="gd-table-toolbar">
              <button
                class="icon-btn"
                title="Add"
                onClick={() => setForm({ old: null, name: remotes()?.some((r) => r.name === "origin") ? "" : "origin", url: "" })}
              >
                <Plus size={15} />
              </button>
              <button class="icon-btn" title="Remove" disabled={!current()} onClick={() => current() && void remove(current()!.name)}>
                <Trash2 size={14} />
              </button>
              <button class="icon-btn" title="Edit" disabled={!current()} onClick={() => current() && setForm({ old: current()!.name, name: current()!.name, url: current()!.fetchUrl })}>
                <Pencil size={14} />
              </button>
            </div>
            <table class="gd-table">
              <thead>
                <tr>
                  <th>Name</th>
                  <th>URL</th>
                </tr>
              </thead>
              <tbody>
                <For each={remotes() ?? []} fallback={<tr><td colSpan={2} class="git-chooser-empty">No remotes defined</td></tr>}>
                  {(r) => (
                    <tr
                      classList={{ selected: r.name === selected() }}
                      onClick={() => setSelected(r.name)}
                      onDblClick={() => setForm({ old: r.name, name: r.name, url: r.fetchUrl })}
                    >
                      <td>{r.name}</td>
                      <td>
                        {r.fetchUrl}
                        <Show when={r.pushUrl}>
                          <span class="muted"> (push: {r.pushUrl})</span>
                        </Show>
                      </td>
                    </tr>
                  )}
                </For>
              </tbody>
            </table>
          </div>
        </Frame>
      }
    >
      {(f) => (
        <Frame title={f().old ? "Edit Remote" : "Define Remote"} okLabel="OK" okDisabled={!f().name.trim() || !f().url.trim()} onOk={save}
          extra={
            <button class="btn-secondary" onClick={() => setForm(null)}>
              Back
            </button>
          }
        >
          <div class="gd-grid">
            <Row label="Name:">
              <input class="gd-input" value={f().name} onInput={(e) => setForm({ ...f(), name: e.currentTarget.value })} spellcheck={false} />
            </Row>
            <Row label="URL:">
              <input class="gd-input" data-autofocus value={f().url} onInput={(e) => setForm({ ...f(), url: e.currentTarget.value })} spellcheck={false} />
            </Row>
          </div>
        </Frame>
      )}
    </Show>
  );
}

// ---- Shelve / Unshelve ------------------------------------------------------------------------

/** Files with checkboxes (Shelve Changes, Unshelve Changes). */
function FileChecks(props: { files: { path: string; status: string }[]; checked: Set<string>; onChange: (s: Set<string>) => void }) {
  const all = () => props.files.length > 0 && props.files.every((f) => props.checked.has(f.path));
  return (
    <div class="git-chooser-list gd-list gd-file-checks">
      <label class="gd-file gd-file-all">
        <input type="checkbox" checked={all()} onChange={(e) => props.onChange(e.currentTarget.checked ? new Set(props.files.map((f) => f.path)) : new Set())} />
        <span class="strong">Changes</span>
        <span class="tree-hint">
          {props.checked.size} of {props.files.length} files
        </span>
      </label>
      <For each={props.files}>
        {(f) => (
          <label class="gd-file gd-file-indent" data-path={f.path}>
            <input
              type="checkbox"
              checked={props.checked.has(f.path)}
              onChange={(e) => {
                const next = new Set(props.checked);
                e.currentTarget.checked ? next.add(f.path) : next.delete(f.path);
                props.onChange(next);
              }}
            />
            <FileIcon name={nameOf(f.path)} />
            <span class={`vcs-${f.status}`}>{nameOf(f.path)}</span>
            <span class="tree-hint">{dirOf(f.path)}</span>
          </label>
        )}
      </For>
    </div>
  );
}

const statusName = (path: string): string => {
  const e = status()?.entries.find((x) => x.path === path);
  if (!e) return "modified";
  if (e.worktree === "untracked") return "untracked";
  if (e.index === "added") return "added";
  if (e.index === "deleted" || e.worktree === "deleted") return "deleted";
  return "modified";
};

function ShelveDialog(props: { paths: string[] }) {
  const files = createMemo(() =>
    (status()?.entries ?? [])
      .filter((e) => !e.conflict && e.worktree !== "ignored")
      .map((e) => ({ path: e.path, status: statusName(e.path) }))
      .sort((a, b) => a.path.localeCompare(b.path)),
  );
  const [checked, setChecked] = createSignal(new Set(props.paths));
  const [comment, setComment] = createSignal(status()?.branch ? `Changes on ${status()!.branch}` : "Changes");
  return (
    <Frame
      title="Shelve Changes"
      class="gd-wide"
      okLabel="Shelve Changes"
      okDisabled={!checked().size || !comment().trim()}
      onOk={async () => {
        const paths = files().map((f) => f.path).filter((p) => checked().has(p));
        closeGitDialog();
        await shelve(paths, comment());
      }}
    >
      <FileChecks files={files()} checked={checked()} onChange={setChecked} />
      <label class="git-dialog-label">Comment:</label>
      <textarea class="gd-textarea" data-autofocus value={comment()} onInput={(e) => setComment(e.currentTarget.value)} spellcheck={false} />
    </Frame>
  );
}

function UnshelveDialog(props: { changelist: import("./opsApi").ShelvedChangelist; paths: string[] }) {
  const files = () => props.changelist.files.map((f) => ({ path: f.path, status: f.status }));
  const [checked, setChecked] = createSignal(new Set(props.paths));
  const [remove, setRemove] = createSignal(true);
  return (
    <Frame
      title={`Unshelve Changes: ${props.changelist.name}`}
      class="gd-wide"
      okLabel="Unshelve"
      okDisabled={!checked().size}
      onOk={async () => {
        const all = checked().size === props.changelist.files.length;
        const paths = all ? null : files().map((f) => f.path).filter((p) => checked().has(p));
        closeGitDialog();
        await unshelve(props.changelist, paths, remove());
      }}
    >
      <FileChecks files={files()} checked={checked()} onChange={setChecked} />
      <Check label="Remove successfully applied files from the shelf" checked={remove()} onChange={setRemove} />
    </Frame>
  );
}

// ---- host ----------------------------------------------------------------------------------

function render(d: GitDialog): JSX.Element {
  switch (d.kind) {
    case "stash":
      return <StashDialog />;
    case "unstash":
      return <UnstashDialog stashes={d.stashes} />;
    case "pull":
      return <PullDialog />;
    case "merge":
      return <MergeDialog />;
    case "rebase":
      return <RebaseDialog />;
    case "reset":
      return <ResetDialog rev={d.rev} />;
    case "tag":
      return <TagDialog commit={d.commit} />;
    case "remotes":
      return <RemotesDialog />;
    case "shelve":
      return <ShelveDialog paths={d.paths} />;
    case "unshelve":
      return <UnshelveDialog changelist={d.changelist} paths={d.paths} />;
    case "reword":
      return <RewordDialog sha={d.sha} message={d.message} />;
    case "squashCommits":
      return <SquashCommitsDialog shas={d.shas} message={d.message} />;
    case "interactiveRebase":
      return <InteractiveRebaseDialog sha={d.sha} plan={d.plan} />;
  }
}

export function GitDialogs() {
  return (
    <Show when={gitDialog()} keyed>
      {(d) => render(d)}
    </Show>
  );
}
