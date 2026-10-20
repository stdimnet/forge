/**
 * Browser-mode code host: the demo project `acme/acme-shop` hosted on a Forgejo server
 * (git.acme.dev) with pull requests, inline review threads on `User.php`, pipelines and
 * issues. Everything lives in memory; nothing is written to a project.
 */
import { invoke, registerMock } from "@/ipc";
import type {
  Account,
  ChangedFile,
  CiStatus,
  Discussion,
  HostKind,
  Hunk,
  Issue,
  IssueDetails,
  IssueFilter,
  Job,
  Label,
  MergeOptions,
  MergeRequest,
  MergeRequestDetails,
  MrFilter,
  NewMergeRequest,
  Note,
  Pipeline,
  Position,
  RepoContext,
  Reviewer,
  User,
} from "@/ipc/forges";
import type { Branches } from "@/features/git/api";
import { unifiedDiff } from "@/features/git/textDiff";
import { demoFiles } from "./demoProject";
import { fakeSha } from "./gitMock";
import { demoStatus } from "./demoStatus";

const SERVER = "https://git.acme.dev";
const REPO = "acme/acme-shop";
const WEB = `${SERVER}/${REPO}`;
const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));
const HOUR = 3600_000;
const ago = (hours: number) => new Date(Date.now() - hours * HOUR).toISOString();

const people: Record<string, User> = {
  marlon: { username: "marlon", name: "Marlon", avatarUrl: null },
  anna: { username: "anna", name: "Anna Weber", avatarUrl: null },
  jonas: { username: "jonas", name: "Jonas Klein", avatarUrl: null },
  sofia: { username: "sofia", name: "Sofia Rossi", avatarUrl: null },
  renovate: { username: "renovate", name: "Renovate Bot", avatarUrl: null },
};
const me = people.marlon;

const labels: Record<string, Label> = {
  backend: { name: "backend", color: "#5ea4ff" },
  checkout: { name: "checkout", color: "#ff8a3d" },
  security: { name: "security", color: "#ef5f5f" },
  i18n: { name: "i18n", color: "#6fd2c4" },
  bug: { name: "bug", color: "#ef5f5f" },
  enhancement: { name: "enhancement", color: "#73c48a" },
  frontend: { name: "frontend", color: "#c9a6ff" },
  dependencies: { name: "dependencies", color: "#e5b449" },
  "good first issue": { name: "good first issue", color: "#7cc5e8" },
};

// ---- file contents and diffs -----------------------------------------------------------

const replace = (s: string, from: string, to: string) => {
  if (!s.includes(from)) throw new Error(`forgesMock: snippet not found: ${from.slice(0, 40)}`);
  return s.replace(from, to);
};

const userBase = demoFiles["app/Models/User.php"];
let userHead = replace(
  userBase,
  "    protected $hidden = ['password', 'remember_token'];",
  "    protected $hidden = ['password', 'remember_token', 'two_factor_secret'];",
);
userHead = replace(
  userHead,
  "    protected function casts(): array",
  `    public function isAdmin(): bool
    {
        return $this->role === UserRole::Admin;
    }

    protected function casts(): array`,
);
userHead = replace(userHead, "            'password' => 'hashed',", "            'password' => 'hashed',\n            'role' => UserRole::class,");
userHead = replace(userHead, "use Illuminate\\Notifications\\Notifiable;", "use App\\Enums\\UserRole;\nuse Illuminate\\Notifications\\Notifiable;");

const userRole = `<?php

declare(strict_types=1);

namespace App\\Enums;

enum UserRole: string
{
    case Customer = 'customer';
    case Admin = 'admin';
}
`;

const userTest = `<?php

namespace Tests\\Unit;

use App\\Enums\\UserRole;
use App\\Models\\User;
use PHPUnit\\Framework\\TestCase;

class UserTest extends TestCase
{
    public function test_admin_role_is_detected(): void
    {
        $user = new User(['role' => UserRole::Admin]);

        $this->assertTrue($user->isAdmin());
    }

    public function test_two_factor_secret_is_hidden(): void
    {
        $user = new User();

        $this->assertContains('two_factor_secret', $user->getHidden());
    }
}
`;

