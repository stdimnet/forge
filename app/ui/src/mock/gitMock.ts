/**
 * Browser-mode git backend for the demo project: a small in-memory repository with HEAD
 * and index snapshots (the working tree is the workspace mock), ~25 commits with branches,
 * merges and tags, blame data, and a mock for the shared `ai_complete` command.
 */
import { invoke, registerMock, emitMock } from "@/ipc";
import { webDemo } from "./demoWebProjects";
import type { FileEntry } from "@/ipc/workspace";
import type {
  Blame,
  BlameCommit,
  BranchInfo,
  Branches,
  ChangedFile,
  Commit,
  CommitDetails,
  LogQuery,
  RefLabel,
  RepoStatus,
  StatusEntry,
  FileState,
} from "@/features/git/api";
import { lineDiff, unifiedDiff } from "@/features/git/textDiff";
import { demoFiles } from "./demoProject";
import { createIgnoreMatcher } from "./gitignoreMatcher";
import { isLuaDemo } from "./demoLuaProject";
import { isPythonDemo, pythonDemoFiles } from "./demoPythonProject";
import { goDemoFiles, isGoDemo } from "./demoGoProject";
import { isJavaDemo, JAVA_PKG, javaDemoFiles } from "./demoJavaProject";
import { demoStatus } from "./demoStatus";
import { CONFLICT_BRANCH, demoConflicts, type DemoConflict } from "./demoConflicts";
import { callWasm, forgeWasm } from "@/wasm";
import type { ConflictSet, MergeFile, MergeModel, SideChange } from "@/features/git/api";


const DAY = 86400;
const now = Math.floor(Date.now() / 1000);

const people = {
  marlon: ["Marlon", "marlon@acme.dev"],
  anna: ["Anna Weber", "anna.weber@acme.dev"],
  jonas: ["Jonas Klein", "jonas@acme.dev"],
  sofia: ["Sofia Rossi", "sofia.rossi@acme.dev"],
} as const;
type Person = keyof typeof people;

/** Deterministic fake sha from a seed. */
export function fakeSha(seed: string): string {
  let h1 = 0x811c9dc5;
  let out = "";
  for (let round = 0; out.length < 40; round++) {
    for (const ch of seed + round) h1 = Math.imul(h1 ^ ch.charCodeAt(0), 16777619) >>> 0;
    out += h1.toString(16).padStart(8, "0");
  }
  return out.slice(0, 40);
}

interface MockCommit extends Omit<Commit, "refs"> {
  body?: string;
  files: ChangedFile[];
}

const f = (status: string, ...paths: string[]): ChangedFile[] => paths.map((path) => ({ path, origPath: null, status }));

// Newest first: [id, parents, who, age in days, subject, files, body?]
const history: [string, string[], Person, number, string, ChangedFile[], string?][] = [
  ["c1", ["c2"], "marlon", 2.1, "Add order total calculation to Checkout", f("M", "app/Services/Checkout.php", "app/Models/Order.php")],
  ["c2", ["c4"], "marlon", 2.9, "Cast email_verified_at to datetime", f("M", "app/Models/User.php"), "Laravel 12 no longer casts it implicitly, which broke the\nverification banner for customers created via the API."],
  ["c3", ["c5"], "anna", 3.4, "Extract Checkout service from OrderController", f("M", "app/Http/Controllers/OrderController.php")],
  ["c4", ["c6"], "marlon", 4.2, "Introduce OrderStatus enum", [...f("A", "app/Models/OrderStatus.php"), ...f("M", "app/Models/Order.php")]],
  ["c5", ["c7"], "anna", 5.1, "Validate cart before checkout", f("M", "routes/web.php")],
  ["c6", ["c8", "c9"], "jonas", 6.0, "Merge branch 'bugfix/login-redirect'", f("M", "routes/web.php")],
  ["c7", ["c8"], "anna", 6.4, "Add checkout route", f("M", "routes/web.php")],
  ["c8", ["c10"], "sofia", 8.3, "Add orders relation to User", f("M", "app/Models/User.php")],
  ["c9", ["c10"], "jonas", 9.0, "Fix redirect after login for guests", f("M", "routes/web.php")],
  ["c10", ["c11", "c12"], "jonas", 12.2, "Merge pull request #42 from acme/feature/orders-table", f("A", "database/migrations/2026_01_01_000000_create_orders_table.php")],
  ["c11", ["c13"], "sofia", 13.0, "Bump laravel/framework to 12.3", f("M", "composer.json")],
  ["c12", ["c13"], "anna", 13.5, "Add orders table migration", f("A", "database/migrations/2026_01_01_000000_create_orders_table.php")],
  ["c13", ["c14"], "marlon", 15.1, "Add OrderController show action", f("A", "app/Http/Controllers/OrderController.php")],
  ["c14", ["c15"], "sofia", 17.8, "Configure Doctrine DBAL", f("M", "composer.json")],
  ["c15", ["c16"], "anna", 19.2, "Add Order model", f("A", "app/Models/Order.php")],
  ["c16", ["c17"], "jonas", 24.0, "Prepare 1.0.0 release", f("M", "README.md", "composer.json")],
  ["c17", ["c18"], "marlon", 26.5, "Add feature test scaffolding", f("A", "phpunit.xml", "tests/TestCase.php")],
  ["c18", ["c19", "c20"], "marlon", 29.0, "Merge branch 'feature/auth'", f("M", "app/Models/User.php")],
  ["c19", ["c21"], "marlon", 30.2, "Hide password and remember token in User", f("M", "app/Models/User.php")],
  ["c20", ["c21"], "sofia", 31.0, "Add authentication scaffolding", f("M", "app/Models/User.php", "app/Http/Controllers/Controller.php")],
  ["c21", ["c22"], "anna", 33.4, "Add welcome view", f("A", "resources/views/welcome.blade.php")],
  ["c22", ["c23"], "jonas", 36.0, "Add web routes", f("A", "routes/web.php")],
  ["c23", ["c24"], "marlon", 41.0, "Add User model", f("A", "app/Models/User.php")],
  ["c24", ["c25"], "marlon", 44.0, "Add composer.json and artisan", f("A", "composer.json", "artisan", ".env.example")],
  ["c25", [], "marlon", 45.0, "Initial commit", f("A", "README.md")],
];

