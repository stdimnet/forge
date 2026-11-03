/**
 * Browser-mode review mode (`review_*`): viewed files, pending comments and canned AI
 * results for the demo pull requests of `forgesMock`. The AI answers are fixed texts; the
 * provider, the `ai.enabled` switch and `.aiignore` come from the AI mocks, so the same
 * privacy rules apply as in the desktop app. Everything lives in memory.
 */
import { invoke, registerMock } from "@/ipc";
import type { AiEditorStatus } from "@/ipc/aiEditor";
import type { ChangedFile, MergeRequestDetails, Position } from "@/ipc/forges";
import type {
  AiInfo,
  CachedSummary,
  Draft,
  ExplainResult,
  Finding,
  FindingsView,
  FindingUpdate,
  ReviewEvent,
  ReviewSession,
  Severity,
  Summary,
} from "@/ipc/codeReview";

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

interface MockRecord {
  viewed: Map<string, string>;
  drafts: Draft[];
  summary: CachedSummary | null;
  findings: (FindingsView & { all: Finding[]; handled: Set<string> }) | null;
  nextId: number;
}

const records = new Map<number, MockRecord>();
const record = (n: number): MockRecord => {
  let r = records.get(n);
  if (!r) {
    r = { viewed: new Map(), drafts: [], summary: null, findings: null, nextId: 0 };
    records.set(n, r);
  }
  return r;
};

function digest(f: ChangedFile): string {
  let h = 0x811c9dc5;
  for (const c of f.path + JSON.stringify(f.hunks)) h = Math.imul(h ^ c.charCodeAt(0), 0x01000193) >>> 0;
  return h.toString(16).padStart(8, "0");
}

const files = (n: number) => invoke<ChangedFile[]>("forges_mr_files", { number: n });
const details = (n: number) => invoke<MergeRequestDetails>("forges_merge_request", { number: n });

async function status(path?: string): Promise<AiEditorStatus> {
  return invoke<AiEditorStatus>("ai_editor_status", path ? { path } : {});
}

async function aiInfo(): Promise<AiInfo> {
  const s = await status();
  return {
    provider: s.actions?.name ?? null,
    model: s.actions?.model ?? null,
    local: s.actions?.local ?? false,
    problem: s.actionsProblem ?? (s.projectEnabled ? null : "AI features are turned off for this project"),
  };
}

const excluded = async (path: string) => (await status(path)).fileActions?.status !== "allowed";

/** Refuse like the desktop app: no provider, AI off for the project. */
async function requireAi(): Promise<AiInfo> {
  const info = await aiInfo();
  if (info.problem) throw new Error(info.problem);
  return info;
}

function view(r: NonNullable<MockRecord["findings"]>): FindingsView {
  const { all, handled, ...rest } = r;
  return { ...rest, findings: all.filter((f) => !handled.has(f.id)), total: all.length };
}

/** New-file line of the first added or context line containing `snippet`. */
function lineOf(fs: ChangedFile[], path: string, snippet: string): number | null {
  const f = fs.find((x) => x.path === path);
  for (const h of f?.hunks ?? []) {
    let n = h.newStart;
    for (const l of h.lines) {
      if (l.kind === "removed") continue;
      if (l.text.includes(snippet)) return n;
      n++;
    }
  }
  return null;
}

const USER = "app/Models/User.php";
const MIGRATION = "database/migrations/2026_10_14_000000_add_role_to_users.php";
const TEST = "tests/Unit/UserTest.php";

