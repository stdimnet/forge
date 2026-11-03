/**
 * Browser-mode Code With Me guest: `?project=cwm-guest` opens the demo project as the host
 * "Robert" shares it with the guest "Alex" (`&permission=edit` for Edit files, Read-only
 * otherwise). With `&follow` the host is in `OrderController.php` and the guest follows it
 * (`&follow=all`: the host asked everyone to follow); without it the guest does not follow on
 * joining, so the window starts on the demo's `User.php`. The project, search and language
 * features are the demo's own mocks, as if the host answered them. Nothing leaves the page.
 *
 * `&share` adds the host's shared terminal "Local" and its run of `UserTest` (S5);
 * `&permission=full` gives the guest Full access (it can type, run and stop); `&unshare`
 * makes the host stop sharing "Local" two seconds after it is shown.
 *
 * `&reconnecting` shows the window while the connection to the host is being restored;
 * `&reconnecting=resume` restores it after three seconds (`collab://resumed`).
 */
import * as Y from "yjs";
import { emitMock, invoke, registerMock } from "@/ipc";
import { encodeOpened, encodeSynced, PRESETS, sharedRunId, type ConnectionState, type Permissions, type Presence, type SessionView, type SharedRun, type SharedTerminal } from "@/ipc/collab";
import type { RunConfigurations } from "@/ipc/run";
import { contentHash } from "@/state/editor";
import type { WorkspaceInfo } from "@/ipc/workspace";
import { demoProjectName } from "./demoProject";
import { setSetting } from "./settingsMock";

const params = () => new URLSearchParams(typeof location === "undefined" ? "" : location.search);

/**
 * `?project=cwm-join`: the window starts with its own project `~/Downloads/legacy-crm` open
 * in Safe Mode; Tools | Code With Me | Join Session… (any link) joins the demo session, and
 * the window gets the host's project. With `&welcome` it starts on the Welcome screen.
 */
export const isCollabJoinDemo = (): boolean => params().get("project") === "cwm-join";

/** The `?project=cwm-join` window's own project. */
export const JOIN_DEMO_ROOT = "~/Downloads/legacy-crm";

let joined = params().get("project") === "cwm-guest";

/** `?project=cwm-guest` (or `cwm-join` after joining): the window is a guest of a demo session. */
export const isCollabGuestDemo = (): boolean => joined;

/** Secret files the host does not share (a subset of `forge_collab::excludes::DEFAULT_PATTERNS`). */
const HIDDEN = [/(^|\/)\.env(\.(?!example$|dist$)[^/]*)?$/, /\.(pem|key|p12|pfx|kdbx)$/, /(^|\/)(id_rsa|id_ed25519)[^/]*$/, /(^|\/)auth\.json$/, /(^|\/)\.(npmrc|pypirc|netrc)$/];

/** Whether the browser-mode host hides `path` from the guest. */
export const hiddenFromGuest = (path: string): boolean => isCollabGuestDemo() && HIDDEN.some((re) => re.test(path));

const HOST = "Robert";
const HOST_LABEL = "Robert (workstation)";

export function collabGuestWorkspace(): WorkspaceInfo {
  return {
    name: demoProjectName,
    root: `cwm://${HOST}/${demoProjectName}`,
    collab: { host: HOST, label: HOST_LABEL },
  };
}

const permissions = (): Permissions => {
  const p = params().get("permission");
  return p === "edit" ? PRESETS.editFiles : p === "full" ? PRESETS.fullAccess : PRESETS.readOnly;
};

/** The host's terminal and runs with `&share`. */
const sharing = () => params().has("share");
const terminals: SharedTerminal[] = [{ id: 1, title: "Local", owner: 1, cols: 110, rows: 24, running: true }];
const runs: SharedRun[] = [{ id: 1, title: "UserTest", kind: "phpunit", startedBy: 1, running: true }];

/** What the host's terminal showed before the guest joined. */
const TRANSCRIPT = [
  "\x1b[32mdev@forge\x1b[0m:\x1b[34m~/Projects/acme-shop\x1b[0m$ php artisan migrate:status",
  "",
  "  Migration name .................................................. Batch / Status",
  "  2026_01_01_000000_create_users_table ................................ [1] Ran",
  "  2026_01_01_000000_create_orders_table ............................... [1] Ran",
  "  2026_02_14_120000_add_discount_to_orders ............................ Pending",
  "",
  "",
].join("\n");

/** Where the host is with `&follow`. */
const HOST_PRESENCE: Presence = {
  file: "app/Http/Controllers/OrderController.php",
  caret: { line: 24, column: 9 },
  viewport: { topLine: 14, bottomLine: 46 },
};

const idle: SessionView = { role: "none", canHost: false, participants: [], files: [] };

let connection: ConnectionState = params().has("reconnecting")
  ? { state: "reconnecting", attempt: 3, nextRetrySecs: 4, error: "Cannot connect to the host: network is unreachable" }
  : { state: "connected" };
