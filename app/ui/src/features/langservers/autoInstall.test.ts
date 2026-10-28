import { describe, expect, it, vi } from "vitest";
import type { ServerRow } from "@/ipc/langservers";
import {
  AUTO_INSTALL_KEY,
  intelephenseAction,
  managedOption,
  OFFER_TEXT,
  offerIntelephense,
  progressFraction,
  progressText,
  updateSummary,
  type PromptInput,
  failureDetail,
} from "./autoInstall";

const missing: ServerRow = {
  id: "intelephense",
  name: "Intelephense",
  group: "PHP",
  languages: ["PHP"],
  command: "intelephense",
  args: ["--stdio"],
  homepage: "https://intelephense.com",
  license: "Proprietary",
  enabled: true,
  defaultEnabled: true,
  status: "notInstalled",
  inProject: true,
  install: [
    { index: 0, tool: "npm", command: "npm install --prefix /data/forge/langservers intelephense", runnable: true, available: true, managed: true },
  ],
  defaults: { initializationOptions: null, settings: null },
  source: "php",
};

const input = (extra: Partial<PromptInput> = {}): PromptInput => ({
  php: true,
  row: missing,
  autoInstall: "ask",
  server: "intelephense",
  offered: false,
  ...extra,
});

describe("Intelephense install offer", () => {
  it("asks in a local PHP project without Intelephense", () => {
    expect(intelephenseAction(input())).toBe("ask");
    expect(intelephenseAction(input({ autoInstall: "always" }))).toBe("install");
  });

  it("asks before installing on a remote project's host", () => {
    const host = "deploy@shop.example.com";
    expect(intelephenseAction(input({ host }))).toBe("ask");
    expect(intelephenseAction(input({ host, autoInstall: "always" }))).toBe("ask");
    const showBalloon = vi.fn((..._args: unknown[]) => 1);
    offerIntelephense(input({ host }), { showBalloon, install: vi.fn(async () => undefined), saveSetting: vi.fn() });
    expect(showBalloon.mock.calls[0][0]).toBe(`${OFFER_TEXT} on ${host}`);
  });

  it("stays quiet when there is nothing to offer or the user said no", () => {
    expect(intelephenseAction(input({ php: false }))).toBe("none");
    expect(intelephenseAction(input({ offered: true }))).toBe("none");
    expect(intelephenseAction(input({ autoInstall: "never" }))).toBe("none");
    expect(intelephenseAction(input({ server: "phpactor" }))).toBe("none");
    expect(intelephenseAction(input({ server: "none" }))).toBe("none");
    // php-lsp is built in: no Intelephense nag with it or with Auto.
    expect(intelephenseAction(input({ server: "php-lsp" }))).toBe("none");
    expect(intelephenseAction(input({ server: "auto" }))).toBe("none");
    expect(intelephenseAction(input({ row: { ...missing, status: "running", path: "/x/intelephense" } }))).toBe("none");
    expect(intelephenseAction(input({ row: { ...missing, install: [{ ...missing.install[0], managed: false }] } }))).toBe("none");
    expect(intelephenseAction(input({ row: undefined }))).toBe("none");
  });

  it("shows a sticky balloon whose actions install, dismiss or turn the offer off", () => {
    const showBalloon = vi.fn((..._args: unknown[]) => 1);
    const install = vi.fn(async () => undefined);
    const saveSetting = vi.fn();
    expect(offerIntelephense(input(), { showBalloon, install, saveSetting })).toBe("ask");
    expect(showBalloon).toHaveBeenCalledOnce();
    const [text, , ms, actions] = showBalloon.mock.calls[0] as [string, string, number, { label: string; run: () => void }[]];
    expect(text).toBe(OFFER_TEXT);
    expect(ms).toBe(0);
    expect(actions.map((a) => a.label)).toEqual(["Install", "Not now", "Don't ask again"]);

    actions[1].run();
    expect(install).not.toHaveBeenCalled();
    expect(saveSetting).not.toHaveBeenCalled();
    actions[2].run();
    expect(saveSetting).toHaveBeenCalledWith(AUTO_INSTALL_KEY, "never");
    actions[0].run();
    expect(install).toHaveBeenCalledWith(missing);
  });

  it("installs without asking when set to always", () => {
    const showBalloon = vi.fn(() => 1);
    const install = vi.fn(async () => undefined);
    expect(offerIntelephense(input({ autoInstall: "always" }), { showBalloon, install, saveSetting: vi.fn() })).toBe("install");
    expect(install).toHaveBeenCalledWith(missing);
    expect(showBalloon).not.toHaveBeenCalled();
  });

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
    expect(updateSummary([{ name: "intelephense", installed: "1.12.6", latest: "1.14.4" }])).toBe("intelephense 1.12.6 → 1.14.4");
  });
});

describe("failureDetail", () => {
  it("names npm's error and the exit code", () => {
    const npm = [
      "npm error code SELF_SIGNED_CERT_IN_CHAIN",
      "npm error request to https://registry.npmjs.org/intelephense failed, reason: self-signed certificate in certificate chain",
      "npm error A complete log of this run can be found in: /home/deploy/.npm/_logs/x.log",
    ];
    expect(failureDetail(1, npm)).toBe(
      "request to https://registry.npmjs.org/intelephense failed, reason: self-signed certificate in certificate chain. Exit code 1.",
    );
  });
  it("falls back to the last line, or to the settings page", () => {
    expect(failureDetail(null, ["killed"])).toBe("killed.");
    expect(failureDetail(undefined, [])).toBe("See Settings | Languages & Frameworks | Language Servers.");
  });
});