function cannedSummary(n: number, fs: ChangedFile[], title: string): Summary {
  if (n === 41)
    return {
      overview:
        "Adds a `UserRole` enum and a `role` column (default `customer`) to `users`, casts `role` to the enum in `User`, adds `User::isAdmin()` and hides `two_factor_secret` from JSON output. Two unit tests cover the admin check and the hidden attribute.",
      risks: [
        {
          title: "Unknown role values",
          detail: "The enum cast throws a `ValueError` for values outside `UserRole`, so rows written by other services (or a typo in a seeder) break loading the user.",
          severity: "error",
          files: [USER, MIGRATION],
        },
        {
          title: "Mass assignment of `role`",
          detail: "`role` is not in `$fillable`, so `new User(['role' => …])` silently drops it; `test_admin_role_is_detected` relies on it.",
          severity: "warning",
          files: [TEST],
        },
        {
          title: "Recovery codes still serialized",
          detail: "`two_factor_recovery_codes` is as sensitive as the secret and is still part of the JSON output.",
          severity: "warning",
          files: [USER],
        },
      ],
      files: [
        { path: USER, summary: "Casts `role` to `UserRole`, adds `isAdmin()` and hides `two_factor_secret`." },
        { path: "app/Enums/UserRole.php", summary: "New backed enum with the cases `Customer` and `Admin`." },
        { path: MIGRATION, summary: "Adds the `role` string column with the default `customer` after `email`." },
        { path: TEST, summary: "Tests the admin check and that the 2FA secret is hidden." },
      ],
      testPlan: [
        "Run `php artisan migrate` on a copy of production data and load a few existing users.",
        "`php artisan test --filter=UserTest`",
        "Log in as an admin and as a customer and open the admin area.",
        "Check that `GET /api/user` no longer returns `two_factor_secret`.",
      ],
    };
  return {
    overview: `${title}. The change touches ${fs.length} file${fs.length === 1 ? "" : "s"}: ${fs.map((f) => `\`${f.path}\``).join(", ")}.`,
    risks: [],
    files: fs.map((f) => ({ path: f.path, summary: `${f.additions} line${f.additions === 1 ? "" : "s"} added, ${f.deletions} removed.` })),
    testPlan: ["Run the test suite: `php artisan test`."],
  };
}

function finding(
  fs: ChangedFile[],
  path: string,
  snippet: string,
  severity: Severity,
  title: string,
  explanation: string,
  suggestion: string | null = null,
): Finding | null {
  const line = lineOf(fs, path, snippet);
  if (line == null) return null;
  const position: Position = { path, oldPath: null, newLine: line, oldLine: null };
  return { id: `f-${path}-${line}`, path, line, endLine: null, severity, title, explanation, suggestion, position, startLine: null };
}

function cannedFindings(n: number, fs: ChangedFile[]): Finding[] {
  if (n !== 41) return [];
  return [
    finding(
      fs,
      USER,
      "'role' => UserRole::class",
      "error",
      "Unknown role values throw",
      "Laravel's enum cast calls `UserRole::from()`, which throws a `ValueError` for any value that is not a case of the enum. Validate the column (a check constraint or a data migration) or cast with a custom cast that falls back to `UserRole::Customer`.",
    ),
    finding(
      fs,
      USER,
      "two_factor_secret",
      "warning",
      "Recovery codes are still serialized",
      "Fortify stores `two_factor_recovery_codes` next to the secret; they grant the same access and should be hidden as well.",
      "    protected $hidden = ['password', 'remember_token', 'two_factor_secret', 'two_factor_recovery_codes'];",
    ),
    finding(
      fs,
      TEST,
      "new User(['role' => UserRole::Admin])",
      "error",
      "`role` is not mass assignable",
      "`User::$fillable` does not contain `role`, so the constructor drops it and `isAdmin()` returns `false`: the test fails. Set the attribute explicitly.",
      "        $user = (new User())->forceFill(['role' => UserRole::Admin]);",
    ),
    finding(
      fs,
      MIGRATION,
      "$table->string('role')",
      "info",
      "Consider an index",
      "The admin area lists users by role; an index keeps that query fast on large tables.",
      "            $table->string('role')->default('customer')->after('email')->index();",
    ),
  ].filter((f): f is Finding => !!f);
}

function bodyOf(f: Finding): string {
  const word = f.severity === "error" ? "Error" : f.severity === "warning" ? "Warning" : "Note";
  let body = `**${word}: ${f.title}**\n\n${f.explanation}`;
  if (f.suggestion) body += `\n\nSuggested change:\n\n\`\`\`\n${f.suggestion}\n\`\`\``;
  return body;
}

registerMock("review_session", async ({ number }): Promise<ReviewSession> => {
  const n = Number(number);
  const [d, fs, ai] = await Promise.all([details(n), files(n), aiInfo()]);
  const r = record(n);
  const head = d.headSha ?? "";
  return {
    headSha: head,
    files: await Promise.all(
      fs.map(async (f) => ({ path: f.path, digest: digest(f), viewed: r.viewed.get(f.path) === digest(f), aiExcluded: await excluded(f.path) })),
    ),
    changes: fs,
    drafts: structuredClone(r.drafts),
    events: ["comment", "approve", "requestChanges"],
    summary: r.summary?.headSha === head ? structuredClone(r.summary) : null,
    findings: r.findings?.headSha === head ? view(r.findings) : null,
    ai,
  };
});

registerMock("review_set_viewed", ({ number, path, digest: d, viewed }) => {
  const r = record(Number(number));
  if (viewed) r.viewed.set(String(path), String(d));
  else r.viewed.delete(String(path));
});

registerMock("review_save_draft", ({ number, draft }) => {
  const r = record(Number(number));
  const d = structuredClone(draft as Draft);
  if (!d.body.trim()) throw new Error("A review comment needs some text");
  const existing = r.drafts.findIndex((x) => d.id && x.id === d.id);
  if (existing >= 0) r.drafts[existing] = d;
  else r.drafts.push({ ...d, id: `d${++r.nextId}` });
  return structuredClone(r.drafts);
});

