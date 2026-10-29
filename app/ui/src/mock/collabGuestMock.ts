/**
 * Browser-mode Code With Me guest: `?project=cwm-guest` opens the demo project as the host
 * "Robert" shares it with the guest "Alex" (`&permission=edit` for Edit files, Read-only
 * otherwise). With `&follow` the host is in `OrderController.php` and the guest follows it
 * (`&follow=all`: the host asked everyone to follow); without it the guest does not follow on
 * joining, so the window starts on the demo's `User.php`. The project, search and language
 * features are the demo's own mocks, as if the host answered them. Nothing leaves the page.
 */
import * as Y from "yjs";
import { emitMock, invoke, registerMock } from "@/ipc";
import { encodeOpened, PRESETS, type Permissions, type Presence, type SessionView } from "@/ipc/collab";
import { contentHash } from "@/state/editor";
import type { WorkspaceInfo } from "@/ipc/workspace";
import { demoProjectName } from "./demoProject";
import { setSetting } from "./settingsMock";

const params = () => new URLSearchParams(typeof location === "undefined" ? "" : location.search);

/** `?project=cwm-guest`: the window is a guest of a demo session. */
export const isCollabGuestDemo = (): boolean => params().get("project") === "cwm-guest";

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

const permissions = (): Permissions => (params().get("permission") === "edit" ? PRESETS.editFiles : PRESETS.readOnly);

/** Where the host is with `&follow`. */
const HOST_PRESENCE: Presence = {
  file: "app/Http/Controllers/OrderController.php",
  caret: { line: 24, column: 9 },
  viewport: { topLine: 14, bottomLine: 46 },
};

const idle: SessionView = { role: "none", canHost: false, participants: [], files: [] };

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
  };
}

if (isCollabGuestDemo()) {
  // The framework balloon would cover screenshots; announce only when asked.
  if (!params().has("announce")) setSetting("project.announcedFrameworks", [`${collabGuestWorkspace().root}#laravel`], "user");
  let joined = true;
  registerMock("collab_status", () => (joined ? view() : idle));
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
  registerMock("collab_leave", () => {
    joined = false;
    emitMock("collab://ended", { reason: "You left the session." });
    emitMock("collab://status", idle);
  });
}