const shaOf = (id: string) => fakeSha(`acme-shop/${id}`);
const commits: MockCommit[] = history.map(([id, parents, who, age, subject, files, body]) => ({
  sha: shaOf(id),
  parents: parents.map(shaOf),
  author: people[who][0],
  email: people[who][1],
  time: now - Math.round(age * DAY) + 3600 * (id.length % 3),
  subject,
  body,
  files,
}));
const bySha = new Map(commits.map((c) => [c.sha, c]));

// The other demos (`?project=lua|react|…`) are on their own branches (demoStatus.ts).
const demo = demoStatus();
const laravelBranches = demo.branch === "feature/order-totals";
const otherBranches = (shas: string[]) =>
  laravelBranches
    ? [
        ["main", shaOf("c6")],
        ["feature/checkout-flow", shaOf("c3")],
        ["bugfix/login-redirect", shaOf("c9")],
      ]
    : demo.branches.map((b, i): [string, string] => [b, shaOf(shas[i % shas.length])]);

const state = {
  current: demo.branch as string | null,
  head: shaOf("c1"),
  local: new Map<string, string>([[demo.branch, shaOf("c1")], ...(otherBranches(["c6", "c3", "c9"]) as [string, string][])]),
  upstream: new Map<string, string>([
    [demo.branch, `origin/${demo.branch}`],
    ...(laravelBranches
      ? ([
          ["main", "origin/main"],
          ["feature/checkout-flow", "origin/feature/checkout-flow"],
        ] as [string, string][])
      : demo.branches.filter((b) => b === "main").map((b): [string, string] => [b, `origin/${b}`])),
  ]),
  remote: new Map<string, string>([
    ["origin/main", shaOf("c6")],
    [`origin/${demo.branch}`, shaOf("c2")],
    ...(laravelBranches
      ? ([
          ["origin/feature/checkout-flow", shaOf("c3")],
          ["origin/release/1.1", shaOf("c10")],
        ] as [string, string][])
      : []),
  ]),
  tags: new Map<string, string>([
    ["v1.1.0", shaOf("c10")],
    ["v1.0.0", shaOf("c16")],
  ]),
  recent: laravelBranches ? ["main", "feature/checkout-flow", "bugfix/login-redirect"] : demo.branches,
  stashes: [
    { branch: "main", message: "Half-done invoice PDF layout", time: now - 3 * DAY, files: {} as Record<string, string> },
  ],
  remotes: [{ name: "origin", fetchUrl: "git@github.com:acme/shop.git", pushUrl: null as string | null }],
  /** Worktrees (`git worktree list`); the first is the main one, where the demo project is. */
  worktrees: [
    { path: "/home/marlon/src/acme-shop", branch: demo.branch as string | null, locked: false, lockReason: null as string | null, prunable: false },
    ...(laravelBranches
      ? [
          { path: "/home/marlon/src/acme-shop-checkout-flow", branch: "feature/checkout-flow" as string | null, locked: false, lockReason: null as string | null, prunable: false },
          { path: "/media/usb/acme-shop-hotfix", branch: null as string | null, locked: true, lockReason: "on a USB drive" as string | null, prunable: false },
        ]
      : []),
  ],
};

/** Signatures of the demo history (`%G?`, signer). */
const signatures: Record<string, [Commit["signature"], string]> = {
  c1: ["good", "Marlon <marlon@acme.dev>"],
  c2: ["good", "Marlon <marlon@acme.dev>"],
  c4: ["unverified", "Marlon <marlon@acme.dev>"],
  c10: ["good", "GitHub <noreply@github.com>"],
  c13: ["good", "Marlon <marlon@acme.dev>"],
};
const signatureOf = (sha: string) => {
  const id = history.find(([hid]) => shaOf(hid) === sha)?.[0];
  return id ? signatures[id] : undefined;
};
/** Mark a commit made from Forge with signing on as signed. */
const signedCommits = new Set<string>();
export function signMockCommit(sha: string): void {
  signedCommits.add(sha);
}

/** Paths added with "Add to .gitignore"; the cached demo ignore rules do not know them. */
const ignoredFromMenu: string[] = [];

// ---- file snapshots -------------------------------------------------------------------

const replaceOnce = (s: string, from: string, to: string) => {
  if (!s.includes(from)) throw new Error(`gitMock: snippet not found: ${from.slice(0, 40)}`);
  return s.replace(from, to);
};

