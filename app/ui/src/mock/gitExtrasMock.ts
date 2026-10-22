/**
 * Browser-mode Get from VCS, worktrees, Commit Message History and signing keys. Clones are
 * simulated with progress (nothing is written anywhere); the demo accounts list a few
 * repositories; worktrees live in the git mock's state.
 */
import { emitMock, registerMock } from "@/ipc";
import { getSetting } from "@/state/settings";
import type { HostRepository, Worktree } from "@/features/git/extrasApi";
import { mockRepo, mockRepoOps, signMockCommit } from "./gitMock";

const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));

// ---- Get from VCS ----------------------------------------------------------------------------

const repo = (path: string, description: string | null, priv: boolean, server: string, updatedAt: string, fork = false): HostRepository => ({
  path,
  name: path.split("/").pop()!,
  description,
  private: priv,
  fork,
  cloneUrl: `${server}/${path}.git`,
  sshUrl: `git@${server.replace(/^https:\/\//, "")}:${path}.git`,
  webUrl: `${server}/${path}`,
  updatedAt,
});

const reposByAccount: Record<string, HostRepository[]> = {
  "forgejo-git.acme.dev": [
    repo("acme/shop", "Online shop (Laravel 12)", true, "https://git.acme.dev", "2026-10-21T09:12:00Z"),
    repo("acme/shop-api", "Public REST API for partners", true, "https://git.acme.dev", "2026-10-19T16:40:00Z"),
    repo("acme/design-system", "Blade and Vue components", false, "https://git.acme.dev", "2026-10-12T11:00:00Z"),
    repo("acme/infrastructure", "Ansible and Docker Compose setup", true, "https://git.acme.dev", "2026-09-30T08:20:00Z"),
    repo("marlon/dotfiles", null, false, "https://git.acme.dev", "2026-08-02T19:45:00Z"),
    repo("marlon/laravel-pint-config", "Shared Pint rules", false, "https://git.acme.dev", "2026-07-14T10:00:00Z", true),
  ],
};

registerMock("forges_repositories", async ({ accountId }) => {
  await delay(300);
  return (reposByAccount[String(accountId)] ?? []).map((r) => ({ ...r }));
});

registerMock("git_clone_directory_name", ({ url }) => {
  const u = String(url ?? "").trim().replace(/[\\/]+$/, "").replace(/\.git$/, "");
  const name = u.split(/[\\/:]/).pop() ?? "";
  return name && name !== "." && name !== ".." ? name : null;
});

registerMock("git_clone_home", ({ connectionId }) => (connectionId ? "/home/deploy" : "/home/marlon"));

registerMock("git_clone_test", async ({ url }) => {
  await delay(500);
  const u = String(url ?? "");
  if (/nope|missing|invalid/.test(u)) throw new Error(`repository '${u}' not found`);
  return [
    { sha: mockRepo.state.head, name: "refs/heads/main" },
    { sha: mockRepo.state.head, name: "refs/heads/develop" },
  ];
});

let cancelled = false;
const PHASES: [string, number, number][] = [
  ["Enumerating objects", 0, 0.02],
  ["Counting objects", 0.02, 0.06],
  ["Compressing objects", 0.06, 0.1],
  ["Receiving objects", 0.1, 0.8],
  ["Resolving deltas", 0.8, 0.95],
  ["Updating files", 0.95, 1],
];

registerMock("git_clone", async ({ url, directory, connectionId }) => {
  cancelled = false;
  const dir = String(directory ?? "").replace(/^~/, connectionId ? "/home/deploy" : "/home/marlon");
  const step = Number(new URLSearchParams(location.search).get("cloneStep") ?? 120);
  emitMock("git://clone-progress", { phase: "Cloning", percent: null, fraction: 0, text: `Cloning into '${dir}'...` });
  for (const [phase, start, end] of PHASES) {
    for (const percent of [0, 25, 50, 75, 100]) {
      await delay(step);
      if (cancelled) throw new Error("Clone was cancelled");
      emitMock("git://clone-progress", { phase, percent, fraction: start + ((end - start) * percent) / 100, text: `${phase}: ${percent}%` });
    }
  }
  if (/nope|missing|invalid/.test(String(url))) throw new Error(`repository '${url}' not found`);
  return { path: dir };
});

registerMock("git_clone_cancel", () => {
  cancelled = true;
});

registerMock("remote_project_connection", () => (new URLSearchParams(location.search).get("project") === "remote" ? "demo-connection" : null));

// ---- worktrees -------------------------------------------------------------------------------

function worktreeList(): Worktree[] {
  const { state } = mockRepo;
  return state.worktrees.map((w, i) => {
    const branch = i === 0 ? state.current : w.branch;
    const head = branch ? (state.local.get(branch) ?? state.head) : state.head;
    return {
      path: w.path,
      head,
      branch,
      detached: !branch,
      bare: false,
      main: i === 0,
      current: i === 0,
      locked: w.locked,
      lockReason: w.lockReason,
      prunable: w.prunable,
    };
  });
}

