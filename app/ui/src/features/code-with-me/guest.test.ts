import { afterEach, describe, expect, it } from "vitest";
import { PRESETS, type SessionView } from "@/ipc/collab";
import { adoptWorkspace, closeWorkspace, isCollabGuest } from "@/state/workspace";
import { toolWindowsFor, registerToolWindow } from "@/features/registry";
import { fileMenuEntries } from "@/features/project/fileMenu";
import { Folder } from "lucide-solid";
import { EditorState } from "@codemirror/state";
import { applySession } from "./store";
import { guestCanEdit, guestLevel, guestTitle, hiddenForGuest } from "./guest";
import { latencyClass } from "./GuestStatusItem";
import { extensionsFor } from "@/editor/extensions";

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
    expect(labels()).toContain("Rename…");
    join(PRESETS.readOnly);
    expect(labels()).not.toContain("Rename…");
    expect(labels()).not.toContain("Delete…");
    expect(labels()).not.toContain("New");
    expect(labels()).toContain("Find in Files…");
    closeWorkspace();
    join(PRESETS.editFiles);
    expect(labels()).toContain("Rename…");
    expect(labels()).toContain("New");
  });
});

describe("a guest's editors", () => {
  it("are read-only without Files: Edit", () => {
    expect(extensionsFor("app/User.php").length).toBe(0);
    join(PRESETS.readOnly);
    const exts = extensionsFor("app/User.php");
    expect(exts.length).toBe(1);
    const state = EditorState.create({ doc: "x", extensions: exts });
    expect(state.readOnly).toBe(true);
    closeWorkspace();
    join(PRESETS.editFiles);
    expect(extensionsFor("app/User.php").length).toBe(0);
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
