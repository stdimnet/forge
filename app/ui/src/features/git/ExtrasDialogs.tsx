/**
 * Get from Version Control (Git ▸ Clone…), New Worktree…, the Worktrees tab of the Git tool
 * window and the Commit Message History popup (Ctrl+M), laid out like PHPStorm's. Keys inside
 * a dialog or popup are handled on it while the keymap is suspended, like the other git modals.
 */
import { createEffect, createMemo, createResource, createSignal, For, on, onCleanup, onMount, Show } from "solid-js";
import {
  CircleCheck,
  FolderOpen,
  GitBranch,
  Link,
  LoaderCircle,
  Lock,
  LockOpen,
  Plus,
  RefreshCw,
  Search,
  Trash2,
  Eraser,
  GitFork,
} from "lucide-solid";
import { suspendKeymap } from "@/keymap";
import { isTauri } from "@/ipc";
import { forgesApi, type Account, type HostKind } from "@/ipc/forges";
import { remoteApi, type SshConfigView } from "@/ipc/remote";
import { getSetting } from "@/state/settings";
import { openSettings } from "@/features/settings/dialogState";
import { chooseRemoteFolder, connectWithConfirmation } from "@/features/remote/store";
import { closeGitDialog } from "./gitDialogs";
import { Check, Frame, Row } from "./DialogFrame";
import { gitApi } from "./api";
import { extrasApi, type HostRepository, type Worktree } from "./extrasApi";
import {
  cancelClone,
  cloneProgress,
  createWorktree,
  joinDir,
  messageHistory,
  offerToOpen,
  openWorktree,
  pruneWorktrees,
  refreshWorktrees,
  removeWorktree,
  runClone,
  setMessageHistory,
  suggestWorktreePath,
  toggleWorktreeLock,
  useHistoryMessage,
  worktrees,
  newWorktreeDialog,
  type CloneRequest,
  type CloneTarget,
} from "./extras";
import { revision } from "./store";
import "./extras.css";
import { t as tr } from "@/i18n";

const HOSTS: { kind: HostKind; title: string; page: string }[] = [
  { kind: "github", title: "GitHub", page: "vcs.github" },
  { kind: "gitlab", title: "GitLab", page: "vcs.gitlab" },
  { kind: "forgejo", title: tr("git.extrasDialogs.forgejoGitea"), page: "vcs.forgejo" },
  { kind: "azure", title: tr("git.extrasDialogs.azureDevops"), page: "vcs.azure" },
];

const errorText = (e: unknown) => String((e as Error)?.message ?? e);

// ---- Get from Version Control ----------------------------------------------------------------

/** "Clone to": this computer, the open project's host or a saved SSH connection. */
function TargetRow(props: { target: CloneTarget; onChange: (t: CloneTarget) => void; disabled: boolean }) {
  const [configs] = createResource(() => remoteApi.configs().catch(() => [] as SshConfigView[]));
  const [connecting, setConnecting] = createSignal<string | null>(null);
  const value = () => (props.target.connectionId ? `conn:${props.target.connectionId}` : "local");
  const pick = async (v: string) => {
    if (v === "local") return props.onChange({ connectionId: null, label: null });
    if (v.startsWith("cfg:")) {
      const cfg = configs()?.find((c) => c.id === v.slice(4));
      if (!cfg) return;
      setConnecting(tr("git.extrasDialogs.connectingToLabel", { label: cfg.label }));
      try {
        const { label: _l, hasSecret: _h, ...config } = cfg;
        const r = await connectWithConfirmation(config);
        if ("error" in r) {
          setConnecting(`Cannot connect to ${cfg.label}: ${"message" in r.error ? r.error.message : "host key not accepted"}`);
          return;
        }
        setConnecting(null);
        props.onChange({ connectionId: r.connectionId, label: r.label });
      } catch (e) {
        setConnecting(tr("git.extrasDialogs.cannotConnectToLabelError", { label: cfg.label, error: errorText(e) }));
      }
    }
  };
  return (
    <Row label={tr("git.extrasDialogs.cloneTo")}>
      <div class="gd-line">
        <select class="gd-select" value={value()} disabled={props.disabled} onChange={(e) => void pick(e.currentTarget.value)} data-testid="clone-target">
          <option value="local">{tr("git.extrasDialogs.thisComputer")}</option>
          <Show when={props.target.connectionId}>
            <option value={`conn:${props.target.connectionId}`}>{props.target.label} (SSH)</option>
          </Show>
          <For each={(configs() ?? []).filter((c) => c.label !== props.target.label)}>
            {(c) => <option value={`cfg:${c.id}`}>{c.name || c.label} (SSH)</option>}
          </For>
        </select>
        <Show when={connecting()}>
          <span class="gd-note">{connecting()}</span>
        </Show>
      </div>
    </Row>
  );
}