const migration = `<?php

use Illuminate\\Database\\Migrations\\Migration;
use Illuminate\\Database\\Schema\\Blueprint;
use Illuminate\\Support\\Facades\\Schema;

return new class extends Migration
{
    public function up(): void
    {
        Schema::table('users', function (Blueprint $table) {
            $table->string('role')->default('customer')->after('email');
        });
    }
};
`;

const checkoutBase = demoFiles["app/Services/Checkout.php"];
const checkoutHead = replace(
  checkoutBase,
  "        $this->taxRate = $taxRate;\n    }\n",
  `        $this->taxRate = $taxRate;
    }

    /** Net sum of all lines plus tax, rounded to cents. */
    public function total(array $lines): float
    {
        $net = array_sum(array_map(fn (array $l) => $l['price'] * $l['qty'], $lines));

        return round($net * (1 + $this->taxRate), 2);
    }
`,
);

/** File contents per pull request: path -> [base, head] (`null` = absent). */
const contents: Record<number, Record<string, [string | null, string | null]>> = {
  41: {
    "app/Models/User.php": [userBase, userHead],
    "app/Enums/UserRole.php": [null, userRole],
    "database/migrations/2026_10_14_000000_add_role_to_users.php": [null, migration],
    "tests/Unit/UserTest.php": [null, userTest],
  },
  42: {
    "app/Services/Checkout.php": [checkoutBase, checkoutHead],
    "app/Models/Order.php": [demoFiles["app/Models/Order.php"], replace(demoFiles["app/Models/Order.php"], "<?php", "<?php\n\n// Totals are calculated by App\\Services\\Checkout.")],
  },
};

function parseHunks(diff: string): Hunk[] {
  const hunks: Hunk[] = [];
  for (const line of diff.split("\n")) {
    const m = /^@@ -(\d+),(\d+) \+(\d+),(\d+) @@ ?(.*)$/.exec(line);
    if (m) {
      hunks.push({ oldStart: +m[1], oldLines: +m[2], newStart: +m[3], newLines: +m[4], header: m[5], lines: [] });
      continue;
    }
    const h = hunks[hunks.length - 1];
    if (!h || line.startsWith("diff ") || line.startsWith("---") || line.startsWith("+++")) continue;
    if (line.startsWith("+")) h.lines.push({ kind: "added", text: line.slice(1) });
    else if (line.startsWith("-")) h.lines.push({ kind: "removed", text: line.slice(1) });
    else if (line.startsWith(" ")) h.lines.push({ kind: "context", text: line.slice(1) });
  }
  return hunks;
}

function filesOf(number: number): ChangedFile[] {
  const files = contents[number] ?? genericFiles(number);
  return Object.entries(files).map(([path, [a, b]]) => {
    const hunks = parseHunks(unifiedDiff(path, a, b));
    const lines = hunks.flatMap((h) => h.lines);
    return {
      path,
      oldPath: null,
      status: a === null ? "added" : b === null ? "deleted" : "modified",
      additions: lines.filter((l) => l.kind === "added").length,
      deletions: lines.filter((l) => l.kind === "removed").length,
      hunks,
    };
  });
}

function genericFiles(number: number): Record<string, [string | null, string | null]> {
  const mr = mrs.find((m) => m.number === number);
  const path = mr?.labels.some((l) => l.name === "i18n") ? "lang/de/checkout.php" : "routes/web.php";
  const base = demoFiles[path] ?? "<?php\n\nreturn [\n];\n";
  return { [path]: [base, `${base.trimEnd()}\n// ${mr?.title ?? "Change"}\n`] };
}

const lineOf = (text: string, snippet: string) => text.split("\n").findIndex((l) => l.includes(snippet)) + 1;

