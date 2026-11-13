/**
 * Browser-mode Local History: the same Rust code as the desktop app (`forge-localhistory` as
 * WebAssembly, over an in-memory store). File writes, deletes, renames and reads of the
 * other mocks are wrapped so saving in browser mode records revisions; a few days of demo
 * history (saves, a refactoring, an update from VCS, an external change, a label) are
 * recorded at startup.
 */
import { emitMock, mockHandler, registerMock } from "@/ipc";
import { LOCAL_HISTORY_CHANGED, type ChangeKind, type LhRecordedFile } from "@/ipc/localHistory";
import { callWasm, forgeWasm } from "@/wasm";
import { mockFiles } from "./workspaceMock";

async function lh<T>(request: Record<string, unknown>): Promise<T> {
  const wasm = await forgeWasm();
  return callWasm<T>(wasm.local_history_call, request);
}

const changed = () => emitMock(LOCAL_HISTORY_CHANGED, null);

/** The operation in progress (`local_history_begin`), its change set group. */
let operation: { token: number; group: number; kind: ChangeKind; name: string } | null = null;
let tokens = 0;

async function record(kind: ChangeKind, name: string, files: (LhRecordedFile & { from?: string })[], time = Date.now()): Promise<number | null> {
  const files2 = files.filter((f) => f.before !== f.after || f.from);
  if (!files2.length) return null;
  const op = kind === "revert" ? null : operation;
  const id = await lh<number | null>({
    op: "record",
    kind: op?.kind ?? kind,
    name: op?.name ?? name,
    time,
    group: op?.group ?? null,
    files: files2,
  });
  if (id !== null) changed();
  return id;
}

const under = (path: string, dir: string) => !dir || path === dir || path.startsWith(`${dir}/`);
const nameOf = (path: string) => path.split("/").pop() ?? path;
const parentOf = (path: string) => path.split("/").slice(0, -1).join("/");

// ---- demo history ---------------------------------------------------------------------------

const MIN = 60_000;
const HOUR = 60 * MIN;
const DAY = 24 * HOUR;

/** Replace `from` with `to` (when present) to derive an older version of a demo file. */
const older = (text: string, from: string, to: string) => (text.includes(from) ? text.replace(from, to) : text);

async function seed(): Promise<void> {
  await lh({ op: "reset" });
  const now = Date.now();
  const day = (d: number, h: number, m = 0) => {
    const t = new Date(now - d * DAY);
    t.setHours(h, m, 0, 0);
    return Math.min(t.getTime(), now - 5 * MIN);
  };
  const user = "app/Models/User.php";
  const current = mockFiles[user];
  if (current === undefined) {
    // Other demo projects: a couple of saves of the first source files.
    const files = Object.keys(mockFiles).filter((p) => !p.includes("/.") && !p.startsWith(".")).slice(0, 3);
    for (const [i, p] of files.entries()) {
      const text = mockFiles[p];
      await record("save", "Saved", [{ path: p, before: `${text}\n`, after: text }], day(1, 10 + i));
    }
    return;
  }
  // User.php, newest first: v3 → current (save today), v2 → v3 (refactoring yesterday), ...
  const v3 = older(current, "            'password' => 'hashed',\n", "");
  const v2 = older(v3, "function totalSpent()", "function spent()");
  const v1 = older(v2, "    protected $hidden = ['password', 'remember_token'];\n\n", "");
  const v0 = older(v1, "use HasFactory, Notifiable;", "use Notifiable;");
  const caller = Object.keys(mockFiles).find((p) => p !== user && mockFiles[p].includes("->totalSpent()"));
  const routes = mockFiles["routes/web.php"];
  const checkout = mockFiles["app/Services/Checkout.php"];

  await record("save", "Saved", [{ path: user, before: v0, after: v1 }], day(3, 9, 41));
  await record(
    "create",
    "Create file LegacyInvoice.php",
    [{ path: "app/Services/LegacyInvoice.php", before: null, after: "<?php\n\nnamespace App\\Services;\n\nfinal class LegacyInvoice\n{\n}\n" }],
    day(3, 9, 58),
  );
  await record("save", "Saved", [{ path: user, before: v1, after: v2 }], day(2, 11, 5));
  await lh({ op: "label", name: "Before VIP discount", time: day(2, 16, 30) });
  await record(
    "delete",
    "Delete LegacyInvoice.php",
    [{ path: "app/Services/LegacyInvoice.php", before: "<?php\n\nnamespace App\\Services;\n\nfinal class LegacyInvoice\n{\n}\n", after: null }],
    day(2, 16, 42),
  );
  const refactoring: LhRecordedFile[] = [{ path: user, before: v2, after: v3 }];
  if (caller) refactoring.push({ path: caller, before: mockFiles[caller].replaceAll("->totalSpent()", "->spent()"), after: mockFiles[caller] });
  await record("refactoring", "Refactoring: Rename method spent() to totalSpent()", refactoring, day(1, 10, 12));
  if (checkout) {
    await record(
      "vcs",
      "Update from VCS",
      [{ path: "app/Services/Checkout.php", before: checkout.replace("\n", "\n// TODO: remove the legacy invoice fallback\n"), after: checkout }],
      day(1, 14, 3),
    );
  }
  if (routes) {
    await record(
      "external",
      "External change",
      [{ path: "routes/web.php", before: older(routes, "Route::view('/about', 'pages.about')->name('about');\n", ""), after: routes }],
      now - 3 * HOUR,
    );
  }
  await record("save", "Saved", [{ path: user, before: v3, after: current }], now - 20 * MIN);
}

