import { beforeEach, describe, expect, it, vi } from "vitest";
import { balloons } from "@/keymap";
import { emitMock, registerMock } from "@/ipc";
import { hideWaiting, showJoinRequest, showWaiting, trackApprovals } from "./approval";

const texts = () => balloons().map((b) => b.text);
const click = (label: string, balloon = balloons()[balloons().length - 1]) => balloon.actions!.find((a) => a.label === label)!.run();

const answers: [number, boolean][] = [];
registerMock("collab_answer_join", ({ id, accept }) => {
  answers.push([Number(id), Boolean(accept)]);
});
const cancelled = vi.fn();
registerMock("collab_cancel_join", cancelled);

beforeEach(() => {
  answers.length = 0;
  cancelled.mockClear();
  hideWaiting();
});

describe("host approval", () => {
  it("asks about the guest with the code and the permissions it would get", () => {
    showJoinRequest({ id: 7, name: "Alex", code: "482 913", level: "readOnly" });
    const b = balloons().find((x) => x.text === "Alex wants to join the session")!;
    expect(b.detail).toContain("482 913");
    expect(b.detail).toContain("Read-only");
    expect(b.sticky).toBe(true);
    expect(b.actions!.map((a) => a.label)).toEqual(["Accept", "Decline"]);
  });

  it("answers and removes the balloon", async () => {
    showJoinRequest({ id: 8, name: "Sam", code: "111 222", level: "editFiles" });
    const b = balloons().find((x) => x.text === "Sam wants to join the session")!;
    click("Accept", b);
    await vi.waitFor(() => expect(answers).toEqual([[8, true]]));
    expect(texts()).not.toContain("Sam wants to join the session");
    showJoinRequest({ id: 9, name: "Kim", code: "333 444", level: "readOnly" });
    click("Decline", balloons().find((x) => x.text === "Kim wants to join the session")!);
    await vi.waitFor(() => expect(answers).toContainEqual([9, false]));
  });

  it("drops a request that expired", async () => {
    await trackApprovals();
    emitMock("collab://join-request", { id: 21, name: "Lee", code: "555 666", level: "readOnly" });
    await vi.waitFor(() => expect(texts()).toContain("Lee wants to join the session"));
    emitMock("collab://join-closed", { id: 21 });
    await vi.waitFor(() => expect(texts()).not.toContain("Lee wants to join the session"));
  });
});

describe("a waiting guest", () => {
  it("sees the code and can cancel", async () => {
    showWaiting("Robert", "482 913");
    const b = balloons().find((x) => x.text === "Waiting for Robert to accept…")!;
    expect(b.detail).toContain("482 913");
    click("Cancel", b);
    await vi.waitFor(() => expect(cancelled).toHaveBeenCalled());
    hideWaiting();
    expect(texts()).not.toContain("Waiting for Robert to accept…");
  });
});