// ---- pull requests ----------------------------------------------------------------------

type MockMr = MergeRequestDetails & { discussions: Discussion[] };

const job = (name: string, status: CiStatus, stage: string | null = null, duration: number | null = null): Job => ({
  name,
  stage,
  status,
  webUrl: `${WEB}/actions/runs/${Math.abs(hash(name)) % 900}`,
  duration,
});

function hash(s: string): number {
  let h = 0;
  for (const c of s) h = (Math.imul(h, 31) + c.charCodeAt(0)) | 0;
  return h;
}

function pipeline(branch: string, status: CiStatus, jobs: [string, CiStatus][], hoursAgo: number): Pipeline {
  return {
    id: String(300 + (Math.abs(hash(branch)) % 600)),
    name: "ci",
    status,
    ref: branch,
    sha: fakeSha(branch),
    webUrl: `${WEB}/actions/runs/${300 + (Math.abs(hash(branch)) % 600)}`,
    createdAt: ago(hoursAgo),
    jobs: jobs.map(([n, s], i) => job(n, s, null, s === "running" || s === "pending" ? null : 40 + i * 37)),
  };
}

const allGreen: [string, CiStatus][] = [
  ["lint / php-cs-fixer", "success"],
  ["analyse / phpstan (level 8)", "success"],
  ["test / phpunit (php 8.4)", "success"],
  ["build / vite", "success"],
];

let noteId = 1000;
const note = (who: string, body: string, hoursAgo: number, system = false): Note => ({
  id: String(++noteId),
  author: people[who],
  body,
  createdAt: ago(hoursAgo),
  system,
});

function mr(
  number: number,
  title: string,
  who: string,
  source: string,
  state: MergeRequest["state"],
  hoursAgo: number,
  opts: Partial<MockMr> & { ciJobs?: [string, CiStatus][] } = {},
): MockMr {
  const jobs = opts.ciJobs ?? allGreen;
  const ci = state === "closed" ? null : (pipeline(source, CiCombine(jobs.map((j) => j[1])), jobs, hoursAgo));
  const { ciJobs: _j, ...rest } = opts;
  return {
    number,
    title,
    state,
    draft: false,
    author: people[who],
    sourceBranch: source,
    targetBranch: "main",
    labels: [],
    comments: 0,
    ci: ci?.status ?? null,
    createdAt: ago(hoursAgo + 30),
    updatedAt: ago(hoursAgo),
    webUrl: `${WEB}/pulls/${number}`,
    headSha: fakeSha(source),
    description: "",
    reviewers: [],
    assignees: [people[who]],
    approvals: { required: 1, approvedBy: [] },
    pipeline: ci,
    mergeable: state === "open" ? true : null,
    conflicts: false,
    baseSha: fakeSha("main"),
    startSha: fakeSha("main"),
    mergedBy: state === "merged" ? people.marlon : null,
    discussions: [],
    ...rest,
  };
}

function CiCombine(all: CiStatus[]): CiStatus {
  for (const s of ["failed", "running", "pending", "manual", "canceled", "success", "skipped"] as CiStatus[]) if (all.includes(s)) return s;
  return "success";
}

const userPath = "app/Models/User.php";