const userWorking = demoFiles["app/Models/User.php"];
const totalSpent = userWorking.slice(
  userWorking.indexOf("    public function totalSpent()"),
  userWorking.indexOf("    protected function casts()"),
);
const userHead = replaceOnce(
  replaceOnce(
    replaceOnce(userWorking, totalSpent, ""),
    "protected $fillable = ['name', 'email', 'password'];",
    "protected $fillable = ['name', 'email'];",
  ),
  "            'password' => 'hashed',\n",
  "            'password' => 'hashed',\n            'is_admin' => 'boolean',\n",
);

const controllerWorking = demoFiles["app/Http/Controllers/OrderController.php"];
const controllerHead = replaceOnce(controllerWorking, "        $this->authorize('view', $order);\n\n", "");
const routesHead = replaceOnce(
  demoFiles["routes/web.php"],
  "Route::get('/orders/{order}', [OrderController::class, 'show'])->name('orders.show');\n",
  "Route::get('/orders/{id}', [OrderController::class, 'show']);\n",
);

const DELETED = "app/Http/Middleware/RedirectIfGuest.php";
const UNTRACKED_EXTRA = "tests/Unit/CheckoutTest.php";

/** Ignored by `.gitignore` but committed with `git add -f`. */
const FORCE_ADDED = new Set(["forced.log"]);
const ignores = createIgnoreMatcher((dir) => demoFiles[dir ? `${dir}/.gitignore` : ".gitignore"]);
const isUntrackedIgnored = (path: string) => !FORCE_ADDED.has(path) && ignores.isIgnored(path, false);

// HEAD holds what a real repository would: ignored files (.env, vendor/, logs) are not in it.
const head: Record<string, string> = Object.fromEntries(
  Object.entries(demoFiles).filter(([path]) => !isUntrackedIgnored(path)),
);
delete head["app/Services/Checkout.php"];
delete head["tests/Feature/OrderTest.php"];
head["app/Models/User.php"] = userHead;
head["app/Http/Controllers/OrderController.php"] = controllerHead;
head["routes/web.php"] = routesHead;
head[DELETED] = `<?php

namespace App\\Http\\Middleware;

use Closure;
use Illuminate\\Http\\Request;

class RedirectIfGuest
{
    public function handle(Request $request, Closure $next)
    {
        return $request->user() ? $next($request) : redirect('/login');
    }
}
`;

/** Index snapshot: HEAD plus staged changes. */
const index: Record<string, string> = { ...head };
index["app/Services/Checkout.php"] = demoFiles["app/Services/Checkout.php"];
index["app/Http/Controllers/OrderController.php"] = controllerWorking;

// The WebForge demos (`?project=react|vue|angular`) are committed as they are.
const web = webDemo();
if (web || isPythonDemo() || isGoDemo()) {
  for (const snapshot of [head, index]) {
    for (const k of Object.keys(snapshot)) delete snapshot[k];
    Object.assign(snapshot, web ? web.files : isGoDemo() ? goDemoFiles : pythonDemoFiles);
  }
  // The Python demo has one change: the new import in main.py.
  if (!web && isPythonDemo()) {
    const main = "src/shop_api/main.py";
    for (const snapshot of [head, index]) snapshot[main] = pythonDemoFiles[main].replace("from rich.console import Console\n", "").replace("console = Console()\n\n\n", "");
  }
}
// The Java demo (`?project=java`) is committed except for the low-stock report in progress.
if (isJavaDemo()) {
  for (const snapshot of [head, index]) {
    for (const k of Object.keys(snapshot)) delete snapshot[k];
    Object.assign(snapshot, javaDemoFiles);
  }
  head[`${JAVA_PKG}/Inventory.java`] = javaDemoFiles[`${JAVA_PKG}/Inventory.java`].replace(/\n    public List<StockItem> lowStock[\s\S]*?\n    }\n/, "\n");
}

// Extra untracked file, written into the workspace mock (absent in tests that load only
// the git mock). The web demos and the Lua demo game have no PHP tests.
if (!web && !isLuaDemo() && !isPythonDemo() && !isGoDemo() && !isJavaDemo()) void Promise.resolve().then(() =>
  invoke("fs_write_file", {
    path: UNTRACKED_EXTRA,
    contents: `<?php\n\nnamespace Tests\\Unit;\n\nuse App\\Services\\Checkout;\nuse PHPUnit\\Framework\\TestCase;\n\nclass CheckoutTest extends TestCase\n{\n    public function test_default_currency_is_euro(): void\n    {\n        $this->assertInstanceOf(Checkout::class, new Checkout());\n    }\n}\n`,
  }),
).catch(() => {});

async function listAll(dir = ""): Promise<string[]> {
  const entries = await invoke<FileEntry[]>("fs_list_dir", { path: dir });
  const out: string[] = [];
  for (const e of entries) {
    if (e.kind === "dir") out.push(...(await listAll(e.path)));
    else out.push(e.path);
  }
  return out;
}

async function working(): Promise<Record<string, string>> {
  const files: Record<string, string> = {};
  for (const p of await listAll()) files[p] = await invoke<string>("fs_read_file", { path: p });
  return files;
}

const readWorking = async (path: string): Promise<string | null> => {
  try {
    return await invoke<string>("fs_read_file", { path });
  } catch {
    return null;
  }
};

function sideState(from: string | undefined, to: string | undefined): FileState {
  if (from === to) return "unmodified";
  if (from === undefined) return "added";
  if (to === undefined) return "deleted";
  return "modified";
}

