import { describe, expect, it } from "vitest";
import { bumpVersion, comparePre, msiVersion, parseVersion, readVersion, writeVersion } from "./versionBump.mjs";

describe("Pride Versioning", () => {
  it("resets the lower parts like the pridever.org example", () => {
    expect(bumpVersion("1.2.3", "proud")).toBe("2.0.0");
    expect(bumpVersion("1.2.3", "default")).toBe("1.3.0");
    expect(bumpVersion("1.2.3", "shame")).toBe("1.2.4");
  });

  it("drops or adds pre-release labels", () => {
    expect(bumpVersion("0.2.0-beta.1", "shame")).toBe("0.2.1");
    expect(bumpVersion("0.1.0", "default", "beta.1")).toBe("0.2.0-beta.1");
    expect(parseVersion("2.7.123-rc.2")).toEqual({ proud: 2, default: 7, shame: 123, pre: "rc.2" });
  });

  it("bumps only the pre-release label with pre", () => {
    expect(bumpVersion("0.1.1-beta.1", "pre")).toBe("0.1.1-beta.2");
    expect(bumpVersion("0.1.1-beta.9", "pre")).toBe("0.1.1-beta.10");
    expect(bumpVersion("0.1.1-beta", "pre")).toBe("0.1.1-beta.1");
    expect(bumpVersion("0.1.1-beta.2", "pre", "rc.1")).toBe("0.1.1-rc.1");
    expect(() => bumpVersion("0.1.1", "pre")).toThrow(/not a pre-release/);
    expect(() => bumpVersion("0.1.1-rc.1", "pre", "beta.3")).toThrow(/does not come after/);
    expect(() => bumpVersion("0.1.1-beta.2", "pre", "beta.2")).toThrow(/does not come after/);
  });

  it("drops the pre-release label with release", () => {
    expect(bumpVersion("0.1.1-rc.1", "release")).toBe("0.1.1");
    expect(() => bumpVersion("0.1.1", "release")).toThrow(/already a release/);
    expect(() => bumpVersion("0.1.1-rc.1", "release", "rc.2")).toThrow(/no --pre/);
  });

  it("orders pre-release labels like SemVer", () => {
    expect(comparePre("beta.2", "beta.10")).toBe(-1);
    expect(comparePre("beta", "beta.1")).toBe(-1);
    expect(comparePre("rc.1", "beta.5")).toBe(1);
    expect(comparePre("1", "alpha")).toBe(-1);
    expect(comparePre("beta.1", "beta.1")).toBe(0);
  });

  it("rejects what is not PROUD.DEFAULT.SHAME", () => {
    expect(() => parseVersion("1.2")).toThrow();
    expect(() => bumpVersion("1.2.3", /** @type {any} */ ("minor"))).toThrow(/proud, default, shame, pre or release/);
    expect(() => bumpVersion("1.2.3", "proud", "bad label")).toThrow(/pre-release/);
  });

  it("reads and writes the version in Cargo.toml and JSON files", () => {
    const cargo = '[workspace]\nmembers = []\n\n[workspace.package]\nversion = "0.1.0"\nedition = "2021"\n\n[dependencies]\nserde = { version = "1" }\n';
    expect(readVersion("Cargo.toml", cargo)).toBe("0.1.0");
    const bumped = writeVersion("Cargo.toml", cargo, "0.2.0");
    expect(readVersion("Cargo.toml", bumped)).toBe("0.2.0");
    expect(bumped).toContain('serde = { version = "1" }');

    const json = '{\n  "name": "forge-ui",\n  "version": "0.1.0",\n  "deps": { "x": { "version": "9.9.9" } }\n}\n';
    const out = writeVersion("app/ui/package.json", json, "1.0.0");
    expect(readVersion("app/ui/package.json", out)).toBe("1.0.0");
    expect(out).toContain('"version": "9.9.9"');
  });
});

describe("MSI version", () => {
  it("keeps releases as they are", () => {
    expect(msiVersion("0.1.1")).toBe("0.1.1");
    expect(msiVersion("255.255.65535")).toBe("255.255.65535");
  });

  it("puts the pre-release number into the fourth field", () => {
    expect(msiVersion("0.1.1-beta.1")).toBe("0.1.1.1");
    expect(msiVersion("0.1.1-alpha.3")).toBe("0.1.1.3");
    expect(msiVersion("0.1.1-rc.2")).toBe("0.1.1.2");
    expect(msiVersion("0.1.1-beta.65535")).toBe("0.1.1.65535");
    expect(msiVersion("0.1.1-7")).toBe("0.1.1.7");
    expect(msiVersion("0.1.1-beta")).toBe("0.1.1.0");
  });

  it("keeps the first three fields of a pre-release and its final version equal", () => {
    // Windows Installer compares only these for a major upgrade.
    const first3 = (/** @type {string} */ v) => msiVersion(v).split(".").slice(0, 3).join(".");
    expect(first3("0.1.1-beta.1")).toBe(first3("0.1.1"));
    expect(first3("0.1.1-beta.2")).toBe(first3("0.1.1-rc.1"));
  });

  it("rejects what Windows Installer cannot store", () => {
    expect(() => msiVersion("0.1.1-beta.65536")).toThrow(/at most 65535 for the pre-release number, not 65536/);
    expect(() => msiVersion("0.1.1-70000")).toThrow(/at most 65535/);
    expect(() => msiVersion("256.0.0")).toThrow(/at most 255 for the major/);
    expect(() => msiVersion("1.256.0")).toThrow(/at most 255 for the minor/);
    expect(() => msiVersion("1.2.65536")).toThrow(/at most 65535 for the build/);
    expect(() => msiVersion("0.1.1-beta.1.2")).toThrow(/label like beta\.1/);
    expect(() => msiVersion("0.1.1-beta-1")).toThrow(/label like beta\.1/);
    expect(() => msiVersion("0.1.1-1.beta")).toThrow(/label like beta\.1/);
  });

  it("refuses to bump to a version without one", () => {
    expect(() => bumpVersion("0.1.0", "shame", "beta.65536")).toThrow(/at most 65535/);
    expect(() => bumpVersion("0.1.1-beta.65535", "pre")).toThrow(/at most 65535/);
    expect(() => bumpVersion("0.1.0", "default", "beta.1.1")).toThrow(/MSI installer/);
    expect(bumpVersion("0.1.1-beta.2", "pre", "rc.1")).toBe("0.1.1-rc.1");
  });
});