registerMock("review_delete_draft", ({ number, id }) => {
  const r = record(Number(number));
  const d = r.drafts.find((x) => x.id === id);
  if (d?.finding) r.findings?.handled.delete(d.finding);
  r.drafts = r.drafts.filter((x) => x.id !== id);
  return structuredClone(r.drafts);
});

registerMock("review_submit", async ({ number, event, body }) => {
  const n = Number(number);
  const r = record(n);
  const ev = event as ReviewEvent;
  const text = String(body ?? "").trim();
  if (ev !== "approve" && !text && !r.drafts.length)
    throw new Error(ev === "requestChanges" ? "Describe the changes you are requesting" : "Add a comment before submitting the review");
  await sleep(300);
  for (const d of r.drafts) await invoke("forges_comment", { number: n, body: d.body, position: d.position });
  if (text) await invoke("forges_comment", { number: n, body: text, position: null });
  if (ev === "approve") await invoke("forges_approve", { number: n });
  r.drafts = [];
});

registerMock("review_summarize", async ({ number, refresh }): Promise<CachedSummary> => {
  const n = Number(number);
  const r = record(n);
  const d = await details(n);
  const head = d.headSha ?? "";
  if (!refresh && r.summary?.headSha === head) return structuredClone(r.summary);
  const ai = await requireAi();
  const fs = await files(n);
  const ex = (await Promise.all(fs.map(async (f) => ((await excluded(f.path)) ? f.path : null)))).filter((p): p is string => !!p);
  await sleep(900);
  const summary = cannedSummary(n, fs.filter((f) => !ex.includes(f.path)), d.title);
  summary.files = summary.files.filter((f) => !ex.includes(f.path));
  r.summary = { headSha: head, summary, provider: ai.provider ?? "", model: ai.model, excluded: ex, omitted: [], createdAt: Math.floor(Date.now() / 1000) };
  return structuredClone(r.summary);
});

registerMock("review_ai", async ({ number, refresh }): Promise<FindingsView> => {
  const n = Number(number);
  const r = record(n);
  const d = await details(n);
  const head = d.headSha ?? "";
  if (!refresh && r.findings?.headSha === head) return view(r.findings);
  const ai = await requireAi();
  const fs = await files(n);
  const ex = (await Promise.all(fs.map(async (f) => ((await excluded(f.path)) ? f.path : null)))).filter((p): p is string => !!p);
  await sleep(1200);
  const all = cannedFindings(n, fs).filter((f) => !ex.includes(f.path));
  r.findings = { headSha: head, findings: all, all, handled: new Set(), total: all.length, provider: ai.provider ?? "", model: ai.model, excluded: ex, omitted: [] };
  return view(r.findings);
});

registerMock("review_finding", ({ number, id, action }): FindingUpdate => {
  const r = record(Number(number));
  const f = r.findings?.all.find((x) => x.id === id);
  if (!f || !r.findings) throw new Error("This finding is no longer available");
  if (action === "draft") r.drafts.push({ id: `d${++r.nextId}`, body: bodyOf(f), position: f.position, startLine: f.startLine, finding: f.id });
  r.findings.handled.add(f.id);
  return { drafts: structuredClone(r.drafts), findings: view(r.findings) };
});

registerMock("review_explain", async ({ number, path, hunk }): Promise<ExplainResult> => {
  const ai = await requireAi();
  if (await excluded(String(path))) throw new Error(`${path} is excluded from AI by .aiignore`);
  const fs = await files(Number(number));
  const h = fs.find((f) => f.path === path)?.hunks[Number(hunk)];
  if (!h) throw new Error(`${path} has no change #${Number(hunk) + 1}`);
  await sleep(700);
  const added = h.lines.filter((l) => l.kind === "added").map((l) => l.text.trim());
  const removed = h.lines.filter((l) => l.kind === "removed").length;
  const text =
    path === USER && added.some((l) => l.includes("two_factor_secret"))
      ? "This change adds `two_factor_secret` to `$hidden`, so the encrypted 2FA secret is no longer included when a user is converted to an array or JSON (API responses, Inertia props, queued job payloads).\n\n- Nothing changes for code that reads the attribute directly.\n- `two_factor_recovery_codes` is still visible and should probably be hidden too."
      : `This change adds ${added.length} line${added.length === 1 ? "" : "s"}${removed ? ` and removes ${removed}` : ""} in \`${path}\`${h.header ? ` (in \`${h.header.trim()}\`)` : ""}. ${added[0] ? `It starts with \`${added[0]}\`.` : ""}`;
  return { text, provider: ai.provider ?? "", model: ai.model };
});