const mrs: MockMr[] = [
  mr(42, "Add order total calculation to Checkout", "marlon", "feature/order-totals", "open", 1.5, {
    labels: [labels.checkout, labels.backend],
    reviewers: [{ user: people.anna, state: "requested" }],
    ciJobs: [
      ["lint / php-cs-fixer", "success"],
      ["analyse / phpstan (level 8)", "success"],
      ["test / phpunit (php 8.4)", "running"],
      ["build / vite", "pending"],
    ],
    description:
      "Moves the order total calculation from `OrderController` into the `Checkout` service.\n\n- `Checkout::total()` sums all lines and adds tax\n- amounts are rounded to cents\n\nCloses #28",
    discussions: [
      { id: "c1", notes: [note("anna", "I'll review this after lunch. Could you add a test for empty carts?", 1.2)], position: null, resolvable: false, resolved: false },
    ],
  }),
  mr(41, "Harden User model: roles, hidden 2FA secret and casts", "anna", "feature/user-hardening", "open", 3, {
    labels: [labels.security, labels.backend],
    reviewers: [
      { user: people.marlon, state: "requested" },
      { user: people.jonas, state: "approved" },
    ],
    approvals: { required: 2, approvedBy: [people.jonas] },
    description:
      "## Summary\n\nPrepares the user model for the admin area.\n\n- adds a `UserRole` enum and a `role` column (default `customer`)\n- hides `two_factor_secret` from JSON output\n- casts `role` to the enum\n\n## Testing\n\n`php artisan test --filter=UserTest`\n\n> The migration is backwards compatible; existing users become customers.",
  }),
  mr(40, "Extract Checkout service from OrderController", "anna", "feature/checkout-flow", "open", 20, {
    labels: [labels.checkout],
    reviewers: [{ user: people.marlon, state: "approved" }],
    approvals: { required: 1, approvedBy: [people.marlon] },
    description: "Controllers should stay thin. The checkout logic now lives in `App\\Services\\Checkout`.",
  }),
  mr(39, "Wishlist for logged-in customers", "sofia", "feature/wishlist", "open", 26, {
    draft: true,
    labels: [labels.frontend, labels.enhancement],
    ciJobs: [
      ["lint / php-cs-fixer", "success"],
      ["analyse / phpstan (level 8)", "pending"],
      ["test / phpunit (php 8.4)", "pending"],
    ],
    description: "First draft of the wishlist. Not ready for review yet.",
  }),
  mr(36, "Verify Stripe webhook signatures", "sofia", "fix/stripe-webhook-signature", "open", 46, {
    labels: [labels.security],
    reviewers: [
      { user: people.marlon, state: "changesRequested" },
      { user: people.anna, state: "commented" },
    ],
    ciJobs: [
      ["lint / php-cs-fixer", "success"],
      ["analyse / phpstan (level 8)", "success"],
      ["test / phpunit (php 8.4)", "failed"],
      ["build / vite", "success"],
    ],
    mergeable: false,
    conflicts: true,
    description: "Rejects webhook calls whose `Stripe-Signature` header does not match.",
  }),
  mr(34, "Translate checkout e-mails (de, it)", "sofia", "i18n/checkout-mails", "open", 70, {
    labels: [labels.i18n],
    reviewers: [{ user: people.jonas, state: "requested" }],
    description: "German and Italian versions of the order confirmation and shipping e-mails.",
  }),
  mr(38, "Fix login redirect after password reset", "jonas", "bugfix/login-redirect", "merged", 96, {
    labels: [labels.bug],
    reviewers: [{ user: people.marlon, state: "approved" }],
    approvals: { required: 1, approvedBy: [people.marlon] },
    description: "After resetting the password users landed on `/home`, which no longer exists.",
  }),
  mr(37, "Bump laravel/framework to 12.4", "renovate", "renovate/laravel-framework-12.x", "merged", 140, {
    labels: [labels.dependencies],
    description: "This PR updates `laravel/framework` from 12.3.0 to 12.4.1.",
  }),
  mr(35, "Experiment: Livewire cart", "jonas", "spike/livewire-cart", "closed", 220, {
    labels: [labels.frontend],
    description: "Spike, closed in favour of the Vue cart.",
  }),
];