function DirectoryRow(props: { value: string; onInput: (v: string) => void; target: CloneTarget; disabled: boolean }) {
  const browse = async () => {
    if (props.target.connectionId) {
      const dir = await chooseRemoteFolder(props.target.connectionId, props.target.label ?? "", "~");
      if (dir) props.onInput(dir);
      return;
    }
    if (!isTauri()) return;
    const { open } = await import("@tauri-apps/plugin-dialog");
    const dir = await open({ directory: true, multiple: false, title: tr("git.extrasDialogs.destinationDirectory") });
    if (typeof dir === "string") props.onInput(dir);
  };
  return (
    <Row label={tr("git.extrasDialogs.directory")}>
      <div class="gd-line">
        <input
          class="gd-input"
          value={props.value}
          disabled={props.disabled}
          spellcheck={false}
          onInput={(e) => props.onInput(e.currentTarget.value)}
          data-testid="clone-directory"
        />
        <button class="icon-btn" title={tr("git.extrasDialogs.browse")} disabled={props.disabled} onClick={() => void browse()}>
          <FolderOpen size={15} />
        </button>
      </div>
    </Row>
  );
}

function RepositoryList(props: { account: Account; selected: string | null; onSelect: (r: HostRepository) => void; onOpen: () => void }) {
  const [query, setQuery] = createSignal("");
  const [repos] = createResource(
    () => props.account.id,
    (id) => extrasApi.repositories(id),
  );
  const shown = createMemo(() => {
    const q = query().trim().toLowerCase();
    const list = repos() ?? [];
    return q ? list.filter((r) => `${r.path} ${r.description ?? ""}`.toLowerCase().includes(q)) : list;
  });
  const move = (delta: number) => {
    const list = shown();
    if (!list.length) return;
    const i = list.findIndex((r) => r.cloneUrl === props.selected);
    const next = list[Math.max(0, Math.min(list.length - 1, i + delta))];
    props.onSelect(next);
    document.querySelector(`.clone-repo[data-url="${CSS.escape(next.cloneUrl)}"]`)?.scrollIntoView({ block: "nearest" });
  };
  return (
    <div class="clone-repos">
      <label class="clone-search">
        <Search size={14} class="muted" />
        <input
          placeholder={tr("git.extrasDialogs.searchRepositories")}
          value={query()}
          onInput={(e) => setQuery(e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key === "ArrowDown" || e.key === "ArrowUp") {
              e.preventDefault();
              e.stopPropagation();
              move(e.key === "ArrowDown" ? 1 : -1);
            }
          }}
          data-testid="clone-search"
        />
      </label>
      <div class="clone-repo-list" role="listbox">
        <Show when={!repos.loading} fallback={<div class="clone-empty"><LoaderCircle size={14} class="spin" /> {tr("git.extrasDialogs.loadingRepositories")}</div>}>
          <Show when={!repos.error} fallback={<div class="clone-empty gd-invalid">{errorText(repos.error)}</div>}>
            <For each={shown()} fallback={<div class="clone-empty">{tr("git.extrasDialogs.noRepositories")}</div>}>
              {(r) => (
                <div
                  class="clone-repo"
                  classList={{ selected: props.selected === r.cloneUrl }}
                  role="option"
                  data-url={r.cloneUrl}
                  title={r.webUrl}
                  onClick={() => props.onSelect(r)}
                  onDblClick={() => {
                    props.onSelect(r);
                    props.onOpen();
                  }}
                >
                  <span class="clone-repo-icon">{r.fork ? <GitFork size={14} /> : <GitBranch size={14} />}</span>
                  <span class="clone-repo-path">{r.path}</span>
                  <Show when={r.private}>
                    <Lock size={12} class="muted" />
                  </Show>
                  <span class="clone-repo-desc">{r.description}</span>
                </div>
              )}
            </For>
          </Show>
        </Show>
      </div>
    </div>
  );
}