async function computeStatus(): Promise<RepoStatus> {
  const wt = await working();
  const paths = new Set([...Object.keys(head), ...Object.keys(index), ...Object.keys(wt), ...merging.conflicts.keys()]);
  const entries: StatusEntry[] = [];
  for (const path of [...paths].sort()) {
    if (merging.conflicts.has(path)) {
      entries.push({ path, origPath: null, index: "conflicted", worktree: "conflicted", conflict: true });
      continue;
    }
    const h = head[path];
    const i = index[path];
    const w = wt[path];
    if (i === undefined && h === undefined) {
      if (w !== undefined && !ignoredFromMenu.some((p) => path === p || path.startsWith(`${p}/`)))
        entries.push({ path, origPath: null, index: "untracked", worktree: "untracked", conflict: false });
      continue;
    }
    const idx = sideState(h, i);
    const tree = i === undefined ? "unmodified" : sideState(i, w);
    if (idx !== "unmodified" || tree !== "unmodified") entries.push({ path, origPath: null, index: idx, worktree: tree, conflict: false });
  }
  const up = state.current ? state.upstream.get(state.current) : undefined;
  const upSha = up ? state.remote.get(up) : undefined;
  const { ahead, behind } = upSha ? aheadBehind(state.head, upSha) : { ahead: 0, behind: 0 };
  return { branch: state.current, head: state.head, upstream: up ?? null, ahead, behind, entries };
}

function ancestors(sha: string): Set<string> {
  const seen = new Set<string>();
  const stack = [sha];
  while (stack.length) {
    const s = stack.pop()!;
    if (seen.has(s)) continue;
    seen.add(s);
    stack.push(...(bySha.get(s)?.parents ?? []));
  }
  return seen;
}

function aheadBehind(a: string, b: string) {
  const A = ancestors(a);
  const B = ancestors(b);
  return { ahead: [...A].filter((s) => !B.has(s)).length, behind: [...B].filter((s) => !A.has(s)).length };
}

function changed(): void {
  emitMock("git://changed", null);
}

// ---- refs and log ---------------------------------------------------------------------

function refsFor(sha: string): RefLabel[] {
  const refs: RefLabel[] = [];
  if (state.current === null && state.head === sha) refs.push({ name: "HEAD", kind: "head", current: true });
  for (const [name, s] of state.local) if (s === sha) refs.push({ name, kind: "local", current: name === state.current });
  for (const [name, s] of state.remote) if (s === sha) refs.push({ name, kind: "remote", current: false });
  for (const [name, s] of state.tags) if (s === sha) refs.push({ name, kind: "tag", current: false });
  return refs.sort((a, b) => Number(b.current) - Number(a.current));
}

function resolveRev(rev: string): string | undefined {
  if (rev === "HEAD") return state.head;
  return (
    state.local.get(rev) ??
    state.remote.get(rev) ??
    state.tags.get(rev) ??
    commits.find((c) => c.sha.startsWith(rev) && rev.length >= 4)?.sha
  );
}

function log(q: LogQuery): Commit[] {
  let list = commits.slice().sort((a, b) => b.time - a.time);
  // `a..b`: the commits of b that are not in a (Compare with Current).
  const range = q.rev?.includes("..") ? q.rev.split("..") : null;
  const tips = range ? [resolveRev(range[1] || "HEAD")] : q.rev ? [resolveRev(q.rev)] : q.all ? [state.head, ...state.local.values(), ...state.remote.values(), ...state.tags.values()] : [state.head];
  const reachable = new Set<string>();
  for (const t of tips) if (t) ancestors(t).forEach((s) => reachable.add(s));
  list = list.filter((c) => reachable.has(c.sha));
  if (range) {
    const excluded = ancestors(resolveRev(range[0] || "HEAD") ?? "");
    list = list.filter((c) => !excluded.has(c.sha));
  }
  if (q.text) {
    const t = q.text.toLowerCase();
    list = list.filter((c) => c.subject.toLowerCase().includes(t) || (c.body ?? "").toLowerCase().includes(t));
  }
  if (q.author) list = list.filter((c) => c.author.toLowerCase().includes(q.author!.toLowerCase()));
  if (q.path) list = list.filter((c) => c.files.some((file) => file.path === q.path || file.path.startsWith(`${q.path}/`)));
  const skip = q.skip ?? 0;
  return list.slice(skip, skip + (q.limit || 200)).map(({ body: _b, files: _f, ...c }) => {
    const sig = signedCommits.has(c.sha) ? (["good", ""] as const) : signatureOf(c.sha);
    return { ...c, refs: refsFor(c.sha), ...(sig ? { signature: sig[0] } : {}) };
  });
}

function branches(): Branches {
  const info = (name: string, sha: string, upstream?: string): BranchInfo => {
    const c = bySha.get(sha);
    const ab = upstream && state.remote.get(upstream) ? aheadBehind(sha, state.remote.get(upstream)!) : { ahead: 0, behind: 0 };
    const worktree = name !== state.current ? (state.worktrees.find((w, i) => i > 0 && w.branch === name)?.path ?? null) : null;
    return { name, sha, upstream: upstream ?? null, ...ab, time: c?.time ?? now, subject: c?.subject ?? "", current: name === state.current, worktree };
  };
  return {
    current: state.current,
    local: [...state.local].map(([n, s]) => info(n, s, state.upstream.get(n))),
    remote: [...state.remote].map(([n, s]) => info(n, s)),
    tags: [...state.tags].map(([n, s]) => info(n, s)),
    recent: state.recent.filter((r) => r !== state.current && state.local.has(r)).slice(0, 5),
  };
}