// Review threads of #41 anchored on lines of the new User.php.
{
  const pr = mrs.find((m) => m.number === 41)!;
  const hiddenLine = lineOf(userHead, "two_factor_secret");
  const adminLine = lineOf(userHead, "return $this->role === UserRole::Admin;");
  const castLine = lineOf(userHead, "'role' => UserRole::class");
  pr.discussions = [
    { id: "c11", notes: [note("anna", "Ready for review. The admin area in #43 builds on this.", 30)], position: null, resolvable: false, resolved: false },
    {
      id: `n${hiddenLine}:${userPath}`,
      position: { path: userPath, newLine: hiddenLine },
      resolvable: true,
      resolved: false,
      notes: [
        note("marlon", "Should `two_factor_recovery_codes` be hidden as well? Fortify stores them next to the secret.", 22),
        note("anna", "Good catch, I'll add it together with the Fortify upgrade.", 21),
      ],
    },
    {
      id: `n${adminLine}:${userPath}`,
      position: { path: userPath, newLine: adminLine },
      resolvable: true,
      resolved: true,
      notes: [
        note("jonas", "Could this be a policy instead of a model method?", 20),
        note("anna", "The policy will call it; `isAdmin()` keeps the check in one place.", 19),
        note("jonas", "Makes sense 👍", 18.5),
      ],
    },
    { id: "r1", notes: [note("jonas", "approved these changes", 18, true)], position: null, resolvable: false, resolved: false },
    {
      id: `n${castLine}:${userPath}`,
      position: { path: userPath, newLine: castLine },
      resolvable: true,
      resolved: false,
      notes: [note("sofia", "Existing rows have `NULL` here until the migration runs. Is the cast safe for `null`?", 4)],
    },
  ];
  pr.comments = pr.discussions.reduce((n, d) => n + d.notes.filter((x) => !x.system).length, 0);
}
for (const m of mrs) if (!m.comments) m.comments = m.discussions.reduce((n, d) => n + d.notes.length, 0) || (m.number % 3);

const summary = (m: MockMr): MergeRequest => ({
  number: m.number,
  title: m.title,
  state: m.state,
  draft: m.draft,
  author: m.author,
  sourceBranch: m.sourceBranch,
  targetBranch: m.targetBranch,
  labels: m.labels,
  comments: m.comments,
  ci: m.ci,
  createdAt: m.createdAt,
  updatedAt: m.updatedAt,
  webUrl: m.webUrl,
  headSha: m.headSha,
});

function find(number: unknown): MockMr {
  const m = mrs.find((x) => x.number === Number(number));
  if (!m) throw new Error(`Forgejo: pull request #${number} not found`);
  return m;
}

const eq = (a: string | null | undefined, b: string) => !a || a.toLowerCase() === b.toLowerCase();

// ---- issues -----------------------------------------------------------------------------

const issues: IssueDetails[] = [
  [31, "Checkout returns 500 when the cart is empty", "open", "jonas", ["bug", "checkout"], 2, 5, "Steps to reproduce:\n\n1. Log in\n2. Remove all items from the cart\n3. Open `/checkout`\n\nExpected a redirect to the cart, got a 500 (`DivisionByZeroError` in `Checkout`)."],
  [30, "Customers cannot change their e-mail address", "open", "sofia", ["bug"], 1, 9, "The profile form ignores the new address. Probably `email` is missing from a form request."],
  [29, "Add PayPal as payment method", "open", "anna", ["enhancement", "checkout"], 4, 30, "Many customers asked for PayPal. We should add it next to Stripe."],
  [28, "Order confirmation e-mail shows wrong currency", "open", "marlon", ["bug"], 3, 50, "Totals are always shown in USD even for EUR orders."],
  [27, "Dark mode for the storefront", "open", "sofia", ["frontend", "good first issue"], 0, 80, "Respect `prefers-color-scheme` in the storefront layout."],
  [26, "Document local setup with Docker", "open", "jonas", ["good first issue"], 1, 120, "The README still describes Homestead."],
  [25, "Invoices are generated twice", "closed", "anna", ["bug"], 6, 200, "Fixed by the idempotency key in `InvoiceService`."],
  [24, "Upgrade to PHP 8.4", "closed", "marlon", ["dependencies"], 2, 260, "Done in #33."],
].map(([number, title, state, who, ls, comments, hours, description]) => ({
  number: number as number,
  title: title as string,
  state: state as "open" | "closed",
  author: people[who as string],
  labels: (ls as string[]).map((l) => labels[l]),
  assignees: number === 31 ? [people.marlon] : [],
  comments: comments as number,
  createdAt: ago((hours as number) + 10),
  updatedAt: ago(hours as number),
  webUrl: `${WEB}/issues/${number}`,
  description: description as string,
  notes: Array.from({ length: comments as number }, (_, i) =>
    note(["anna", "marlon", "jonas", "sofia"][i % 4], ["I can reproduce this on staging.", "Looking into it.", "This also happens with a single item.", "Fixed locally, PR follows."][i % 4], (hours as number) - i),
  ),
}));