export function CloneDialog(props: { request: CloneRequest }) {
  const [tab, setTab] = createSignal<"url" | HostKind>("url");
  const [target, setTarget] = createSignal<CloneTarget>(props.request.target);
  const [url, setUrl] = createSignal(props.request.url ?? "");
  const [accountId, setAccountId] = createSignal<string | null>(null);
  const [directory, setDirectory] = createSignal("");
  const [dirEdited, setDirEdited] = createSignal(false);
  const [test, setTest] = createSignal<{ busy: boolean; ok?: boolean; message?: string } | null>(null);
  const [accounts] = createResource(() => forgesApi.accounts().catch(() => [] as Account[]));
  const busy = () => cloneProgress() !== null;
  const base = () => {
    const dir = getSetting<string>("system.defaultProjectDirectory", "~/Projects") || "~/Projects";
    return target().connectionId ? "~/Projects" : dir;
  };
  // The directory follows the URL until the user edits it.
  createEffect(
    on([url, target], async ([u]) => {
      setTest(null);
      if (dirEdited()) return;
      const name = u.trim() ? await extrasApi.directoryName(u).catch(() => null) : null;
      if (!dirEdited()) setDirectory(name ? joinDir(base(), name) : "");
    }),
  );
  const accountsOf = (kind: HostKind) => (accounts() ?? []).filter((a) => a.kind === kind);
  const [hostAccount, setHostAccount] = createSignal<Record<string, string>>({});
  const accountFor = (kind: HostKind) => {
    const list = accountsOf(kind);
    return list.find((a) => a.id === hostAccount()[kind]) ?? list[0];
  };

  const canClone = () => !busy() && url().trim() !== "" && directory().trim() !== "";
  const runTest = async () => {
    setTest({ busy: true });
    try {
      const refs = await extrasApi.testUrl(url().trim(), accountId(), target().connectionId);
      setTest({ busy: false, ok: true, message: refs.length ? tr("git.extrasDialogs.connectionSuccessfulBranches", { count: refs.length }) : tr("git.extrasDialogs.connectionSuccessful") });
    } catch (e) {
      setTest({ busy: false, ok: false, message: errorText(e) });
    }
  };
  const clone = async () => {
    if (!canClone()) return;
    const t = target();
    const path = await runClone(url().trim(), directory().trim(), accountId(), t);
    if (!path) return;
    closeGitDialog();
    await offerToOpen(path, t.connectionId);
  };
  const cancel = () => (busy() ? cancelClone() : closeGitDialog());

  let root!: HTMLDivElement;
  const release = suspendKeymap();
  onCleanup(release);
  onMount(() => root.querySelector<HTMLElement>("[data-autofocus]")?.focus());
  const onKeyDown = (e: KeyboardEvent) => {
    const t = e.target as HTMLElement;
    if (e.key === "Escape") cancel();
    else if (e.key === "Enter" && t.tagName !== "BUTTON" && t.tagName !== "SELECT") void clone();
    else return;
    e.preventDefault();
    e.stopPropagation();
  };

  return (
    <div class="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && !busy() && closeGitDialog()}>
      <div ref={root} class="git-dialog gd clone-dialog" role="dialog" aria-label={tr("git.extrasDialogs.getFromVersionControl")} onKeyDown={onKeyDown}>
        <header class="dialog-title">
          <span>{tr("git.extrasDialogs.getFromVersionControl")}</span>
        </header>
        <div class="clone-body">
          <nav class="clone-nav">
            <button class="clone-nav-item" classList={{ active: tab() === "url" }} onClick={() => setTab("url")} data-clone-tab="url">
              <Link size={14} /> {tr("git.extrasDialogs.repositoryUrl")}
            </button>
            <For each={HOSTS}>
              {(h) => (
                <button class="clone-nav-item" classList={{ active: tab() === h.kind }} onClick={() => setTab(h.kind)} data-clone-tab={h.kind}>
                  <GitBranch size={14} /> {h.title}
                </button>
              )}
            </For>
          </nav>
          <div class="clone-main">
            <Show
              when={tab() !== "url" ? HOSTS.find((h) => h.kind === tab()) : undefined}
              fallback={
                <div class="gd-grid">
                  <Row label={tr("git.extrasDialogs.versionControl")}>
                    <select class="gd-select" disabled>
                      <option>Git</option>
                    </select>
                  </Row>
                  <Row label={tr("git.extrasDialogs.url")}>
                    <div class="gd-line">
                      <input
                        class="gd-input"
                        data-autofocus
                        value={url()}
                        disabled={busy()}
                        spellcheck={false}
                        placeholder="https://github.com/owner/repository.git or git@host:owner/repository.git"
                        onInput={(e) => {
                          setUrl(e.currentTarget.value);
                          setAccountId(null);
                        }}
                        data-testid="clone-url"
                      />
                      <button class="btn-secondary" disabled={!url().trim() || busy() || test()?.busy} onClick={() => void runTest()}>
                        {tr("git.extrasDialogs.test")}
                      </button>
                    </div>
                  </Row>
                  <Show when={test()}>
                    {(t) => (
                      <>
                        <span />
                        <div class={t().busy ? "gd-note" : t().ok ? "gd-valid clone-test-ok" : "gd-invalid"} data-testid="clone-test">
                          <Show when={t().busy} fallback={<Show when={t().ok}><CircleCheck size={12} /> </Show>}>
                            <LoaderCircle size={12} class="spin" />{" "}
                          </Show>
                          {t().busy ? tr("git.extrasDialogs.testing") : t().message}
                        </div>
                      </>
                    )}
                  </Show>
                  <TargetRow target={target()} onChange={setTarget} disabled={busy()} />
                  <DirectoryRow
                    value={directory()}
                    target={target()}
                    disabled={busy()}
                    onInput={(v) => {
                      setDirEdited(true);
                      setDirectory(v);
                    }}
                  />
                </div>
              }
            >
              {(host) => (
                <Show
                  when={accountFor(host().kind)}
                  fallback={
                    <div class="clone-login">
                      <p>{tr("git.extrasDialogs.noAccount", { host: host().title })}</p>
                      <p class="muted">{tr("git.extrasDialogs.loginHint")}</p>
                      <button class="btn-primary" onClick={() => openSettings(host().page)}>
                        {tr("git.extrasDialogs.logIn")}
                      </button>
                    </div>
                  }
                >
                  {(account) => (
                    <div class="clone-account">
                      <div class="clone-account-head">
                        <Show
                          when={accountsOf(host().kind).length > 1}
                          fallback={
                            <span class="clone-account-name">
                              {account().username ?? "?"} <span class="clone-account-server">{account().server}</span>
                            </span>
                          }
                        >
                          <select
                            class="gd-select"
                            value={account().id}
                            onChange={(e) => setHostAccount({ ...hostAccount(), [host().kind]: e.currentTarget.value })}
                          >
                            <For each={accountsOf(host().kind)}>
                              {(a) => (
                                <option value={a.id}>
                                  {a.username} — {a.server}
                                </option>
                              )}
                            </For>
                          </select>
                        </Show>
                      </div>
                      <RepositoryList
                        account={account()}
                        selected={accountId() === account().id ? url() : null}
                        onSelect={(r) => {
                          setUrl(r.cloneUrl);
                          setAccountId(account().id);
                        }}
                        onOpen={() => void clone()}
                      />
                      <div class="gd-grid">
                        <TargetRow target={target()} onChange={setTarget} disabled={busy()} />
                        <DirectoryRow
                          value={directory()}
                          target={target()}
                          disabled={busy()}
                          onInput={(v) => {
                            setDirEdited(true);
                            setDirectory(v);
                          }}
                        />
                      </div>
                    </div>
                  )}
                </Show>
              )}
            </Show>
          </div>
        </div>
        <footer class="git-dialog-actions">
          <Show when={cloneProgress()}>
            {(p) => (
              <div class="clone-progress" data-testid="clone-progress">
                <div class="clone-progress-bar">
                  <div class="clone-progress-fill" style={{ width: `${Math.round((p().fraction ?? 0) * 100)}%` }} />
                </div>
                <span class="clone-progress-text" title={p().text}>
                  {p().phase}
                  {p().percent !== null ? ` ${p().percent}%` : "…"}
                </span>
              </div>
            )}
          </Show>
          <Show when={!cloneProgress()}>
            <span class="spacer" />
          </Show>
          <button class="btn-secondary" onClick={cancel}>
            {tr("common.cancel")}
          </button>
          <button class="btn-primary" disabled={!canClone()} onClick={() => void clone()} data-testid="clone-ok">
            {tr("git.extrasDialogs.clone")}
          </button>
        </footer>
      </div>
    </div>
  );
}