function checkout(name: string): void {
  const prev = state.current;
  if (state.local.has(name)) {
    state.current = name;
    state.head = state.local.get(name)!;
  } else {
    const sha = resolveRev(name);
    if (!sha) throw new Error(`git checkout: pathspec '${name}' did not match any file(s) known to git`);
    state.current = null;
    state.head = sha;
  }
  if (prev) state.recent = [prev, ...state.recent.filter((r) => r !== prev)];
}

function newCommit(subject: string, body: string | undefined, parents: string[], files: ChangedFile[]): string {
  const sha = fakeSha(`${subject}/${Date.now()}/${Math.random()}`);
  const c: MockCommit = { sha, parents, author: people.marlon[0], email: people.marlon[1], time: Math.floor(Date.now() / 1000), subject, body, files };
  commits.unshift(c);
  bySha.set(sha, c);
  state.head = sha;
  if (state.current) state.local.set(state.current, sha);
  return sha;
}

// ---- blame ----------------------------------------------------------------------------

/** Commit per HEAD line of User.php (1-based inclusive ranges); others get a stable spread. */
const userBlame: [number, number, string][] = [
  [1, 7, "c23"],
  [8, 8, "c8"],
  [9, 10, "c20"],
  [11, 16, "c23"],
  [17, 17, "c8"],
  [18, 20, "c23"],
  [21, 21, "c20"],
  [22, 23, "c23"],
  [24, 24, "c19"],
  [25, 25, "c23"],
  [26, 26, "c19"],
  [27, 27, "c23"],
  [28, 31, "c8"],
  [32, 32, "c8"],
  [33, 40, "c2"],
  [41, 41, "c23"],
];

function headLineCommit(path: string, line: number): string {
  if (path === "app/Models/User.php") return shaOf(userBlame.find(([a, b]) => line >= a && line <= b)?.[2] ?? "c23");
  const touching = commits.filter((c) => c.files.some((file) => file.path === path));
  const pool = touching.length ? touching : commits.slice(-3);
  // Chunks of ~6 lines; the oldest commit owns the first chunk.
  const chunk = Math.floor((line - 1) / 6);
  return pool[pool.length - 1 - (chunk % pool.length)].sha;
}

async function blame(path: string, contents?: string | null): Promise<Blame> {
  const base = head[path];
  const text = contents ?? (await readWorking(path)) ?? "";
  if (base === undefined) throw new Error(`git blame: no such path '${path}' in HEAD`);
  const commitIndex = new Map<string, number>();
  const out: Blame = { commits: [], lines: [] };
  const add = (c: BlameCommit) => {
    if (!commitIndex.has(c.sha)) {
      commitIndex.set(c.sha, out.commits.length);
      out.commits.push(c);
    }
    return commitIndex.get(c.sha)!;
  };
  const uncommitted = (): BlameCommit => ({
    sha: "0".repeat(40),
    author: "Not Committed Yet",
    email: "not.committed.yet",
    time: Math.floor(Date.now() / 1000),
    summary: `Version of ${path} from ${path}`,
    uncommitted: true,
  });
  for (const op of lineDiff(base, text)) {
    if (op.kind === "del") continue;
    if (op.kind === "add") {
      out.lines.push(add(uncommitted()));
      continue;
    }
    const c = bySha.get(headLineCommit(path, op.a + 1))!;
    out.lines.push(add({ sha: c.sha, author: c.author, email: c.email, time: c.time, summary: c.subject, uncommitted: false }));
  }
  return out;
}

// ---- AI commit message ----------------------------------------------------------------

function mockCommitMessage(content: string): string {
  if (content.includes("totalSpent") || content.includes("app/Models/User.php"))
    return "Add totalSpent helper and tighten User attributes\n\nSum paid orders in User::totalSpent() for the account page.\nAllow mass-assigning the password, drop the is_admin cast\nand authorize order access in OrderController::show.";
  if (content.includes("Checkout")) return "Add Checkout service with currency and tax rate";
  return "Update project files";
}

// ---- handlers -------------------------------------------------------------------------

const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));
const paths = (a: Record<string, unknown>) => (a.paths as string[] | null | undefined) ?? [];

