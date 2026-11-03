import { afterEach, describe, expect, it } from "vitest";
import { PRESETS, type SessionView } from "@/ipc/collab";
import { adoptWorkspace, closeWorkspace, isCollabGuest } from "@/state/workspace";
import { toolWindowsFor, registerToolWindow } from "@/features/registry";
import { fileMenuEntries } from "@/features/project/fileMenu";
import { Folder } from "lucide-solid";
import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { applySession } from "./store";
import { guestCanEdit, guestLevel, guestTitle, hiddenForGuest, trackGuestProject } from "./guest";
import { latencyClass } from "./GuestStatusItem";
import { extensionsFor } from "@/editor/extensions";
import { balloons } from "@/keymap";

const info = { name: "shop", root: "cwm://Robert/shop", collab: { host: "Robert", label: "Robert (workstation)" } };

function join(permissions: SessionView["permissions"] = PRESETS.readOnly) {
  adoptWorkspace(info);
  applySession({
    role: "guest",
    canHost: false,
    me: 2,
    participants: [],
    files: [],
    permissions,
    hostLabel: info.collab.label,
    workspace: info,
  });
}

afterEach(() => {
  closeWorkspace();
  applySession({ role: "none", canHost: false, participants: [], files: [] });
});

describe("a guest's tool windows", () => {
  it("hides what would use the host's secrets or accounts", () => {
    expect(hiddenForGuest("ai-chat")).toBe(false);
    join();
    for (const id of ["ai-chat", "database", "services", "debug", "pull-requests"]) expect(hiddenForGuest(id)).toBe(true);
    for (const id of ["project", "structure", "problems", "git"]) expect(hiddenForGuest(id)).toBe(false);
  });

  it("shows the Commit window only with Full access to version control", () => {
    join(PRESETS.editFiles);
    expect(hiddenForGuest("commit")).toBe(true);
    closeWorkspace();
    join(PRESETS.fullAccess);
    expect(hiddenForGuest("commit")).toBe(false);
  });

  it("filters the stripe, and shows everything again outside a session", () => {
    registerToolWindow({ id: "ai-chat", title: "AI", icon: Folder, anchor: "right", order: 1, component: () => null });
    registerToolWindow({ id: "problems", title: "Problems", icon: Folder, anchor: "right", order: 2, component: () => null });
    expect(toolWindowsFor("right").map((t) => t.id)).toContain("ai-chat");
    join();
    expect(toolWindowsFor("right").map((t) => t.id)).not.toContain("ai-chat");
    expect(toolWindowsFor("right").map((t) => t.id)).toContain("problems");
    closeWorkspace();
    expect(toolWindowsFor("right").map((t) => t.id)).toContain("ai-chat");
  });
});

describe("a guest's permissions", () => {
  it("allow changing files only with Files: Edit", () => {
    expect(guestCanEdit()).toBe(true); // not a guest: unchanged behavior
    join(PRESETS.readOnly);
    expect(isCollabGuest()).toBe(true);
    expect(guestCanEdit()).toBe(false);
    expect(guestLevel()).toBe("readOnly");
    closeWorkspace();
    join(PRESETS.editFiles);
    expect(guestCanEdit()).toBe(true);
    expect(guestLevel()).toBe("editFiles");
  });

  it("shape the Project view's context menu", () => {
    const labels = () => fileMenuEntries({ name: "User.php", path: "app/User.php", kind: "file" }).map((e) => (e.type === "item" || e.type === "submenu" ? e.label : "-"));
    expect(labels()).toContain("Refactor");
    join(PRESETS.readOnly);
    expect(labels()).not.toContain("Refactor");
    expect(labels()).not.toContain("Delete…");
    expect(labels()).not.toContain("New");
    expect(labels()).toContain("Find in Files…");
    closeWorkspace();
    join(PRESETS.editFiles);
    expect(labels()).toContain("Refactor");
    expect(labels()).toContain("New");
  });
});

describe("a guest's editors", () => {
  const readOnly = () => EditorState.create({ doc: "x", extensions: extensionsFor("app/User.php") }).readOnly;

  it("are read-only without Files: Edit", () => {
    expect(readOnly()).toBe(false);
    join(PRESETS.readOnly);
    expect(readOnly()).toBe(true);
    closeWorkspace();
    join(PRESETS.editFiles);
    expect(readOnly()).toBe(false);
    closeWorkspace();
  });

  it("follow a permission change of the host while they are open", () => {
    trackGuestProject();
    join(PRESETS.editFiles);
    const parent = document.createElement("div");
    const view = new EditorView({ state: EditorState.create({ doc: "x", extensions: extensionsFor("app/User.php") }), parent });
    expect(view.state.readOnly).toBe(false);
    join(PRESETS.readOnly);
    expect(view.state.readOnly).toBe(true);
    join(PRESETS.fullAccess);
    expect(view.state.readOnly).toBe(false);
    view.destroy();
    closeWorkspace();
  });

  it("are announced only when the permission really changed", () => {
    trackGuestProject();
    const seen = new Set(balloons().map((b) => b.id));
    const told = () => balloons().filter((b) => !seen.has(b.id) && b.text === "Code With Me").map((b) => b.detail);
    join(PRESETS.readOnly);
    // Status updates (presence, connection paths) repeat the same permission.
    join(PRESETS.readOnly);
    join(PRESETS.readOnly);
    expect(told()).toEqual([]);
    join(PRESETS.editFiles);
    expect(told()).toEqual(["The host lets you edit files now."]);
    closeWorkspace();
  });
});

describe("the guest title", () => {
  it("is the project, the host and Code With Me", () => {
    expect(guestTitle()).toBeUndefined();
    join();
    expect(guestTitle()).toBe("shop — Robert (Code With Me)");
  });
});

describe("latency class", () => {
  it("is good, fair, poor or unknown", () => {
    expect(latencyClass(20)).toBe("good");
    expect(latencyClass(120)).toBe("fair");
    expect(latencyClass(400)).toBe("poor");
    expect(latencyClass(null)).toBe("unknown");
  });
});
