import { readFileSync, readdirSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import {
  addMetainfoRelease,
  channelFeed,
  checksumFor,
  parseChecksums,
  pkgbuildFields,
  releaseChannel,
  srcinfo,
  stableVersion,
  triggerBlocks,
  updateFlatpakManifest,
  updatePkgbuild,
  workflowTriggerErrors,
} from "./releaseTools.mjs";

const root = resolve(import.meta.dirname, "../../..");
const read = (f) => readFileSync(resolve(root, f), "utf8");
const SHA = "ab".repeat(32);

describe("release channel", () => {
  it("maps tags to channels", () => {
    expect(releaseChannel({ tag: "v0.2.0", prerelease: false, version: "0.2.0" })).toEqual({
      channel: "stable",
      errors: [],
    });
    expect(releaseChannel({ tag: "v0.3.0-beta.1", prerelease: true, version: "0.3.0-beta.1" })).toEqual({
      channel: "beta",
      errors: [],
    });
  });

  it("refuses releases whose tag, version or pre-release flag disagree", () => {
    const errors = (r) => releaseChannel(r).errors.join("\n");
    expect(errors({ tag: "v0.3.0-beta.1", prerelease: false, version: "0.3.0-beta.1" })).toContain("publish it as a pre-release");
    expect(errors({ tag: "v0.3.0", prerelease: true, version: "0.3.0" })).toContain("no pre-release label");
    expect(errors({ tag: "v0.3.0", prerelease: false, version: "0.2.0" })).toContain("does not match");
    expect(errors({ tag: "beta", prerelease: true, version: "0.2.0" })).toContain("not vX.Y.Z");
  });
});

describe("workflow triggers", () => {
  const release = "name: Release\n# comment\non:\n  release:\n    types: [published]  # only this\n\njobs: {}\n";
  const ci = "name: CI\non:\n  workflow_dispatch:\n\njobs: {}\n";

  it("accepts the repository's workflows", () => {
    const dir = resolve(root, ".github/workflows");
    const workflows = readdirSync(dir).map((name) => ({ name, text: read(`.github/workflows/${name}`) }));
    expect(workflowTriggerErrors(workflows)).toEqual([]);
  });

  it("ignores comments and blank lines", () => {
    expect(triggerBlocks(release)).toEqual(["on:\n  release:\n    types: [published]"]);
    expect(workflowTriggerErrors([{ name: "release.yml", text: release }, { name: "ci.yml", text: ci }])).toEqual([]);
  });

  it("flags added or changed triggers", () => {
    const push = ci.replace("workflow_dispatch:", "workflow_dispatch:\n  push:");
    expect(workflowTriggerErrors([{ name: "release.yml", text: release }, { name: "ci.yml", text: push }])[0]).toContain(
      "ci.yml: triggers changed",
    );
    const types = release.replace("[published]", "[published, created]");
    expect(workflowTriggerErrors([{ name: "release.yml", text: types }])[0]).toContain("release.yml");
    const twice = `${ci}"on": [push]\n`;
    expect(workflowTriggerErrors([{ name: "release.yml", text: release }, { name: "x.yml", text: twice }])[0]).toContain(
      "found 2",
    );
    const scheduled = "on:\n  schedule:\n    - cron: '0 0 * * *'\n";
    expect(workflowTriggerErrors([{ name: "release.yml", text: release }, { name: "nightly.yml", text: scheduled }])).toHaveLength(1);
    expect(workflowTriggerErrors([{ name: "ci.yml", text: ci }])).toEqual(["release.yml is missing"]);
  });
});

describe("checksums", () => {
  it("reads sha256sum output", () => {
    const sums = parseChecksums(`${SHA}  Forge_0.2.0_amd64.deb\n${"cd".repeat(32)} *dist/Forge.AppImage\nnoise\n`);
    expect(checksumFor(sums, "Forge_0.2.0_amd64.deb")).toBe(SHA);
    expect(checksumFor(sums, "Forge.AppImage")).toBe("cd".repeat(32));
    expect(() => checksumFor(sums, "Forge_0.3.0_amd64.deb")).toThrow(/no entry/);
  });

  it("packages follow stable releases only", () => {
    expect(stableVersion("v0.2.0")).toBe("0.2.0");
    expect(() => stableVersion("v0.3.0-beta.1")).toThrow(/pre-release/);
    expect(() => stableVersion("main")).toThrow(/not a release tag/);
  });
});

describe("AUR", () => {
  const pkgbuild = read("packaging/aur/forge-bin/PKGBUILD");

  it("sets pkgver, pkgrel and the checksum", () => {
    const next = updatePkgbuild(pkgbuild.replace("pkgrel=1", "pkgrel=3"), { version: "0.2.0", sha256: SHA });
    const f = pkgbuildFields(next);
    expect(f.get("pkgver")).toEqual(["0.2.0"]);
    expect(f.get("pkgrel")).toEqual(["1"]);
    expect(f.get("sha256sums")).toEqual([SHA]);
    expect(f.get("source")?.[0]).toBe(
      "Forge_0.2.0_amd64.deb::https://github.com/Reeperk/Forge/releases/download/v0.2.0/Forge_0.2.0_amd64.deb",
    );
    // Same version: a packaging fix keeps its pkgrel.
    const same = updatePkgbuild(pkgbuild.replace("pkgrel=1", "pkgrel=3"), { version: "0.1.0", sha256: SHA });
    expect(pkgbuildFields(same).get("pkgrel")).toEqual(["3"]);
  });

  it("writes .SRCINFO like makepkg", () => {
    const info = srcinfo(updatePkgbuild(pkgbuild, { version: "0.2.0", sha256: SHA }));
    expect(info.startsWith("pkgbase = forge-bin\n\tpkgdesc = ")).toBe(true);
    expect(info).toContain("\tpkgver = 0.2.0\n\tpkgrel = 1\n");
    expect(info).toContain("\toptdepends = git: version control\n");
    expect(info).toContain(`\tsha256sums = ${SHA}\n`);
    expect(info.endsWith("\npkgname = forge-bin\n")).toBe(true);
  });

  it("keeps the committed .SRCINFO files in sync", () => {
    for (const dir of ["packaging/aur/forge-bin", "packaging/aur/forge-git"]) {
      expect(read(`${dir}/.SRCINFO`)).toBe(srcinfo(read(`${dir}/PKGBUILD`)));
    }
  });
});

describe("Flatpak", () => {
  it("points the manifest at a release", () => {
    const next = updateFlatpakManifest(read("packaging/flatpak/dev.forge.ide.yml"), { version: "0.2.0", sha256: SHA });
    expect(next).toContain(
      `url: https://github.com/Reeperk/Forge/releases/download/v0.2.0/Forge_0.2.0_amd64.deb\n        sha256: ${SHA}`,
    );
    expect(() => updateFlatpakManifest("id: x\n", { version: "0.2.0", sha256: SHA })).toThrow(/no release .deb/);
  });

  it("adds the release to the metainfo once", () => {
    const xml = read("packaging/flatpak/dev.forge.ide.metainfo.xml");
    const next = addMetainfoRelease(xml, { version: "0.2.0", date: "2026-12-01" });
    expect(next.indexOf('version="0.2.0"')).toBeLessThan(next.indexOf('version="0.1.0"'));
    expect(addMetainfoRelease(next, { version: "0.2.0", date: "2026-12-02" })).toBe(next);
  });
});

describe("beta feed", () => {
  it("marks latest.json with its channel", () => {
    const feed = JSON.parse(channelFeed(JSON.stringify({ version: "0.3.0-beta.1", platforms: {} }), "beta"));
    expect(feed).toEqual({ version: "0.3.0-beta.1", platforms: {}, channel: "beta" });
    expect(() => channelFeed("{}", "beta")).toThrow(/no version/);
  });
});