registerMock("git_info", async () => ({ root: "~/Projects/acme-shop", branch: state.current, head: state.head }));
registerMock("git_status", () => computeStatus());
registerMock("git_show", async ({ rev, path }) => {
  const p = String(path);
  if (rev === "") return index[p] ?? null;
  if (rev === "HEAD" || resolveRev(String(rev)) === state.head) return head[p] ?? null;
  return revisionContent(String(rev), p);
});
registerMock("git_diff", async (a) => {
  const wt = await working();
  const list = paths(a).length ? paths(a) : [...new Set([...Object.keys(head), ...Object.keys(index)])].sort();
  return list
    .map((p) => (a.staged ? unifiedDiff(p, head[p] ?? null, index[p] ?? null) : index[p] === undefined ? "" : unifiedDiff(p, index[p] ?? null, wt[p] ?? null)))
    .join("");
});
registerMock("git_diff_head", async (a) => {
  const wt = await working();
  const list = paths(a).length ? paths(a) : [...new Set([...Object.keys(head), ...Object.keys(index)])].sort();
  return list.map((p) => unifiedDiff(p, head[p] ?? null, index[p] === undefined && head[p] === undefined ? null : wt[p] ?? null)).join("");
});
registerMock("git_blame", ({ path, contents }) => blame(String(path), contents as string | null));
registerMock("git_log", ({ query }) => log((query ?? {}) as LogQuery));
registerMock("git_commit_details", ({ sha }): CommitDetails => {
  const c = bySha.get(String(sha));
  if (!c) throw new Error(`git show: bad object ${sha}`);
  return {
    sha: c.sha,
    parents: c.parents,
    author: c.author,
    email: c.email,
    time: c.time,
    committer: c.author,
    commitTime: c.time,
    message: c.body ? `${c.subject}\n\n${c.body}` : c.subject,
    files: c.files,
    ...(() => {
      const sig = signedCommits.has(c.sha) ? (["good", "Marlon <marlon@acme.dev>"] as const) : signatureOf(c.sha);
      return sig ? { signature: sig[0], signer: sig[1], signingKey: sig[0] === "good" ? "3AA5C34371567BD2" : null } : { signature: "none" as const, signer: null, signingKey: null };
    })(),
  };
});
registerMock("git_branches", () => branches());
registerMock("git_checkout", async ({ rev, remote }) => {
  await delay(150);
  let name = String(rev);
  if (remote) {
    const local = name.split("/").slice(1).join("/");
    if (!state.local.has(local)) {
      state.local.set(local, state.remote.get(name)!);
      state.upstream.set(local, name);
    }
    name = local;
  }
  checkout(name);
  changed();
});
registerMock("git_create_branch", ({ name, start, checkout: co }) => {
  const n = String(name);
  if (state.local.has(n)) throw new Error(`git branch: a branch named '${n}' already exists`);
  const sha = start ? resolveRev(String(start)) : state.head;
  if (!sha) throw new Error(`git branch: not a valid object name: '${start}'`);
  state.local.set(n, sha);
  if (co) checkout(n);
  changed();
});
registerMock("git_delete_branch", ({ name }) => {
  const n = String(name);
  if (n === state.current) throw new Error(`git branch: cannot delete branch '${n}' checked out`);
  state.local.delete(n);
  state.upstream.delete(n);
  changed();
});
registerMock("git_merge", async ({ rev }) => {
  if (String(rev) === CONFLICT_BRANCH || String(rev) === `origin/${CONFLICT_BRANCH}`) {
    await startConflictMerge(String(rev));
    changed();
    throw new Error(
      `git merge: Auto-merging app/Services/PriceCalculator.php\nCONFLICT (content): Merge conflict in app/Services/PriceCalculator.php\nCONFLICT (modify/delete): app/Reports/SalesReport.php deleted in ${rev} and modified in HEAD.\nCONFLICT (binary): Merge conflict in public/images/logo.png\nAutomatic merge failed; fix conflicts and then commit the result.`,
    );
  }
  const sha = resolveRev(String(rev));
  if (!sha) throw new Error(`git merge: ${rev} - not something we can merge`);
  if (ancestors(state.head).has(sha)) return "Already up to date.";
  newCommit(`Merge branch '${rev}'${state.current ? ` into ${state.current}` : ""}`, undefined, [state.head, sha], bySha.get(sha)?.files ?? []);
  changed();
  return "Merge made by the 'ort' strategy.";
});
registerMock("git_rebase", async ({ onto }) => {
  await delay(300);
  changed();
  return `Successfully rebased and updated refs/heads/${state.current} onto ${onto}.`;
});
registerMock("git_stage", async (a) => {
  for (const p of paths(a)) {
    merging.conflicts.delete(p);
    const w = await readWorking(p);
    if (w === null) delete index[p];
    else index[p] = w;
  }
  changed();
});
registerMock("git_unstage", (a) => {
  for (const p of paths(a)) {
    if (head[p] === undefined) delete index[p];
    else index[p] = head[p];
  }
  changed();
});
registerMock("git_rollback", async (a) => {
  for (const p of paths(a)) {
    if (head[p] !== undefined) {
      index[p] = head[p];
      await invoke("fs_write_file", { path: p, contents: head[p] });
    } else delete index[p];
  }
  changed();
});
registerMock("git_commit", async ({ message, amend, paths: ps }) => {
  await delay(250);
  const msg = String(message ?? "").trim();
  if (!msg && !amend) throw new Error("git commit: commit message is empty");
  if (merging.conflicts.size) throw new Error("git commit: Committing is not possible because you have unmerged files.");
  const st = await computeStatus();
  const selected = (ps as string[] | null) ?? st.entries.filter((e) => e.index !== "unmodified" && e.index !== "untracked").map((e) => e.path);
  if (!selected.length && !amend) throw new Error("git commit: nothing to commit");
  const files: ChangedFile[] = [];
  for (const p of selected) {
    const w = await readWorking(p);
    files.push({ path: p, origPath: null, status: head[p] === undefined ? "A" : w === null ? "D" : "M" });
    if (w === null) {
      delete head[p];
      delete index[p];
    } else {
      head[p] = w;
      index[p] = w;
    }
  }
  const [subject, ...rest] = msg.split("\n");
  const body = rest.join("\n").trim() || undefined;
  let sha: string;
  if (amend) {
    const prev = bySha.get(state.head)!;
    commits.splice(commits.indexOf(prev), 1);
    sha = newCommit(subject || prev.subject, body ?? prev.body, prev.parents, [...prev.files, ...files]);
  } else sha = newCommit(subject, body, merging.theirs ? [state.head, merging.theirsSha] : [state.head], files);
  merging.theirs = null;
  changed();
  return sha;
});
registerMock("git_push", async () => {
  await delay(700);
  if (!state.current) throw new Error("git push: cannot push a detached HEAD");
  const up = state.upstream.get(state.current) ?? `origin/${state.current}`;
  state.upstream.set(state.current, up);
  state.remote.set(up, state.head);
  changed();
  return { output: `To github.com:acme/shop.git\n   ${state.head.slice(0, 7)}  ${state.current} -> ${state.current}` };
});
registerMock("git_pull", async () => {
  await delay(600);
  changed();
  return { output: "Already up to date." };
});
registerMock("git_fetch", async () => {
  await delay(500);
  changed();
  return { output: "" };
});
// The demo repository has no core.autocrlf: files read back with CRLF are reported.
registerMock("git_crlf_files", async ({ paths }) => {
  const out: string[] = [];
  for (const p of paths as string[]) {
    const file = await invoke<{ eol: string }>("fs_read_text", { path: p }).catch(() => null);
    if (file?.eol === "\r\n") out.push(p);
  }
  return out;
});