const seeded = seed().catch((e) => console.warn("local history demo failed", e));

// ---- commands ----------------------------------------------------------------------------

registerMock("local_history_list", async ({ path, dir, filter }) => {
  await seeded;
  return lh({ op: "list", path, dir, filter: filter ?? "" });
});
registerMock("local_history_content", async ({ path, rev }) => {
  await seeded;
  return lh({ op: "content", path, rev });
});
registerMock("local_history_folder_changes", async ({ path, rev }) => {
  await seeded;
  return lh({ op: "folderChanges", path, rev });
});
registerMock("local_history_selection", async ({ path, text, start, end }) => {
  await seeded;
  return lh({ op: "selection", path, text, start, end });
});
registerMock("local_history_recent", async ({ limit }) => {
  await seeded;
  return lh({ op: "recent", limit: limit ?? 50 });
});
registerMock("local_history_change_files", async ({ from, to }) => {
  await seeded;
  return lh({ op: "changeFiles", from, to });
});
registerMock("local_history_put_label", async ({ name }) => {
  await seeded;
  const label = String(name ?? "").trim();
  if (!label) throw new Error("The label name is empty");
  const id = await lh<number>({ op: "label", name: label, time: Date.now() });
  changed();
  return id;
});
registerMock("local_history_patch", async ({ path, dir, revPath, rev }) => {
  await seeded;
  const current: Record<string, string | null> = {};
  for (const [p, t] of Object.entries(mockFiles)) if (under(p, String(path))) current[p] = t;
  if (!dir) current[String(path)] = mockFiles[String(path)] ?? null;
  return lh({ op: "patch", path, dir, revPath, rev, current });
});
registerMock("local_history_revert", async ({ target, name }) => {
  await seeded;
  const plan = await lh<{ ops: { path: string; kind: string; text?: string }[]; skipped: string[] }>({ op: "revert", target });
  const files: LhRecordedFile[] = [];
  const done: string[] = [];
  for (const op of plan.ops) {
    const before = mockFiles[op.path] ?? null;
    if (op.kind === "write") mockFiles[op.path] = op.text ?? "";
    else if (op.kind === "delete") delete mockFiles[op.path];
    else continue;
    files.push({ path: op.path, before, after: mockFiles[op.path] ?? null });
    done.push(op.path);
  }
  await record("revert", String(name), files);
  return { changed: done, skipped: plan.skipped, failed: [] };
});
registerMock("local_history_record", async ({ kind, name, files }) => {
  await seeded;
  await record(kind as ChangeKind, String(name), files as LhRecordedFile[]);
});
registerMock("local_history_begin", async ({ kind, name }) => {
  await seeded;
  const token = ++tokens;
  operation = { token, group: token, kind: kind as ChangeKind, name: String(name) };
  return token;
});
registerMock("local_history_end", async ({ token }) => {
  const op = operation;
  if (!op || op.token !== token) return;
  const group = op.group;
  operation = null;
  await lh({ op: "closeGroup", group });
});
registerMock("local_history_snapshot", async ({ paths }) => {
  await seeded;
  for (const p of (paths as string[]) ?? []) {
    for (const [f, text] of Object.entries(mockFiles)) {
      if (under(f, p)) await lh({ op: "baseline", path: f, text, time: Date.now() });
    }
  }
});

// ---- recording what the other mocks do ----------------------------------------------------

function wrap(cmd: string, around: (args: Record<string, unknown>, run: () => Promise<unknown>) => Promise<unknown>): void {
  const inner = mockHandler(cmd);
  if (!inner) return;
  registerMock(cmd, (args) => around(args, async () => inner(args)));
}

const snapshotOf = (path: string) => Object.entries(mockFiles).filter(([p]) => under(p, path));

for (const cmd of ["fs_write_text", "fs_write_file"]) {
  wrap(cmd, async (args, run) => {
    const path = String(args.path);
    const before = mockFiles[path] ?? null;
    const out = await run();
    void seeded.then(() => record("save", "Saved", [{ path, before, after: mockFiles[path] ?? null }]));
    return out;
  });
}
wrap("fs_read_text", async (args, run) => {
  const out = await run();
  const path = String(args.path);
  const text = mockFiles[path];
  if (text !== undefined) void seeded.then(() => lh({ op: "baseline", path, text, time: Date.now() }));
  return out;
});
wrap("fs_create_file", async (args, run) => {
  const out = await run();
  const path = String(args.path);
  void seeded.then(() => record("create", `Create file ${nameOf(path)}`, [{ path, before: null, after: "" }]));
  return out;
});
wrap("fs_delete", async (args, run) => {
  const path = String(args.path);
  const gone = snapshotOf(path);
  const out = await run();
  void seeded.then(() => record("delete", `Delete ${nameOf(path)}`, gone.map(([p, t]) => ({ path: p, before: t, after: null }))));
  return out;
});
wrap("fs_rename", async (args, run) => {
  const from = String(args.from);
  const to = String(args.to);
  const moved = snapshotOf(from);
  const out = await run();
  const rename = parentOf(from) === parentOf(to);
  const name = rename ? `Rename ${nameOf(from)} to ${nameOf(to)}` : `Move ${nameOf(from)} to ${parentOf(to) || "the project root"}`;
  const files = moved.flatMap(([p, t]) => [
    { path: p, before: t, after: null },
    { path: to + p.slice(from.length), before: null, after: t, from: p },
  ]);
  void seeded.then(() => record(rename ? "rename" : "move", name, files));
  return out;
});