// ---- New Worktree --------------------------------------------------------------------------

export function NewWorktreeDialog(props: { branch: string | null }) {
  const [branches] = createResource(() => gitApi.branches().catch(() => null));
  const [mode, setMode] = createSignal<"new" | "existing">(props.branch ? "existing" : "new");
  const [existing, setExisting] = createSignal(props.branch ?? "");
  const [newName, setNewName] = createSignal("");
  const [base, setBase] = createSignal("");
  const [path, setPath] = createSignal("");
  const [pathEdited, setPathEdited] = createSignal(false);
  const [open, setOpen] = createSignal(true);
  const free = () => (branches()?.local ?? []).filter((b) => !b.current && !b.worktree);
  createEffect(() => {
    if (!base() && branches()?.current) setBase(branches()!.current!);
    if (mode() === "existing" && !existing() && free()[0]) setExisting(free()[0].name);
  });
  const branchName = () => (mode() === "new" ? newName().trim() : existing());
  createEffect(
    on([branchName, worktrees], ([b, list]) => {
      if (!pathEdited()) setPath(b ? suggestWorktreePath(list, b) : "");
    }),
  );
  const ok = async () => {
    const options =
      mode() === "new"
        ? { path: path().trim(), newBranch: newName().trim(), base: base() || null }
        : { path: path().trim(), branch: existing() };
    if (await createWorktree(options, false)) {
      closeGitDialog();
      if (open()) await openWorktree(options.path);
    }
  };
  return (
    <Frame title={tr("git.extrasDialogs.newWorktree")} okLabel={tr("git.extrasDialogs.create")} okDisabled={!path().trim() || !branchName()} onOk={ok} class="gd-wide">
      <div class="gd-grid">
        <Row label={tr("git.extrasDialogs.branch")}>
          <div class="gd-inline">
            <label class="git-dialog-check">
              <input type="radio" checked={mode() === "new"} onChange={() => setMode("new")} /> {tr("git.extrasDialogs.newBranchOption")}
            </label>
            <label class="git-dialog-check">
              <input type="radio" checked={mode() === "existing"} onChange={() => setMode("existing")} /> {tr("git.extrasDialogs.existingBranchOption")}
            </label>
          </div>
        </Row>
        <Show
          when={mode() === "new"}
          fallback={
            <Row label={tr("git.extrasDialogs.existingBranch")}>
              <select class="gd-select" value={existing()} onChange={(e) => setExisting(e.currentTarget.value)}>
                <For each={free()}>{(b) => <option value={b.name}>{b.name}</option>}</For>
              </select>
            </Row>
          }
        >
          <Row label={tr("git.extrasDialogs.newBranch")}>
            <input class="gd-input" data-autofocus value={newName()} spellcheck={false} onInput={(e) => setNewName(e.currentTarget.value)} data-testid="worktree-branch" />
          </Row>
          <Row label={tr("git.extrasDialogs.basedOn")}>
            <select class="gd-select" value={base()} onChange={(e) => setBase(e.currentTarget.value)}>
              <For each={[...(branches()?.local ?? []), ...(branches()?.remote ?? [])]}>{(b) => <option value={b.name}>{b.name}</option>}</For>
            </select>
          </Row>
        </Show>
        <Row label={tr("git.extrasDialogs.directory")}>
          <input
            class="gd-input"
            value={path()}
            spellcheck={false}
            onInput={(e) => {
              setPathEdited(true);
              setPath(e.currentTarget.value);
            }}
            data-testid="worktree-path"
          />
        </Row>
        <span />
        <Check label={tr("git.extrasDialogs.openTheWorktreeAfterCreating")} checked={open()} onChange={setOpen} />
      </div>
    </Frame>
  );
}