registerMock("git_add_to_gitignore", async (a) => {
  const patterns: string[] = [];
  for (const raw of paths(a)) {
    const p = raw.replace(/^\/+|\/+$/g, "");
    if (!p || p.split("/").some((s) => s === ".." || s === ".")) throw new Error(`invalid path: "${raw}"`);
    const isDir = (await invoke<FileEntry[]>("fs_list_dir", { path: p }).catch(() => [])).length > 0;
    patterns.push(`/${p}${isDir ? "/" : ""}`);
    if (!ignoredFromMenu.includes(p)) ignoredFromMenu.push(p);
  }
  const existing = (await readWorking(".gitignore")) ?? "";
  const missing = [...new Set(patterns)].filter((p) => !existing.split("\n").includes(p));
  if (missing.length) {
    const sep = existing && !existing.endsWith("\n") ? "\n" : "";
    await invoke("fs_write_file", { path: ".gitignore", contents: `${existing}${sep}${missing.join("\n")}\n` });
  }
  changed();
  return patterns;
});
registerMock("git_create_tag", ({ name, rev }) => {
  const n = String(name);
  if (!n || n.startsWith("-")) throw new Error(`invalid revision: "${n}"`);
  if (state.tags.has(n)) throw new Error(`git tag: tag '${n}' already exists`);
  const sha = rev ? resolveRev(String(rev)) : state.head;
  if (!sha) throw new Error(`git tag: Failed to resolve '${rev}' as a valid ref.`);
  state.tags.set(n, sha);
  changed();
});
registerMock("git_stash", async ({ message }) => {
  const st = await computeStatus();
  const tracked = st.entries.filter((e) => e.worktree !== "untracked");
  if (!tracked.length) return "No local changes to save";
  const files: Record<string, string> = {};
  for (const e of tracked) {
    const w = await readWorking(e.path);
    if (w !== null) files[e.path] = w;
    const base = index[e.path] ?? head[e.path];
    if (base !== undefined) await invoke("fs_write_file", { path: e.path, contents: base });
  }
  const msg = String(message ?? "").trim() || `${state.head.slice(0, 7)} ${bySha.get(state.head)?.subject ?? ""}`;
  state.stashes.unshift({ branch: state.current ?? "", message: msg, time: Math.floor(Date.now() / 1000), files });
  changed();
  return `Saved working directory and index state On ${state.current}: ${msg}`;
});
registerMock("git_stash_list", () => state.stashes.map((s, index) => ({ index, branch: s.branch || null, message: s.message, time: s.time })));
registerMock("git_unstash", async ({ index: i, pop }) => {
  const s = state.stashes[Number(i)];
  if (!s) throw new Error(`git stash: stash@{${i}} is not a valid reference`);
  for (const [path, contents] of Object.entries(s.files)) await invoke("fs_write_file", { path, contents });
  if (pop) state.stashes.splice(Number(i), 1);
  changed();
  return pop ? `Dropped refs/stash@{${i}}` : "";
});
registerMock("git_drop_stash", ({ index: i }) => {
  state.stashes.splice(Number(i), 1);
  changed();
});
registerMock("git_remotes", () => state.remotes.map((r) => ({ ...r })));
registerMock("git_add_remote", ({ name, url }) => {
  const n = String(name);
  if (state.remotes.some((r) => r.name === n)) throw new Error(`git remote: remote ${n} already exists.`);
  state.remotes.push({ name: n, fetchUrl: String(url), pushUrl: null });
  changed();
});
registerMock("git_remove_remote", ({ name }) => {
  const at = state.remotes.findIndex((r) => r.name === name);
  if (at < 0) throw new Error(`git remote: No such remote: '${name}'`);
  state.remotes.splice(at, 1);
  changed();
});

// ---- merge conflicts (demoConflicts.ts) ------------------------------------------------

const merging = {
  /** The branch being merged, while a merge is in progress. */
  theirs: null as string | null,
  theirsSha: "",
  conflicts: new Map<string, DemoConflict>(),
};

const mergeModel = async (c: DemoConflict) =>
  callWasm<MergeModel>((await forgeWasm()).vcs_merge, { base: c.base ?? "", left: c.ours ?? "", right: c.theirs ?? "" });

/** What git leaves in the working tree: merged text with conflict markers. */
async function withMarkers(c: DemoConflict, theirs: string): Promise<string> {
  const model = await mergeModel(c);
  const lines = (c.base ?? "").split(/(?<=\n)/);
  let out = "";
  let pos = 0;
  for (const ch of model.chunks) {
    out += lines.slice(pos, ch.base.start).join("");
    if (ch.kind === "conflict") out += `<<<<<<< HEAD\n${ch.leftText}=======\n${ch.rightText}>>>>>>> ${theirs}\n`;
    else out += ch.kind === "right" ? ch.rightText : ch.leftText;
    pos = ch.base.end;
  }
  return out + lines.slice(pos).join("");
}

