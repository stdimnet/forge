import { afterEach, describe, expect, it } from "vitest";
import { render } from "solid-js/web";
import "@/mock/workspaceMock";
import "@/mock/gitMock";
import "@/mock/gitOpsMock";
import { findToolWindow } from "@/features/registry";
import "./index";

let dispose: (() => void) | undefined;
afterEach(() => {
  dispose?.();
  dispose = undefined;
});

describe("compact Git tool window", () => {
  it("shows its tabs in the tool window header, not in a row of their own", () => {
    const git = findToolWindow("git")!;
    expect(git.headerContent).toBeTypeOf("function");
    const header = document.createElement("div");
    const body = document.createElement("div");
    document.body.append(header, body);
    const disposeHeader = render(() => git.headerContent!(), header);
    const Body = git.component;
    const disposeBody = render(() => <Body />, body);
    dispose = () => (disposeHeader(), disposeBody());

    const tabs = [...header.querySelectorAll("[role=tab]")].map((t) => t.textContent?.trim());
    expect(tabs.slice(0, 2)).toEqual(["Log", "Worktrees"]);
    expect(body.querySelector(".git-panel")).not.toBeNull();
    expect(body.querySelector(".git-tabs, [role=tablist]")).toBeNull();
    // One toolbar row: the log filters.
    expect(body.querySelectorAll(".git-log .panel-toolbar").length).toBe(1);
  });
});