// ---- accounts ---------------------------------------------------------------------------

const accounts: Account[] = [
  { id: "forgejo-git.acme.dev", kind: "forgejo", server: SERVER, username: "marlon", tokenSource: "keychain" },
];

registerMock("forges_accounts", () => accounts.map((a) => ({ ...a })));
registerMock("forges_login", async ({ kind, server, token }) => {
  await delay(500);
  const t = String(token ?? "").trim();
  if (!t || t.length < 8 || t.includes("invalid")) {
    throw new Error(`${kind === "github" ? "GitHub" : kind === "gitlab" ? "GitLab" : "Forgejo"}: the token was rejected. Log in again in Settings | Version Control`);
  }
  const host = String(server || "").replace(/\/$/, "") || "https://example.com";
  const id = `${kind}-${host.replace(/^https?:\/\//, "").replace(/[/:]/g, "-")}`;
  const account: Account = { id, kind: kind as HostKind, server: host, username: "marlon", tokenSource: "keychain" };
  const i = accounts.findIndex((a) => a.id === id);
  if (i >= 0) accounts[i] = account;
  else accounts.push(account);
  return account;
});
registerMock("forges_logout", ({ id }) => {
  const i = accounts.findIndex((a) => a.id === id);
  if (i >= 0) accounts.splice(i, 1);
});
registerMock("forges_verify", async ({ id }) => {
  await delay(400);
  const a = accounts.find((x) => x.id === id);
  if (!a) throw new Error(`unknown account ${id}`);
  return people[a.username ?? "marlon"] ?? me;
});

// ---- repository -------------------------------------------------------------------------

const repoRef = { kind: "forgejo" as const, server: SERVER, path: REPO, remote: "origin" };

registerMock("forges_repo", (): RepoContext => ({
  repo: repoRef,
  repos: [repoRef],
  account: accounts.find((a) => a.kind === "forgejo" && a.server === SERVER) ?? null,
  capabilities: {
    resolveThreads: false,
    approvals: true,
    draft: true,
    mergeMethods: ["merge", "squash", "rebase"],
    requestName: "Pull Request",
    numberPrefix: "#",
  },
}));

registerMock("forges_merge_requests", async ({ filter }) => {
  await delay(250);
  const f = (filter ?? {}) as MrFilter;
  const state = f.state ?? "open";
  const q = f.search?.trim().toLowerCase();
  const items = mrs
    .filter((m) => state === "all" || m.state === state)
    .filter((m) => eq(f.author, m.author.username))
    .filter((m) => !f.reviewer || m.reviewers.some((r) => eq(f.reviewer, r.user.username)))
    .filter((m) => !f.assignee || m.assignees.some((a) => eq(f.assignee, a.username)))
    .filter((m) => !f.label || m.labels.some((l) => eq(f.label, l.name)))
    .filter((m) => !q || m.title.toLowerCase().includes(q) || `#${m.number}`.includes(q) || m.sourceBranch.includes(q))
    .sort((a, b) => b.updatedAt.localeCompare(a.updatedAt))
    .map(summary);
  return { items, nextPage: null, total: items.length };
});