async function startConflictMerge(rev: string): Promise<void> {
  merging.theirs = rev.replace(/^origin\//, "");
  merging.theirsSha = resolveRev(rev) ?? shaOf("c3");
  for (const [path, c] of Object.entries(demoConflicts())) {
    merging.conflicts.set(path, c);
    if (c.binary) continue;
    const text = c.ours !== null && c.theirs !== null ? await withMarkers(c, merging.theirs) : (c.ours ?? c.theirs ?? "");
    await invoke("fs_write_file", { path, contents: text });
  }
}

// `?conflicts` opens the demo in the middle of that merge (screenshots).
if (!web && !isLuaDemo() && !isPythonDemo() && !isGoDemo() && !isJavaDemo() && typeof location !== "undefined" && new URLSearchParams(location.search).has("conflicts")) {
  void Promise.resolve()
    .then(() => startConflictMerge(CONFLICT_BRANCH))
    .then(changed)
    .catch(() => {});
}

const sideChange = (base: string | null, side: string | null): SideChange => (base === null ? "added" : side === null ? "deleted" : "modified");

registerMock("git_conflicts", (): ConflictSet => {
  const current = state.current ?? state.head.slice(0, 7);
  const theirs = merging.theirs ?? "stash";
  return {
    operation: merging.theirs ? "merge" : "unstash",
    description: merging.theirs ? `Merging branch ${theirs} into branch ${current}` : `Unstashing changes into branch ${current}`,
    yoursName: current,
    theirsName: theirs,
    yoursTitle: `Changes from ${current}`,
    theirsTitle: `Changes from ${theirs}`,
    reversed: false,
    files: [...merging.conflicts].map(([path, c]) => ({ path, yours: sideChange(c.base, c.ours), theirs: sideChange(c.base, c.theirs) })),
  };
});
registerMock("git_merge_file", async ({ path }): Promise<MergeFile> => {
  const p = String(path);
  const c = merging.conflicts.get(p);
  if (!c) throw new Error(`${p} has no merge conflict`);
  const current = state.current ?? state.head.slice(0, 7);
  return {
    path: p,
    yoursTitle: `Changes from ${current}`,
    theirsTitle: `Changes from ${merging.theirs ?? "stash"}`,
    base: c.base,
    yours: c.ours,
    theirs: c.theirs,
    binary: !!c.binary,
    crlf: false,
    model: c.binary || c.ours === null || c.theirs === null ? null : await mergeModel(c),
  };
});
registerMock("git_accept_conflict_side", async ({ paths: ps, side }) => {
  for (const p of ps as string[]) {
    const c = merging.conflicts.get(p);
    if (!c) continue;
    merging.conflicts.delete(p);
    if (c.binary) continue;
    const text = side === "yours" ? c.ours : c.theirs;
    if (text === null) {
      await invoke("fs_delete", { path: p });
      delete index[p];
    } else {
      await invoke("fs_write_file", { path: p, contents: text });
      index[p] = text;
    }
  }
  changed();
});
registerMock("git_resolve_merge", async ({ path, text }) => {
  const p = String(path);
  await invoke("fs_write_file", { path: p, contents: String(text) });
  index[p] = String(text);
  merging.conflicts.delete(p);
  changed();
});

registerMock("ai_complete", async ({ request }) => {
  const req = request as { messages?: { content: string }[] };
  await delay(900);
  return mockCommitMessage(req.messages?.map((m) => m.content).join("\n") ?? "");
});

/** Paths git tracks in the demo repository (HEAD plus the index), for the file tree. */
export const trackedPaths = (): string[] => [...new Set([...Object.keys(head), ...Object.keys(index)])];

/**
 * A file as it was in an older demo commit: the HEAD lines blamed on that commit or an older
 * one (`null` before the file was added). Keeps file history, Compare with Revision and
 * Annotate Revision consistent with the blame data.
 */
export function revisionContent(rev: string, path: string): string | null {
  const base = head[path];
  const sha = resolveRev(rev);
  const at = sha ? bySha.get(sha) : undefined;
  if (base === undefined || !at) return base ?? null;
  const oldest = commits.filter((c) => c.files.some((file) => file.path === path)).pop();
  if (oldest && at.time < oldest.time) return null;
  return revisionLines(path, at.time)
    .map((l) => l.text)
    .join("");
}

/** HEAD lines of `path` (with their line breaks and 1-based HEAD numbers) that existed at `time`. */
export function revisionLines(path: string, time: number): { text: string; line: number; sha: string }[] {
  const base = head[path] ?? "";
  const lines = base.match(/[^\n]*\n|[^\n]+$/g) ?? [];
  return lines
    .map((text, i) => ({ text, line: i + 1, sha: headLineCommit(path, i + 1) }))
    .filter((l) => (bySha.get(l.sha)?.time ?? 0) <= time);
}

/** The demo repository for the history mocks (`gitHistoryMock.ts`). */
export const mockRepo = { commits, bySha, head, state, log, ancestors, resolveRev, computeStatus };

/** Working-tree and ref helpers for the dialog and shelf mocks (`gitOpsMock.ts`). */
export const mockRepoOps = { index, changed, readWorking, working, newCommit, checkout, merging, startConflictMerge, CONFLICT_BRANCH };
