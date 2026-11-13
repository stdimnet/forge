import { describe, expect, it } from "vitest";
import type { ServerRow } from "@/ipc/langservers";
import { failureDetail, managedOption, progressFraction, progressText, updateSummary } from "./autoInstall";

const missing: ServerRow = {
  id: "yaml-language-server",
  name: "yaml-language-server",
  group: "YAML",
  languages: ["YAML"],
  command: "yaml-language-server",
  args: ["--stdio"],
  homepage: "https://github.com/redhat-developer/yaml-language-server",
  license: "MIT",
  enabled: true,
  defaultEnabled: true,
  status: "notInstalled",
  inProject: true,
  install: [
    { index: 0, tool: "npm", command: "npm install --prefix /data/forge/langservers yaml-language-server", runnable: true, available: true, managed: true },
  ],
  defaults: { initializationOptions: null, settings: null },
  source: "catalog",
};

describe("managed installs", () => {
  it("finds the managed install option", () => {
    expect(managedOption(missing)?.index).toBe(0);
    expect(managedOption({ ...missing, install: [] })).toBeUndefined();
  });
});

describe("install progress text", () => {
  it("shows percent, megabytes or just the step", () => {
    expect(progressText("Downloading Node.js v22.11.0", 24, 48)).toBe("Downloading Node.js v22.11.0 · 50%");
    expect(progressText("Downloading x", 3 * 1048576, null)).toBe("Downloading x · 3.0 MB");
    expect(progressText("Unpacking Node.js…", 0)).toBe("Unpacking Node.js…");
    expect(progressFraction(1, 4)).toBe(0.25);
    expect(progressFraction(1, null)).toBeNull();
    expect(updateSummary([{ name: "yaml-language-server", installed: "1.14.0", latest: "1.15.0" }])).toBe("yaml-language-server 1.14.0 → 1.15.0");
  });
});

describe("failureDetail", () => {
  it("names npm's error and the exit code", () => {
    const npm = [
      "npm error code SELF_SIGNED_CERT_IN_CHAIN",
      "npm error request to https://registry.npmjs.org/yaml-language-server failed, reason: self-signed certificate in certificate chain",
      "npm error A complete log of this run can be found in: /home/deploy/.npm/_logs/x.log",
    ];
    expect(failureDetail(1, npm)).toBe(
      "request to https://registry.npmjs.org/yaml-language-server failed, reason: self-signed certificate in certificate chain. Exit code 1.",
    );
  });
  it("falls back to the last line, or to the settings page", () => {
    expect(failureDetail(null, ["killed"])).toBe("killed.");
    expect(failureDetail(undefined, [])).toBe("See Settings | Languages & Frameworks | Language Servers.");
  });
});
