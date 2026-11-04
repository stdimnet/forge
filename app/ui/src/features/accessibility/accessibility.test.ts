import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { announce, liveRegion } from "./announce";
import { describeDialog, installDialogAccessibility, labelControls, openDialogs, tabbables } from "./dialogs";
import { focusArea, trackAreaFocus } from "./areas";
import { presentationOf, type Lookup } from "./presentation";
import type { ActionInfo } from "@/ipc/keymap";
import type { CommandInvocation } from "@/state/commands";

const tick = (ms = 0) => new Promise((r) => setTimeout(r, ms));

beforeAll(() => {
  installDialogAccessibility(document);
  trackAreaFocus(document);
});

afterEach(() => {
  for (const el of [...document.body.children]) if (!(el as HTMLElement).dataset.focusGuard && !(el as HTMLElement).dataset.liveRegion) el.remove();
});

function html(markup: string): HTMLElement {
  const host = document.createElement("div");
  host.innerHTML = markup;
  document.body.appendChild(host);
  return host;
}

describe("live regions", () => {
  it("announces text through a polite and an assertive region", async () => {
    announce("Line 3, column 5.");
    announce("Build failed", "assertive");
    await tick(40);
    expect(liveRegion("polite").textContent).toBe("Line 3, column 5.");
    expect(liveRegion("polite").getAttribute("aria-live")).toBe("polite");
    expect(liveRegion("assertive").getAttribute("role")).toBe("alert");
    expect(liveRegion("assertive").textContent).toBe("Build failed");
  });
});

describe("dialogs", () => {
  it("names a dialog after its title and marks it modal inside a backdrop", () => {
    const host = html(`<div class="popup-backdrop"><div class="dialog"><div class="dialog-title">Rename</div><input></div></div>`);
    const dialog = host.querySelector<HTMLElement>(".dialog")!;
    describeDialog(dialog);
    expect(dialog.getAttribute("role")).toBe("dialog");
    expect(dialog.getAttribute("aria-modal")).toBe("true");
    const title = document.getElementById(dialog.getAttribute("aria-labelledby")!);
    expect(title?.textContent).toBe("Rename");
  });

  it("connects a label to the control after it", () => {
    const host = html(`<label>New name:</label><input>`);
    labelControls(host);
    const input = host.querySelector("input")!;
    expect(input.labels?.[0]?.textContent).toBe("New name:");
  });

  it("traps Tab inside the topmost modal dialog and returns focus to the opener", async () => {
    const opener = html(`<button id="opener">Open</button><button id="behind">Behind</button>`);
    const button = opener.querySelector<HTMLButtonElement>("#opener")!;
    button.focus();
    const host = html(
      `<div class="modal-backdrop"><div role="dialog" aria-label="Commit"><input id="first"><button id="last">OK</button></div></div>`,
    );
    await tick();
    expect(openDialogs().map((d) => d.getAttribute("aria-label"))).toContain("Commit");
    const dialog = host.querySelector<HTMLElement>("[role=dialog]")!;
    expect(tabbables(dialog).map((e) => e.id)).toEqual(["first", "last"]);

    // Tab past the last control lands after the dialog: focus wraps to the first control.
    document.querySelector<HTMLElement>("[data-focus-guard='end']")!.focus();
    expect(document.activeElement?.id).toBe("first");
    // Shift+Tab before the first control lands before the dialog: wrap to the last one.
    opener.querySelector<HTMLButtonElement>("#behind")!.focus();
    expect(document.activeElement?.id).toBe("last");

    host.remove();
    await tick();
    await tick();
    expect(document.activeElement).toBe(button);
  });
});

describe("focus areas", () => {
  it("cycles between the visible areas and remembers the last control of each", () => {
    html(`
      <header class="main-toolbar"><button id="menu">Menu</button></header>
      <nav class="tool-stripe-left"><button id="project">Project</button></nav>
      <main class="editor-area"><div class="editor-slot"><div class="cm-content" tabindex="0" id="editor"></div></div></main>
      <footer class="status-bar"><button id="caret">1:1</button><button id="branch">main</button></footer>`);
    document.getElementById("editor")!.focus();
    expect(focusArea(1)).toBe("statusbar");
    expect(document.activeElement?.id).toBe("caret");
    document.getElementById("branch")!.focus();
    expect(focusArea(1)).toBe("toolbar");
    expect(focusArea(-1)).toBe("statusbar");
    // Back in the status bar: the control that had focus there last.
    expect(document.activeElement?.id).toBe("branch");
    expect(focusArea(-1)).toBe("editor");
    expect(document.activeElement?.id).toBe("editor");
  });
});

describe("Presentation Assistant", () => {
  const action = (id: string, title: string, display: string[]): ActionInfo =>
    ({ id, title, display, shortcuts: display, group: "", context: "any", native: false, modified: false }) as ActionInfo;
  const lookup: Lookup = {
    action: (id) =>
      ({ GotoAction: action("GotoAction", "Find Action…", ["Ctrl+Shift+A"]), ShowSettings: action("ShowSettings", "Settings…", ["Ctrl+Alt+S"]) })[id],
    alternate: () => ({ keymap: "macOS", title: "PHPStorm (macOS)", platform: "mac", display: { GotoAction: ["⇧⌘A"] } }),
  };
  const inv = (id: string, actionId?: string): CommandInvocation => ({ command: { id, title: id, run: vi.fn() }, actionId });

  it("shows the action title, its shortcut and the alternate keymap's shortcut", () => {
    expect(presentationOf(inv("GotoAction", "GotoAction"), { enabled: true, alternateKeymap: true }, lookup)).toEqual({
      id: "GotoAction",
      title: "Find Action",
      shortcuts: ["Ctrl+Shift+A"],
      alternate: { shortcut: "⇧⌘A", keymap: "PHPStorm (macOS)" },
    });
  });

  it("finds the action of a command invoked by its own id (toolbar buttons)", () => {
    const p = presentationOf(inv("ShowSettings"), { enabled: true, alternateKeymap: false }, lookup);
    expect(p).toMatchObject({ title: "Settings", shortcuts: ["Ctrl+Alt+S"] });
    expect(p?.alternate).toBeUndefined();
  });

  it("stays quiet when off or for internal commands", () => {
    expect(presentationOf(inv("GotoAction", "GotoAction"), { enabled: false, alternateKeymap: true }, lookup)).toBeNull();
    expect(presentationOf(inv("git.refreshInternal"), { enabled: true, alternateKeymap: true }, lookup)).toBeNull();
  });
});
