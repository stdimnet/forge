import { describe, expect, it } from "vitest";
import { joinDir, suggestWorktreePath } from "./extrasModel";
import { describeSigningKey } from "./SigningSettings";
import type { Worktree } from "./extrasApi";

const wt = (path: string, main = false): Worktree => ({
  path,
  head: "abc",
  branch: "main",
  detached: false,
  bare: false,
  main,
  current: main,
  locked: false,
  lockReason: null,
  prunable: false,
});

describe("Get from VCS and worktree paths", () => {
  it("joins directories with the parent's separator", () => {
    expect(joinDir("~/Projects/", "shop")).toBe("~/Projects/shop");
    expect(joinDir("C:\\Users\\m\\Projects", "shop")).toBe("C:\\Users\\m\\Projects\\shop");
  });

  it("suggests a sibling of the main worktree named after the branch", () => {
    const list = [wt("/home/m/src/shop", true), wt("/home/m/src/shop-x")];
    expect(suggestWorktreePath(list, "feature/checkout flow")).toBe("/home/m/src/shop-feature-checkout-flow");
    expect(suggestWorktreePath([], "x")).toBe("");
  });

  it("describes the chosen signing key", () => {
    expect(describeSigningKey("")).toBe("Key from git config (user.signingkey)");
    expect(describeSigningKey("openpgp:3AA5C34371567BD2")).toBe("GPG key 3AA5C34371567BD2");
    expect(describeSigningKey("ssh:/home/m/.ssh/id_ed25519.pub")).toBe("SSH key /home/m/.ssh/id_ed25519.pub");
  });
});
