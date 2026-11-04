import { describe, expect, it } from "vitest";
import { flatpakStatusText } from "./index";

const base = { inFlatpak: true, flatpakId: "dev.forge.ide", enabled: true, active: true, hostOsVisible: true };

describe("flatpakStatusText", () => {
  it("names the sandbox and where tools run", () => {
    expect(flatpakStatusText(base, true)).toContain("flatpak-spawn --host");
    expect(flatpakStatusText(base, true)).toContain("dev.forge.ide");
    expect(flatpakStatusText(base, false)).toContain("inside the sandbox");
  });

  it("says when Forge runs natively", () => {
    expect(flatpakStatusText({ ...base, inFlatpak: false }, true)).toContain("does not run in a Flatpak sandbox");
  });
});