let shownTerminals = terminals;

function view(): SessionView {
  const p = permissions();
  return {
    role: "guest",
    canHost: false,
    me: 2,
    participants: [
      { id: 1, name: HOST, role: "host", color: 1, permissions: PRESETS.fullAccess },
      { id: 2, name: "Alex", role: "guest", color: 2, permissions: p },
    ],
    files: [],
    permissions: p,
    hostLabel: HOST_LABEL,
    workspace: collabGuestWorkspace(),
    presence: params().has("follow") ? [{ participant: 1, ...HOST_PRESENCE }] : [],
    followEveryone: params().get("follow") === "all",
    terminals: sharing() && p.terminal !== "none" ? shownTerminals : [],
    runs: sharing() && p.run !== "none" ? runs : [],
    connection,
  };
}

if (isCollabGuestDemo() || isCollabJoinDemo()) {
  // The framework balloon would cover screenshots; announce only when asked.
  if (!params().has("announce")) setSetting("project.announcedFrameworks", [`${collabGuestWorkspace().root}#laravel`], "user");
  registerMock("collab_status", () => (joined ? view() : idle));
  registerMock("collab_join", async ({ link }) => {
    if (joined) throw new Error("A Code With Me session is already running.");
    await invoke("collab_parse_link", { link });
    joined = true;
    emitMock("collab://status", view());
    return view();
  });
  registerMock("collab_ping", () => 23);
  if (!params().has("follow")) setSetting("codeWithMe.followHostOnJoin", false, "user");
  // The host's files as shared documents (following the host opens them), read-only here.
  let nextDoc = 1;
  registerMock("collab_doc_open", async ({ path }) => {
    const text = String(await invoke<string>("fs_read_file", { path }));
    const doc = new Y.Doc();
    doc.getText("content").insert(0, text);
    return encodeOpened({ docId: nextDoc++, path: String(path), update: Y.encodeStateAsUpdate(doc), saved: { hash: contentHash(text) }, readOnly: permissions().files !== "edit" });
  });
  registerMock("collab_doc_save", () => undefined);
  // Nothing changed on the demo host while the connection was down.
  registerMock("collab_doc_sync", ({ docId, stateVector }) => {
    void stateVector;
    return encodeSynced({ docId: Number(docId), path: "", update: Y.encodeStateAsUpdate(new Y.Doc()), saved: { hash: "" }, readOnly: permissions().files !== "edit", stateVector: Y.encodeStateVector(new Y.Doc()) });
  });
  if (params().get("reconnecting") === "resume") {
    setTimeout(() => {
      connection = { state: "connected" };
      emitMock("collab://status", view());
      emitMock("collab://resumed", {});
    }, 3000);
  }

  // The host's shared terminal: a fake shell that starts with what it showed before.
  registerMock("collab_terminal_attach", async ({ terminal, id }) => {
    const t = terminals.find((x) => x.id === Number(terminal));
    if (!t || !sharing()) throw new Error("The terminal is no longer shared.");
    // Loaded lazily: the terminal mock depends on mocks that load after this one.
    const { startMockShell } = await import("./terminalMock");
    startMockShell(String(id), TRANSCRIPT);
    if (params().has("unshare")) {
      setTimeout(() => {
        shownTerminals = [];
        emitMock("collab://status", view());
      }, 2000);
    }
    return { scrollback: "", cols: t.cols, rows: t.rows, running: t.running };
  });
  // The host's runs: the demo runs the configuration here, on the shared run's events.
  const startDemoRun = async (run: SharedRun) => {
    const file = await invoke<RunConfigurations>("run_configurations_load");
    const configuration = file.configurations.find((c) => c.name === run.title);
    if (configuration) await invoke("run_start", { request: { runId: sharedRunId(run.id), configuration, testRunner: true } });
  };
  registerMock("collab_run_attach", ({ run }) => {
    const r = runs.find((x) => x.id === Number(run));
    if (!r) throw new Error("The run is no longer shown.");
    setTimeout(() => void startDemoRun(r), 50);
    return [];
  });
  registerMock("collab_run_detach", () => undefined);
  registerMock("collab_run_start", ({ configuration }) => {
    if (permissions().run !== "full") throw new Error("Your permission does not allow running code on the host.");
    runs.push({ id: runs.length + 1, title: String(configuration), startedBy: 2, running: true });
    emitMock("collab://status", view());
  });
  registerMock("collab_run_stop", ({ run }) => invoke("run_stop", { runId: sharedRunId(Number(run)) }));
  registerMock("collab_run_input", ({ run, data }) => invoke("run_write", { runId: sharedRunId(Number(run)), data }));
  registerMock("collab_leave", () => {
    joined = false;
    emitMock("collab://ended", { reason: "You left the session." });
    emitMock("collab://status", idle);
  });
}