const find = (path: unknown) => {
  const w = mockRepo.state.worktrees.find((x) => x.path === String(path));
  if (!w) throw new Error(`git worktree: '${path}' is not a working tree`);
  return w;
};

registerMock("git_worktrees", () => worktreeList());
registerMock("git_worktree_add", async ({ options }) => {
  await delay(200);
  const o = options as { path: string; branch?: string | null; newBranch?: string | null; base?: string | null };
  const { state } = mockRepo;
  if (state.worktrees.some((w) => w.path === o.path)) throw new Error(`git worktree: '${o.path}' already exists`);
  let branch = o.branch ?? null;
  if (o.newBranch) {
    if (state.local.has(o.newBranch)) throw new Error(`git worktree: a branch named '${o.newBranch}' already exists`);
    state.local.set(o.newBranch, mockRepo.resolveRev(o.base || "HEAD") ?? state.head);
    branch = o.newBranch;
  } else if (branch && (branch === state.current || state.worktrees.some((w) => w.branch === branch))) {
    throw new Error(`git worktree: '${branch}' is already checked out`);
  }
  state.worktrees.push({ path: o.path, branch, locked: false, lockReason: null, prunable: false });
  mockRepoOps.changed();
  return `Preparing worktree (checking out '${branch}')`;
});
registerMock("git_worktree_remove", ({ path, force }) => {
  const w = find(path);
  if (w.locked && !force) throw new Error("git worktree: cannot remove a locked working tree, lock reason: " + (w.lockReason ?? "") + "\nuse 'remove -f -f' to override or unlock first");
  mockRepo.state.worktrees.splice(mockRepo.state.worktrees.indexOf(w), 1);
  mockRepoOps.changed();
});
registerMock("git_worktree_prune", () => {
  const gone = mockRepo.state.worktrees.filter((w) => w.prunable);
  mockRepo.state.worktrees = mockRepo.state.worktrees.filter((w) => !w.prunable);
  return gone.map((w) => `Removing worktrees/${w.path.split("/").pop()}: gitdir file points to non-existent location`).join("\n");
});
registerMock("git_worktree_lock", ({ path, reason }) => {
  const w = find(path);
  w.locked = true;
  w.lockReason = (reason as string | null) ?? null;
});
registerMock("git_worktree_unlock", ({ path }) => {
  const w = find(path);
  w.locked = false;
  w.lockReason = null;
});

// ---- Commit Message History ------------------------------------------------------------------

const history: string[] = [
  "Add order total calculation to Checkout",
  "Cast email_verified_at to datetime\n\nLaravel 12 no longer casts it implicitly, which broke the\nverification banner for customers created via the API.",
  "Introduce OrderStatus enum",
];

/** Called by the commit mocks after a commit. */
export function recordMockCommit(message: string, sha: string): void {
  const m = message.trim();
  if (!m) return;
  const i = history.indexOf(m);
  if (i >= 0) history.splice(i, 1);
  history.unshift(m);
  history.splice(getSetting<number>("vcs.commit.messageHistorySize", 20));
  if (getSetting<boolean>("vcs.git.commit.gpgSign", false)) signMockCommit(sha);
}

registerMock("git_message_history", () => {
  const own = mockRepo.commits.filter((c) => c.author === "Marlon").map((c) => (c.body ? `${c.subject}\n\n${c.body}` : c.subject));
  const out: string[] = [];
  for (const m of [...history, ...own]) if (!out.includes(m)) out.push(m);
  return out.slice(0, getSetting<number>("vcs.commit.messageHistorySize", 20));
});

// ---- signing keys ----------------------------------------------------------------------------

registerMock("git_signing_info", async () => {
  await delay(150);
  return {
    config: { signingKey: null, format: null, gpgSign: false },
    gpgKeys: [
      {
        id: "3AA5C34371567BD2",
        fingerprint: "4F1C0D9E2B7A6C5D8E9F01233AA5C34371567BD2",
        userIds: ["Marlon <marlon@acme.dev>"],
        expires: Math.floor(Date.now() / 1000) + 400 * 86400,
        canSign: true,
        invalid: false,
      },
      { id: "42E8A0B1C2D3E4F5", fingerprint: "AAAA42E8A0B1C2D3E4F5", userIds: ["Marlon (old laptop) <marlon@acme.dev>"], expires: 1600000000, canSign: true, invalid: true },
    ],
    gpgError: null,
    sshKeys: [
      { path: "/home/marlon/.ssh/id_ed25519.pub", keyType: "ssh-ed25519", comment: "marlon@workstation" },
      { path: "/home/marlon/.ssh/acme_deploy.pub", keyType: "ssh-rsa", comment: "deploy key" },
    ],
    remote: false,
  };
});
