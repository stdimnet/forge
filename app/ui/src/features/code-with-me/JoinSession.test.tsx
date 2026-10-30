import { afterEach, describe, expect, it, vi } from "vitest";
import { registerMock } from "@/ipc";
import type { SessionView } from "@/ipc/collab";
import { joinDialogOpen, linkProblem, openJoinDialog, showJoinWaiting } from "./JoinSession";

const LINK = "forge://cwm/join?v=1&name=Robert&m=workstation&s=abc#h=x&k=y";
const joined: SessionView = { role: "guest", canHost: false, participants: [], files: [] };

let joinAnswer: (s: SessionView | Error) => void = () => undefined;
const cancel = vi.fn(() => joinAnswer(new Error("Cancelled.")));
registerMock("collab_parse_link", ({ link }) => {
  if (!String(link).includes("name=")) throw new Error("Invalid session link: name is missing");
  return { hostName: "Robert", machine: "workstation", hostLabel: "Robert (workstation)", sessionId: "abc", userName: "Alex" };
});
registerMock("collab_join", () => new Promise((resolve, reject) => (joinAnswer = (r) => (r instanceof Error ? reject(r) : resolve(r)))));
registerMock("collab_cancel_join", cancel);

const dialog = () => document.querySelector(".cwm-join-dialog") as HTMLElement | null;
const button = (label: string) => [...(dialog()?.querySelectorAll("button") ?? [])].find((b) => b.textContent?.trim() === label) as HTMLButtonElement | undefined;
const typeLink = (text: string) => {
  const input = dialog()!.querySelector("input")!;
  input.value = text;
  input.dispatchEvent(new InputEvent("input", { bubbles: true }));
};

afterEach(() => {
  button("Cancel")?.click();
  cancel.mockClear();
});

describe("the Join Session dialog", () => {
  it("checks the link before asking the backend", () => {
    expect(linkProblem("")).toContain("Paste");
    expect(linkProblem("https://example.com")).toContain("not a Code With Me link");
    expect(linkProblem(` ${LINK} `)).toBeNull();
    expect(linkProblem("cwm1:abc")).toBeNull();
  });

  it("shows the host and machine, then the verification code while the host decides", async () => {
    const result = openJoinDialog();
    await vi.waitFor(() => expect(dialog()).not.toBeNull());
    expect(button("Join")!.disabled).toBe(true);
    typeLink(LINK);
    await vi.waitFor(() => expect(dialog()!.querySelector("[data-testid=cwm-join-summary]")).not.toBeNull());
    const summary = dialog()!.querySelector("[data-testid=cwm-join-summary]")!.textContent!;
    expect(summary).toContain("Robert");
    expect(summary).toContain("workstation");
    expect(summary).toContain("Alex");
    button("Join")!.click();
    await vi.waitFor(() => expect(dialog()!.textContent).toContain("Connecting to Robert (workstation)"));
    showJoinWaiting("Robert (workstation)", "482 913");
    await vi.waitFor(() => expect(dialog()!.querySelector("[data-testid=cwm-join-code]")?.textContent).toBe("482 913"));
    expect(dialog()!.textContent).toContain("Waiting for Robert (workstation) to accept…");
    expect(button("Join")).toBeUndefined();
    joinAnswer(joined);
    expect(await result).toEqual(joined);
    expect(joinDialogOpen()).toBe(false);
  });

  it("cancels a waiting join and shows a refusal in the dialog", async () => {
    const result = openJoinDialog(LINK);
    await vi.waitFor(() => expect(button("Join")?.disabled).toBe(false));
    button("Join")!.click();
    showJoinWaiting("Robert", "111 222");
    await vi.waitFor(() => expect(dialog()!.textContent).toContain("111 222"));
    button("Cancel")!.click();
    expect(cancel).toHaveBeenCalled();
    // Back at the link; the dialog stays open.
    await vi.waitFor(() => expect(button("Join")).toBeDefined());
    button("Join")!.click();
    joinAnswer(new Error("The host declined your request."));
    await vi.waitFor(() => expect(dialog()!.querySelector("[role=alert]")?.textContent).toBe("The host declined your request."));
    expect(button("Try Again")).toBeDefined();
    button("Cancel")!.click();
    expect(await result).toBeNull();
  });

  it("reports a link the backend refuses", async () => {
    void openJoinDialog();
    await vi.waitFor(() => expect(dialog()).not.toBeNull());
    typeLink("forge://cwm/join?v=1");
    await vi.waitFor(() => expect(dialog()!.querySelector(".dialog-error")?.textContent).toContain("name is missing"));
    expect(button("Join")!.disabled).toBe(true);
  });
});