// ---- Worktrees tab -------------------------------------------------------------------------

export function WorktreesPanel() {
  const [selected, setSelected] = createSignal<string | null>(null);
  createEffect(on(revision, () => void refreshWorktrees()));
  const current = () => worktrees().find((w) => w.path === selected()) ?? null;
  const short = (sha: string | null) => (sha ? sha.slice(0, 8) : "");
  return (
    <div class="worktrees-panel" data-testid="worktrees">
      <div class="panel-toolbar">
        <button class="icon-btn" title={tr("git.extrasDialogs.newWorktree2")} onClick={() => void newWorktreeDialog()}>
          <Plus size={15} />
        </button>
        <button class="icon-btn" title={tr("git.extrasDialogs.open")} disabled={!current() || current()!.current || current()!.bare} onClick={() => void openWorktree(current()!.path)}>
          <FolderOpen size={15} />
        </button>
        <button class="icon-btn" title={current()?.locked ? tr("git.extrasDialogs.unlock") : tr("git.extrasDialogs.lock")} disabled={!current() || current()!.main} onClick={() => void toggleWorktreeLock(current()!)}>
          <Show when={current()?.locked} fallback={<Lock size={15} />}>
            <LockOpen size={15} />
          </Show>
        </button>
        <button class="icon-btn" title={tr("git.extrasDialogs.remove")} disabled={!current() || current()!.main || current()!.current} onClick={() => void removeWorktree(current()!)}>
          <Trash2 size={15} />
        </button>
        <div class="toolbar-sep" />
        <button class="icon-btn" title={tr("git.extrasDialogs.pruneWorktrees")} disabled={!worktrees().some((w) => w.prunable)} onClick={() => void pruneWorktrees()}>
          <Eraser size={15} />
        </button>
        <button class="icon-btn" title={tr("git.extrasDialogs.refresh")} onClick={() => void refreshWorktrees()}>
          <RefreshCw size={15} />
        </button>
      </div>
      <div class="worktrees-table" role="grid">
        <div class="worktrees-row worktrees-head">
          <span>{tr("git.extrasDialogs.path")}</span>
          <span>{tr("git.extrasDialogs.branch2")}</span>
          <span>{tr("git.extrasDialogs.revision")}</span>
        </div>
        <For each={worktrees()} fallback={<div class="commit-empty">{tr("git.extrasDialogs.noWorktrees")}</div>}>
          {(w: Worktree) => (
            <div
              class="worktrees-row"
              classList={{ selected: selected() === w.path, prunable: w.prunable }}
              role="row"
              data-path={w.path}
              onClick={() => setSelected(w.path)}
              onDblClick={() => !w.current && !w.bare && void openWorktree(w.path)}
              title={w.lockReason ? tr("git.extrasDialogs.lockedLockreason", { lockReason: w.lockReason }) : w.prunable ? tr("git.extrasDialogs.theDirectoryIsGonePrune") : w.path}
            >
              <span class="worktrees-path">
                <FolderOpen size={14} class="muted" />
                <span class="worktrees-path-text">{w.path}</span>
                <Show when={w.main}>
                  <span class="worktrees-badge">main</span>
                </Show>
                <Show when={w.current}>
                  <span class="worktrees-badge current">current</span>
                </Show>
                <Show when={w.locked}>
                  <Lock size={12} class="muted" />
                </Show>
                <Show when={w.prunable}>
                  <span class="worktrees-badge warn">prunable</span>
                </Show>
              </span>
              <span class="worktrees-branch">
                {w.bare ? "(bare)" : w.branch ?? <span class="muted">detached</span>}
              </span>
              <span class="log-sha">{short(w.head)}</span>
            </div>
          )}
        </For>
      </div>
    </div>
  );
}