registerMock("forges_merge_request", async ({ number }) => {
  await delay(200);
  const { discussions: _d, ...details } = find(number);
  return structuredClone(details);
});
registerMock("forges_mr_files", async ({ number }) => {
  await delay(150);
  return filesOf(Number(number));
});
registerMock("forges_discussions", async ({ number }) => {
  await delay(150);
  return structuredClone(find(number).discussions);
});
registerMock("forges_file", ({ path, gitRef }) => {
  for (const [n, files] of Object.entries(contents)) {
    const pr = find(n);
    const pair = files[String(path)];
    if (!pair) continue;
    if (gitRef === pr.headSha) return pair[1];
    if (gitRef === pr.baseSha) return pair[0];
  }
  return demoFiles[String(path)] ?? null;
});

registerMock("forges_comment", async ({ number, body, position }) => {
  await delay(200);
  const pr = find(number);
  const pos = position as Position | null;
  const n = note("marlon", String(body), 0);
  if (pos) {
    const id = `${pos.newLine ? `n${pos.newLine}` : `o${pos.oldLine}`}:${pos.path}`;
    const existing = pr.discussions.find((d) => d.id === id);
    if (existing) existing.notes.push(n);
    else pr.discussions.push({ id, notes: [n], position: pos, resolvable: true, resolved: false });
  } else {
    pr.discussions.push({ id: `c${n.id}`, notes: [n], position: null, resolvable: false, resolved: false });
  }
  pr.comments = (pr.comments ?? 0) + 1;
  pr.updatedAt = n.createdAt;
});
registerMock("forges_reply", async ({ number, discussionId, body }) => {
  await delay(200);
  const pr = find(number);
  const d = pr.discussions.find((x) => x.id === discussionId);
  const n = note("marlon", String(body), 0);
  if (d?.position) d.notes.push(n);
  else pr.discussions.push({ id: `c${n.id}`, notes: [n], position: null, resolvable: false, resolved: false });
  pr.comments = (pr.comments ?? 0) + 1;
});
registerMock("forges_resolve", () => {
  throw new Error("Forgejo does not support resolving conversations through its API");
});
registerMock("forges_approve", async ({ number }) => {
  await delay(300);
  const pr = find(number);
  const existing = pr.reviewers.find((r) => r.user.username === me.username);
  if (existing) existing.state = "approved";
  else pr.reviewers.push({ user: me, state: "approved" } as Reviewer);
  if (!pr.approvals.approvedBy.some((u) => u.username === me.username)) pr.approvals.approvedBy.push(me);
  pr.discussions.push({ id: `r${++noteId}`, notes: [note("marlon", "approved these changes", 0, true)], position: null, resolvable: false, resolved: false });
});
registerMock("forges_merge", async ({ number, options }) => {
  await delay(500);
  const pr = find(number);
  if (pr.state !== "open") throw new Error(`Forgejo: pull request #${number} is not open`);
  if (pr.conflicts) throw new Error("Forgejo: Please resolve the conflicts before merging");
  pr.state = "merged";
  pr.mergedBy = me;
  pr.mergeable = null;
  pr.updatedAt = ago(0);
  const o = options as MergeOptions;
  pr.discussions.push({
    id: `r${++noteId}`,
    notes: [note("marlon", `merged commit into \`main\` (${o.method})${o.deleteSourceBranch ? ` and deleted \`${pr.sourceBranch}\`` : ""}`, 0, true)],
    position: null,
    resolvable: false,
    resolved: false,
  });
});
registerMock("forges_close", async ({ number }) => {
  await delay(250);
  const pr = find(number);
  pr.state = "closed";
  pr.updatedAt = ago(0);
});
registerMock("forges_create_mr", async ({ request }) => {
  await delay(600);
  const r = request as NewMergeRequest;
  if (!r.title.trim()) throw new Error("Forgejo: title cannot be empty");
  if (mrs.some((m) => m.state === "open" && m.sourceBranch === r.sourceBranch && m.targetBranch === r.targetBranch))
    throw new Error(`Forgejo: a pull request for ${r.sourceBranch} already exists`);
  const number = Math.max(...mrs.map((m) => m.number)) + 1;
  const created = mr(number, r.title, "marlon", r.sourceBranch, "open", 0, {
    draft: r.draft,
    description: r.description,
    targetBranch: r.targetBranch,
    labels: r.labels.map((l) => labels[l] ?? { name: l, color: null }),
    reviewers: r.reviewers.map((u) => ({ user: people[u] ?? { username: u, name: null, avatarUrl: null }, state: "requested" as const })),
    ciJobs: allGreen.map(([n]) => [n, "pending"] as [string, CiStatus]),
  });
  mrs.unshift(created);
  return summary(created);
});
registerMock("forges_members", () => Object.values(people).filter((p) => p.username !== "renovate"));

registerMock("forges_checkout", async ({ number, sourceBranch }) => {
  await delay(300);
  const branch = String(sourceBranch);
  const b = await invoke<Branches>("git_branches");
  if (b.local.some((x) => x.name === branch)) await invoke("git_checkout", { rev: branch, remote: false });
  else if (b.remote.some((x) => x.name === `origin/${branch}`)) await invoke("git_checkout", { rev: `origin/${branch}`, remote: true });
  else {
    const local = `pr-${number}`;
    if (!b.local.some((x) => x.name === local)) await invoke("git_create_branch", { name: local, start: "HEAD", checkout: true });
    else await invoke("git_checkout", { rev: local, remote: false });
    return local;
  }
  return branch;
});

// ---- issues & pipelines ---------------------------------------------------------------

registerMock("forges_issues", async ({ filter }) => {
  await delay(200);
  const f = (filter ?? {}) as IssueFilter;
  const state = f.state ?? "open";
  const q = f.search?.trim().toLowerCase();
  const items: Issue[] = issues
    .filter((i) => state === "all" || i.state === state)
    .filter((i) => eq(f.author, i.author.username))
    .filter((i) => !f.label || i.labels.some((l) => eq(f.label, l.name)))
    .filter((i) => !f.assignee || i.assignees.some((a) => eq(f.assignee, a.username)))
    .filter((i) => !q || i.title.toLowerCase().includes(q) || `#${i.number}` === q)
    .map(({ description: _d, notes: _n, ...rest }) => rest);
  return { items, nextPage: null, total: items.length };
});
registerMock("forges_issue", async ({ number }) => {
  await delay(150);
  const i = issues.find((x) => x.number === Number(number));
  if (!i) throw new Error(`Forgejo: issue #${number} not found`);
  return structuredClone(i);
});
registerMock("forges_issue_comment", ({ number, body }) => {
  const i = issues.find((x) => x.number === Number(number));
  if (!i) throw new Error(`Forgejo: issue #${number} not found`);
  i.notes.push(note("marlon", String(body), 0));
  i.comments++;
});

const branchPipelines: Record<string, Pipeline> = Object.fromEntries(
  mrs.filter((m) => m.pipeline).map((m) => [m.sourceBranch, m.pipeline!]),
);
branchPipelines.main = pipeline("main", "success", allGreen, 5);
// The other demos: their own branch's pipeline, or none for a project without CI.
const demo = demoStatus();
if (demo.branch !== "feature/order-totals") {
  for (const k of Object.keys(branchPipelines)) delete branchPipelines[k];
  if (demo.ci) branchPipelines[demo.branch] = pipeline(demo.branch, demo.ci, demo.jobs, 1);
}

registerMock("forges_pipelines", ({ branch }) => {
  const list = Object.values(branchPipelines).sort((a, b) => (b.createdAt ?? "").localeCompare(a.createdAt ?? ""));
  return branch ? list.filter((p) => p.ref === branch) : list;
});
registerMock("forges_ci_status", ({ gitRef }) => {
  const ref = String(gitRef);
  return branchPipelines[ref] ?? Object.values(branchPipelines).find((p) => p.sha === ref) ?? null;
});
