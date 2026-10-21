/**
 * Browser-mode Code With Me guest: `?project=cwm-guest` opens the demo project as the host
 * "Robert" shares it with the guest "Alex" (`&permission=edit` for Edit files, Read-only
 * otherwise). The project, search and language features are the demo's own mocks, as if the
 * host answered them. Nothing leaves the page.
 */
import { emitMock, registerMock } from "@/ipc";
import { PRESETS, type Permissions, type SessionView } from "@/ipc/collab";
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
  };
}

if (isCollabGuestDemo()) {
  // The framework balloon would cover screenshots; announce only when asked.
  if (!params().has("announce")) setSetting("project.announcedFrameworks", [`${collabGuestWorkspace().root}#laravel`], "user");
  let joined = true;
  registerMock("collab_status", () => (joined ? view() : idle));
  registerMock("collab_ping", () => 23);
  registerMock("collab_leave", () => {
    joined = false;
    emitMock("collab://ended", { reason: "You left the session." });
    emitMock("collab://status", idle);
  });
}