// ---- Commit Message History ----------------------------------------------------------------

export function MessageHistoryPopup(props: { messages: string[] }) {
  const [query, setQuery] = createSignal("");
  const [index, setIndex] = createSignal(0);
  const shown = createMemo(() => {
    const q = query().trim().toLowerCase();
    return q ? props.messages.filter((m) => m.toLowerCase().includes(q)) : props.messages;
  });
  const current = () => shown()[Math.min(index(), shown().length - 1)];
  const release = suspendKeymap();
  onCleanup(release);
  let input!: HTMLInputElement;
  onMount(() => input.focus());
  const close = () => {
    setMessageHistory(null);
    document.querySelector<HTMLTextAreaElement>(".commit-message textarea")?.focus();
  };
  const onKeyDown = (e: KeyboardEvent) => {
    const n = shown().length;
    if (e.key === "Escape") close();
    else if (e.key === "ArrowDown") setIndex((i) => Math.min(n - 1, i + 1));
    else if (e.key === "ArrowUp") setIndex((i) => Math.max(0, i - 1));
    else if (e.key === "Enter") {
      const m = current();
      if (m) useHistoryMessage(m);
    } else return;
    e.preventDefault();
    e.stopPropagation();
    document.querySelector(".message-history-item.selected")?.scrollIntoView({ block: "nearest" });
  };
  const anchor = () => document.querySelector(".commit-message")?.getBoundingClientRect();
  const style = () => {
    const r = anchor();
    if (!r) return { left: "40%", top: "30%" };
    const top = Math.max(8, r.top - 424);
    return { left: `${Math.max(8, r.left)}px`, top: `${top}px`, width: `${Math.max(360, Math.min(560, r.width + 120))}px` };
  };
  return (
    <div class="message-history-backdrop" onMouseDown={(e) => e.target === e.currentTarget && close()}>
      <div class="message-history" style={style()} role="dialog" aria-label={tr("git.extrasDialogs.commitMessageHistory")} onKeyDown={onKeyDown} data-testid="message-history">
        <div class="message-history-title">{tr("git.extrasDialogs.commitMessageHistory")}</div>
        <label class="clone-search">
          <Search size={14} class="muted" />
          <input
            ref={input}
            value={query()}
            placeholder={tr("git.extrasDialogs.typeToSearch")}
            onInput={(e) => {
              setQuery(e.currentTarget.value);
              setIndex(0);
            }}
          />
        </label>
        <div class="message-history-list" role="listbox">
          <For each={shown()} fallback={<div class="clone-empty">{props.messages.length ? tr("common.nothingFound") : tr("git.extrasDialogs.noCommitMessagesYet")}</div>}>
            {(m, i) => (
              <div
                class="message-history-item"
                classList={{ selected: i() === Math.min(index(), shown().length - 1) }}
                role="option"
                onMouseEnter={() => setIndex(i())}
                onClick={() => useHistoryMessage(m)}
              >
                {m.split("\n")[0]}
              </div>
            )}
          </For>
        </div>
        <Show when={current()}>
          <pre class="message-history-preview">{current()}</pre>
        </Show>
      </div>
    </div>
  );
}

export function MessageHistoryHost() {
  return <Show when={messageHistory()}>{(list) => <MessageHistoryPopup messages={list()} />}</Show>;
}
